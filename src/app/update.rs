//! Reducer logic for semantic actions and asynchronous completion events.
//!
//! This module is the only place that applies interaction policy to [`AppState`].
//! It preserves selection where possible, rejects stale request IDs, and returns
//! typed effects instead of performing repository I/O directly.

use crate::app::repository_search::{
    file_content_last_line, file_content_overlay_action, load_file_view, move_file_content_cursor,
    overlay_action as repository_search_overlay_action, selected_file_history_diff,
};
use crate::app::{
    Action, AppState, AppView, ErrorNotice, Event, FocusedPane, GitEffect, HistoryPanel, LoadState,
    Overlay, RepositorySearchKind, VimMotion, VimMotionKind, VisibleTreeEntry,
};
use crate::domain::{CommitSummary, DiffTarget, RepoPath, SourcePosition, TreeKind};

pub(crate) fn apply_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    let action = if action == Action::DismissSearchOrClose {
        if state.has_active_search_highlights() {
            state.search.dismiss_highlights();
            return Vec::new();
        }
        Action::CloseOverlay
    } else {
        action
    };
    if matches!(
        action,
        Action::FocusLeft | Action::FocusRight | Action::CancelSearch
    ) {
        state.search.cancel_input();
    }
    if action == Action::Quit {
        state.should_quit = true;
        return Vec::new();
    }
    if matches!(action, Action::OpenFileSearch | Action::OpenContentSearch) {
        let kind = if action == Action::OpenFileSearch {
            RepositorySearchKind::Files
        } else {
            RepositorySearchKind::Content
        };
        crate::app::repository_search::open(state, kind);
        return Vec::new();
    }
    if matches!(
        action,
        Action::StartSearch(_)
            | Action::InsertSearch(_)
            | Action::DeleteSearch
            | Action::ConfirmSearch
            | Action::CancelSearch
            | Action::NextMatch
            | Action::PreviousMatch
    ) && apply_document_search_action(state, action)
    {
        return Vec::new();
    }
    if let Action::SetVimMark(mark) = action {
        if let Some(origin) = crate::app::semantic_navigation::current_origin(state)
            && state.view == AppView::Code
            && (state.focus == FocusedPane::Diff || state.overlay == Overlay::CodeContent)
        {
            state.vim_marks.insert(mark, origin);
            state.notice = None;
        } else {
            state.notice = Some(ErrorNotice::new(
                "Vim marks are available in the working-tree Code viewer.",
            ));
        }
        return Vec::new();
    }
    if let Action::JumpToVimMark {
        mark,
        linewise,
        record_jump,
    } = action
    {
        if matches!(mark, '\'' | '`') {
            return crate::app::semantic_navigation::jump_to_previous(state, linewise)
                .into_iter()
                .filter_map(|effect| match effect {
                    crate::app::AppEffect::Git(effect) => Some(effect),
                    crate::app::AppEffect::Lsp(_) => None,
                })
                .collect();
        }
        let Some(origin) = state.vim_marks.get(&mark).cloned() else {
            state.notice = Some(ErrorNotice::new(format!("Mark {mark:?} is not set.")));
            return Vec::new();
        };
        return crate::app::semantic_navigation::jump_to_mark(state, origin, linewise, record_jump)
            .into_iter()
            .filter_map(|effect| match effect {
                crate::app::AppEffect::Git(effect) => Some(effect),
                crate::app::AppEffect::Lsp(_) => None,
            })
            .collect();
    }
    if let Action::VimMotion(motion) = action {
        return apply_vim_motion(state, motion);
    }
    if action == Action::Activate
        && matches!(
            state.overlay,
            Overlay::CodeContent | Overlay::Diff | Overlay::FileContent | Overlay::CommitMessage
        )
    {
        return apply_vim_motion(state, VimMotion::new(VimMotionKind::NextLineFirstNonBlank));
    }
    if state.overlay != Overlay::None {
        return overlay_action(state, action);
    }
    match action {
        Action::ShowChanges => switch_view(state, AppView::Changes),
        Action::ShowHistory => switch_view(state, AppView::History),
        Action::ShowGraph => switch_view(state, AppView::Graph),
        Action::ShowCode => switch_view(state, AppView::Code),
        Action::FocusLeft => {
            state.focus = previous_pane(state.view, state.focus);
            Vec::new()
        }
        Action::FocusRight => {
            state.focus = next_pane(state.view, state.focus);
            Vec::new()
        }
        Action::MoveCursorLeft => {
            state.focus = previous_pane(state.view, state.focus);
            Vec::new()
        }
        Action::MoveCursorRight => {
            state.focus = next_pane(state.view, state.focus);
            Vec::new()
        }
        Action::MoveUp => move_selection(state, -1),
        Action::MoveDown => move_selection(state, 1),
        Action::MoveTop => move_to_edge(state, false),
        Action::MoveBottom => move_to_edge(state, true),
        Action::HalfPageUp => move_half_page(state, -10),
        Action::HalfPageDown => move_half_page(state, 10),
        Action::ScrollLeft => {
            if state.view == AppView::Code {
                state.code_view.viewport_horizontal =
                    state.code_view.viewport_horizontal.saturating_sub(4);
            } else if state.view == AppView::FileHistory && !state.file_view.showing_history_diff {
                state.file_view.horizontal = state.file_view.horizontal.saturating_sub(4);
            } else {
                state.diff.horizontal = state.diff.horizontal.saturating_sub(4);
            }
            Vec::new()
        }
        Action::ScrollRight => {
            if state.view == AppView::Code {
                state.code_view.viewport_horizontal =
                    state.code_view.viewport_horizontal.saturating_add(4);
            } else if state.view == AppView::FileHistory && !state.file_view.showing_history_diff {
                state.file_view.horizontal = state.file_view.horizontal.saturating_add(4);
            } else {
                state.diff.horizontal = state.diff.horizontal.saturating_add(4);
            }
            Vec::new()
        }
        Action::Refresh => refresh(state),
        Action::ToggleMessage => toggle_message(state),
        Action::ToggleDetails => toggle_details(state),
        Action::ToggleTree => toggle_tree(state),
        Action::Activate => activate(state),
        Action::ToggleHelp => {
            state.overlay = Overlay::Help;
            Vec::new()
        }
        Action::CloseOverlay if state.view == AppView::GraphDetails => {
            state.view = AppView::Graph;
            state.focus = FocusedPane::Primary;
            Vec::new()
        }
        Action::CloseOverlay if state.view == AppView::CommitDetails => {
            state.view = AppView::History;
            state.focus = FocusedPane::Primary;
            Vec::new()
        }
        Action::CloseOverlay if state.view == AppView::FileHistory => {
            state.view = state.file_view.return_view;
            state.focus = FocusedPane::Primary;
            Vec::new()
        }
        Action::StartSearch(_)
        | Action::InsertSearch(_)
        | Action::DeleteSearch
        | Action::ConfirmSearch
        | Action::CancelSearch
        | Action::NextMatch
        | Action::PreviousMatch
        | Action::CloseOverlay
        | Action::DismissSearchOrClose
        | Action::Tick
        | Action::Quit
        | Action::OpenFileSearch
        | Action::OpenContentSearch
        | Action::ToggleLspHover
        | Action::GoToSemanticTarget(_)
        | Action::GoBackFromSemanticTarget
        | Action::GoForwardFromSemanticTarget
        | Action::JumpListBack(_)
        | Action::JumpListForward(_)
        | Action::VimMotion(_)
        | Action::SetVimMark(_)
        | Action::JumpToVimMark { .. } => Vec::new(),
    }
}

pub(crate) fn apply_event(state: &mut AppState, event: Event) -> Vec<GitEffect> {
    match event {
        Event::ChangesLoaded { request_id, result }
            if state.changes.loading_request() == Some(request_id) =>
        {
            match result {
                Ok(changes) => {
                    let preferred = state.preferred_change.take();
                    let selected = preferred
                        .as_ref()
                        .and_then(|path| changes.iter().position(|change| change.path() == path));
                    state.change_selection.reset_to(changes.len(), selected);
                    state.changes = LoadState::Ready(changes);
                    selected_change_diff(state)
                }
                Err(error) => {
                    state.changes = LoadState::Failed(ErrorNotice::new(error.to_string()));
                    Vec::new()
                }
            }
        }
        Event::CommitsLoaded {
            request_id,
            append,
            limit,
            result,
        } if (!append && state.commits.loading_request() == Some(request_id))
            || (append && state.history_page.loading_more == Some(request_id)) =>
        {
            state.history_page.loading_more = None;
            match result {
                Ok(mut loaded) => {
                    state.history_page.has_more = loaded.len() == limit;
                    if append {
                        if let LoadState::Ready(existing) = &mut state.commits {
                            existing.append(&mut loaded);
                            state.commit_selection.clamp(existing.len());
                        }
                        Vec::new()
                    } else {
                        let preferred = state.preferred_commit.take();
                        let selected = preferred
                            .as_ref()
                            .and_then(|id| loaded.iter().position(|commit| commit.id() == id));
                        state.commit_selection.reset_to(loaded.len(), selected);
                        state.commits = LoadState::Ready(loaded);
                        selected_commit_context(state)
                    }
                }
                Err(error) => {
                    if append {
                        state.notice = Some(ErrorNotice::new(error.to_string()));
                    } else {
                        state.commits = LoadState::Failed(ErrorNotice::new(error.to_string()));
                    }
                    Vec::new()
                }
            }
        }
        Event::FilesLoaded {
            request_id,
            commit,
            result,
        } if state.files.loading_request() == Some(request_id)
            && selected_commit(state).is_some_and(|selected| selected.id() == &commit) =>
        {
            match result {
                Ok(files) => {
                    state.file_selection.reset(files.len());
                    state.files = LoadState::Ready(files);
                    selected_file_diff(state)
                }
                Err(error) => {
                    state.files = LoadState::Failed(ErrorNotice::new(error.to_string()));
                    Vec::new()
                }
            }
        }
        Event::DiffLoaded { request_id, result }
            if state.diff.content.loading_request() == Some(request_id) =>
        {
            match result {
                Ok(document) => {
                    if let Some(target) = state.diff.target.clone() {
                        state.cache_diff(target, document.clone());
                    }
                    state.diff.content = LoadState::Ready(document);
                    state.diff.vertical = state.diff.vertical.min(diff_last_line(state));
                    state.diff.viewport_vertical =
                        state.diff.viewport_vertical.min(state.diff.vertical);
                }
                Err(error) => {
                    state.diff.content = LoadState::Failed(ErrorNotice::new(error.to_string()));
                    state.diff.vertical = 0;
                    state.diff.byte_column = 0;
                    state.diff.desired_display_column = None;
                    state.diff.viewport_vertical = 0;
                }
            }
            Vec::new()
        }
        Event::MessageLoaded {
            request_id,
            commit,
            result,
        } if state.message.content.loading_request() == Some(request_id)
            && state.message.commit.as_ref() == Some(&commit) =>
        {
            state.message.content = match result {
                Ok(message) => LoadState::Ready(message),
                Err(error) => LoadState::Failed(ErrorNotice::new(error.to_string())),
            };
            Vec::new()
        }
        Event::TreeLoaded {
            request_id,
            commit,
            parent,
            result,
        } if state.tree.pending == Some(request_id)
            && state.tree.commit.as_ref() == Some(&commit) =>
        {
            state.tree.pending = None;
            tree_loaded(state, parent, result)
        }
        Event::RepositorySearchLoaded { request_id, result }
            if state.repository_search.results.loading_request() == Some(request_id) =>
        {
            match result {
                Ok(results) => {
                    state.repository_search.selection.reset(results.len());
                    state.repository_search.results = LoadState::Ready(results);
                }
                Err(error) => {
                    state.repository_search.results =
                        LoadState::Failed(ErrorNotice::new(error.to_string()));
                }
            }
            Vec::new()
        }
        Event::FileHistoryLoaded {
            request_id,
            path,
            result,
        } if state.file_view.commits.loading_request() == Some(request_id)
            && state.file_view.path.as_ref() == Some(&path) =>
        {
            match result {
                Ok(commits) => {
                    state.file_view.selection.reset(commits.len());
                    state.file_view.commits = LoadState::Ready(commits);
                }
                Err(error) => {
                    state.file_view.commits =
                        LoadState::Failed(ErrorNotice::new(error.to_string()));
                }
            }
            Vec::new()
        }
        Event::FileContentLoaded {
            request_id,
            path,
            result,
        } if state.file_view.content.loading_request() == Some(request_id)
            && state.file_view.path.as_ref() == Some(&path) =>
        {
            state.file_view.content = match result {
                Ok(document) => LoadState::Ready(document),
                Err(error) => LoadState::Failed(ErrorNotice::new(error.to_string())),
            };
            state.file_view.vertical = state.file_view.vertical.min(file_content_last_line(state));
            Vec::new()
        }
        Event::CodeTreeLoaded { request_id, result }
            if state.code_view.visible.loading_request() == Some(request_id) =>
        {
            crate::app::code_view::tree_loaded(state, result)
        }
        Event::CodeFileLoaded {
            request_id,
            path,
            result,
        } if state.code_view.content.loading_request() == Some(request_id)
            && state.code_view.path.as_ref() == Some(&path) =>
        {
            crate::app::code_view::file_loaded(state, result)
        }
        _ => Vec::new(),
    }
}

fn overlay_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    match state.overlay {
        Overlay::Diff => diff_overlay_action(state, action),
        Overlay::CommitMessage => message_overlay_action(state, action),
        Overlay::RepositorySearch => repository_search_overlay_action(state, action),
        Overlay::FileContent => file_content_overlay_action(state, action),
        Overlay::CodeContent => crate::app::code_view::content_action(state, action),
        Overlay::SemanticTargets => Vec::new(),
        Overlay::LspHover => Vec::new(),
        Overlay::Help => {
            if matches!(action, Action::CloseOverlay | Action::ToggleHelp) {
                state.overlay = Overlay::None;
            }
            Vec::new()
        }
        Overlay::None => Vec::new(),
    }
}

fn message_overlay_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    match action {
        Action::CloseOverlay | Action::ToggleMessage => state.overlay = Overlay::None,
        Action::MoveUp => move_full_message_cursor(state, -1),
        Action::MoveDown => move_full_message_cursor(state, 1),
        Action::MoveTop => state.message.scroll = 0,
        Action::MoveBottom => state.message.scroll = full_message_last_line(state),
        Action::HalfPageUp => move_full_message_cursor(state, -10),
        Action::HalfPageDown => move_full_message_cursor(state, 10),
        _ => {}
    }
    Vec::new()
}

fn diff_overlay_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    match action {
        Action::CloseOverlay | Action::Activate => close_diff_overlay(state),
        Action::MoveUp => {
            move_diff_cursor(state, -1);
        }
        Action::MoveDown => {
            move_diff_cursor(state, 1);
        }
        Action::MoveTop => {
            state.diff.vertical = 0;
        }
        Action::MoveBottom => {
            state.diff.vertical = diff_last_line(state);
        }
        Action::HalfPageUp => {
            move_diff_cursor(state, -10);
        }
        Action::HalfPageDown => {
            move_diff_cursor(state, 10);
        }
        Action::ScrollLeft => {
            state.diff.horizontal = state.diff.horizontal.saturating_sub(4);
        }
        Action::ScrollRight => {
            state.diff.horizontal = state.diff.horizontal.saturating_add(4);
        }
        Action::StartSearch(direction) => state.search.begin(direction),
        Action::InsertSearch(character) => state.search.push(character),
        Action::DeleteSearch => state.search.pop(),
        Action::ConfirmSearch => confirm_diff_search(state),
        Action::CancelSearch => state.search.cancel_input(),
        Action::NextMatch => {
            let direction = state.search.direction();
            if let Some(line) = state.search.select_next(direction) {
                state.diff.vertical = line;
            }
        }
        Action::PreviousMatch => {
            let direction = state.search.direction().reversed();
            if let Some(line) = state.search.select_next(direction) {
                state.diff.vertical = line;
            }
        }
        _ => {}
    }
    Vec::new()
}

fn close_diff_overlay(state: &mut AppState) {
    state.overlay = Overlay::None;
    state.search.clear();
}

fn confirm_diff_search(state: &mut AppState) {
    let anchor = state.diff.vertical;
    let line = match &state.diff.content {
        LoadState::Ready(document) if document.message().is_some() => {
            state.search.confirm(document.message(), anchor)
        }
        LoadState::Ready(document) => state.search.confirm(
            document.lines().iter().map(crate::domain::DiffLine::text),
            anchor,
        ),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => {
            state.search.cancel_input();
            None
        }
    };
    if let Some(line) = line {
        state.diff.vertical = line;
    }
}

fn move_diff_cursor(state: &mut AppState, delta: isize) {
    let next = state.diff.vertical.saturating_add_signed(delta);
    state.diff.vertical = match &state.diff.content {
        LoadState::Loading { .. } => next,
        LoadState::Ready(_) => next.min(diff_last_line(state)),
        LoadState::Idle | LoadState::Failed(_) => 0,
    };
}

fn diff_last_line(state: &AppState) -> usize {
    match &state.diff.content {
        LoadState::Ready(document) if document.message().is_some() => 0,
        LoadState::Ready(document) => document
            .lines()
            .len()
            .saturating_add(usize::from(document.is_truncated()))
            .saturating_sub(1),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => 0,
    }
}

fn move_message_cursor(state: &mut AppState, delta: isize) {
    let last = message_last_line(state);
    state.message.scroll = state
        .message
        .scroll
        .min(last)
        .saturating_add_signed(delta)
        .min(last);
}

fn message_last_line(state: &AppState) -> usize {
    match &state.message.content {
        LoadState::Ready(message) => message.body().lines().count().saturating_sub(1),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => 0,
    }
}

fn move_full_message_cursor(state: &mut AppState, delta: isize) {
    let last = full_message_last_line(state);
    state.message.scroll = state
        .message
        .scroll
        .min(last)
        .saturating_add_signed(delta)
        .min(last);
}

fn full_message_last_line(state: &AppState) -> usize {
    match &state.message.content {
        LoadState::Ready(message) => message.as_str().lines().count().saturating_sub(1),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => 0,
    }
}

fn previous_pane(view: AppView, focus: FocusedPane) -> FocusedPane {
    match (view, focus) {
        (AppView::Changes, FocusedPane::Primary | FocusedPane::Secondary) => FocusedPane::Primary,
        (AppView::Changes, FocusedPane::Diff) => FocusedPane::Primary,
        (AppView::History | AppView::CommitDetails, FocusedPane::Primary) => FocusedPane::Primary,
        (AppView::History | AppView::CommitDetails, FocusedPane::Secondary) => FocusedPane::Primary,
        (AppView::History | AppView::CommitDetails, FocusedPane::Diff) => FocusedPane::Secondary,
        (AppView::Graph, _) => FocusedPane::Primary,
        (AppView::GraphDetails, FocusedPane::Diff) => FocusedPane::Secondary,
        (AppView::GraphDetails, FocusedPane::Primary | FocusedPane::Secondary) => {
            FocusedPane::Secondary
        }
        (AppView::FileHistory, FocusedPane::Diff) => FocusedPane::Primary,
        (AppView::FileHistory, FocusedPane::Primary | FocusedPane::Secondary) => {
            FocusedPane::Primary
        }
        (AppView::Code, FocusedPane::Diff) => FocusedPane::Primary,
        (AppView::Code, FocusedPane::Primary | FocusedPane::Secondary) => FocusedPane::Primary,
    }
}

fn next_pane(view: AppView, focus: FocusedPane) -> FocusedPane {
    match (view, focus) {
        (AppView::Changes, FocusedPane::Primary | FocusedPane::Secondary) => FocusedPane::Diff,
        (AppView::Changes, FocusedPane::Diff) => FocusedPane::Diff,
        (AppView::History | AppView::CommitDetails, FocusedPane::Primary) => FocusedPane::Secondary,
        (AppView::History | AppView::CommitDetails, FocusedPane::Secondary | FocusedPane::Diff) => {
            FocusedPane::Diff
        }
        (AppView::Graph, _) => FocusedPane::Primary,
        (AppView::GraphDetails, _) => FocusedPane::Diff,
        (AppView::FileHistory, _) => FocusedPane::Diff,
        (AppView::Code, _) => FocusedPane::Diff,
    }
}

fn switch_view(state: &mut AppState, view: AppView) -> Vec<GitEffect> {
    state.view = view;
    state.focus = FocusedPane::Primary;
    match view {
        AppView::Changes if matches!(state.changes, LoadState::Idle) => state.request_changes(),
        AppView::History | AppView::CommitDetails | AppView::Graph | AppView::GraphDetails
            if matches!(state.commits, LoadState::Idle) =>
        {
            state.request_commits(false)
        }
        AppView::Code if matches!(state.code_view.visible, LoadState::Idle) => {
            crate::app::code_view::request_tree(state)
        }
        _ => Vec::new(),
    }
}

fn apply_vim_motion(state: &mut AppState, motion: VimMotion) -> Vec<GitEffect> {
    if matches!(
        motion.kind(),
        VimMotionKind::PreviousMarkLine
            | VimMotionKind::PreviousMarkExact
            | VimMotionKind::NextMarkLine
            | VimMotionKind::NextMarkExact
    ) {
        return apply_mark_motion(state, motion);
    }
    let (height, width) = focused_viewport_dimensions(state);
    if is_search_motion(motion.kind())
        && let Some(document) = active_text_document(state)
    {
        apply_document_search_motion(state, document, motion, height, width);
        return Vec::new();
    }
    match state.overlay {
        Overlay::CodeContent => {
            apply_code_vim_motion(state, motion, height, width);
            return Vec::new();
        }
        Overlay::Diff => {
            apply_diff_vim_motion(state, motion, height, width);
            return Vec::new();
        }
        Overlay::FileContent => {
            apply_file_vim_motion(state, motion, height, width);
            return Vec::new();
        }
        Overlay::CommitMessage => {
            apply_message_vim_motion(state, motion, height, width, true);
            return Vec::new();
        }
        Overlay::RepositorySearch if state.repository_search.prompt.is_none() => {
            return apply_list_vim_motion(state, motion, height);
        }
        Overlay::None => {}
        Overlay::Help
        | Overlay::RepositorySearch
        | Overlay::SemanticTargets
        | Overlay::LspHover => return Vec::new(),
    }

    if state.view == AppView::Code && state.focus == FocusedPane::Diff {
        apply_code_vim_motion(state, motion, height, width);
        return Vec::new();
    }
    if state.view == AppView::FileHistory && state.focus == FocusedPane::Diff {
        if state.file_view.showing_history_diff {
            apply_diff_vim_motion(state, motion, height, width);
        } else {
            apply_file_vim_motion(state, motion, height, width);
        }
        return Vec::new();
    }
    if state.view == AppView::CommitDetails && state.focus == FocusedPane::Secondary {
        apply_message_vim_motion(state, motion, height, width, false);
        return Vec::new();
    }
    if state.focus == FocusedPane::Diff
        && matches!(
            state.view,
            AppView::Changes | AppView::History | AppView::GraphDetails
        )
    {
        apply_diff_vim_motion(state, motion, height, width);
        return Vec::new();
    }
    apply_list_vim_motion(state, motion, height)
}

fn apply_mark_motion(state: &mut AppState, motion: VimMotion) -> Vec<GitEffect> {
    let code_active = state.view == AppView::Code
        && (state.focus == FocusedPane::Diff || state.overlay == Overlay::CodeContent);
    let Some(current) = code_active
        .then(|| crate::app::semantic_navigation::current_origin(state))
        .flatten()
    else {
        state.notice = Some(ErrorNotice::new(
            "Vim marks are available in the working-tree Code viewer.",
        ));
        return Vec::new();
    };
    let linewise = matches!(
        motion.kind(),
        VimMotionKind::PreviousMarkLine | VimMotionKind::NextMarkLine
    );
    let forward = matches!(
        motion.kind(),
        VimMotionKind::NextMarkLine | VimMotionKind::NextMarkExact
    );
    let mut candidates = state
        .vim_marks
        .iter()
        .filter(|(mark, origin)| mark.is_ascii_lowercase() && origin.path == current.path)
        .map(|(_, origin)| origin.clone())
        .filter(|origin| {
            let ordering = if linewise {
                origin.cursor.line().cmp(&current.cursor.line())
            } else {
                (origin.cursor.line(), origin.cursor.byte_column())
                    .cmp(&(current.cursor.line(), current.cursor.byte_column()))
            };
            if forward {
                ordering.is_gt()
            } else {
                ordering.is_lt()
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|origin| (origin.cursor.line(), origin.cursor.byte_column()));
    if !forward {
        candidates.reverse();
    }
    let Some(origin) = candidates.get(motion.count().saturating_sub(1)).cloned() else {
        state.notice = Some(ErrorNotice::new("No Vim mark in that direction."));
        return Vec::new();
    };
    crate::app::semantic_navigation::jump_to_mark(state, origin, linewise, true)
        .into_iter()
        .filter_map(|effect| match effect {
            crate::app::AppEffect::Git(effect) => Some(effect),
            crate::app::AppEffect::Lsp(_) => None,
        })
        .collect()
}

#[derive(Clone, Copy)]
enum TextDocument {
    Code,
    Diff,
    File,
    Message { complete: bool },
}

pub(super) fn has_active_search_highlights(state: &AppState) -> bool {
    matches!(
        active_text_document(state),
        Some(TextDocument::Code | TextDocument::Diff)
    ) && state.search.has_highlights()
}

fn active_text_document(state: &AppState) -> Option<TextDocument> {
    match state.overlay {
        Overlay::CodeContent => return Some(TextDocument::Code),
        Overlay::Diff => return Some(TextDocument::Diff),
        Overlay::FileContent => return Some(TextDocument::File),
        Overlay::CommitMessage => return Some(TextDocument::Message { complete: true }),
        Overlay::None => {}
        Overlay::Help
        | Overlay::RepositorySearch
        | Overlay::SemanticTargets
        | Overlay::LspHover => return None,
    }
    match (state.view, state.focus) {
        (AppView::Code, FocusedPane::Diff) => Some(TextDocument::Code),
        (AppView::FileHistory, FocusedPane::Diff) if state.file_view.showing_history_diff => {
            Some(TextDocument::Diff)
        }
        (AppView::FileHistory, FocusedPane::Diff) => Some(TextDocument::File),
        (AppView::CommitDetails, FocusedPane::Secondary) => {
            Some(TextDocument::Message { complete: false })
        }
        (AppView::Changes | AppView::History | AppView::GraphDetails, FocusedPane::Diff) => {
            Some(TextDocument::Diff)
        }
        _ => None,
    }
}

fn document_lines(state: &AppState, document: TextDocument) -> Vec<String> {
    match document {
        TextDocument::Code => match &state.code_view.content {
            LoadState::Ready(content) if content.message().is_some() => {
                content.message().into_iter().map(str::to_owned).collect()
            }
            LoadState::Ready(content) => content.lines().to_vec(),
            _ => Vec::new(),
        },
        TextDocument::Diff => match &state.diff.content {
            LoadState::Ready(content) if content.message().is_some() => {
                content.message().into_iter().map(str::to_owned).collect()
            }
            LoadState::Ready(content) => content
                .lines()
                .iter()
                .map(|line| line.text().to_owned())
                .collect(),
            _ => Vec::new(),
        },
        TextDocument::File => match &state.file_view.content {
            LoadState::Ready(content) if content.message().is_some() => {
                content.message().into_iter().map(str::to_owned).collect()
            }
            LoadState::Ready(content) => content.lines().to_vec(),
            _ => Vec::new(),
        },
        TextDocument::Message { complete } => match &state.message.content {
            LoadState::Ready(message) if complete => {
                message.as_str().lines().map(str::to_owned).collect()
            }
            LoadState::Ready(message) => message.body().lines().map(str::to_owned).collect(),
            _ => Vec::new(),
        },
    }
}

fn document_position(state: &AppState, document: TextDocument) -> SourcePosition {
    match document {
        TextDocument::Code => state.code_view.cursor,
        TextDocument::Diff => SourcePosition::new(
            u32::try_from(state.diff.vertical).unwrap_or(u32::MAX),
            state.diff.byte_column,
        ),
        TextDocument::File => SourcePosition::new(
            u32::try_from(state.file_view.vertical).unwrap_or(u32::MAX),
            state.file_view.byte_column,
        ),
        TextDocument::Message { .. } => SourcePosition::new(
            u32::try_from(state.message.scroll).unwrap_or(u32::MAX),
            state.message.byte_column,
        ),
    }
}

fn set_document_position(
    state: &mut AppState,
    document: TextDocument,
    position: SourcePosition,
    lines: &[&str],
    height: usize,
    width: usize,
) {
    let (top, left, gutter) = match document {
        TextDocument::Code => (
            state.code_view.viewport_vertical,
            state.code_view.viewport_horizontal,
            8,
        ),
        TextDocument::Diff => (state.diff.viewport_vertical, state.diff.horizontal, 14),
        TextDocument::File => (
            state.file_view.viewport_vertical,
            state.file_view.horizontal,
            8,
        ),
        TextDocument::Message { .. } => {
            (state.message.viewport_vertical, state.message.horizontal, 0)
        }
    };
    let desired = match document {
        TextDocument::Code => state.code_view.desired_display_column,
        TextDocument::Diff => state.diff.desired_display_column,
        TextDocument::File => state.file_view.desired_display_column,
        TextDocument::Message { .. } => state.message.desired_display_column,
    };
    let mut viewport = crate::app::vim::Viewport::new(top, left, height, width, gutter)
        .with_desired_column(desired);
    crate::app::vim::reveal(lines, position, &mut viewport);
    let desired = lines
        .get(usize::try_from(position.line()).unwrap_or(usize::MAX))
        .map(|line| crate::lsp::display_column(line, position.byte_column()));
    match document {
        TextDocument::Code => {
            state.code_view.cursor = position;
            state.code_view.desired_display_column = desired;
            state.code_view.viewport_vertical = viewport.top;
            state.code_view.viewport_horizontal = viewport.left;
        }
        TextDocument::Diff => {
            state.diff.vertical = usize::try_from(position.line()).unwrap_or(usize::MAX);
            state.diff.byte_column = position.byte_column();
            state.diff.desired_display_column = desired;
            state.diff.viewport_vertical = viewport.top;
            state.diff.horizontal = viewport.left;
        }
        TextDocument::File => {
            state.file_view.vertical = usize::try_from(position.line()).unwrap_or(usize::MAX);
            state.file_view.byte_column = position.byte_column();
            state.file_view.desired_display_column = desired;
            state.file_view.viewport_vertical = viewport.top;
            state.file_view.horizontal = viewport.left;
        }
        TextDocument::Message { .. } => {
            state.message.scroll = usize::try_from(position.line()).unwrap_or(usize::MAX);
            state.message.byte_column = position.byte_column();
            state.message.desired_display_column = desired;
            state.message.viewport_vertical = viewport.top;
            state.message.horizontal = viewport.left;
        }
    }
}

fn apply_document_search_action(state: &mut AppState, action: Action) -> bool {
    let Some(document) = active_text_document(state) else {
        return false;
    };
    match action {
        Action::StartSearch(direction) => state.search.begin(direction),
        Action::InsertSearch(character) => state.search.push(character),
        Action::DeleteSearch => state.search.pop(),
        Action::CancelSearch => state.search.cancel_input(),
        Action::ConfirmSearch => {
            let owned = document_lines(state, document);
            let lines = owned.iter().map(String::as_str).collect::<Vec<_>>();
            let anchor = document_position(state, document);
            if let Some(position) = state.search.confirm_position(lines.iter().copied(), anchor) {
                let origin = matches!(document, TextDocument::Code)
                    .then(|| crate::app::semantic_navigation::current_origin(state))
                    .flatten();
                let (height, width) = focused_viewport_dimensions(state);
                set_document_position(state, document, position, &lines, height, width);
                if position != anchor
                    && let Some(origin) = origin
                {
                    crate::app::semantic_navigation::remember_jump(state, origin);
                }
            }
        }
        Action::NextMatch | Action::PreviousMatch => {
            let direction = if action == Action::NextMatch {
                state.search.direction()
            } else {
                state.search.direction().reversed()
            };
            let kind = if direction == state.search.direction() {
                VimMotionKind::SearchNext
            } else {
                VimMotionKind::SearchPrevious
            };
            let (height, width) = focused_viewport_dimensions(state);
            apply_document_search_motion(state, document, VimMotion::new(kind), height, width);
        }
        _ => return false,
    }
    true
}

fn is_search_motion(kind: VimMotionKind) -> bool {
    matches!(
        kind,
        VimMotionKind::SearchNext
            | VimMotionKind::SearchPrevious
            | VimMotionKind::SearchWordForward
            | VimMotionKind::SearchWordBackward
            | VimMotionKind::SearchPartialWordForward
            | VimMotionKind::SearchPartialWordBackward
    )
}

fn apply_document_search_motion(
    state: &mut AppState,
    document: TextDocument,
    motion: VimMotion,
    height: usize,
    width: usize,
) {
    let owned = document_lines(state, document);
    let lines = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let anchor = document_position(state, document);
    let position = match motion.kind() {
        VimMotionKind::SearchNext => state.search.repeat_position(
            lines.iter().copied(),
            anchor,
            state.search.direction(),
            motion.count(),
        ),
        VimMotionKind::SearchPrevious => state.search.repeat_position(
            lines.iter().copied(),
            anchor,
            state.search.direction().reversed(),
            motion.count(),
        ),
        VimMotionKind::SearchWordForward
        | VimMotionKind::SearchWordBackward
        | VimMotionKind::SearchPartialWordForward
        | VimMotionKind::SearchPartialWordBackward => {
            let Some((word, column)) = word_at_or_after(&lines, anchor) else {
                return;
            };
            let direction = if matches!(
                motion.kind(),
                VimMotionKind::SearchWordBackward | VimMotionKind::SearchPartialWordBackward
            ) {
                crate::app::SearchDirection::Backward
            } else {
                crate::app::SearchDirection::Forward
            };
            let whole_word = matches!(
                motion.kind(),
                VimMotionKind::SearchWordForward | VimMotionKind::SearchWordBackward
            );
            state.search.search_word(
                lines.iter().copied(),
                &word,
                whole_word,
                SourcePosition::new(anchor.line(), column),
                direction,
                motion.count(),
            )
        }
        _ => None,
    };
    if let Some(position) = position {
        let origin = matches!(document, TextDocument::Code)
            .then(|| crate::app::semantic_navigation::current_origin(state))
            .flatten();
        set_document_position(state, document, position, &lines, height, width);
        if let Some(origin) = origin {
            crate::app::semantic_navigation::remember_jump(state, origin);
        }
    }
}

fn word_at_or_after(lines: &[&str], position: SourcePosition) -> Option<(String, usize)> {
    let line = lines.get(usize::try_from(position.line()).ok()?)?;
    let mut requested = position.byte_column().min(line.len());
    while !line.is_char_boundary(requested) {
        requested = requested.saturating_sub(1);
    }
    if let Some(found) = line
        .char_indices()
        .find(|(column, character)| *column >= requested && is_keyword(*character))
        .map(|(column, _)| column)
    {
        return Some(token_at(line, found, is_keyword));
    }
    let found = line
        .char_indices()
        .find(|(column, character)| *column >= requested && !character.is_whitespace())?
        .0;
    Some(token_at(line, found, |character| {
        !character.is_whitespace()
    }))
}

fn token_at(line: &str, found: usize, belongs: impl Fn(char) -> bool + Copy) -> (String, usize) {
    let mut start = found;
    for (column, character) in line[..found].char_indices().rev() {
        if !belongs(character) {
            break;
        }
        start = column;
    }
    let end = line[start..]
        .char_indices()
        .find(|(_, character)| !belongs(*character))
        .map_or(line.len(), |(offset, _)| start.saturating_add(offset));
    (line[start..end].to_owned(), start)
}

fn is_keyword(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn apply_code_vim_motion(state: &mut AppState, motion: VimMotion, height: usize, width: usize) {
    let origin = is_jump_motion(motion.kind())
        .then(|| crate::app::semantic_navigation::current_origin(state))
        .flatten();
    crate::app::code_view::apply_vim_motion(state, motion, height, width);
    if let Some(origin) = origin {
        crate::app::semantic_navigation::remember_jump(state, origin);
    }
}

fn is_jump_motion(kind: VimMotionKind) -> bool {
    matches!(
        kind,
        VimMotionKind::BufferTop
            | VimMotionKind::BufferBottom
            | VimMotionKind::BufferBottomEnd
            | VimMotionKind::BufferPercentage
            | VimMotionKind::WindowTop
            | VimMotionKind::WindowMiddle
            | VimMotionKind::WindowBottom
            | VimMotionKind::MatchingPair
            | VimMotionKind::MatchingPairBackward
            | VimMotionKind::SentenceBackward
            | VimMotionKind::SentenceForward
            | VimMotionKind::ParagraphBackward
            | VimMotionKind::ParagraphForward
            | VimMotionKind::SectionStartBackward
            | VimMotionKind::SectionStartForward
            | VimMotionKind::SectionEndBackward
            | VimMotionKind::SectionEndForward
            | VimMotionKind::UnmatchedOpenBackward
            | VimMotionKind::UnmatchedCloseForward
            | VimMotionKind::MethodBackward
            | VimMotionKind::MethodForward
            | VimMotionKind::PreprocessorBackward
            | VimMotionKind::PreprocessorForward
            | VimMotionKind::CommentBackward
            | VimMotionKind::CommentForward
    )
}

fn apply_diff_vim_motion(state: &mut AppState, motion: VimMotion, height: usize, width: usize) {
    if matches!(state.diff.content, LoadState::Loading { .. })
        && let Some(delta) = loading_vertical_delta(motion, height)
    {
        state.diff.vertical = state.diff.vertical.saturating_add_signed(delta);
        state.diff.viewport_vertical = state.diff.viewport_vertical.min(state.diff.vertical);
        return;
    }
    const TRUNCATED: &str = "… diff truncated at the safe output limit …";
    let (position, viewport) = {
        let mut lines = match &state.diff.content {
            LoadState::Ready(document) if document.message().is_some() => {
                document.message().into_iter().collect::<Vec<_>>()
            }
            LoadState::Ready(document) => document
                .lines()
                .iter()
                .map(crate::domain::DiffLine::text)
                .collect::<Vec<_>>(),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => Vec::new(),
        };
        if matches!(&state.diff.content, LoadState::Ready(document) if document.is_truncated()) {
            lines.push(TRUNCATED);
        }
        let mut viewport = crate::app::vim::Viewport::new(
            state.diff.viewport_vertical,
            state.diff.horizontal,
            height,
            width,
            14,
        )
        .with_desired_column(state.diff.desired_display_column);
        let position = crate::app::vim::apply(
            &lines,
            SourcePosition::new(
                u32::try_from(state.diff.vertical).unwrap_or(u32::MAX),
                state.diff.byte_column,
            ),
            &mut viewport,
            motion,
        );
        (position, viewport)
    };
    state.diff.vertical = usize::try_from(position.line()).unwrap_or(usize::MAX);
    state.diff.byte_column = position.byte_column();
    state.diff.desired_display_column = viewport.desired_column;
    state.diff.viewport_vertical = viewport.top;
    state.diff.horizontal = viewport.left;
}

fn apply_file_vim_motion(state: &mut AppState, motion: VimMotion, height: usize, width: usize) {
    if matches!(state.file_view.content, LoadState::Loading { .. })
        && let Some(delta) = loading_vertical_delta(motion, height)
    {
        state.file_view.vertical = state.file_view.vertical.saturating_add_signed(delta);
        state.file_view.viewport_vertical = state
            .file_view
            .viewport_vertical
            .min(state.file_view.vertical);
        return;
    }
    const TRUNCATED: &str = "… file truncated at the safe output limit …";
    let (position, viewport) = {
        let mut lines = match &state.file_view.content {
            LoadState::Ready(document) if document.message().is_some() => {
                document.message().into_iter().collect::<Vec<_>>()
            }
            LoadState::Ready(document) => document.lines().iter().map(String::as_str).collect(),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => Vec::new(),
        };
        if matches!(&state.file_view.content, LoadState::Ready(document) if document.is_truncated())
        {
            lines.push(TRUNCATED);
        }
        let mut viewport = crate::app::vim::Viewport::new(
            state.file_view.viewport_vertical,
            state.file_view.horizontal,
            height,
            width,
            8,
        )
        .with_desired_column(state.file_view.desired_display_column);
        let position = crate::app::vim::apply(
            &lines,
            SourcePosition::new(
                u32::try_from(state.file_view.vertical).unwrap_or(u32::MAX),
                state.file_view.byte_column,
            ),
            &mut viewport,
            motion,
        );
        (position, viewport)
    };
    state.file_view.vertical = usize::try_from(position.line()).unwrap_or(usize::MAX);
    state.file_view.byte_column = position.byte_column();
    state.file_view.desired_display_column = viewport.desired_column;
    state.file_view.viewport_vertical = viewport.top;
    state.file_view.horizontal = viewport.left;
}

fn apply_message_vim_motion(
    state: &mut AppState,
    motion: VimMotion,
    height: usize,
    width: usize,
    complete: bool,
) {
    if matches!(state.message.content, LoadState::Loading { .. })
        && let Some(delta) = loading_vertical_delta(motion, height)
    {
        state.message.scroll = state.message.scroll.saturating_add_signed(delta);
        state.message.viewport_vertical = state.message.viewport_vertical.min(state.message.scroll);
        return;
    }
    let (position, viewport) = {
        let lines = match &state.message.content {
            LoadState::Ready(message) if complete => message.as_str().lines().collect::<Vec<_>>(),
            LoadState::Ready(message) => message.body().lines().collect::<Vec<_>>(),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => Vec::new(),
        };
        let mut viewport = crate::app::vim::Viewport::new(
            state.message.viewport_vertical,
            state.message.horizontal,
            height,
            width,
            0,
        )
        .with_desired_column(state.message.desired_display_column);
        let position = crate::app::vim::apply(
            &lines,
            SourcePosition::new(
                u32::try_from(state.message.scroll).unwrap_or(u32::MAX),
                state.message.byte_column,
            ),
            &mut viewport,
            motion,
        );
        (position, viewport)
    };
    state.message.scroll = usize::try_from(position.line()).unwrap_or(usize::MAX);
    state.message.byte_column = position.byte_column();
    state.message.desired_display_column = viewport.desired_column;
    state.message.viewport_vertical = viewport.top;
    state.message.horizontal = viewport.left;
}

fn apply_list_vim_motion(
    state: &mut AppState,
    motion: VimMotion,
    viewport_height: usize,
) -> Vec<GitEffect> {
    if matches!(
        motion.kind(),
        VimMotionKind::Left | VimMotionKind::LeftWrap | VimMotionKind::ScreenLineStart
    ) {
        state.focus = previous_pane(state.view, state.focus);
        return Vec::new();
    }
    if matches!(
        motion.kind(),
        VimMotionKind::Right
            | VimMotionKind::RightWrap
            | VimMotionKind::ScreenLineEnd
            | VimMotionKind::ScreenLastNonBlank
    ) {
        state.focus = next_pane(state.view, state.focus);
        return Vec::new();
    }

    let count = motion.count().max(1);
    let relative = match motion.kind() {
        VimMotionKind::Up
        | VimMotionKind::PreviousLineFirstNonBlank
        | VimMotionKind::WordBackward
        | VimMotionKind::BigWordBackward
        | VimMotionKind::WordEndBackward
        | VimMotionKind::BigWordEndBackward
        | VimMotionKind::SentenceBackward
        | VimMotionKind::ParagraphBackward
        | VimMotionKind::ScrollLineUp => Some(-(count_as_isize(count))),
        VimMotionKind::Down
        | VimMotionKind::NextLineFirstNonBlank
        | VimMotionKind::WordForward
        | VimMotionKind::BigWordForward
        | VimMotionKind::WordEndForward
        | VimMotionKind::BigWordEndForward
        | VimMotionKind::SentenceForward
        | VimMotionKind::ParagraphForward
        | VimMotionKind::ScrollLineDown => Some(count_as_isize(count)),
        VimMotionKind::CountedLineFirstNonBlank => Some(count_as_isize(count.saturating_sub(1))),
        VimMotionKind::HalfPageUp => {
            Some(-(count_as_isize(half_page_distance(motion, viewport_height))))
        }
        VimMotionKind::HalfPageDown => {
            Some(count_as_isize(half_page_distance(motion, viewport_height)))
        }
        VimMotionKind::PageUp => Some(
            -(count_as_isize(
                viewport_height
                    .saturating_sub(2)
                    .max(1)
                    .saturating_mul(count),
            )),
        ),
        VimMotionKind::PageDown => Some(count_as_isize(
            viewport_height
                .saturating_sub(2)
                .max(1)
                .saturating_mul(count),
        )),
        _ => None,
    };
    if let Some(delta) = relative {
        return move_active_selection(state, delta);
    }

    let Some((current, len)) = active_list_position(state) else {
        return Vec::new();
    };
    let last = len.saturating_sub(1);
    let top = current.saturating_sub(viewport_height.saturating_sub(1));
    let target = match motion.kind() {
        VimMotionKind::LineStart | VimMotionKind::FirstNonBlank => 0,
        VimMotionKind::LineEnd | VimMotionKind::LastNonBlank => last,
        VimMotionKind::BufferTop => {
            if motion.has_explicit_count() {
                count.saturating_sub(1).min(last)
            } else {
                0
            }
        }
        VimMotionKind::BufferBottom | VimMotionKind::BufferBottomEnd => last,
        VimMotionKind::BufferPercentage => count
            .min(100)
            .saturating_mul(len)
            .saturating_add(99)
            .saturating_div(100)
            .saturating_sub(1)
            .min(last),
        VimMotionKind::WindowTop => top.saturating_add(count.saturating_sub(1)).min(last),
        VimMotionKind::WindowMiddle => top
            .saturating_add(viewport_height.saturating_sub(1) / 2)
            .min(last),
        VimMotionKind::WindowBottom => top
            .saturating_add(viewport_height.saturating_sub(count))
            .min(last),
        _ => return Vec::new(),
    };
    move_active_selection(
        state,
        count_as_isize(target).saturating_sub(count_as_isize(current)),
    )
}

fn move_active_selection(state: &mut AppState, delta: isize) -> Vec<GitEffect> {
    if state.overlay == Overlay::RepositorySearch {
        if let LoadState::Ready(items) = &state.repository_search.results {
            state
                .repository_search
                .selection
                .move_by(delta, items.len());
        }
        Vec::new()
    } else {
        move_selection(state, delta)
    }
}

fn active_list_position(state: &AppState) -> Option<(usize, usize)> {
    match (state.overlay, state.view, state.focus, state.history_panel) {
        (Overlay::RepositorySearch, _, _, _) => match &state.repository_search.results {
            LoadState::Ready(items) => {
                Some((state.repository_search.selection.index()?, items.len()))
            }
            _ => None,
        },
        (Overlay::None, AppView::Code, FocusedPane::Primary | FocusedPane::Secondary, _) => {
            match &state.code_view.visible {
                LoadState::Ready(items) => Some((state.code_view.selection.index()?, items.len())),
                _ => None,
            }
        }
        (Overlay::None, AppView::Changes, FocusedPane::Primary, _) => match &state.changes {
            LoadState::Ready(items) => Some((state.change_selection.index()?, items.len())),
            _ => None,
        },
        (
            Overlay::None,
            AppView::History | AppView::CommitDetails | AppView::Graph,
            FocusedPane::Primary,
            _,
        ) => match &state.commits {
            LoadState::Ready(items) => Some((state.commit_selection.index()?, items.len())),
            _ => None,
        },
        (Overlay::None, AppView::History, FocusedPane::Secondary, HistoryPanel::ChangedFiles)
        | (Overlay::None, AppView::CommitDetails, FocusedPane::Diff, _)
        | (Overlay::None, AppView::GraphDetails, FocusedPane::Secondary, _) => match &state.files {
            LoadState::Ready(items) => Some((state.file_selection.index()?, items.len())),
            _ => None,
        },
        (Overlay::None, AppView::History, FocusedPane::Secondary, HistoryPanel::Tree) => {
            match &state.tree.visible {
                LoadState::Ready(items) => Some((state.tree.selection.index()?, items.len())),
                _ => None,
            }
        }
        (Overlay::None, AppView::FileHistory, FocusedPane::Primary, _) => {
            match &state.file_view.commits {
                LoadState::Ready(items) => Some((state.file_view.selection.index()?, items.len())),
                _ => None,
            }
        }
        _ => None,
    }
}

fn focused_viewport_dimensions(state: &AppState) -> (usize, usize) {
    let terminal_height = usize::from(state.terminal_height);
    let terminal_width = usize::from(state.terminal_width);
    let main_height = terminal_height.saturating_sub(1);
    let content = |height: usize, width: usize| {
        (
            height.saturating_sub(2).max(1),
            width.saturating_sub(2).max(1),
        )
    };
    match state.overlay {
        Overlay::CodeContent | Overlay::Diff | Overlay::FileContent => {
            return content(
                terminal_height.saturating_sub(3),
                terminal_width.saturating_sub(2),
            );
        }
        Overlay::CommitMessage => {
            return content(
                percent(terminal_height, 78).saturating_sub(1),
                percent(terminal_width, 82),
            );
        }
        Overlay::RepositorySearch if state.repository_search.prompt.is_none() => {
            return content(
                percent(terminal_height, 82).saturating_sub(3),
                percent(terminal_width, 86),
            );
        }
        _ => {}
    }
    match (state.view, state.focus) {
        (AppView::Changes, FocusedPane::Diff) if terminal_width >= 110 => {
            content(main_height, percent(terminal_width, 68))
        }
        (AppView::Changes, _) => content(main_height, terminal_width),
        (AppView::History, FocusedPane::Primary) => {
            content(percent(main_height, 25), terminal_width)
        }
        (AppView::History, FocusedPane::Secondary) => {
            content(percent(main_height, 25), terminal_width)
        }
        (AppView::History, FocusedPane::Diff) => content(percent(main_height, 50), terminal_width),
        (AppView::CommitDetails, FocusedPane::Primary) => {
            content(percent(main_height, 25), terminal_width)
        }
        (AppView::CommitDetails, FocusedPane::Secondary) => {
            content(percent(main_height, 45), terminal_width)
        }
        (AppView::CommitDetails, FocusedPane::Diff) => {
            content(percent(main_height, 30), terminal_width)
        }
        (AppView::Graph, _) => content(main_height, terminal_width),
        (AppView::GraphDetails, FocusedPane::Secondary) => content(
            percent(percent(main_height, 88), 38),
            percent(terminal_width, 90),
        ),
        (AppView::GraphDetails, FocusedPane::Diff) => content(
            percent(percent(main_height, 88), 62),
            percent(terminal_width, 90),
        ),
        (AppView::FileHistory, FocusedPane::Primary) => {
            content(percent(main_height, 38), terminal_width)
        }
        (AppView::FileHistory, FocusedPane::Diff) => {
            content(percent(main_height, 62), terminal_width)
        }
        (AppView::Code, FocusedPane::Primary | FocusedPane::Secondary) => {
            content(percent(main_height, 42), terminal_width)
        }
        (AppView::Code, FocusedPane::Diff) => content(percent(main_height, 58), terminal_width),
        _ => content(main_height, terminal_width),
    }
}

fn percent(value: usize, percentage: usize) -> usize {
    value.saturating_mul(percentage).saturating_div(100)
}

fn count_as_isize(count: usize) -> isize {
    isize::try_from(count).unwrap_or(isize::MAX)
}

fn half_page_distance(motion: VimMotion, viewport_height: usize) -> usize {
    if motion.has_explicit_count() {
        motion.count()
    } else {
        viewport_height.saturating_div(2).max(1)
    }
}

fn loading_vertical_delta(motion: VimMotion, viewport_height: usize) -> Option<isize> {
    let count = motion.count().max(1);
    match motion.kind() {
        VimMotionKind::Up => Some(-count_as_isize(count)),
        VimMotionKind::Down => Some(count_as_isize(count)),
        VimMotionKind::HalfPageUp => {
            Some(-count_as_isize(half_page_distance(motion, viewport_height)))
        }
        VimMotionKind::HalfPageDown => {
            Some(count_as_isize(half_page_distance(motion, viewport_height)))
        }
        VimMotionKind::PageUp => Some(-count_as_isize(
            viewport_height
                .saturating_sub(2)
                .max(1)
                .saturating_mul(count),
        )),
        VimMotionKind::PageDown => Some(count_as_isize(
            viewport_height
                .saturating_sub(2)
                .max(1)
                .saturating_mul(count),
        )),
        _ => None,
    }
}

fn move_selection(state: &mut AppState, delta: isize) -> Vec<GitEffect> {
    if state.view == AppView::Code {
        return if state.focus == FocusedPane::Diff {
            crate::app::code_view::move_content_cursor(state, delta);
            Vec::new()
        } else {
            crate::app::code_view::move_selection(state, delta)
        };
    }
    if state.view == AppView::FileHistory && state.focus == FocusedPane::Diff {
        if state.file_view.showing_history_diff {
            move_diff_cursor(state, delta);
        } else {
            move_file_content_cursor(state, delta);
        }
        return Vec::new();
    }
    if state.view != AppView::CommitDetails && state.focus == FocusedPane::Diff {
        move_diff_cursor(state, delta);
        return Vec::new();
    }
    match (state.view, state.focus, state.history_panel) {
        (AppView::Changes, FocusedPane::Primary, _) => {
            let changed = match &state.changes {
                LoadState::Ready(items) => state.change_selection.move_by(delta, items.len()),
                _ => false,
            };
            if changed {
                selected_change_diff(state)
            } else {
                Vec::new()
            }
        }
        (AppView::History | AppView::CommitDetails | AppView::Graph, FocusedPane::Primary, _) => {
            let changed = match &state.commits {
                LoadState::Ready(items) => state.commit_selection.move_by(delta, items.len()),
                _ => false,
            };
            if changed {
                let mut effects = selected_commit_context(state);
                effects.extend(maybe_load_more(state));
                effects
            } else {
                maybe_load_more(state)
            }
        }
        (AppView::History, FocusedPane::Secondary, HistoryPanel::ChangedFiles) => {
            let changed = match &state.files {
                LoadState::Ready(items) => state.file_selection.move_by(delta, items.len()),
                _ => false,
            };
            if changed {
                selected_file_diff(state)
            } else {
                Vec::new()
            }
        }
        (AppView::History, FocusedPane::Secondary, HistoryPanel::Tree) => {
            if let LoadState::Ready(items) = &state.tree.visible {
                state.tree.selection.move_by(delta, items.len());
            }
            Vec::new()
        }
        (AppView::CommitDetails, FocusedPane::Secondary, _) => {
            move_message_cursor(state, delta);
            Vec::new()
        }
        (AppView::CommitDetails, FocusedPane::Diff, _) => {
            let changed = match &state.files {
                LoadState::Ready(items) => state.file_selection.move_by(delta, items.len()),
                _ => false,
            };
            if changed {
                selected_file_diff(state)
            } else {
                Vec::new()
            }
        }
        (AppView::GraphDetails, FocusedPane::Secondary, _) => {
            let changed = match &state.files {
                LoadState::Ready(items) => state.file_selection.move_by(delta, items.len()),
                _ => false,
            };
            if changed {
                selected_file_diff(state)
            } else {
                Vec::new()
            }
        }
        (AppView::GraphDetails, FocusedPane::Diff, _) => {
            move_diff_cursor(state, delta);
            Vec::new()
        }
        (AppView::FileHistory, FocusedPane::Primary, _) => {
            let changed = match &state.file_view.commits {
                LoadState::Ready(items) => state.file_view.selection.move_by(delta, items.len()),
                _ => false,
            };
            if changed {
                selected_file_history_diff(state)
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn move_to_edge(state: &mut AppState, bottom: bool) -> Vec<GitEffect> {
    if state.view == AppView::Code {
        return if state.focus == FocusedPane::Diff {
            let target = if bottom {
                crate::app::code_view::last_line(state)
            } else {
                0
            };
            let current = usize::try_from(state.code_view.cursor.line()).unwrap_or(usize::MAX);
            crate::app::code_view::move_content_cursor(
                state,
                isize::try_from(target)
                    .unwrap_or(isize::MAX)
                    .saturating_sub(isize::try_from(current).unwrap_or(isize::MAX)),
            );
            Vec::new()
        } else {
            crate::app::code_view::move_to_edge(state, bottom)
        };
    }
    if state.view == AppView::FileHistory && state.focus == FocusedPane::Diff {
        if state.file_view.showing_history_diff {
            state.diff.vertical = if bottom { diff_last_line(state) } else { 0 };
        } else {
            state.file_view.vertical = if bottom {
                file_content_last_line(state)
            } else {
                0
            };
        }
        return Vec::new();
    }
    if state.view != AppView::CommitDetails && state.focus == FocusedPane::Diff {
        state.diff.vertical = if bottom { diff_last_line(state) } else { 0 };
        return Vec::new();
    }
    if state.view == AppView::CommitDetails && state.focus == FocusedPane::Secondary {
        state.message.scroll = if bottom { message_last_line(state) } else { 0 };
        return Vec::new();
    }
    let moved = match (state.view, state.focus, state.history_panel) {
        (AppView::Changes, FocusedPane::Primary, _) => match &state.changes {
            LoadState::Ready(items) => edge(&mut state.change_selection, items.len(), bottom),
            _ => false,
        },
        (AppView::History | AppView::CommitDetails | AppView::Graph, FocusedPane::Primary, _) => {
            match &state.commits {
                LoadState::Ready(items) => edge(&mut state.commit_selection, items.len(), bottom),
                _ => false,
            }
        }
        (AppView::History, FocusedPane::Secondary, HistoryPanel::ChangedFiles) => {
            match &state.files {
                LoadState::Ready(items) => edge(&mut state.file_selection, items.len(), bottom),
                _ => false,
            }
        }
        (AppView::History, FocusedPane::Secondary, HistoryPanel::Tree) => {
            match &state.tree.visible {
                LoadState::Ready(items) => edge(&mut state.tree.selection, items.len(), bottom),
                _ => false,
            }
        }
        (AppView::CommitDetails, FocusedPane::Diff, _) => match &state.files {
            LoadState::Ready(items) => edge(&mut state.file_selection, items.len(), bottom),
            _ => false,
        },
        (AppView::GraphDetails, FocusedPane::Secondary, _) => match &state.files {
            LoadState::Ready(items) => edge(&mut state.file_selection, items.len(), bottom),
            _ => false,
        },
        (AppView::FileHistory, FocusedPane::Primary, _) => match &state.file_view.commits {
            LoadState::Ready(items) => edge(&mut state.file_view.selection, items.len(), bottom),
            _ => false,
        },
        _ => false,
    };
    if !moved {
        return maybe_load_more(state);
    }
    match (state.view, state.focus, state.history_panel) {
        (AppView::Changes, _, _) => selected_change_diff(state),
        (AppView::History | AppView::CommitDetails | AppView::Graph, FocusedPane::Primary, _) => {
            let mut effects = selected_commit_context(state);
            effects.extend(maybe_load_more(state));
            effects
        }
        (AppView::History, _, HistoryPanel::ChangedFiles) => selected_file_diff(state),
        (AppView::CommitDetails, FocusedPane::Diff, _) => selected_file_diff(state),
        (AppView::GraphDetails, FocusedPane::Secondary, _) => selected_file_diff(state),
        (AppView::FileHistory, FocusedPane::Primary, _) => selected_file_history_diff(state),
        _ => Vec::new(),
    }
}

fn edge(selection: &mut crate::app::model::Selection, len: usize, bottom: bool) -> bool {
    if bottom {
        selection.bottom(len)
    } else {
        selection.top(len)
    }
}

fn move_half_page(state: &mut AppState, delta: isize) -> Vec<GitEffect> {
    if state.view == AppView::Code && state.focus == FocusedPane::Diff {
        crate::app::code_view::move_content_cursor(state, delta);
        Vec::new()
    } else if state.view == AppView::CommitDetails && state.focus == FocusedPane::Secondary {
        move_message_cursor(state, delta);
        Vec::new()
    } else if state.view == AppView::FileHistory && state.focus == FocusedPane::Diff {
        if state.file_view.showing_history_diff {
            move_diff_cursor(state, delta);
        } else {
            move_file_content_cursor(state, delta);
        }
        Vec::new()
    } else if state.view != AppView::CommitDetails && state.focus == FocusedPane::Diff {
        move_diff_cursor(state, delta);
        Vec::new()
    } else {
        move_selection(state, delta)
    }
}

fn refresh(state: &mut AppState) -> Vec<GitEffect> {
    match state.view {
        AppView::Changes => {
            if let (LoadState::Ready(changes), Some(index)) =
                (&state.changes, state.change_selection.index())
                && let Some(change) = changes.get(index)
            {
                state.preferred_change = Some(change.path().clone());
            }
        }
        AppView::History | AppView::CommitDetails | AppView::Graph | AppView::GraphDetails => {
            if let Some(commit) = selected_commit(state) {
                state.preferred_commit = Some(commit.id().clone());
            }
        }
        AppView::FileHistory => {
            state.clear_cache();
            return state
                .file_view
                .path
                .clone()
                .map(|path| load_file_view(state, path))
                .unwrap_or_default();
        }
        AppView::Code => return crate::app::code_view::request_tree(state),
    }
    state.clear_cache();
    state.diff = crate::app::model::DiffViewState {
        target: None,
        content: LoadState::Idle,
        vertical: 0,
        byte_column: 0,
        desired_display_column: None,
        viewport_vertical: 0,
        horizontal: 0,
    };
    match state.view {
        AppView::Changes => state.request_changes(),
        AppView::History | AppView::CommitDetails | AppView::Graph | AppView::GraphDetails => {
            state.request_commits(false)
        }
        AppView::FileHistory => Vec::new(),
        AppView::Code => crate::app::code_view::request_tree(state),
    }
}

fn toggle_details(state: &mut AppState) -> Vec<GitEffect> {
    if state.view == AppView::CommitDetails {
        state.view = AppView::History;
        state.focus = FocusedPane::Primary;
        return if state.history_panel == HistoryPanel::Tree {
            ensure_tree(state)
        } else {
            Vec::new()
        };
    }
    if state.view != AppView::History {
        return Vec::new();
    }
    let Some(commit) = selected_commit(state).map(|summary| summary.id().clone()) else {
        return Vec::new();
    };
    state.view = AppView::CommitDetails;
    state.focus = FocusedPane::Primary;
    state.message.scroll = 0;
    state.message.byte_column = 0;
    state.message.desired_display_column = None;
    state.message.viewport_vertical = 0;
    state.message.horizontal = 0;
    request_message(state, commit)
}

fn toggle_message(state: &mut AppState) -> Vec<GitEffect> {
    if !matches!(
        state.view,
        AppView::History | AppView::CommitDetails | AppView::Graph | AppView::GraphDetails
    ) {
        return Vec::new();
    }
    let Some(commit) = selected_commit(state).map(|summary| summary.id().clone()) else {
        return Vec::new();
    };
    state.message.scroll = 0;
    state.message.byte_column = 0;
    state.message.desired_display_column = None;
    state.message.viewport_vertical = 0;
    state.message.horizontal = 0;
    state.overlay = Overlay::CommitMessage;
    request_message(state, commit)
}

fn request_message(state: &mut AppState, commit: crate::domain::ObjectId) -> Vec<GitEffect> {
    if state.message.commit.as_ref() == Some(&commit)
        && matches!(state.message.content, LoadState::Ready(_))
    {
        return Vec::new();
    }
    if state.message.commit.as_ref() != Some(&commit) {
        state.message.scroll = 0;
        state.message.byte_column = 0;
        state.message.desired_display_column = None;
        state.message.viewport_vertical = 0;
        state.message.horizontal = 0;
    }
    let request_id = state.request_id();
    state.message.commit = Some(commit.clone());
    state.message.content = LoadState::Loading { request_id };
    vec![GitEffect::LoadMessage { request_id, commit }]
}

fn toggle_tree(state: &mut AppState) -> Vec<GitEffect> {
    if state.view != AppView::History {
        return Vec::new();
    }
    state.history_panel = match state.history_panel {
        HistoryPanel::ChangedFiles => HistoryPanel::Tree,
        HistoryPanel::Tree => HistoryPanel::ChangedFiles,
    };
    if state.history_panel == HistoryPanel::Tree {
        ensure_tree(state)
    } else {
        Vec::new()
    }
}

fn activate(state: &mut AppState) -> Vec<GitEffect> {
    if state.view == AppView::Code {
        return if state.focus == FocusedPane::Diff {
            crate::app::code_view::open_content(state);
            Vec::new()
        } else {
            crate::app::code_view::activate_tree(state)
        };
    }
    match (state.view, state.focus, state.history_panel) {
        (AppView::History, FocusedPane::Primary, _) => {
            state.history_panel = HistoryPanel::ChangedFiles;
            state.focus = FocusedPane::Secondary;
            Vec::new()
        }
        (AppView::CommitDetails, FocusedPane::Primary, _) => {
            state.focus = FocusedPane::Diff;
            Vec::new()
        }
        (AppView::Graph, FocusedPane::Primary, _) => {
            state.view = AppView::GraphDetails;
            state.focus = FocusedPane::Secondary;
            selected_commit_context(state)
        }
        (AppView::History, FocusedPane::Secondary, HistoryPanel::Tree) => activate_tree(state),
        (AppView::Changes, FocusedPane::Primary | FocusedPane::Diff, _)
        | (AppView::History, FocusedPane::Secondary | FocusedPane::Diff, _) => {
            open_diff_overlay(state);
            Vec::new()
        }
        (AppView::CommitDetails, FocusedPane::Diff, _) => {
            open_diff_overlay(state);
            Vec::new()
        }
        (AppView::GraphDetails, FocusedPane::Secondary | FocusedPane::Diff, _) => {
            open_diff_overlay(state);
            Vec::new()
        }
        (AppView::FileHistory, _, _) if state.file_view.showing_history_diff => {
            open_diff_overlay(state);
            Vec::new()
        }
        (AppView::FileHistory, _, _) => {
            if state.file_view.path.is_some() {
                state.overlay = Overlay::FileContent;
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn open_diff_overlay(state: &mut AppState) {
    if state.diff.target.is_some() {
        state.search.clear();
        state.overlay = Overlay::Diff;
    }
}

fn selected_change_diff(state: &mut AppState) -> Vec<GitEffect> {
    let target = match (&state.changes, state.change_selection.index()) {
        (LoadState::Ready(changes), Some(index)) => {
            changes.get(index).map(|change| DiffTarget::Worktree {
                path: change.path().clone(),
                untracked: change.kind() == crate::domain::ChangeKind::Untracked,
            })
        }
        _ => None,
    };
    target
        .map(|target| state.request_diff(target))
        .unwrap_or_default()
}

fn selected_commit(state: &AppState) -> Option<&CommitSummary> {
    match (&state.commits, state.commit_selection.index()) {
        (LoadState::Ready(commits), Some(index)) => commits.get(index),
        _ => None,
    }
}

fn selected_commit_context(state: &mut AppState) -> Vec<GitEffect> {
    let Some(commit) = selected_commit(state).cloned() else {
        state.files = LoadState::Ready(Vec::new());
        state.diff.content = LoadState::Idle;
        return Vec::new();
    };
    let request_id = state.request_id();
    state.files = LoadState::Loading { request_id };
    state.file_selection.reset(0);
    state.diff.target = None;
    state.diff.content = LoadState::Idle;
    let mut effects = vec![GitEffect::LoadFiles {
        request_id,
        commit: commit.id().clone(),
        baseline: commit.baseline(),
    }];
    if state.view == AppView::History && state.history_panel == HistoryPanel::Tree {
        effects.extend(reset_and_load_tree(state, &commit));
    }
    if matches!(state.view, AppView::CommitDetails | AppView::GraphDetails) {
        effects.extend(request_message(state, commit.id().clone()));
    }
    effects
}

fn selected_file_diff(state: &mut AppState) -> Vec<GitEffect> {
    let path = match (&state.files, state.file_selection.index()) {
        (LoadState::Ready(files), Some(index)) => files.get(index).map(|file| file.path().clone()),
        _ => None,
    };
    let commit = selected_commit(state).cloned();
    match (commit, path) {
        (Some(commit), Some(path)) => state.request_diff(DiffTarget::Commit {
            commit: commit.id().clone(),
            baseline: commit.baseline(),
            path,
        }),
        _ => Vec::new(),
    }
}

fn ensure_tree(state: &mut AppState) -> Vec<GitEffect> {
    let Some(commit) = selected_commit(state).cloned() else {
        return Vec::new();
    };
    if state.tree.commit.as_ref() == Some(commit.id())
        && !matches!(state.tree.visible, LoadState::Idle)
    {
        Vec::new()
    } else {
        reset_and_load_tree(state, &commit)
    }
}

fn reset_and_load_tree(state: &mut AppState, commit: &CommitSummary) -> Vec<GitEffect> {
    let request_id = state.request_id();
    state.tree.commit = Some(commit.id().clone());
    state.tree.visible = LoadState::Loading { request_id };
    state.tree.selection.reset(0);
    state.tree.children.clear();
    state.tree.pending = Some(request_id);
    vec![GitEffect::LoadTree {
        request_id,
        commit: commit.id().clone(),
        treeish: commit.id().clone(),
        parent: None,
    }]
}

fn activate_tree(state: &mut AppState) -> Vec<GitEffect> {
    if state.view != AppView::History
        || state.history_panel != HistoryPanel::Tree
        || state.focus != FocusedPane::Secondary
    {
        return Vec::new();
    }
    let Some(index) = state.tree.selection.index() else {
        return Vec::new();
    };
    let selected = match &state.tree.visible {
        LoadState::Ready(visible) => visible.get(index).cloned(),
        _ => None,
    };
    let Some(selected) = selected else {
        return Vec::new();
    };
    if selected.entry().kind() != TreeKind::Directory {
        let Some(commit) = selected_commit(state).cloned() else {
            return Vec::new();
        };
        let effects = state.request_diff(DiffTarget::Commit {
            commit: commit.id().clone(),
            baseline: commit.baseline(),
            path: selected.path().clone(),
        });
        open_diff_overlay(state);
        return effects;
    }
    if selected.expanded() {
        collapse_tree(state, index);
        return Vec::new();
    }
    if let Some(children) = state
        .tree
        .children
        .get(selected.entry().object_id())
        .cloned()
    {
        insert_children(state, index, &selected, children);
        return Vec::new();
    }
    let Some(commit) = selected_commit(state).map(|summary| summary.id().clone()) else {
        return Vec::new();
    };
    let request_id = state.request_id();
    state.tree.pending = Some(request_id);
    vec![GitEffect::LoadTree {
        request_id,
        commit,
        treeish: selected.entry().object_id().clone(),
        parent: Some(selected),
    }]
}

fn collapse_tree(state: &mut AppState, index: usize) {
    let LoadState::Ready(visible) = &mut state.tree.visible else {
        return;
    };
    let Some(depth) = visible.get(index).map(VisibleTreeEntry::depth) else {
        return;
    };
    if let Some(parent) = visible.get_mut(index) {
        parent.set_expanded(false);
    }
    let end = visible[index + 1..]
        .iter()
        .position(|entry| entry.depth() <= depth)
        .map_or(visible.len(), |offset| index + 1 + offset);
    visible.drain(index + 1..end);
    state.tree.selection.clamp(visible.len());
}

fn insert_children(
    state: &mut AppState,
    index: usize,
    parent: &VisibleTreeEntry,
    children: Vec<crate::domain::TreeEntry>,
) {
    let LoadState::Ready(visible) = &mut state.tree.visible else {
        return;
    };
    if let Some(parent_entry) = visible.get_mut(index) {
        parent_entry.set_expanded(true);
    }
    let additions = children.into_iter().map(|entry| {
        let path = parent.path().join(entry.name());
        VisibleTreeEntry::new(entry, path, parent.depth().saturating_add(1))
    });
    visible.splice(index + 1..index + 1, additions);
}

fn tree_loaded(
    state: &mut AppState,
    parent: Option<VisibleTreeEntry>,
    result: Result<Vec<crate::domain::TreeEntry>, crate::git::GitError>,
) -> Vec<GitEffect> {
    match (parent, result) {
        (None, Ok(entries)) => {
            let visible = entries
                .into_iter()
                .map(|entry| {
                    let path = RepoPath::root_marker().join(entry.name());
                    VisibleTreeEntry::new(entry, path, 0)
                })
                .collect::<Vec<_>>();
            state.tree.selection.reset(visible.len());
            state.tree.visible = LoadState::Ready(visible);
        }
        (Some(parent), Ok(entries)) => {
            state
                .tree
                .children
                .insert(parent.entry().object_id().clone(), entries.clone());
            let index = match &state.tree.visible {
                LoadState::Ready(visible) => visible.iter().position(|entry| {
                    entry.path() == parent.path()
                        && entry.entry().object_id() == parent.entry().object_id()
                }),
                _ => None,
            };
            if let Some(index) = index {
                insert_children(state, index, &parent, entries);
            }
        }
        (_, Err(error)) => {
            if matches!(state.tree.visible, LoadState::Loading { .. }) {
                state.tree.visible = LoadState::Failed(ErrorNotice::new(error.to_string()));
            } else {
                state.notice = Some(ErrorNotice::new(error.to_string()));
            }
        }
    }
    Vec::new()
}

fn maybe_load_more(state: &mut AppState) -> Vec<GitEffect> {
    if !matches!(
        state.view,
        AppView::History | AppView::CommitDetails | AppView::Graph | AppView::GraphDetails
    ) || state.focus != FocusedPane::Primary
        || !state.history_page.has_more
        || state.history_page.loading_more.is_some()
    {
        return Vec::new();
    }
    let at_end = match &state.commits {
        LoadState::Ready(commits) => state.commit_selection.index() == commits.len().checked_sub(1),
        _ => false,
    };
    if at_end {
        state.request_commits(true)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{apply_action, apply_event};
    use crate::app::{
        Action, AppState, AppView, Event, FocusedPane, GitEffect, HistoryPanel, LoadState, Overlay,
        RepositorySearchKind, SearchDirection, VimMotion, VimMotionKind,
    };
    use crate::domain::{
        ChangeKind, ChangedFile, CommitMessage, CommitSummary, DiffDocument, DiffLine,
        DiffLineKind, DiffTarget, FileDocument, ObjectId, RepoPath, RepositoryRoot, SearchHit,
        SourcePosition, TreeEntry, TreeKind, WorktreeChange,
    };

    fn state() -> AppState {
        let root = RepositoryRoot::new(PathBuf::from("/tmp/repo"))
            .unwrap_or_else(|error| panic!("{error}"));
        AppState::new(root, AppView::Changes)
    }

    #[test]
    fn stale_change_response_is_ignored() {
        let mut state = state();
        let first = state.start();
        let request_id = match &first[0] {
            crate::app::GitEffect::LoadChanges { request_id } => *request_id,
            _ => panic!("expected change request"),
        };
        let _newer = apply_action(&mut state, Action::Refresh);
        let _effects = apply_event(
            &mut state,
            Event::ChangesLoaded {
                request_id,
                result: Ok(vec![WorktreeChange::new(
                    RepoPath::from_bytes(b"old".to_vec()).unwrap_or_else(|error| panic!("{error}")),
                    None,
                    ChangeKind::Modified,
                )]),
            },
        );
        assert!(matches!(state.changes, LoadState::Loading { .. }));
    }

    #[test]
    fn diff_response_for_current_request_is_applied() {
        let mut state = state();
        let target = crate::domain::DiffTarget::Worktree {
            path: RepoPath::from_bytes(b"file".to_vec()).unwrap_or_else(|error| panic!("{error}")),
            untracked: false,
        };
        let effect = state.request_diff(target);
        let request_id = match &effect[0] {
            crate::app::GitEffect::LoadDiff { request_id, .. } => *request_id,
            _ => panic!("expected diff request"),
        };
        let _effects = apply_event(
            &mut state,
            Event::DiffLoaded {
                request_id,
                result: Ok(DiffDocument::Empty {
                    message: "none".to_owned(),
                }),
            },
        );
        assert!(matches!(state.diff.content, LoadState::Ready(_)));
    }

    #[test]
    fn switching_views_while_loading_starts_the_other_resource() {
        let mut state = state();
        let _changes = state.start();
        let effects = apply_action(&mut state, Action::ShowHistory);
        assert!(matches!(state.changes, LoadState::Loading { .. }));
        assert!(matches!(state.commits, LoadState::Loading { .. }));
        assert!(matches!(
            effects.first(),
            Some(crate::app::GitEffect::LoadCommits { .. })
        ));
    }

    #[test]
    fn refresh_retries_after_a_loading_error() {
        let mut state = state();
        let effects = state.start();
        let request_id = match &effects[0] {
            crate::app::GitEffect::LoadChanges { request_id } => *request_id,
            _ => panic!("expected change request"),
        };
        let _none = apply_event(
            &mut state,
            Event::ChangesLoaded {
                request_id,
                result: Err(crate::git::GitError::Unsupported("failed".to_owned())),
            },
        );
        assert!(matches!(state.changes, LoadState::Failed(_)));
        let retry = apply_action(&mut state, Action::Refresh);
        assert!(matches!(state.changes, LoadState::Loading { .. }));
        assert!(matches!(
            retry.first(),
            Some(crate::app::GitEffect::LoadChanges { .. })
        ));
    }

    #[test]
    fn refresh_preserves_the_selected_change_by_path() {
        let mut state = state();
        let first =
            RepoPath::from_bytes(b"first".to_vec()).unwrap_or_else(|error| panic!("{error}"));
        let selected =
            RepoPath::from_bytes(b"selected".to_vec()).unwrap_or_else(|error| panic!("{error}"));
        state.changes = LoadState::Ready(vec![
            WorktreeChange::new(first.clone(), None, ChangeKind::Modified),
            WorktreeChange::new(selected.clone(), None, ChangeKind::Modified),
        ]);
        state.change_selection.reset_to(2, Some(1));

        let refresh = apply_action(&mut state, Action::Refresh);
        let request_id = match refresh.first() {
            Some(crate::app::GitEffect::LoadChanges { request_id }) => *request_id,
            _ => panic!("expected refresh request"),
        };
        let effects = apply_event(
            &mut state,
            Event::ChangesLoaded {
                request_id,
                result: Ok(vec![
                    WorktreeChange::new(selected.clone(), None, ChangeKind::Modified),
                    WorktreeChange::new(first, None, ChangeKind::Modified),
                ]),
            },
        );

        assert_eq!(state.change_selection.index(), Some(0));
        assert!(matches!(
            effects.first(),
            Some(crate::app::GitEffect::LoadDiff {
                target: crate::domain::DiffTarget::Worktree { path, .. },
                ..
            }) if path == &selected
        ));
    }

    #[test]
    fn history_refresh_preserves_the_selected_commit_by_id() {
        let mut state = state();
        state.view = AppView::History;
        let first = commit('a', "first");
        let selected = commit('b', "selected");
        state.commits = LoadState::Ready(vec![first.clone(), selected.clone()]);
        state.commit_selection.reset_to(2, Some(1));

        let refresh = apply_action(&mut state, Action::Refresh);
        let request_id = match refresh.first() {
            Some(crate::app::GitEffect::LoadCommits { request_id, .. }) => *request_id,
            _ => panic!("expected history refresh request"),
        };
        let effects = apply_event(
            &mut state,
            Event::CommitsLoaded {
                request_id,
                append: false,
                limit: 200,
                result: Ok(vec![selected.clone(), first]),
            },
        );

        assert_eq!(state.commit_selection.index(), Some(0));
        assert!(matches!(
            effects.first(),
            Some(crate::app::GitEffect::LoadFiles { commit, .. }) if commit == selected.id()
        ));
    }

    #[test]
    fn help_overlay_does_not_change_commit_body_scroll() {
        let mut state = state();
        state.message.scroll = 7;
        let _none = apply_action(&mut state, Action::ToggleHelp);
        let _none = apply_action(&mut state, Action::HalfPageDown);
        assert_eq!(state.message.scroll, 7);
        let _none = apply_action(&mut state, Action::CloseOverlay);
        assert_eq!(state.overlay, crate::app::Overlay::None);
    }

    #[test]
    fn focus_moves_across_every_history_and_detail_pane() {
        for view in [AppView::History, AppView::CommitDetails] {
            let mut state = state();
            state.view = view;

            let _none = apply_action(&mut state, Action::FocusRight);
            assert_eq!(state.focus, FocusedPane::Secondary);
            let _none = apply_action(&mut state, Action::FocusRight);
            assert_eq!(state.focus, FocusedPane::Diff);
            let _none = apply_action(&mut state, Action::FocusLeft);
            assert_eq!(state.focus, FocusedPane::Secondary);
            let _none = apply_action(&mut state, Action::FocusLeft);
            assert_eq!(state.focus, FocusedPane::Primary);
        }
    }

    #[test]
    fn enter_on_a_history_commit_focuses_changed_files() {
        let mut state = state();
        state.view = AppView::History;
        state.focus = FocusedPane::Primary;
        state.history_panel = HistoryPanel::Tree;

        let effects = apply_action(&mut state, Action::Activate);
        assert!(effects.is_empty());
        assert_eq!(state.history_panel, HistoryPanel::ChangedFiles);
        assert_eq!(state.focus, FocusedPane::Secondary);

        state.view = AppView::CommitDetails;
        state.focus = FocusedPane::Primary;
        let effects = apply_action(&mut state, Action::Activate);
        assert!(effects.is_empty());
        assert_eq!(state.focus, FocusedPane::Diff);
    }

    #[test]
    fn diff_navigation_is_preserved_while_the_diff_loads() {
        let mut state = state();
        let target = DiffTarget::Worktree {
            path: RepoPath::from_bytes(b"pending.txt".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
            untracked: false,
        };
        let effects = state.request_diff(target);
        let request_id = match effects.first() {
            Some(GitEffect::LoadDiff { request_id, .. }) => *request_id,
            _ => panic!("expected diff request"),
        };
        state.overlay = Overlay::Diff;

        let _none = apply_action(&mut state, Action::HalfPageDown);
        assert_eq!(state.diff.vertical, 10);
        let _none = apply_action(&mut state, Action::HalfPageUp);
        assert_eq!(state.diff.vertical, 0);
        let _none = apply_action(&mut state, Action::HalfPageDown);
        assert_eq!(state.diff.vertical, 10);

        let _none = apply_event(
            &mut state,
            Event::DiffLoaded {
                request_id,
                result: Ok(DiffDocument::Text {
                    lines: (0..30)
                        .map(|index| {
                            DiffLine::new(
                                DiffLineKind::Context,
                                None,
                                None,
                                format!("line {index}"),
                            )
                        })
                        .collect(),
                    bytes: 210,
                }),
            },
        );
        assert_eq!(state.diff.vertical, 10);
    }

    #[test]
    fn changed_file_opens_a_scrollable_searchable_diff_overlay() {
        let mut state = state();
        state.view = AppView::History;
        state.focus = FocusedPane::Secondary;
        let selected = commit('a', "searchable");
        let path =
            RepoPath::from_bytes(b"src/lib.rs".to_vec()).unwrap_or_else(|error| panic!("{error}"));
        state.commits = LoadState::Ready(vec![selected.clone()]);
        state.commit_selection.reset(1);
        state.files = LoadState::Ready(vec![ChangedFile::new(
            path.clone(),
            None,
            ChangeKind::Modified,
        )]);
        state.file_selection.reset(1);
        state.diff.target = Some(DiffTarget::Commit {
            commit: selected.id().clone(),
            baseline: selected.baseline(),
            path,
        });
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: vec![
                DiffLine::new(DiffLineKind::Context, None, None, "first".to_owned()),
                DiffLine::new(DiffLineKind::Added, None, None, "+search needle".to_owned()),
                DiffLine::new(
                    DiffLineKind::Removed,
                    None,
                    None,
                    "-second needle".to_owned(),
                ),
            ],
            bytes: 34,
        });

        let _none = apply_action(&mut state, Action::Activate);
        assert_eq!(state.overlay, Overlay::Diff);

        let _none = apply_action(&mut state, Action::MoveDown);
        assert_eq!(state.diff.vertical, 1);
        let _none = apply_action(&mut state, Action::MoveUp);
        assert_eq!(state.diff.vertical, 0);

        state.diff.vertical = 1;
        state.diff.byte_column = 0;
        let _none = apply_action(
            &mut state,
            Action::VimMotion(VimMotion::new(VimMotionKind::WordForward)),
        );
        assert_eq!(state.diff.vertical, 1);
        assert_eq!(state.diff.byte_column, 1);

        let _none = apply_action(&mut state, Action::StartSearch(SearchDirection::Forward));
        for character in "needle".chars() {
            let _none = apply_action(&mut state, Action::InsertSearch(character));
        }
        let _none = apply_action(&mut state, Action::ConfirmSearch);
        assert_eq!(state.diff.vertical, 1);
        assert_eq!(state.search.match_count(), 2);

        let _none = apply_action(&mut state, Action::NextMatch);
        assert_eq!(state.diff.vertical, 2);
        let _none = apply_action(&mut state, Action::PreviousMatch);
        assert_eq!(state.diff.vertical, 1);

        let _none = apply_action(&mut state, Action::CloseOverlay);
        assert_eq!(state.overlay, Overlay::None);
        assert!(state.search.query().is_empty());
    }

    #[test]
    fn reaching_the_history_page_end_requests_and_appends_the_next_page() {
        let mut state = state();
        state.view = AppView::History;
        state.commits = LoadState::Ready((0..200).map(numbered_commit).collect());
        state.commit_selection.reset_to(200, Some(198));
        state.history_page.has_more = true;

        let effects = apply_action(&mut state, Action::MoveDown);
        let (request_id, skip, limit) = effects
            .iter()
            .find_map(|effect| match effect {
                GitEffect::LoadCommits {
                    request_id,
                    skip,
                    limit,
                    append: true,
                } => Some((*request_id, *skip, *limit)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected next history page request"));
        assert_eq!((skip, limit), (200, 200));

        let _none = apply_event(
            &mut state,
            Event::CommitsLoaded {
                request_id,
                append: true,
                limit,
                result: Ok(vec![numbered_commit(200)]),
            },
        );
        assert!(matches!(&state.commits, LoadState::Ready(commits) if commits.len() == 201));
        assert!(!state.history_page.has_more);
        assert!(state.history_page.loading_more.is_none());
    }

    #[test]
    fn tree_expands_collapses_and_selects_a_blob_diff() {
        let mut state = state();
        state.view = AppView::History;
        let selected_commit = commit('a', "tree");
        state.commits = LoadState::Ready(vec![selected_commit.clone()]);
        state.commit_selection.reset(1);

        let root_effects = apply_action(&mut state, Action::ToggleTree);
        assert_eq!(state.history_panel, HistoryPanel::Tree);
        let (root_request, commit_id) = match root_effects.first() {
            Some(GitEffect::LoadTree {
                request_id,
                commit,
                parent: None,
                ..
            }) => (*request_id, commit.clone()),
            _ => panic!("expected root tree request"),
        };
        let directory = tree_entry('b', TreeKind::Directory, "dir", "040000");
        let _none = apply_event(
            &mut state,
            Event::TreeLoaded {
                request_id: root_request,
                commit: commit_id.clone(),
                parent: None,
                result: Ok(vec![directory.clone()]),
            },
        );
        let _none = apply_action(&mut state, Action::FocusRight);

        let child_effects = apply_action(&mut state, Action::Activate);
        let (child_request, parent) = match child_effects.first() {
            Some(GitEffect::LoadTree {
                request_id,
                parent: Some(parent),
                ..
            }) => (*request_id, parent.clone()),
            _ => panic!("expected child tree request"),
        };
        let file = tree_entry('c', TreeKind::File, "file.txt", "100644");
        let _none = apply_event(
            &mut state,
            Event::TreeLoaded {
                request_id: child_request,
                commit: commit_id,
                parent: Some(parent),
                result: Ok(vec![file]),
            },
        );
        assert!(matches!(
            &state.tree.visible,
            LoadState::Ready(entries) if entries.len() == 2 && entries[0].expanded()
        ));

        let _none = apply_action(&mut state, Action::Activate);
        assert!(matches!(&state.tree.visible, LoadState::Ready(entries) if entries.len() == 1));
        let cached = apply_action(&mut state, Action::Activate);
        assert!(cached.is_empty());
        assert!(matches!(&state.tree.visible, LoadState::Ready(entries) if entries.len() == 2));
        let _none = apply_action(&mut state, Action::MoveDown);
        let diff = apply_action(&mut state, Action::Activate);
        assert!(matches!(
            diff.first(),
            Some(GitEffect::LoadDiff {
                target: crate::domain::DiffTarget::Commit { path, .. },
                ..
            }) if path.display() == "dir/file.txt"
        ));
        assert_eq!(state.overlay, Overlay::Diff);
    }

    #[test]
    fn commit_message_overlay_retries_errors_and_scrolls_the_full_message() {
        let mut state = state();
        state.view = AppView::History;
        let selected = commit('a', "message");
        state.commits = LoadState::Ready(vec![selected.clone()]);
        state.commit_selection.reset(1);

        let first = apply_action(&mut state, Action::ToggleMessage);
        assert_eq!(state.overlay, Overlay::CommitMessage);
        let first_request = match first.first() {
            Some(GitEffect::LoadMessage { request_id, .. }) => *request_id,
            _ => panic!("expected message request"),
        };
        let _none = apply_event(
            &mut state,
            Event::MessageLoaded {
                request_id: first_request,
                commit: selected.id().clone(),
                result: Err(crate::git::GitError::Unsupported(
                    "message failed".to_owned(),
                )),
            },
        );
        assert!(matches!(state.message.content, LoadState::Failed(_)));
        let _closed = apply_action(&mut state, Action::ToggleMessage);
        assert_eq!(state.overlay, Overlay::None);

        let retry = apply_action(&mut state, Action::ToggleMessage);
        let retry_request = match retry.first() {
            Some(GitEffect::LoadMessage { request_id, .. }) => *request_id,
            _ => panic!("expected message retry"),
        };
        let _none = apply_event(
            &mut state,
            Event::MessageLoaded {
                request_id: retry_request,
                commit: selected.id().clone(),
                result: Ok(CommitMessage::new(
                    "message\n\nbody one\nbody two\nbody three\n".to_owned(),
                )),
            },
        );
        let _none = apply_action(&mut state, Action::HalfPageDown);
        assert_eq!(state.message.scroll, 4);
        assert!(matches!(state.message.content, LoadState::Ready(_)));

        let _closed = apply_action(&mut state, Action::ToggleMessage);
        assert_eq!(state.overlay, Overlay::None);
    }

    #[test]
    fn commit_body_view_keeps_the_commit_list_interactive() {
        let mut state = state();
        state.view = AppView::History;
        let first = commit('a', "first");
        let second = commit('b', "second");
        state.commits = LoadState::Ready(vec![first.clone(), second.clone()]);
        state.commit_selection.reset(2);

        let open = apply_action(&mut state, Action::ToggleDetails);
        assert_eq!(state.view, AppView::CommitDetails);
        let message_request = match open.first() {
            Some(GitEffect::LoadMessage { request_id, commit }) if commit == first.id() => {
                *request_id
            }
            _ => panic!("expected first commit message request"),
        };
        let _none = apply_event(
            &mut state,
            Event::MessageLoaded {
                request_id: message_request,
                commit: first.id().clone(),
                result: Ok(CommitMessage::new(
                    "first\n\nbody one\nbody two\nbody three\n".to_owned(),
                )),
            },
        );

        let _none = apply_action(&mut state, Action::FocusRight);
        let _none = apply_action(&mut state, Action::MoveDown);
        assert_eq!(state.message.scroll, 1);
        let _none = apply_action(&mut state, Action::FocusLeft);
        let effects = apply_action(&mut state, Action::MoveDown);
        assert_eq!(state.commit_selection.index(), Some(1));
        assert_eq!(state.message.scroll, 0);
        assert!(effects.iter().any(
            |effect| matches!(effect, GitEffect::LoadFiles { commit, .. } if commit == second.id())
        ));
        assert!(effects.iter().any(
            |effect| matches!(effect, GitEffect::LoadMessage { commit, .. } if commit == second.id())
        ));

        state.history_panel = HistoryPanel::Tree;
        state.tree.commit = Some(first.id().clone());
        let close = apply_action(&mut state, Action::ToggleDetails);
        assert_eq!(state.view, AppView::History);
        assert!(close.iter().any(
            |effect| matches!(effect, GitEffect::LoadTree { commit, .. } if commit == second.id())
        ));
    }

    #[test]
    fn commit_body_view_changed_file_opens_a_diff() {
        let mut state = state();
        state.view = AppView::CommitDetails;
        state.focus = FocusedPane::Diff;
        let selected = commit('a', "details");
        let path =
            RepoPath::from_bytes(b"src/lib.rs".to_vec()).unwrap_or_else(|error| panic!("{error}"));
        state.commits = LoadState::Ready(vec![selected.clone()]);
        state.commit_selection.reset(1);
        state.files = LoadState::Ready(vec![ChangedFile::new(
            path.clone(),
            None,
            ChangeKind::Modified,
        )]);
        state.file_selection.reset(1);
        state.diff.target = Some(DiffTarget::Commit {
            commit: selected.id().clone(),
            baseline: selected.baseline(),
            path,
        });

        let _none = apply_action(&mut state, Action::Activate);
        assert_eq!(state.overlay, Overlay::Diff);
    }

    #[test]
    fn close_action_returns_from_commit_details() {
        let mut state = state();
        state.view = AppView::CommitDetails;
        state.focus = FocusedPane::Diff;

        let effects = apply_action(&mut state, Action::CloseOverlay);

        assert!(effects.is_empty());
        assert_eq!(state.view, AppView::History);
        assert_eq!(state.focus, FocusedPane::Primary);
    }

    #[test]
    fn graph_opens_changed_files_and_diff_then_returns_with_escape() {
        let mut state = state();
        state.view = AppView::Graph;
        let selected = commit('a', "graph commit");
        state.commits = LoadState::Ready(vec![selected.clone()]);
        state.commit_selection.reset(1);

        let effects = apply_action(&mut state, Action::Activate);
        assert_eq!(state.view, AppView::GraphDetails);
        assert_eq!(state.focus, FocusedPane::Secondary);
        let files_request = effects
            .iter()
            .find_map(|effect| match effect {
                GitEffect::LoadFiles { request_id, .. } => Some(*request_id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("graph details must request changed files"));
        let path = RepoPath::from_bytes(b"src/graph.rs".to_vec())
            .unwrap_or_else(|error| panic!("{error}"));
        let diff_effects = apply_event(
            &mut state,
            Event::FilesLoaded {
                request_id: files_request,
                commit: selected.id().clone(),
                result: Ok(vec![ChangedFile::new(path, None, ChangeKind::Modified)]),
            },
        );
        assert!(
            diff_effects
                .iter()
                .any(|effect| matches!(effect, GitEffect::LoadDiff { .. }))
        );

        let _none = apply_action(&mut state, Action::Activate);
        assert_eq!(state.overlay, Overlay::Diff);
        let _none = apply_action(&mut state, Action::CloseOverlay);
        assert_eq!(state.overlay, Overlay::None);
        let _none = apply_action(&mut state, Action::CloseOverlay);
        assert_eq!(state.view, AppView::Graph);

        let message = apply_action(&mut state, Action::ToggleMessage);
        assert_eq!(state.overlay, Overlay::CommitMessage);
        assert!(
            message
                .iter()
                .any(|effect| matches!(effect, GitEffect::LoadMessage { .. }))
        );
    }

    #[test]
    fn global_content_search_opens_current_file_then_history_diff() {
        let mut state = state();
        let _none = apply_action(&mut state, Action::OpenContentSearch);
        assert_eq!(state.overlay, Overlay::RepositorySearch);
        assert_eq!(state.repository_search.kind, RepositorySearchKind::Content);
        let first_search = apply_action(&mut state, Action::InsertSearch('n'));
        let first_request = match first_search.first() {
            Some(GitEffect::SearchContent { request_id, query }) => {
                assert_eq!(query, "n");
                *request_id
            }
            other => panic!("unexpected first live-search effect: {other:?}"),
        };
        for character in "eedle".chars() {
            let _live_search = apply_action(&mut state, Action::InsertSearch(character));
        }
        let delete_search = apply_action(&mut state, Action::DeleteSearch);
        assert!(matches!(
            delete_search.first(),
            Some(GitEffect::SearchContent { query, .. }) if query == "needl"
        ));
        let latest_search = apply_action(&mut state, Action::InsertSearch('e'));
        let search_request = match latest_search.first() {
            Some(GitEffect::SearchContent { request_id, query }) => {
                assert_eq!(query, "needle");
                *request_id
            }
            other => panic!("unexpected latest live-search effect: {other:?}"),
        };
        let path = RepoPath::from_bytes(b"src/search.rs".to_vec())
            .unwrap_or_else(|error| panic!("{error}"));
        let _ignored = apply_event(
            &mut state,
            Event::RepositorySearchLoaded {
                request_id: first_request,
                result: Ok(vec![SearchHit::content(
                    path.clone(),
                    1,
                    "old result".to_owned(),
                )]),
            },
        );
        assert_eq!(
            state.repository_search.results.loading_request(),
            Some(search_request),
            "a result from an earlier query must not replace the live search"
        );
        let confirm = apply_action(&mut state, Action::ConfirmSearch);
        assert!(confirm.is_empty());
        assert!(state.repository_search.prompt.is_none());
        let _none = apply_event(
            &mut state,
            Event::RepositorySearchLoaded {
                request_id: search_request,
                result: Ok(vec![SearchHit::content(
                    path.clone(),
                    2,
                    "needle".to_owned(),
                )]),
            },
        );
        let file_effects = apply_action(&mut state, Action::Activate);
        assert_eq!(state.view, AppView::FileHistory);
        assert_eq!(state.overlay, Overlay::None);
        assert_eq!(state.file_view.vertical, 1);

        let history_request = file_effects
            .iter()
            .find_map(|effect| match effect {
                GitEffect::LoadFileHistory { request_id, .. } => Some(*request_id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("file history must load"));
        let content_request = file_effects
            .iter()
            .find_map(|effect| match effect {
                GitEffect::LoadFileContent { request_id, .. } => Some(*request_id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("file content must load"));
        let first = commit('a', "newer");
        let second = commit('b', "older");
        let _none = apply_event(
            &mut state,
            Event::FileHistoryLoaded {
                request_id: history_request,
                path: path.clone(),
                result: Ok(vec![first, second.clone()]),
            },
        );
        let _none = apply_event(
            &mut state,
            Event::FileContentLoaded {
                request_id: content_request,
                path,
                result: Ok(FileDocument::Text {
                    lines: vec!["one".to_owned(), "needle".to_owned()],
                    source: "one\nneedle".to_owned(),
                    valid_utf8: true,
                    truncated: false,
                }),
            },
        );
        assert!(!state.file_view.showing_history_diff);
        let _none = apply_action(&mut state, Action::Activate);
        assert_eq!(state.overlay, Overlay::FileContent);
        let _none = apply_action(&mut state, Action::CloseOverlay);

        let diff = apply_action(&mut state, Action::MoveDown);
        assert!(state.file_view.showing_history_diff);
        assert!(diff.iter().any(|effect| {
            matches!(
                effect,
                GitEffect::LoadDiff {
                    target: DiffTarget::Commit { commit, .. },
                    ..
                } if commit == second.id()
            )
        }));
        let _none = apply_action(&mut state, Action::Activate);
        assert_eq!(state.overlay, Overlay::Diff);
        let _none = apply_action(&mut state, Action::CloseOverlay);
        let _none = apply_action(&mut state, Action::CloseOverlay);
        assert_eq!(state.view, AppView::Changes);
    }

    #[test]
    fn repository_search_can_return_to_the_prompt_and_search_again() {
        let mut state = state();
        let _none = apply_action(&mut state, Action::OpenFileSearch);
        let initial = apply_action(&mut state, Action::InsertSearch('R'));
        assert!(matches!(
            initial.first(),
            Some(GitEffect::SearchFiles { query, .. }) if query == "R"
        ));

        let focus_results = apply_action(&mut state, Action::FocusRight);
        assert!(focus_results.is_empty());
        assert!(state.repository_search.prompt.is_none());

        let focus_search = apply_action(&mut state, Action::FocusLeft);
        assert!(focus_search.is_empty());
        assert_eq!(state.repository_search.prompt.as_deref(), Some("R"));

        let repeated = apply_action(&mut state, Action::InsertSearch('E'));
        assert!(matches!(
            repeated.first(),
            Some(GitEffect::SearchFiles { query, .. }) if query == "RE"
        ));
        assert_eq!(state.repository_search.query, "RE");

        let confirm = apply_action(&mut state, Action::ConfirmSearch);
        assert!(confirm.is_empty());
        assert!(state.repository_search.prompt.is_none());
    }

    #[test]
    fn code_view_expands_files_opens_full_content_and_keeps_search_in_the_workflow() {
        let mut state = state();
        let tree_effects = apply_action(&mut state, Action::ShowCode);
        assert_eq!(state.view, AppView::Code);
        let tree_request = match tree_effects.first() {
            Some(GitEffect::LoadCodeTree { request_id }) => *request_id,
            other => panic!("expected code-tree request, got {other:?}"),
        };
        let readme = RepoPath::from_bytes(b"README.md".to_vec())
            .unwrap_or_else(|error| panic!("invalid fixture path: {error}"));
        let source = RepoPath::from_bytes(b"src/lib.rs".to_vec())
            .unwrap_or_else(|error| panic!("invalid fixture path: {error}"));
        let preview = apply_event(
            &mut state,
            Event::CodeTreeLoaded {
                request_id: tree_request,
                result: Ok(vec![readme, source.clone()]),
            },
        );
        assert!(preview.is_empty());
        assert!(matches!(
            &state.code_view.visible,
            LoadState::Ready(entries)
                if entries.len() == 2
                    && entries[0].path().display() == "src"
                    && entries[1].path().display() == "README.md"
        ));

        let _expanded = apply_action(&mut state, Action::Activate);
        assert!(matches!(
            &state.code_view.visible,
            LoadState::Ready(entries) if entries.len() == 3 && entries[0].expanded()
        ));
        let file_effects = apply_action(&mut state, Action::MoveDown);
        let file_request = match file_effects.first() {
            Some(GitEffect::LoadCodeFile { request_id, path }) if path == &source => *request_id,
            other => panic!("expected code-file request, got {other:?}"),
        };
        let _none = apply_event(
            &mut state,
            Event::CodeFileLoaded {
                request_id: file_request,
                path: source,
                result: Ok(FileDocument::Text {
                    lines: vec!["first".to_owned(), "searchable code".to_owned()],
                    source: "first\nsearchable code".to_owned(),
                    valid_utf8: true,
                    truncated: false,
                }),
            },
        );
        let _none = apply_action(&mut state, Action::FocusRight);
        let _none = apply_action(&mut state, Action::Activate);
        assert_eq!(state.overlay, Overlay::CodeContent);

        let _none = apply_action(&mut state, Action::StartSearch(SearchDirection::Forward));
        for character in "searchable".chars() {
            let _none = apply_action(&mut state, Action::InsertSearch(character));
        }
        let _none = apply_action(&mut state, Action::ConfirmSearch);
        assert_eq!(state.code_view.cursor.line(), 1);
        assert_eq!(state.semantic_navigation.back_stack.len(), 1);
        assert_eq!(
            state
                .semantic_navigation
                .back_stack
                .back()
                .map(|origin| origin.cursor),
            Some(SourcePosition::new(0, 0))
        );
        let _none = apply_action(&mut state, Action::CloseOverlay);
        assert_eq!(state.overlay, Overlay::None);
        assert_eq!(state.focus, FocusedPane::Diff);
        let _none = apply_action(&mut state, Action::FocusLeft);
        assert_eq!(state.focus, FocusedPane::Primary);
        let _none = apply_action(&mut state, Action::FocusRight);
        assert_eq!(state.focus, FocusedPane::Diff);

        let _none = apply_action(&mut state, Action::OpenContentSearch);
        let search = apply_action(&mut state, Action::InsertSearch('x'));
        let search_request = match search.first() {
            Some(GitEffect::SearchContent { request_id, .. }) => *request_id,
            other => panic!("expected repository search, got {other:?}"),
        };
        let match_path = RepoPath::from_bytes(b"src/lib.rs".to_vec())
            .unwrap_or_else(|error| panic!("invalid fixture path: {error}"));
        let _none = apply_event(
            &mut state,
            Event::RepositorySearchLoaded {
                request_id: search_request,
                result: Ok(vec![SearchHit::content(
                    match_path.clone(),
                    2,
                    "x".to_owned(),
                )]),
            },
        );
        let _none = apply_action(&mut state, Action::ConfirmSearch);
        let opened = apply_action(&mut state, Action::Activate);
        assert_eq!(state.view, AppView::Code);
        assert_eq!(state.focus, FocusedPane::Diff);
        assert_eq!(state.code_view.cursor.line(), 1);
        assert!(matches!(
            opened.first(),
            Some(GitEffect::LoadCodeFile { path, .. }) if path == &match_path
        ));
    }

    #[test]
    fn star_search_word_selection_falls_back_to_the_nearest_nonblank_word() {
        assert_eq!(
            super::word_at_or_after(&["foo -> bar"], SourcePosition::new(0, 4)),
            Some(("bar".to_owned(), 7))
        );
        assert_eq!(
            super::word_at_or_after(&["foo ---"], SourcePosition::new(0, 4)),
            Some(("---".to_owned(), 4))
        );
    }

    fn searchable_code(lines: &[&str]) -> AppState {
        let mut state = state();
        state.view = AppView::Code;
        state.focus = FocusedPane::Diff;
        state.code_view.content = LoadState::Ready(FileDocument::Text {
            source: lines.join("\n"),
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
            valid_utf8: true,
            truncated: false,
        });
        state
    }

    fn search_key(state: &mut AppState, mapper: &mut crate::tui::keymap::KeyMapper, key: char) {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let code = match key {
            '\u{1b}' => KeyCode::Esc,
            '\n' => KeyCode::Enter,
            character => KeyCode::Char(character),
        };
        if let Some(action) = mapper.map(
            KeyEvent::new(code, KeyModifiers::NONE),
            state.is_search_input_active(),
        ) {
            assert!(state.handle_app_action(action).is_empty());
        }
    }

    fn search_keys(state: &mut AppState, mapper: &mut crate::tui::keymap::KeyMapper, keys: &str) {
        for key in keys.chars() {
            search_key(state, mapper, key);
        }
    }

    #[test]
    fn search_escape_preserves_position_and_resumes_in_panes_and_floats() {
        for code in [false, true] {
            for float in [false, true] {
                for (prompt, direction) in [
                    ('/', SearchDirection::Forward),
                    ('?', SearchDirection::Backward),
                ] {
                    let mut state = searchable_code(&["cat cat", "cat cat", "cat cat"]);
                    if !code {
                        state.view = AppView::GraphDetails;
                        state.diff.content = LoadState::Ready(DiffDocument::Text {
                            lines: (0..3)
                                .map(|_| {
                                    DiffLine::new(
                                        DiffLineKind::Context,
                                        None,
                                        None,
                                        "cat cat".to_owned(),
                                    )
                                })
                                .collect(),
                            bytes: 21,
                        });
                    }
                    state.overlay = match (code, float) {
                        (true, true) => Overlay::CodeContent,
                        (false, true) => Overlay::Diff,
                        _ => Overlay::None,
                    };
                    let mut mapper = crate::tui::keymap::KeyMapper::new();
                    search_keys(&mut state, &mut mapper, &format!("{prompt}cat\n"));
                    assert_eq!(state.search.direction(), direction);
                    assert_eq!(state.search.match_count(), 6);
                    // Non-default viewport coordinates must survive dismissal exactly.
                    state.code_view.viewport_vertical = 1;
                    state.code_view.viewport_horizontal = 4;
                    state.diff.viewport_vertical = 1;
                    state.diff.horizontal = 4;
                    let before = (
                        state.view,
                        state.overlay,
                        state.focus,
                        state.code_view.cursor,
                        state.code_view.viewport_vertical,
                        state.code_view.viewport_horizontal,
                        state.diff.vertical,
                        state.diff.byte_column,
                        state.diff.viewport_vertical,
                        state.diff.horizontal,
                    );
                    let ordinal = state.search.current_ordinal();
                    search_key(&mut state, &mut mapper, '\u{1b}');
                    assert!(!state.search.has_highlights());
                    assert_eq!(
                        before,
                        (
                            state.view,
                            state.overlay,
                            state.focus,
                            state.code_view.cursor,
                            state.code_view.viewport_vertical,
                            state.code_view.viewport_horizontal,
                            state.diff.vertical,
                            state.diff.byte_column,
                            state.diff.viewport_vertical,
                            state.diff.horizontal
                        )
                    );
                    assert_eq!(state.search.query(), "cat");
                    assert_eq!(state.search.direction(), direction);
                    assert_eq!(state.search.current_ordinal(), ordinal);
                    // Six hits: a counted repeat wraps to the same hit, then N
                    // goes in the opposite direction without changing the query direction.
                    search_keys(&mut state, &mut mapper, "6n");
                    assert!(state.search.has_highlights());
                    assert_eq!(state.search.current_ordinal(), ordinal);
                    search_keys(&mut state, &mut mapper, "\u{1b}N");
                    assert!(state.search.has_highlights());
                    let previous = ordinal.unwrap_or(0);
                    let expected = if direction == SearchDirection::Forward {
                        (previous + 4) % 6 + 1
                    } else {
                        previous % 6 + 1
                    };
                    assert_eq!(state.search.current_ordinal(), Some(expected));
                    assert_eq!(state.search.direction(), direction);
                    search_keys(&mut state, &mut mapper, "\u{1b}/at\n");
                    assert!(state.search.has_highlights());
                    assert_eq!(state.search.query(), "at");
                    search_keys(&mut state, &mut mapper, "\u{1b}\u{1b}");
                    assert_eq!(state.overlay, Overlay::None);
                    assert_eq!(
                        state.view,
                        if code {
                            AppView::Code
                        } else if float {
                            AppView::GraphDetails
                        } else {
                            AppView::Graph
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn search_cancel_and_frontmost_screens_keep_underlying_highlights() {
        let mut state = searchable_code(&["cat cat"]);
        let mut mapper = crate::tui::keymap::KeyMapper::new();
        search_keys(&mut state, &mut mapper, "/cat\n");
        for visible in [true, false] {
            if !visible {
                search_key(&mut state, &mut mapper, '\u{1b}');
            }
            let position = state.code_view.cursor;
            for pending in ["/new", "?new", "f", "F", "t", "T", "m", "'", "`"] {
                search_keys(&mut state, &mut mapper, pending);
                search_key(&mut state, &mut mapper, '\u{1b}');
                assert!(!state.search.is_input_active());
                assert_eq!(state.search.has_highlights(), visible, "{pending}");
                assert_eq!(state.search.query(), "cat");
                assert_eq!(state.search.direction(), SearchDirection::Forward);
                assert_eq!(state.code_view.cursor, position);
                assert_eq!(state.overlay, Overlay::None);
            }
        }
        search_key(&mut state, &mut mapper, 'n');
        for overlay in [
            Overlay::Help,
            Overlay::RepositorySearch,
            Overlay::LspHover,
            Overlay::SemanticTargets,
            Overlay::CommitMessage,
            Overlay::FileContent,
        ] {
            state.overlay = overlay;
            state.repository_search.prompt = None;
            search_key(&mut state, &mut mapper, '\u{1b}');
            assert_ne!(state.overlay, overlay, "{overlay:?}");
            assert!(state.search.has_highlights(), "{overlay:?}");
        }
        state.overlay = Overlay::RepositorySearch;
        state.repository_search.prompt = Some("cat".to_owned());
        search_key(&mut state, &mut mapper, '\u{1b}');
        assert_ne!(state.overlay, Overlay::RepositorySearch);
        assert!(state.search.has_highlights());
    }

    #[test]
    fn search_q_no_match_and_custom_close_need_no_dismissal() {
        for code in [false, true] {
            for (query, close) in [("cat", "q"), ("missing", "\u{1b}")] {
                let mut state = searchable_code(&["cat cat"]);
                state.overlay = if code {
                    Overlay::CodeContent
                } else {
                    Overlay::Diff
                };
                state.diff.content = LoadState::Ready(DiffDocument::Text {
                    lines: vec![DiffLine::new(
                        DiffLineKind::Context,
                        None,
                        None,
                        "cat cat".to_owned(),
                    )],
                    bytes: 7,
                });
                let mut mapper = crate::tui::keymap::KeyMapper::new();
                search_keys(&mut state, &mut mapper, &format!("/{query}\n{close}"));
                assert_eq!(state.overlay, Overlay::None);
            }
        }
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("keymap.conf");
        for (config, close) in [
            ("close = x", "x"),
            ("close = q, esc", "\u{1b}"),
            ("close = x\nrefresh = esc", "x"),
        ] {
            std::fs::write(&path, config).unwrap_or_else(|error| panic!("{error}"));
            let mut mapper = crate::tui::keymap::KeyMapper::load(Some(&path))
                .unwrap_or_else(|error| panic!("{error}"));
            let mut state = searchable_code(&["cat cat"]);
            state.overlay = Overlay::CodeContent;
            search_keys(&mut state, &mut mapper, "/cat\n");
            assert!(state.search.has_highlights());
            search_keys(&mut state, &mut mapper, close);
            assert_eq!(state.overlay, Overlay::None, "{config}");
        }
    }

    #[test]
    fn repeated_search_uses_the_current_cursor_after_a_motion() {
        let mut state = searchable_code(&["cat", "cat", "cat", "cat"]);
        let _none = apply_action(&mut state, Action::StartSearch(SearchDirection::Forward));
        for character in "cat".chars() {
            let _none = apply_action(&mut state, Action::InsertSearch(character));
        }
        let _none = apply_action(&mut state, Action::ConfirmSearch);
        assert_eq!(state.code_view.cursor.line(), 1);
        let _none = apply_action(
            &mut state,
            Action::VimMotion(VimMotion::new(VimMotionKind::Down)),
        );
        let _none = apply_action(
            &mut state,
            Action::VimMotion(VimMotion::new(VimMotionKind::SearchNext)),
        );
        assert_eq!(state.code_view.cursor.line(), 3);
        let _none = apply_action(
            &mut state,
            Action::VimMotion(VimMotion::new(VimMotionKind::Up)),
        );
        let _none = apply_action(&mut state, Action::PreviousMatch);
        assert_eq!(state.code_view.cursor.line(), 1);
    }

    #[test]
    fn backward_word_search_skips_the_word_containing_the_cursor() {
        let mut state = searchable_code(&["cat cat cat"]);
        state.code_view.cursor = SourcePosition::new(0, 5);
        let _none = apply_action(
            &mut state,
            Action::VimMotion(VimMotion::new(VimMotionKind::SearchWordBackward)),
        );
        assert_eq!(state.code_view.cursor, SourcePosition::new(0, 0));
    }

    #[test]
    fn repeat_search_rebuilds_matches_for_the_active_document() {
        let mut state = searchable_code(&["cat", "cat", "cat"]);
        let _none = apply_action(
            &mut state,
            Action::VimMotion(VimMotion::new(VimMotionKind::SearchWordForward)),
        );
        state.view = AppView::FileHistory;
        state.file_view.content = LoadState::Ready(FileDocument::Text {
            source: "catfish cat".to_owned(),
            lines: vec!["catfish cat".to_owned()],
            valid_utf8: true,
            truncated: false,
        });
        let _none = apply_action(
            &mut state,
            Action::VimMotion(VimMotion::new(VimMotionKind::SearchNext)),
        );
        assert_eq!(
            (state.file_view.vertical, state.file_view.byte_column),
            (0, 8)
        );
    }

    #[test]
    fn leaving_document_search_does_not_trap_input_in_an_unfocused_pane() {
        let mut state = searchable_code(&["cat"]);
        let _none = apply_action(&mut state, Action::StartSearch(SearchDirection::Forward));
        let _none = apply_action(&mut state, Action::FocusLeft);
        let _none = apply_action(&mut state, Action::CancelSearch);
        assert!(!state.search.is_input_active());
    }

    fn commit(value: char, subject: &str) -> CommitSummary {
        CommitSummary::new(
            ObjectId::parse(value.to_string().repeat(40))
                .unwrap_or_else(|error| panic!("invalid fixture object ID: {error}")),
            Vec::new(),
            "Ada".to_owned(),
            "2026-01-01T00:00:00Z".to_owned(),
            subject.to_owned(),
        )
    }

    fn numbered_commit(value: usize) -> CommitSummary {
        CommitSummary::new(
            ObjectId::parse(format!("{value:040x}"))
                .unwrap_or_else(|error| panic!("invalid numbered object ID: {error}")),
            Vec::new(),
            "Ada".to_owned(),
            "2026-01-01T00:00:00Z".to_owned(),
            format!("commit {value}"),
        )
    }

    fn tree_entry(value: char, kind: TreeKind, name: &str, mode: &str) -> TreeEntry {
        TreeEntry::new(
            ObjectId::parse(value.to_string().repeat(40))
                .unwrap_or_else(|error| panic!("invalid tree object ID: {error}")),
            mode.to_owned(),
            kind,
            RepoPath::from_bytes(name.as_bytes().to_vec())
                .unwrap_or_else(|error| panic!("invalid tree path: {error}")),
        )
    }
}
