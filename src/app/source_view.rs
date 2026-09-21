//! Complete-source loading and the diff-to-source projection.

use std::collections::BTreeSet;

use crate::app::model::{FullFileDeletion, FullFileMode, SourceFileIdentity};
use crate::app::{
    Action, AppEffect, AppState, AppView, CursorColumnPolicy, Event, FALLBACK_HALF_PAGE_LINES,
    FocusedPane, GitEffect, HORIZONTAL_SCROLL_COLUMNS, HorizontalDirection, LoadState, Overlay,
    SourceDiffContext, VimMotion,
};
use crate::domain::{DiffLineKind, DiffTarget, FileDocument, FileRevision, SourcePosition};
use crate::layout::SOURCE_GUTTER_COLUMNS;

#[derive(Clone)]
struct ActiveSource {
    identity: SourceFileIdentity,
    cursor: SourcePosition,
    changed_lines: BTreeSet<u32>,
    deleted_lines: Vec<FullFileDeletion>,
    diff_context: SourceDiffContext,
    loaded: Option<FileDocument>,
    return_overlay: Overlay,
}

#[derive(Clone, Copy)]
enum SourcePreparation {
    Display,
    Symbols,
}

impl SourcePreparation {
    const fn requests_symbols(self) -> bool {
        matches!(self, Self::Symbols)
    }
}

pub(crate) fn apply_action(state: &mut AppState, action: Action) -> Option<Vec<AppEffect>> {
    match action {
        Action::OpenFullFile => Some(open_full_file(state)),
        Action::ToggleFullFileMode if state.overlay == Overlay::FullFile => {
            state.full_file.mode = match state.full_file.mode {
                FullFileMode::Changes => FullFileMode::New,
                FullFileMode::New => FullFileMode::Changes,
            };
            Some(Vec::new())
        }
        Action::MoveCursorLeft if state.overlay == Overlay::FullFile => {
            move_cursor_horizontally(state, HorizontalDirection::Left);
            Some(Vec::new())
        }
        Action::MoveCursorRight if state.overlay == Overlay::FullFile => {
            move_cursor_horizontally(state, HorizontalDirection::Right);
            Some(Vec::new())
        }
        _ => None,
    }
}

pub(crate) fn apply_event(state: &mut AppState, event: Event) -> Option<Vec<AppEffect>> {
    let Event::FullFileLoaded {
        request_id,
        revision,
        path,
        result,
    } = event
    else {
        return None;
    };
    if state.full_file.content.loading_request() != Some(request_id)
        || state.full_file.identity.as_ref() != Some(&SourceFileIdentity { revision, path })
    {
        return Some(Vec::new());
    }
    state.full_file.content = match result {
        Ok(document) => LoadState::Ready(document),
        Err(error) => LoadState::Failed(crate::app::ErrorNotice::new(error.to_string())),
    };
    clamp_cursor(state);
    if state.symbol_context.pending_source_request == Some(request_id) {
        state.symbol_context.pending_source_request = None;
        return Some(crate::app::semantic_navigation::request_document_symbols(
            state,
        ));
    }
    Some(Vec::new())
}

pub(crate) fn open_full_file(state: &mut AppState) -> Vec<AppEffect> {
    if state.overlay == Overlay::SymbolContext {
        if state.symbol_context.pending_source_request.take().is_some() {
            state.symbol_context.symbols = LoadState::Idle;
            state.symbol_context.status = None;
            state.full_file.return_overlay = state.symbol_context.return_overlay;
        } else {
            state.full_file.return_overlay = Overlay::SymbolContext;
        }
        state.overlay = Overlay::FullFile;
        state.search.clear();
        return Vec::new();
    }
    let Some(source) = active_source(state) else {
        state.notice = Some(crate::app::ErrorNotice::new(
            "Focus a diff or source document before opening the full file.",
        ));
        return Vec::new();
    };
    if source.diff_context.is_pending() {
        state.notice = Some(crate::app::ErrorNotice::new(
            "Wait for the active diff to finish loading before opening its complete source.",
        ));
        return Vec::new();
    }
    prepare(state, source, SourcePreparation::Display)
}

pub(crate) fn prepare_for_symbols(state: &mut AppState) -> Vec<AppEffect> {
    let Some(source) = active_source(state) else {
        state.notice = Some(crate::app::ErrorNotice::new(
            "Focus a diff or source document before opening symbol context.",
        ));
        return Vec::new();
    };
    if source.diff_context.is_pending() {
        state.notice = Some(crate::app::ErrorNotice::new(
            "Wait for the active diff to finish loading before requesting changed-symbol context.",
        ));
        return Vec::new();
    }
    prepare(state, source, SourcePreparation::Symbols)
}

fn prepare(
    state: &mut AppState,
    source: ActiveSource,
    preparation: SourcePreparation,
) -> Vec<AppEffect> {
    let symbol_return_overlay = if state.overlay == Overlay::FullFile {
        Overlay::FullFile
    } else {
        source.return_overlay
    };
    let full_file_return_overlay = state.full_file.return_overlay;
    let same_identity = state.full_file.identity.as_ref() == Some(&source.identity);
    state.full_file.identity = Some(source.identity.clone());
    state.full_file.cursor = source.cursor;
    state.full_file.desired_display_column = None;
    state.full_file.viewport_vertical = usize::try_from(source.cursor.line()).unwrap_or(usize::MAX);
    state.full_file.viewport_horizontal = 0;
    state.full_file.changed_lines = source.changed_lines;
    state.full_file.deleted_lines = source.deleted_lines;
    state.full_file.diff_context = source.diff_context;
    state.full_file.mode = if state.full_file.diff_context.has_diff() {
        FullFileMode::Changes
    } else {
        FullFileMode::New
    };
    state.full_file.return_overlay = source.return_overlay;
    state.search.clear();
    state.notice = None;
    if preparation.requests_symbols() {
        state.symbol_context.return_overlay = symbol_return_overlay;
        state.symbol_context.full_file_return_overlay = full_file_return_overlay;
    }

    if let Some(document) = source.loaded {
        state.full_file.document_revision.advance();
        state.full_file.content = LoadState::Ready(document);
        clamp_cursor(state);
        return if preparation.requests_symbols() {
            crate::app::semantic_navigation::request_document_symbols(state)
        } else {
            state.overlay = Overlay::FullFile;
            Vec::new()
        };
    }

    // A path identifies a mutable worktree file, not a source snapshot. Only
    // commit contents remain authoritative when reopening from a diff.
    if same_identity
        && matches!(source.identity.revision, FileRevision::Commit(_))
        && matches!(state.full_file.content, LoadState::Ready(_))
    {
        clamp_cursor(state);
        return if preparation.requests_symbols() {
            crate::app::semantic_navigation::request_document_symbols(state)
        } else {
            state.overlay = Overlay::FullFile;
            Vec::new()
        };
    }

    let request_id = state.request_id();
    state.full_file.document_revision.advance();
    state.full_file.content = LoadState::Loading { request_id };
    if preparation.requests_symbols() {
        state.symbol_context.symbols = LoadState::Loading { request_id };
        state.symbol_context.selection.reset(1);
        state.symbol_context.status = Some("loading complete source".to_owned());
        state.symbol_context.pending_source_request = Some(request_id);
        state.overlay = Overlay::SymbolContext;
    } else {
        state.overlay = Overlay::FullFile;
    }
    vec![AppEffect::Git(GitEffect::LoadFullFile {
        request_id,
        revision: source.identity.revision,
        path: source.identity.path,
    })]
}

fn active_source(state: &AppState) -> Option<ActiveSource> {
    if state.overlay == Overlay::FullFile {
        return state
            .full_file
            .identity
            .clone()
            .map(|identity| ActiveSource {
                identity,
                cursor: state.full_file.cursor,
                changed_lines: state.full_file.changed_lines.clone(),
                deleted_lines: state.full_file.deleted_lines.clone(),
                diff_context: state.full_file.diff_context,
                loaded: match &state.full_file.content {
                    LoadState::Ready(document) => Some(document.clone()),
                    _ => None,
                },
                return_overlay: state.full_file.return_overlay,
            });
    }
    if state.overlay == Overlay::Diff
        || (state.overlay == Overlay::None
            && state.focus == FocusedPane::Diff
            && !state.history_message_focused()
            && state.view != AppView::Code
            && (state.view != AppView::FileHistory || state.file_view.mode.shows_history_diff()))
    {
        return diff_source(state, state.overlay);
    }
    if state.overlay == Overlay::CodeContent
        || (state.overlay == Overlay::None
            && state.view == AppView::Code
            && state.focus == FocusedPane::Diff)
    {
        let path = state.code_view.path.clone()?;
        return Some(ActiveSource {
            identity: SourceFileIdentity {
                revision: FileRevision::WorkingTree,
                path,
            },
            cursor: state.code_view.cursor,
            changed_lines: BTreeSet::new(),
            deleted_lines: Vec::new(),
            diff_context: SourceDiffContext::None,
            loaded: match &state.code_view.content {
                LoadState::Ready(document) => Some(document.clone()),
                _ => None,
            },
            return_overlay: state.overlay,
        });
    }
    if state.overlay == Overlay::FileContent
        || (state.overlay == Overlay::None
            && state.view == AppView::FileHistory
            && state.focus == FocusedPane::Diff
            && !state.file_view.mode.shows_history_diff())
    {
        let path = state.file_view.path.clone()?;
        return Some(ActiveSource {
            identity: SourceFileIdentity {
                revision: FileRevision::WorkingTree,
                path,
            },
            cursor: SourcePosition::new(
                u32::try_from(state.file_view.vertical).unwrap_or(u32::MAX),
                state.file_view.byte_column,
            ),
            changed_lines: BTreeSet::new(),
            deleted_lines: Vec::new(),
            diff_context: SourceDiffContext::None,
            loaded: match &state.file_view.content {
                LoadState::Ready(document) => Some(document.clone()),
                _ => None,
            },
            return_overlay: state.overlay,
        });
    }
    None
}

fn diff_source(state: &AppState, return_overlay: Overlay) -> Option<ActiveSource> {
    let target = state.diff.target.as_ref()?;
    let (revision, path) = match target {
        DiffTarget::Worktree { path, .. } => (FileRevision::WorkingTree, path.clone()),
        DiffTarget::Commit { commit, path, .. } => {
            (FileRevision::Commit(commit.clone()), path.clone())
        }
    };
    let diff_context = if matches!(state.diff.content, LoadState::Ready(_)) {
        SourceDiffContext::Ready
    } else {
        SourceDiffContext::Pending
    };
    let (changed_lines, deleted_lines) = match &state.diff.content {
        LoadState::Ready(document) => diff_decorations(document.lines()),
        _ => (BTreeSet::new(), Vec::new()),
    };
    let line = diff_new_line(state).unwrap_or(0);
    Some(ActiveSource {
        identity: SourceFileIdentity { revision, path },
        cursor: SourcePosition::new(line, 0),
        changed_lines,
        deleted_lines,
        diff_context,
        loaded: None,
        return_overlay,
    })
}

fn diff_decorations(lines: &[crate::domain::DiffLine]) -> (BTreeSet<u32>, Vec<FullFileDeletion>) {
    let mut changed_lines = BTreeSet::new();
    let mut deleted_lines = Vec::new();
    let mut pending = Vec::new();
    let mut insertion_anchor = 0;

    for line in lines {
        match line.kind() {
            DiffLineKind::Added | DiffLineKind::Context => {
                let Some(new_line) = line.new_line() else {
                    continue;
                };
                let anchor = new_line.value().saturating_sub(1);
                flush_deletions(&mut pending, &mut deleted_lines, anchor);
                insertion_anchor = new_line.value();
                if line.kind() == DiffLineKind::Added {
                    changed_lines.insert(anchor);
                }
            }
            DiffLineKind::Removed => pending.push(FullFileDeletion {
                anchor: 0,
                old_line: line.old_line().map(|line| line.value()),
                content: line
                    .text()
                    .strip_prefix('-')
                    .unwrap_or(line.text())
                    .to_owned(),
            }),
            DiffLineKind::Hunk => {
                flush_deletions(&mut pending, &mut deleted_lines, insertion_anchor);
                if let Some(anchor) = hunk_new_anchor(line.text()) {
                    insertion_anchor = anchor;
                }
            }
            DiffLineKind::Header | DiffLineKind::Meta => {}
        }
    }
    flush_deletions(&mut pending, &mut deleted_lines, insertion_anchor);
    deleted_lines.sort_by_key(|line| line.anchor);
    (changed_lines, deleted_lines)
}

fn flush_deletions(
    pending: &mut Vec<FullFileDeletion>,
    deleted_lines: &mut Vec<FullFileDeletion>,
    anchor: u32,
) {
    for mut line in pending.drain(..) {
        line.anchor = anchor;
        deleted_lines.push(line);
    }
}

fn hunk_new_anchor(header: &str) -> Option<u32> {
    let range = header
        .split_whitespace()
        .find(|field| field.starts_with('+'))?
        .strip_prefix('+')?;
    let mut fields = range.split(',');
    let start = fields.next()?.parse::<u32>().ok()?;
    let count = fields
        .next()
        .map(str::parse::<u32>)
        .transpose()
        .ok()?
        .unwrap_or(1);
    Some(if count == 0 {
        start
    } else {
        start.saturating_sub(1)
    })
}

fn diff_new_line(state: &AppState) -> Option<u32> {
    let LoadState::Ready(document) = &state.diff.content else {
        return None;
    };
    let lines = document.lines();
    lines
        .get(state.diff.vertical)
        .and_then(|line| line.new_line())
        .or_else(|| {
            lines
                .iter()
                .skip(state.diff.vertical.saturating_add(1))
                .find_map(|line| line.new_line())
        })
        .or_else(|| {
            lines
                .iter()
                .take(state.diff.vertical)
                .rev()
                .find_map(|line| line.new_line())
        })
        .map(|line| line.value().saturating_sub(1))
}

pub(crate) fn overlay_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    match action {
        Action::CloseOverlay => {
            state.overlay = state.full_file.return_overlay;
            state.search.clear();
        }
        Action::Activate => move_cursor(state, 1),
        Action::MoveUp => move_cursor(state, -1),
        Action::MoveDown => move_cursor(state, 1),
        Action::MoveTop => set_cursor_line(state, 0, CursorColumnPolicy::Preserve),
        Action::MoveBottom => {
            set_cursor_line(state, last_line(state), CursorColumnPolicy::Preserve);
        }
        Action::HalfPageUp => move_cursor(state, -FALLBACK_HALF_PAGE_LINES),
        Action::HalfPageDown => move_cursor(state, FALLBACK_HALF_PAGE_LINES),
        Action::ScrollLeft => {
            state.full_file.viewport_horizontal = state
                .full_file
                .viewport_horizontal
                .saturating_sub(HORIZONTAL_SCROLL_COLUMNS);
        }
        Action::ScrollRight => {
            state.full_file.viewport_horizontal = state
                .full_file
                .viewport_horizontal
                .saturating_add(HORIZONTAL_SCROLL_COLUMNS);
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn apply_vim_motion(
    state: &mut AppState,
    motion: VimMotion,
    viewport_height: usize,
    viewport_width: usize,
) {
    const TRUNCATED: &str = "… file truncated at the safe output limit …";
    let (position, viewport) = {
        let mut lines = match &state.full_file.content {
            LoadState::Ready(document) if document.message().is_none() => document
                .lines()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            LoadState::Ready(document) => document.message().into_iter().collect(),
            _ => Vec::new(),
        };
        if matches!(&state.full_file.content, LoadState::Ready(document) if document.is_truncated())
        {
            lines.push(TRUNCATED);
        }
        let mut viewport = crate::app::vim::Viewport::new(
            state.full_file.viewport_vertical,
            state.full_file.viewport_horizontal,
            viewport_height,
            viewport_width,
            SOURCE_GUTTER_COLUMNS,
        )
        .with_desired_column(state.full_file.desired_display_column);
        let position =
            crate::app::vim::apply(&lines, state.full_file.cursor, &mut viewport, motion);
        (position, viewport)
    };
    state.full_file.cursor = position;
    state.full_file.desired_display_column = viewport.desired_column;
    state.full_file.viewport_vertical = viewport.top;
    state.full_file.viewport_horizontal = viewport.left;
}

pub(crate) fn move_cursor_horizontally(state: &mut AppState, direction: HorizontalDirection) {
    let line_index = usize::try_from(state.full_file.cursor.line()).unwrap_or(usize::MAX);
    let Some(line) = current_lines(state).and_then(|lines| lines.get(line_index)) else {
        return;
    };
    let column = if direction.is_right() {
        crate::lsp::next_byte_column(line, state.full_file.cursor.byte_column())
    } else {
        crate::lsp::previous_byte_column(line, state.full_file.cursor.byte_column())
    };
    state.full_file.cursor = SourcePosition::new(state.full_file.cursor.line(), column);
}

fn move_cursor(state: &mut AppState, delta: isize) {
    let current = usize::try_from(state.full_file.cursor.line()).unwrap_or(usize::MAX);
    set_cursor_line(
        state,
        current.saturating_add_signed(delta),
        CursorColumnPolicy::Preserve,
    );
}

fn set_cursor_line(state: &mut AppState, line: usize, column_policy: CursorColumnPolicy) {
    let line = line.min(last_line(state));
    let byte_column = if column_policy.resets_column() {
        0
    } else {
        let requested = state.full_file.cursor.byte_column();
        current_lines(state)
            .and_then(|lines| lines.get(line))
            .map_or(0, |content| clamp_byte_column(content, requested))
    };
    state.full_file.cursor =
        SourcePosition::new(u32::try_from(line).unwrap_or(u32::MAX), byte_column);
}

fn clamp_cursor(state: &mut AppState) {
    let line = usize::try_from(state.full_file.cursor.line())
        .unwrap_or(usize::MAX)
        .min(last_line(state));
    set_cursor_line(state, line, CursorColumnPolicy::Preserve);
}

fn last_line(state: &AppState) -> usize {
    match &state.full_file.content {
        LoadState::Ready(document) if document.message().is_some() => 0,
        LoadState::Ready(document) => document
            .lines()
            .len()
            .saturating_add(usize::from(document.is_truncated()))
            .saturating_sub(1),
        _ => 0,
    }
}

fn current_lines(state: &AppState) -> Option<&[String]> {
    match &state.full_file.content {
        LoadState::Ready(document) if document.message().is_none() => Some(document.lines()),
        _ => None,
    }
}

fn clamp_byte_column(line: &str, requested: usize) -> usize {
    let mut column = requested.min(line.len());
    while !line.is_char_boundary(column) {
        column = column.saturating_sub(1);
    }
    column
}

pub(crate) fn reveal_symbol(state: &mut AppState, position: SourcePosition) {
    state.full_file.cursor = position;
    clamp_cursor(state);
    state.full_file.viewport_vertical = usize::try_from(state.full_file.cursor.line())
        .unwrap_or(usize::MAX)
        .saturating_sub(3);
    state.full_file.return_overlay = Overlay::SymbolContext;
    state.overlay = Overlay::FullFile;
    state.search.clear();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app::{
        Action, AppEffect, AppState, AppView, Event, FocusedPane, GitEffect, LoadState, LspEffect,
        Overlay,
    };
    use crate::domain::{
        CommitBaseline, DiffDocument, DiffLine, DiffLineKind, DiffTarget, DocumentSymbol,
        DocumentSymbolKind, FileDocument, FileRevision, LineNumber, ObjectId, RepoPath,
        RepositoryRoot, SourcePosition, SourceRange,
    };

    fn path() -> RepoPath {
        RepoPath::from_bytes(b"src/lib.rs".to_vec()).unwrap_or_else(|error| panic!("path: {error}"))
    }

    fn commit() -> ObjectId {
        ObjectId::parse("1111111111111111111111111111111111111111")
            .unwrap_or_else(|error| panic!("commit: {error}"))
    }

    #[test]
    fn history_message_does_not_expose_the_hidden_diff_as_source() {
        let mut state = diff_state();
        state.history_preview = crate::app::HistoryPreview::Message;
        assert!(super::active_source(&state).is_none());
        state.overlay = Overlay::Diff;
        assert!(super::active_source(&state).is_some());
    }

    fn diff_state() -> AppState {
        let root = RepositoryRoot::new(PathBuf::from("/tmp/repo"))
            .unwrap_or_else(|error| panic!("root: {error}"));
        let mut state = AppState::new(root, AppView::History);
        state.history_preview = crate::app::HistoryPreview::Diff;
        state.focus = FocusedPane::Diff;
        state.diff.target = Some(DiffTarget::Commit {
            commit: commit(),
            baseline: CommitBaseline::EmptyTree,
            path: path(),
        });
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: vec![
                DiffLine::new(DiffLineKind::Hunk, None, None, "@@ -1 +1,3 @@".to_owned()),
                DiffLine::new(
                    DiffLineKind::Context,
                    LineNumber::new(1),
                    LineNumber::new(1),
                    " fn first() {}".to_owned(),
                ),
                DiffLine::new(
                    DiffLineKind::Removed,
                    LineNumber::new(2),
                    None,
                    "-fn removed() {}".to_owned(),
                ),
                DiffLine::new(
                    DiffLineKind::Added,
                    None,
                    LineNumber::new(2),
                    "+fn changed() {}".to_owned(),
                ),
            ],
            bytes: 48,
        });
        state.diff.vertical = 2;
        state
    }

    fn document() -> FileDocument {
        FileDocument::exact_text("fn first() {}\nfn changed() {}\nfn last() {}\n")
    }

    #[test]
    fn reopening_worktree_source_reads_a_fresh_snapshot() {
        let mut state = diff_state();
        state.view = AppView::Changes;
        state.diff.target = Some(DiffTarget::Worktree {
            path: path(),
            kind: crate::domain::WorktreeDiffKind::Tracked,
        });
        let effects = state.handle_app_action(Action::OpenFullFile);
        let AppEffect::Git(GitEffect::LoadFullFile { request_id, .. }) = effects[0] else {
            panic!("expected full-file load");
        };
        state.handle_app_event(Event::FullFileLoaded {
            request_id,
            revision: FileRevision::WorkingTree,
            path: path(),
            result: Ok(document()),
        });
        state.handle_app_action(Action::CloseOverlay);
        let effects = state.handle_app_action(Action::OpenFullFile);
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Git(GitEffect::LoadFullFile {
                revision: FileRevision::WorkingTree,
                ..
            })]
        ));
    }

    #[test]
    fn commit_diff_opens_its_new_full_file_at_the_selected_diff_line() {
        let mut state = diff_state();
        let effects = state.handle_app_action(Action::OpenFullFile);
        assert_eq!(state.overlay, Overlay::FullFile);
        assert_eq!(state.full_file.cursor, SourcePosition::new(1, 0));
        assert!(state.full_file.changed_lines.contains(&1));
        assert_eq!(state.full_file.deleted_lines.len(), 1);
        assert_eq!(state.full_file.deleted_lines[0].anchor, 1);
        assert_eq!(state.full_file.deleted_lines[0].old_line, Some(2));
        assert_eq!(state.full_file.deleted_lines[0].content, "fn removed() {}");
        let request_id = match &effects[0] {
            AppEffect::Git(GitEffect::LoadFullFile {
                request_id,
                revision,
                path: requested_path,
            }) => {
                assert_eq!(revision, &FileRevision::Commit(commit()));
                assert_eq!(requested_path, &path());
                *request_id
            }
            other => panic!("unexpected effect: {other:?}"),
        };
        assert!(
            state
                .handle_app_event(Event::FullFileLoaded {
                    request_id,
                    revision: FileRevision::Commit(commit()),
                    path: path(),
                    result: Ok(document()),
                })
                .is_empty()
        );
        assert!(matches!(state.full_file.content, LoadState::Ready(_)));

        assert!(
            state
                .handle_app_action(Action::ToggleFullFileMode)
                .is_empty()
        );
        assert_eq!(state.full_file.mode, super::FullFileMode::New);
        assert!(state.handle_app_action(Action::CloseOverlay).is_empty());
        assert_eq!(state.overlay, Overlay::None);
        assert!(state.handle_app_action(Action::OpenFullFile).is_empty());
        assert!(matches!(state.full_file.content, LoadState::Ready(_)));
    }

    #[test]
    fn deletion_only_hunks_anchor_removed_lines_in_the_new_source() {
        let lines = vec![
            DiffLine::new(DiffLineKind::Hunk, None, None, "@@ -2 +1,0 @@".to_owned()),
            DiffLine::new(
                DiffLineKind::Removed,
                LineNumber::new(2),
                None,
                "-removed near the start".to_owned(),
            ),
            DiffLine::new(DiffLineKind::Hunk, None, None, "@@ -10 +9,0 @@".to_owned()),
            DiffLine::new(
                DiffLineKind::Removed,
                LineNumber::new(10),
                None,
                "-removed near the end".to_owned(),
            ),
        ];

        let (changed, deleted) = super::diff_decorations(&lines);

        assert!(changed.is_empty());
        assert_eq!(
            deleted
                .iter()
                .map(|line| (line.anchor, line.content.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "removed near the start"), (9, "removed near the end")]
        );
    }

    #[test]
    fn disabled_lsp_does_not_enter_symbol_context() {
        let mut state = diff_state();
        assert!(
            state
                .handle_app_action(Action::OpenSymbolContext)
                .is_empty()
        );
        assert_eq!(state.overlay, Overlay::None);
        assert!(
            state
                .notice
                .as_ref()
                .is_some_and(|notice| notice.message().contains("requires LSP"))
        );
    }

    #[test]
    fn source_actions_wait_for_a_diff_before_projecting_its_context() {
        let mut state = diff_state();
        let request_id = state.request_id();
        state.diff.content = LoadState::Loading { request_id };
        state.set_lsp_availability(crate::app::LspAvailability::Enabled);

        assert!(state.handle_app_action(Action::OpenFullFile).is_empty());
        assert_eq!(state.overlay, Overlay::None);
        assert!(
            state
                .handle_app_action(Action::OpenSymbolContext)
                .is_empty()
        );
        assert_eq!(state.overlay, Overlay::None);
        assert!(
            state
                .notice
                .as_ref()
                .is_some_and(|notice| notice.message().contains("finish loading"))
        );
    }

    #[test]
    fn closing_symbol_context_while_source_loads_does_not_reopen_it() {
        let mut state = diff_state();
        state.set_lsp_availability(crate::app::LspAvailability::Enabled);
        let effects = state.handle_app_action(Action::OpenSymbolContext);
        let request_id = match effects[0] {
            AppEffect::Git(GitEffect::LoadFullFile { request_id, .. }) => request_id,
            ref other => panic!("unexpected effect: {other:?}"),
        };

        assert!(state.handle_app_action(Action::CloseOverlay).is_empty());
        assert_eq!(state.overlay, Overlay::None);
        assert!(matches!(state.symbol_context.symbols, LoadState::Idle));
        assert!(
            state
                .handle_app_event(Event::FullFileLoaded {
                    request_id,
                    revision: FileRevision::Commit(commit()),
                    path: path(),
                    result: Ok(document()),
                })
                .is_empty()
        );
        assert_eq!(state.overlay, Overlay::None);
    }

    #[test]
    fn full_file_choice_during_source_load_stays_in_the_full_file() {
        let mut state = diff_state();
        state.set_lsp_availability(crate::app::LspAvailability::Enabled);
        let effects = state.handle_app_action(Action::OpenSymbolContext);
        let request_id = match effects[0] {
            AppEffect::Git(GitEffect::LoadFullFile { request_id, .. }) => request_id,
            ref other => panic!("unexpected effect: {other:?}"),
        };

        assert!(state.handle_app_action(Action::Activate).is_empty());
        assert_eq!(state.overlay, Overlay::FullFile);
        assert!(
            state
                .handle_app_event(Event::FullFileLoaded {
                    request_id,
                    revision: FileRevision::Commit(commit()),
                    path: path(),
                    result: Ok(document()),
                })
                .is_empty()
        );
        assert_eq!(state.overlay, Overlay::FullFile);
        assert!(matches!(state.full_file.content, LoadState::Ready(_)));
    }

    #[test]
    fn working_tree_file_view_can_open_symbols_without_a_diff() {
        let mut state = diff_state();
        state.view = AppView::FileHistory;
        state.focus = FocusedPane::Diff;
        state.diff.target = None;
        state.diff.content = LoadState::Idle;
        state.file_view.path = Some(path());
        state.file_view.content = LoadState::Ready(document());
        state.file_view.mode = crate::app::model::FileViewMode::CurrentContent;
        state.file_view.vertical = 2;
        state.set_lsp_availability(crate::app::LspAvailability::Enabled);

        let effects = state.handle_app_action(Action::OpenSymbolContext);
        assert_eq!(state.overlay, Overlay::SymbolContext);
        assert_eq!(state.full_file.cursor, SourcePosition::new(2, 0));
        assert_eq!(
            state
                .full_file
                .identity
                .as_ref()
                .map(|identity| &identity.revision),
            Some(&FileRevision::WorkingTree)
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Lsp(LspEffect::DocumentSymbols { .. })]
        ));
    }

    #[test]
    fn code_view_can_open_symbols_and_jump_to_complete_source() {
        let mut state = diff_state();
        state.view = AppView::Code;
        state.focus = FocusedPane::Diff;
        state.diff.target = None;
        state.diff.content = LoadState::Idle;
        state.code_view.path = Some(path());
        state.code_view.content = LoadState::Ready(document());
        state.code_view.cursor = SourcePosition::new(0, 3);
        state.set_lsp_availability(crate::app::LspAvailability::Enabled);

        let effects = state.handle_app_action(Action::OpenSymbolContext);
        let (request_id, document_revision) = match effects[0] {
            AppEffect::Lsp(LspEffect::DocumentSymbols {
                request_id,
                document_revision,
                ..
            }) => (request_id, document_revision),
            ref other => panic!("unexpected effect: {other:?}"),
        };
        state.handle_app_event(Event::DocumentSymbolsCompleted {
            request_id,
            path: path(),
            document_revision,
            result: Ok(vec![DocumentSymbol::new(
                "last".to_owned(),
                None,
                DocumentSymbolKind::Function,
                SourceRange::new(SourcePosition::new(2, 0), SourcePosition::new(2, 12)),
                SourcePosition::new(2, 3),
                0,
            )]),
        });
        state.handle_app_action(Action::MoveDown);
        state.handle_app_action(Action::Activate);
        assert_eq!(state.overlay, Overlay::FullFile);
        assert_eq!(state.full_file.cursor, SourcePosition::new(2, 3));
        assert_eq!(state.full_file.mode, super::FullFileMode::New);
    }

    #[test]
    fn commit_symbols_filter_to_changed_context_and_jump_in_the_full_file() {
        let mut state = diff_state();
        state.set_lsp_availability(crate::app::LspAvailability::Enabled);
        let effects = state.handle_app_action(Action::OpenSymbolContext);
        assert_eq!(state.overlay, Overlay::SymbolContext);
        let request_id = match effects[0] {
            AppEffect::Git(GitEffect::LoadFullFile { request_id, .. }) => request_id,
            ref other => panic!("unexpected effect: {other:?}"),
        };
        let effects = state.handle_app_event(Event::FullFileLoaded {
            request_id,
            revision: FileRevision::Commit(commit()),
            path: path(),
            result: Ok(document()),
        });
        let (symbol_request, revision) = match effects[0] {
            AppEffect::Lsp(LspEffect::DocumentSymbols {
                request_id,
                document_revision,
                ..
            }) => (request_id, document_revision),
            ref other => panic!("unexpected effect: {other:?}"),
        };
        assert!(
            state
                .handle_app_event(Event::DocumentSymbolsCompleted {
                    request_id: symbol_request,
                    path: path(),
                    document_revision: revision,
                    result: Ok(vec![
                        DocumentSymbol::new(
                            "first".to_owned(),
                            None,
                            DocumentSymbolKind::Function,
                            SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(0, 13)),
                            SourcePosition::new(0, 3),
                            0,
                        ),
                        DocumentSymbol::new(
                            "previous_multiline".to_owned(),
                            None,
                            DocumentSymbolKind::Function,
                            SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(1, 0)),
                            SourcePosition::new(0, 3),
                            0,
                        ),
                        DocumentSymbol::new(
                            "changed".to_owned(),
                            None,
                            DocumentSymbolKind::Function,
                            SourceRange::new(SourcePosition::new(1, 0), SourcePosition::new(1, 15)),
                            SourcePosition::new(1, 3),
                            0,
                        ),
                    ]),
                })
                .is_empty()
        );
        let LoadState::Ready(symbols) = &state.symbol_context.symbols else {
            panic!("symbols were not ready");
        };
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name(), "changed");

        assert!(state.handle_app_action(Action::MoveDown).is_empty());
        assert!(state.handle_app_action(Action::Activate).is_empty());
        assert_eq!(state.overlay, Overlay::FullFile);
        assert_eq!(state.full_file.cursor, SourcePosition::new(1, 3));
        assert_eq!(state.full_file.return_overlay, Overlay::SymbolContext);
    }
}
