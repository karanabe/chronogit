//! Reducer policy for semantic requests, candidates, jumps, and jump history.

use crate::app::model::NavigationOrigin;
use crate::app::{
    Action, AppEffect, AppState, AppView, ErrorNotice, Event, FocusedPane, LoadState, LspEffect,
    Overlay, VimMotionKind,
};
use crate::domain::{NavigationTarget, RepositoryLocation, SemanticNavigationKind, SourcePosition};

const MAX_JUMP_HISTORY: usize = 64;

pub(crate) fn apply_action(state: &mut AppState, action: Action) -> Option<Vec<AppEffect>> {
    if state.overlay == Overlay::SemanticTargets {
        return Some(candidate_action(state, action));
    }
    if state.overlay == Overlay::LspHover {
        return Some(hover_action(state, action));
    }
    match action {
        Action::MoveCursorLeft if code_content_active(state) => {
            crate::app::code_view::move_cursor_horizontally(state, false);
            Some(Vec::new())
        }
        Action::MoveCursorRight if code_content_active(state) => {
            crate::app::code_view::move_cursor_horizontally(state, true);
            Some(Vec::new())
        }
        Action::ToggleLspHover => Some(request_hover(state)),
        Action::GoToSemanticTarget(kind) => Some(request(state, kind)),
        Action::GoBackFromSemanticTarget => Some(go_back(state)),
        Action::GoForwardFromSemanticTarget => Some(go_forward(state)),
        Action::JumpListBack(count) => Some(jump_list(state, false, count)),
        Action::JumpListForward(count) => Some(jump_list(state, true, count)),
        _ => None,
    }
}

fn jump_list(state: &mut AppState, forward: bool, count: usize) -> Vec<AppEffect> {
    let mut origin = current_origin(state);
    let (source, destination) = if forward {
        (
            &mut state.semantic_navigation.forward_stack,
            &mut state.semantic_navigation.back_stack,
        )
    } else {
        (
            &mut state.semantic_navigation.back_stack,
            &mut state.semantic_navigation.forward_stack,
        )
    };
    let steps = count.max(1).min(source.len());
    for _ in 0..steps {
        let next = source.pop_back();
        if let Some(current) = origin.take() {
            push_bounded(destination, current);
        }
        origin = next;
    }
    if steps > 0
        && let Some(origin) = origin
    {
        reveal_origin(state, origin)
    } else {
        Vec::new()
    }
}

pub(crate) fn apply_event(state: &mut AppState, event: Event) -> Vec<AppEffect> {
    if let Event::LspStatus {
        request_id,
        message,
    } = event
    {
        if state.semantic_navigation.targets.loading_request() == Some(request_id) {
            state.semantic_navigation.status = Some(message);
        } else if state.lsp_hover.content.loading_request() == Some(request_id) {
            state.lsp_hover.status = Some(message);
        }
        return Vec::new();
    }
    if let Event::LspHoverCompleted {
        request_id,
        path,
        position,
        document_revision,
        result,
    } = event
    {
        if state.lsp_hover.content.loading_request() != Some(request_id) {
            return Vec::new();
        }
        if state.code_view.path.as_ref() != Some(&path)
            || state.code_view.cursor != position
            || state.code_view.document_revision != document_revision
            || state.lsp_hover.source_path.as_ref() != Some(&path)
            || state.lsp_hover.source_position != position
            || state.lsp_hover.source_revision != document_revision
        {
            close_hover(state);
            return Vec::new();
        }
        state.lsp_hover.status = None;
        state.lsp_hover.content = match result {
            Ok(content) => LoadState::Ready(content),
            Err(error) => LoadState::Failed(ErrorNotice::new(error.to_string())),
        };
        return Vec::new();
    }
    let Event::SemanticNavigationCompleted {
        request_id,
        path,
        position,
        document_revision,
        kind,
        result,
    } = event
    else {
        return Vec::new();
    };
    if state.semantic_navigation.targets.loading_request() != Some(request_id) {
        return Vec::new();
    }
    if state.code_view.path.as_ref() != Some(&path)
        || state.code_view.cursor != position
        || state.code_view.document_revision != document_revision
        || state.semantic_navigation.source_path.as_ref() != Some(&path)
        || state.semantic_navigation.source_position != position
        || state.semantic_navigation.source_revision != document_revision
    {
        state.semantic_navigation.targets = LoadState::Idle;
        state.semantic_navigation.status = None;
        return Vec::new();
    }
    state.semantic_navigation.status = None;
    match result {
        Err(error) => {
            let notice = ErrorNotice::new(error.to_string());
            state.semantic_navigation.targets = LoadState::Failed(notice.clone());
            state.notice = Some(notice);
            Vec::new()
        }
        Ok(targets) if targets.is_empty() => {
            state.semantic_navigation.targets = LoadState::Ready(Vec::new());
            state.notice = Some(ErrorNotice::new(format!(
                "No {} target at the current cursor.",
                kind.label()
            )));
            Vec::new()
        }
        Ok(mut targets) if targets.len() == 1 => {
            let target = targets.remove(0);
            state.semantic_navigation.targets = LoadState::Ready(vec![target.clone()]);
            jump_to(state, target)
        }
        Ok(targets) => {
            state.semantic_navigation.selection.reset(targets.len());
            state.semantic_navigation.targets = LoadState::Ready(targets);
            state.overlay = Overlay::SemanticTargets;
            state.notice = None;
            Vec::new()
        }
    }
}

fn request_hover(state: &mut AppState) -> Vec<AppEffect> {
    if !code_content_active(state) {
        state.notice = Some(ErrorNotice::new(
            "LSP hover is available only in the current working-tree Code viewer.",
        ));
        return Vec::new();
    }
    let Some(path) = state.code_view.path.clone() else {
        state.notice = Some(ErrorNotice::new("Select a source file first."));
        return Vec::new();
    };
    let Some(text) = (match &state.code_view.content {
        LoadState::Ready(document) => document.source(),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
    }) else {
        state.notice = Some(ErrorNotice::new(
            "LSP hover requires a complete UTF-8 text document.",
        ));
        return Vec::new();
    };
    let text = text.to_owned();
    let position = state.code_view.cursor;
    let document_revision = state.code_view.document_revision;
    let request_id = state.request_id();
    state.semantic_navigation.targets = LoadState::Idle;
    state.semantic_navigation.status = None;
    state.lsp_hover.content = LoadState::Loading { request_id };
    state.lsp_hover.source_path = Some(path.clone());
    state.lsp_hover.source_position = position;
    state.lsp_hover.source_revision = document_revision;
    state.lsp_hover.scroll = 0;
    state.lsp_hover.status = Some("starting or contacting language server".to_owned());
    state.lsp_hover.return_overlay = state.overlay;
    state.overlay = Overlay::LspHover;
    state.notice = None;
    vec![AppEffect::Lsp(LspEffect::Hover {
        request_id,
        path,
        text,
        position,
        document_revision,
    })]
}

fn hover_action(state: &mut AppState, action: Action) -> Vec<AppEffect> {
    match action {
        Action::MoveUp => {
            state.lsp_hover.scroll = state.lsp_hover.scroll.saturating_sub(1);
        }
        Action::MoveDown => {
            state.lsp_hover.scroll = state.lsp_hover.scroll.saturating_add(1);
        }
        Action::VimMotion(motion) => match motion.kind() {
            VimMotionKind::Up
            | VimMotionKind::PreviousLineFirstNonBlank
            | VimMotionKind::ScrollLineUp
            | VimMotionKind::WordBackward
            | VimMotionKind::BigWordBackward => {
                state.lsp_hover.scroll = state.lsp_hover.scroll.saturating_sub(motion.count());
            }
            VimMotionKind::Down
            | VimMotionKind::NextLineFirstNonBlank
            | VimMotionKind::ScrollLineDown
            | VimMotionKind::WordForward
            | VimMotionKind::BigWordForward => {
                state.lsp_hover.scroll = state.lsp_hover.scroll.saturating_add(motion.count());
            }
            VimMotionKind::BufferTop | VimMotionKind::LineStart => {
                state.lsp_hover.scroll = 0;
            }
            _ => {}
        },
        Action::ToggleLspHover | Action::CloseOverlay | Action::DismissSearchOrClose => {
            close_hover(state)
        }
        _ => {}
    }
    Vec::new()
}

fn close_hover(state: &mut AppState) {
    state.overlay = state.lsp_hover.return_overlay;
    state.lsp_hover.content = LoadState::Idle;
    state.lsp_hover.status = None;
    state.lsp_hover.scroll = 0;
}

fn request(state: &mut AppState, kind: SemanticNavigationKind) -> Vec<AppEffect> {
    if !code_content_active(state) {
        state.notice = Some(ErrorNotice::new(
            "Semantic navigation is available only in the current working-tree Code viewer.",
        ));
        return Vec::new();
    }
    let Some(path) = state.code_view.path.clone() else {
        state.notice = Some(ErrorNotice::new("Select a source file first."));
        return Vec::new();
    };
    let Some(text) = (match &state.code_view.content {
        LoadState::Ready(document) => document.source(),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
    }) else {
        state.notice = Some(ErrorNotice::new(
            "Semantic navigation requires a complete UTF-8 text document.",
        ));
        return Vec::new();
    };
    let text = text.to_owned();
    let position = state.code_view.cursor;
    let document_revision = state.code_view.document_revision;
    let request_id = state.request_id();
    state.semantic_navigation.targets = LoadState::Loading { request_id };
    state.semantic_navigation.source_path = Some(path.clone());
    state.semantic_navigation.source_position = position;
    state.semantic_navigation.source_revision = document_revision;
    state.semantic_navigation.kind = Some(kind);
    state.semantic_navigation.status = Some("starting or contacting language server".to_owned());
    state.notice = None;
    vec![AppEffect::Lsp(LspEffect::Navigate {
        request_id,
        kind,
        path,
        text,
        position,
        document_revision,
    })]
}

fn candidate_action(state: &mut AppState, action: Action) -> Vec<AppEffect> {
    match action {
        Action::MoveUp => move_candidate(state, -1),
        Action::MoveDown => move_candidate(state, 1),
        Action::MoveTop => {
            let len = target_count(state);
            state.semantic_navigation.selection.top(len);
        }
        Action::MoveBottom => {
            let len = target_count(state);
            state.semantic_navigation.selection.bottom(len);
        }
        Action::VimMotion(motion) => {
            let len = target_count(state);
            match motion.kind() {
                VimMotionKind::Up
                | VimMotionKind::PreviousLineFirstNonBlank
                | VimMotionKind::WordBackward
                | VimMotionKind::BigWordBackward => {
                    state.semantic_navigation.selection.move_by(
                        -(isize::try_from(motion.count()).unwrap_or(isize::MAX)),
                        len,
                    );
                }
                VimMotionKind::Down
                | VimMotionKind::NextLineFirstNonBlank
                | VimMotionKind::WordForward
                | VimMotionKind::BigWordForward => {
                    state
                        .semantic_navigation
                        .selection
                        .move_by(isize::try_from(motion.count()).unwrap_or(isize::MAX), len);
                }
                VimMotionKind::BufferTop | VimMotionKind::LineStart => {
                    state.semantic_navigation.selection.top(len);
                }
                VimMotionKind::BufferBottom | VimMotionKind::LineEnd => {
                    state.semantic_navigation.selection.bottom(len);
                }
                _ => {}
            }
        }
        Action::Activate => {
            let target = match (
                &state.semantic_navigation.targets,
                state.semantic_navigation.selection.index(),
            ) {
                (LoadState::Ready(targets), Some(index)) => targets.get(index).cloned(),
                _ => None,
            };
            if let Some(target) = target {
                return jump_to(state, target);
            }
        }
        Action::CloseOverlay | Action::DismissSearchOrClose => state.overlay = Overlay::CodeContent,
        _ => {}
    }
    Vec::new()
}

fn move_candidate(state: &mut AppState, delta: isize) {
    let len = target_count(state);
    state.semantic_navigation.selection.move_by(delta, len);
}

fn target_count(state: &AppState) -> usize {
    match &state.semantic_navigation.targets {
        LoadState::Ready(targets) => targets.len(),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => 0,
    }
}

fn jump_to(state: &mut AppState, target: NavigationTarget) -> Vec<AppEffect> {
    state.overlay = Overlay::CodeContent;
    match target {
        NavigationTarget::External { display_uri } => {
            state.notice = Some(ErrorNotice::new(format!(
                "Target is outside the repository or uses an unsupported URI: {display_uri}"
            )));
            Vec::new()
        }
        NavigationTarget::Repository(location) => jump_to_repository(state, location),
    }
}

fn jump_to_repository(state: &mut AppState, location: RepositoryLocation) -> Vec<AppEffect> {
    if let Some(origin) = current_origin(state) {
        push_bounded(&mut state.semantic_navigation.back_stack, origin);
    }
    state.semantic_navigation.forward_stack.clear();
    state.notice = None;
    crate::app::code_view::reveal_location(
        state,
        location.path().clone(),
        location.selection().start(),
    )
    .into_iter()
    .map(AppEffect::from)
    .collect()
}

fn go_back(state: &mut AppState) -> Vec<AppEffect> {
    jump_to_previous(state, false)
}

pub(crate) fn jump_to_previous(state: &mut AppState, linewise: bool) -> Vec<AppEffect> {
    let Some(origin) = state.semantic_navigation.back_stack.pop_back() else {
        state.notice = Some(ErrorNotice::new("No earlier jump location."));
        return Vec::new();
    };
    if let Some(current) = current_origin(state) {
        push_bounded(&mut state.semantic_navigation.forward_stack, current);
    }
    let mut origin = origin;
    if linewise {
        origin.cursor = SourcePosition::new(origin.cursor.line(), origin.first_non_blank_column);
        origin.viewport_horizontal = 0;
    }
    reveal_origin(state, origin)
}

fn go_forward(state: &mut AppState) -> Vec<AppEffect> {
    let Some(origin) = state.semantic_navigation.forward_stack.pop_back() else {
        state.notice = Some(ErrorNotice::new("No newer jump location."));
        return Vec::new();
    };
    if let Some(current) = current_origin(state) {
        push_bounded(&mut state.semantic_navigation.back_stack, current);
    }
    reveal_origin(state, origin)
}

pub(crate) fn current_origin(state: &AppState) -> Option<NavigationOrigin> {
    state.code_view.path.clone().map(|path| NavigationOrigin {
        path,
        cursor: state.code_view.cursor,
        first_non_blank_column: match &state.code_view.content {
            LoadState::Ready(document) => document
                .lines()
                .get(usize::try_from(state.code_view.cursor.line()).unwrap_or(usize::MAX))
                .and_then(|line| {
                    line.char_indices()
                        .find(|(_, character)| !character.is_whitespace())
                        .map(|(column, _)| column)
                })
                .unwrap_or(0),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => 0,
        },
        viewport_vertical: state.code_view.viewport_vertical,
        viewport_horizontal: state.code_view.viewport_horizontal,
    })
}

fn push_bounded(
    stack: &mut std::collections::VecDeque<NavigationOrigin>,
    origin: NavigationOrigin,
) {
    if stack.len() == MAX_JUMP_HISTORY {
        stack.pop_front();
    }
    stack.push_back(origin);
}

/// Adds a completed Vim jump to the shared jump list.
///
/// The caller captures `origin` before moving and calls this only when the
/// cursor actually changed. Vim's jump list and LSP navigation therefore use
/// the same Ctrl-O/Ctrl-I history.
pub(crate) fn remember_jump(state: &mut AppState, origin: NavigationOrigin) {
    if current_origin(state).as_ref() == Some(&origin) {
        return;
    }
    push_bounded(&mut state.semantic_navigation.back_stack, origin);
    state.semantic_navigation.forward_stack.clear();
}

pub(crate) fn jump_to_mark(
    state: &mut AppState,
    origin: NavigationOrigin,
    linewise: bool,
    record_jump: bool,
) -> Vec<AppEffect> {
    let current = current_origin(state);
    let mut origin = origin;
    if linewise {
        origin.cursor = SourcePosition::new(origin.cursor.line(), origin.first_non_blank_column);
        origin.viewport_horizontal = 0;
    }
    let effects = reveal_origin(state, origin);
    if record_jump && let Some(current) = current {
        remember_jump(state, current);
    }
    effects
}

fn reveal_origin(state: &mut AppState, origin: NavigationOrigin) -> Vec<AppEffect> {
    state.view = AppView::Code;
    let effects = crate::app::code_view::reveal_location(state, origin.path, origin.cursor);
    state.code_view.viewport_vertical = origin.viewport_vertical;
    state.code_view.viewport_horizontal = origin.viewport_horizontal;
    state.notice = None;
    effects.into_iter().map(AppEffect::from).collect()
}

fn code_content_active(state: &AppState) -> bool {
    state.view == AppView::Code
        && (state.focus == FocusedPane::Diff || state.overlay == Overlay::CodeContent)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{apply_action, apply_event};
    use crate::app::{
        Action, AppState, AppView, Event, LoadState, Overlay, VimMotion, VimMotionKind,
    };
    use crate::domain::{
        FileDocument, NavigationTarget, RepoPath, RepositoryLocation, RepositoryRoot,
        SemanticNavigationKind, SourcePosition, SourceRange,
    };

    fn state() -> AppState {
        let mut state = AppState::new(
            RepositoryRoot::new(PathBuf::from("/tmp/repo"))
                .unwrap_or_else(|error| panic!("root: {error}")),
            AppView::Code,
        );
        state.focus = crate::app::FocusedPane::Diff;
        state.overlay = Overlay::CodeContent;
        state.code_view.path = Some(
            RepoPath::from_bytes(b"src/main.rs".to_vec())
                .unwrap_or_else(|error| panic!("path: {error}")),
        );
        state.code_view.content = LoadState::Ready(FileDocument::Text {
            source: "fn main() {}\n".to_owned(),
            lines: vec!["fn main() {}".to_owned()],
            valid_utf8: true,
            truncated: false,
        });
        state
    }

    #[test]
    fn vim_and_arrow_actions_move_the_semantic_cursor_over_source_text() {
        let mut state = state();
        let _none = apply_action(&mut state, Action::MoveCursorRight);
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 1));
        let _none = apply_action(&mut state, Action::MoveCursorRight);
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 2));
        let _none = apply_action(&mut state, Action::MoveCursorLeft);
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 1));

        state.code_view.cursor = SourcePosition::new(0, 0);
        assert!(
            state
                .handle_app_action(Action::VimMotion(VimMotion::new(
                    VimMotionKind::WordForward,
                )))
                .is_empty()
        );
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 3));
    }

    #[test]
    fn vim_marks_restore_exact_or_first_non_blank_positions_and_join_jump_history() {
        let mut state = state();
        state.code_view.content = LoadState::Ready(FileDocument::Text {
            source: "zero\n  marked word\n".to_owned(),
            lines: vec!["zero".to_owned(), "  marked word".to_owned()],
            valid_utf8: true,
            truncated: false,
        });
        state.code_view.cursor = SourcePosition::new(1, 9);
        assert!(state.handle_app_action(Action::SetVimMark('a')).is_empty());

        state.code_view.cursor = SourcePosition::new(0, 2);
        let effects = state.handle_app_action(Action::JumpToVimMark {
            mark: 'a',
            linewise: true,
            record_jump: true,
        });
        assert_eq!(effects.len(), 1);
        assert_eq!(state.code_view.cursor, SourcePosition::new(1, 2));
        assert_eq!(state.semantic_navigation.back_stack.len(), 1);
        assert_eq!(
            state
                .semantic_navigation
                .back_stack
                .back()
                .map(|origin| origin.cursor),
            Some(SourcePosition::new(0, 2))
        );
    }

    #[test]
    fn mark_scans_are_counted_and_history_free_mark_jumps_do_not_push() {
        let mut state = state();
        state.code_view.content = LoadState::Ready(FileDocument::Text {
            source: "zero\n  first\n    second\n".to_owned(),
            lines: vec![
                "zero".to_owned(),
                "  first".to_owned(),
                "    second".to_owned(),
            ],
            valid_utf8: true,
            truncated: false,
        });
        state.code_view.cursor = SourcePosition::new(1, 4);
        assert!(state.handle_app_action(Action::SetVimMark('a')).is_empty());
        state.code_view.cursor = SourcePosition::new(2, 7);
        assert!(state.handle_app_action(Action::SetVimMark('b')).is_empty());
        state.code_view.cursor = SourcePosition::new(0, 0);

        let _effects = state.handle_app_action(Action::VimMotion(
            VimMotion::new(VimMotionKind::NextMarkLine).counted(2, true),
        ));
        assert_eq!(state.code_view.cursor, SourcePosition::new(2, 4));

        let history_len = state.semantic_navigation.back_stack.len();
        let _effects = state.handle_app_action(Action::JumpToVimMark {
            mark: 'a',
            linewise: false,
            record_jump: false,
        });
        assert_eq!(state.code_view.cursor, SourcePosition::new(1, 4));
        assert_eq!(state.semantic_navigation.back_stack.len(), history_len);
    }

    #[test]
    fn counted_jump_list_navigation_continues_for_same_file_locations() {
        let mut state = state();
        state.code_view.content = LoadState::Ready(FileDocument::Text {
            source: "zero\none\ntwo\n".to_owned(),
            lines: vec!["zero".to_owned(), "one".to_owned(), "two".to_owned()],
            valid_utf8: true,
            truncated: false,
        });
        state.code_view.cursor = SourcePosition::new(1, 0);
        assert!(
            state
                .handle_app_action(Action::VimMotion(VimMotion::new(
                    VimMotionKind::BufferBottom,
                )))
                .is_empty()
        );
        assert!(
            state
                .handle_app_action(Action::VimMotion(VimMotion::new(VimMotionKind::BufferTop,)))
                .is_empty()
        );

        let _effects = state.handle_app_action(Action::JumpListBack(2));
        assert_eq!(state.code_view.cursor, SourcePosition::new(1, 0));

        let _effects = state.handle_app_action(Action::JumpListForward(2));
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 0));
    }

    #[test]
    fn counted_jumps_preserve_intermediate_mark_columns_without_loading_them() {
        let mut state = state();
        state.semantic_navigation.back_stack.clear();
        let current = super::current_origin(&state).unwrap_or_else(|| panic!("missing origin"));
        for (line, first_non_blank_column) in [(1, 2), (2, 4)] {
            let mut origin = current.clone();
            origin.cursor = SourcePosition::new(line, first_non_blank_column + 1);
            origin.first_non_blank_column = first_non_blank_column;
            state.semantic_navigation.back_stack.push_back(origin);
        }
        state.view = AppView::History;
        let effects = state.handle_app_action(Action::JumpListBack(2));
        assert_eq!(effects.len(), 1);
        assert_eq!(state.view, AppView::Code);
        assert_eq!(
            state
                .semantic_navigation
                .forward_stack
                .back()
                .map(|origin| origin.first_non_blank_column),
            Some(4)
        );
    }

    #[test]
    fn horizontal_keys_focus_code_then_move_while_control_focus_can_return() {
        let mut state = state();
        state.overlay = Overlay::None;
        state.focus = crate::app::FocusedPane::Primary;

        assert!(state.handle_app_action(Action::MoveCursorRight).is_empty());
        assert_eq!(state.focus, crate::app::FocusedPane::Diff);
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 0));

        assert!(state.handle_app_action(Action::MoveCursorRight).is_empty());
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 1));

        assert!(state.handle_app_action(Action::FocusLeft).is_empty());
        assert_eq!(state.focus, crate::app::FocusedPane::Primary);
    }

    #[test]
    fn hover_opens_scrolls_and_toggles_back_to_code_content() {
        let mut state = state();
        state.code_view.cursor = SourcePosition::new(0, 3);
        let effects = apply_action(&mut state, Action::ToggleLspHover).unwrap_or_default();
        let (request_id, document_revision) = match effects[0] {
            crate::app::AppEffect::Lsp(crate::app::LspEffect::Hover {
                request_id,
                document_revision,
                ..
            }) => (request_id, document_revision),
            _ => panic!("expected hover"),
        };
        assert_eq!(state.overlay, Overlay::LspHover);
        let _none = apply_event(
            &mut state,
            Event::LspHoverCompleted {
                request_id,
                path: RepoPath::from_bytes(b"src/main.rs".to_vec())
                    .unwrap_or_else(|error| panic!("path: {error}")),
                position: SourcePosition::new(0, 3),
                document_revision,
                result: Ok(Some("```rust\nfn main()\n```\n\nEntry point.".to_owned())),
            },
        );
        assert!(matches!(state.lsp_hover.content, LoadState::Ready(Some(_))));
        let _none = apply_action(&mut state, Action::MoveDown);
        assert_eq!(state.lsp_hover.scroll, 1);
        let _none = apply_action(&mut state, Action::MoveUp);
        assert_eq!(state.lsp_hover.scroll, 0);
        let _none = apply_action(&mut state, Action::ToggleLspHover);
        assert_eq!(state.overlay, Overlay::CodeContent);
        assert!(matches!(state.lsp_hover.content, LoadState::Idle));

        state.overlay = Overlay::None;
        let _effect = apply_action(&mut state, Action::ToggleLspHover);
        let _none = apply_action(&mut state, Action::CloseOverlay);
        assert_eq!(state.overlay, Overlay::None);
    }

    #[test]
    fn single_target_jumps_back_and_forward_between_saved_positions() {
        let mut state = state();
        state.code_view.cursor = SourcePosition::new(0, 3);
        let effects = apply_action(
            &mut state,
            Action::GoToSemanticTarget(SemanticNavigationKind::Definition),
        )
        .unwrap_or_else(|| panic!("semantic action was not handled"));
        let request_id = match effects.first() {
            Some(crate::app::AppEffect::Lsp(crate::app::LspEffect::Navigate {
                request_id,
                ..
            })) => *request_id,
            other => panic!("unexpected effect: {other:?}"),
        };
        let target_path = RepoPath::from_bytes(b"src/lib.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        let document_revision = state.code_view.document_revision;
        let effects = apply_event(
            &mut state,
            Event::SemanticNavigationCompleted {
                request_id,
                path: RepoPath::from_bytes(b"src/main.rs".to_vec())
                    .unwrap_or_else(|error| panic!("path: {error}")),
                position: SourcePosition::new(0, 3),
                document_revision,
                kind: SemanticNavigationKind::Definition,
                result: Ok(vec![NavigationTarget::Repository(RepositoryLocation::new(
                    target_path,
                    SourceRange::new(SourcePosition::new(7, 2), SourcePosition::new(7, 5)),
                ))]),
            },
        );
        assert_eq!(state.code_view.cursor, SourcePosition::new(7, 2));
        assert_eq!(effects.len(), 1);
        let back = apply_action(&mut state, Action::GoBackFromSemanticTarget)
            .unwrap_or_else(|| panic!("back was not handled"));
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 3));
        assert_eq!(back.len(), 1);
        let forward = apply_action(&mut state, Action::GoForwardFromSemanticTarget)
            .unwrap_or_else(|| panic!("forward was not handled"));
        assert_eq!(state.code_view.cursor, SourcePosition::new(7, 2));
        assert_eq!(forward.len(), 1);
    }

    #[test]
    fn a_new_semantic_jump_after_going_back_discards_newer_history() {
        let mut state = state();
        state.code_view.cursor = SourcePosition::new(0, 3);
        let first_target = RepositoryLocation::new(
            RepoPath::from_bytes(b"src/lib.rs".to_vec())
                .unwrap_or_else(|error| panic!("path: {error}")),
            SourceRange::new(SourcePosition::new(7, 2), SourcePosition::new(7, 5)),
        );
        assert_eq!(super::jump_to_repository(&mut state, first_target).len(), 1);
        assert_eq!(
            apply_action(&mut state, Action::GoBackFromSemanticTarget)
                .unwrap_or_default()
                .len(),
            1
        );
        assert_eq!(state.semantic_navigation.forward_stack.len(), 1);

        let branch_target = RepositoryLocation::new(
            RepoPath::from_bytes(b"src/domain.rs".to_vec())
                .unwrap_or_else(|error| panic!("path: {error}")),
            SourceRange::new(SourcePosition::new(2, 1), SourcePosition::new(2, 4)),
        );
        assert_eq!(
            super::jump_to_repository(&mut state, branch_target).len(),
            1
        );
        assert!(state.semantic_navigation.forward_stack.is_empty());
        assert!(
            apply_action(&mut state, Action::GoForwardFromSemanticTarget)
                .unwrap_or_default()
                .is_empty()
        );
        assert!(
            state
                .notice
                .as_ref()
                .is_some_and(|notice| notice.message().contains("No newer"))
        );
    }

    #[test]
    fn stale_result_does_not_move_a_new_cursor() {
        let mut state = state();
        let effects = apply_action(
            &mut state,
            Action::GoToSemanticTarget(SemanticNavigationKind::Definition),
        )
        .unwrap_or_default();
        let request_id = match effects[0] {
            crate::app::AppEffect::Lsp(crate::app::LspEffect::Navigate { request_id, .. }) => {
                request_id
            }
            _ => panic!("expected navigation"),
        };
        state.code_view.cursor = SourcePosition::new(0, 1);
        let document_revision = state.code_view.document_revision;
        let none = apply_event(
            &mut state,
            Event::SemanticNavigationCompleted {
                request_id,
                path: RepoPath::from_bytes(b"src/main.rs".to_vec())
                    .unwrap_or_else(|error| panic!("path: {error}")),
                position: SourcePosition::new(0, 0),
                document_revision,
                kind: SemanticNavigationKind::Definition,
                result: Ok(Vec::new()),
            },
        );
        assert!(none.is_empty());
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 1));
    }

    #[test]
    fn response_from_before_a_document_refresh_is_discarded() {
        let mut state = state();
        let effects = apply_action(
            &mut state,
            Action::GoToSemanticTarget(SemanticNavigationKind::Definition),
        )
        .unwrap_or_default();
        let (request_id, document_revision) = match effects[0] {
            crate::app::AppEffect::Lsp(crate::app::LspEffect::Navigate {
                request_id,
                document_revision,
                ..
            }) => (request_id, document_revision),
            _ => panic!("expected navigation"),
        };
        state.code_view.document_revision = state.code_view.document_revision.saturating_add(1);
        let none = apply_event(
            &mut state,
            Event::SemanticNavigationCompleted {
                request_id,
                path: RepoPath::from_bytes(b"src/main.rs".to_vec())
                    .unwrap_or_else(|error| panic!("path: {error}")),
                position: SourcePosition::new(0, 0),
                document_revision,
                kind: SemanticNavigationKind::Definition,
                result: Ok(Vec::new()),
            },
        );
        assert!(none.is_empty());
        assert!(matches!(state.semantic_navigation.targets, LoadState::Idle));
    }

    #[test]
    fn only_the_current_request_updates_bounded_server_status() {
        let mut state = state();
        let effects = apply_action(
            &mut state,
            Action::GoToSemanticTarget(SemanticNavigationKind::Definition),
        )
        .unwrap_or_default();
        let request_id = match effects[0] {
            crate::app::AppEffect::Lsp(crate::app::LspEffect::Navigate { request_id, .. }) => {
                request_id
            }
            _ => panic!("expected navigation"),
        };
        apply_event(
            &mut state,
            Event::LspStatus {
                request_id,
                message: "indexing 50%".to_owned(),
            },
        );
        assert_eq!(
            state.semantic_navigation.status.as_deref(),
            Some("indexing 50%")
        );
        let stale_request_id = state.request_id();
        apply_event(
            &mut state,
            Event::LspStatus {
                request_id: stale_request_id,
                message: "stale".to_owned(),
            },
        );
        assert_eq!(
            state.semantic_navigation.status.as_deref(),
            Some("indexing 50%")
        );
    }

    #[test]
    fn multiple_targets_open_a_modal_and_external_target_is_not_opened() {
        let mut state = state();
        let effects = apply_action(
            &mut state,
            Action::GoToSemanticTarget(SemanticNavigationKind::Implementation),
        )
        .unwrap_or_default();
        let request_id = match effects[0] {
            crate::app::AppEffect::Lsp(crate::app::LspEffect::Navigate { request_id, .. }) => {
                request_id
            }
            _ => panic!("expected navigation"),
        };
        let path = RepoPath::from_bytes(b"src/main.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        let internal = NavigationTarget::Repository(RepositoryLocation::new(
            path.clone(),
            SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(0, 2)),
        ));
        let external = NavigationTarget::External {
            display_uri: "jdt://contents/String.class".to_owned(),
        };
        let document_revision = state.code_view.document_revision;
        assert!(
            apply_event(
                &mut state,
                Event::SemanticNavigationCompleted {
                    request_id,
                    path: path.clone(),
                    position: SourcePosition::new(0, 0),
                    document_revision,
                    kind: SemanticNavigationKind::Implementation,
                    result: Ok(vec![internal, external]),
                },
            )
            .is_empty()
        );
        assert_eq!(state.overlay, Overlay::SemanticTargets);
        let _none = apply_action(&mut state, Action::MoveDown);
        let no_jump = apply_action(&mut state, Action::Activate).unwrap_or_default();
        assert!(no_jump.is_empty());
        assert_eq!(state.code_view.path.as_ref(), Some(&path));
        assert!(
            state
                .notice
                .as_ref()
                .is_some_and(|notice| notice.message().contains("unsupported URI"))
        );
    }
}
