//! Working-tree code-viewer state transitions and tree projection.

use std::collections::BTreeMap;

use crate::app::{
    Action, AppState, CodeEntryKind, ErrorNotice, GitEffect, LoadState, Overlay, VisibleCodeEntry,
};
use crate::domain::{FileDocument, RepoPath, SourcePosition};
use crate::git::GitError;

pub(crate) fn request_tree(state: &mut AppState) -> Vec<GitEffect> {
    let request_id = state.request_id();
    state.code_view.visible = LoadState::Loading { request_id };
    state.code_view.selection.reset(0);
    state.code_view.files.clear();
    state.code_view.path = None;
    state.code_view.content = LoadState::Idle;
    state.code_view.cursor = SourcePosition::new(0, 0);
    state.code_view.viewport_vertical = 0;
    state.code_view.viewport_horizontal = 0;
    state.code_view.pending_reveal = None;
    state.code_view.document_revision = state.code_view.document_revision.saturating_add(1);
    state.search.clear();
    state.notice = None;
    vec![GitEffect::LoadCodeTree { request_id }]
}

pub(crate) fn tree_loaded(
    state: &mut AppState,
    result: Result<Vec<RepoPath>, GitError>,
) -> Vec<GitEffect> {
    match result {
        Ok(files) => {
            let visible = direct_children(&files, None, 0);
            state.code_view.selection.reset(visible.len());
            state.code_view.files = files;
            state.code_view.visible = LoadState::Ready(visible);
            if let Some(path) = state.code_view.pending_reveal.take() {
                reveal_path(state, &path);
                Vec::new()
            } else {
                preview_selected_file(state)
            }
        }
        Err(error) => {
            state.code_view.files.clear();
            state.code_view.visible = LoadState::Failed(ErrorNotice::new(error.to_string()));
            Vec::new()
        }
    }
}

pub(crate) fn file_loaded(
    state: &mut AppState,
    result: Result<FileDocument, GitError>,
) -> Vec<GitEffect> {
    state.code_view.content = match result {
        Ok(document) => LoadState::Ready(document),
        Err(error) => LoadState::Failed(ErrorNotice::new(error.to_string())),
    };
    clamp_cursor(state);
    Vec::new()
}

pub(crate) fn move_selection(state: &mut AppState, delta: isize) -> Vec<GitEffect> {
    let changed = match &state.code_view.visible {
        LoadState::Ready(entries) => state.code_view.selection.move_by(delta, entries.len()),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => false,
    };
    if changed {
        preview_selected_file(state)
    } else {
        Vec::new()
    }
}

pub(crate) fn move_to_edge(state: &mut AppState, bottom: bool) -> Vec<GitEffect> {
    let changed = match &state.code_view.visible {
        LoadState::Ready(entries) if bottom => state.code_view.selection.bottom(entries.len()),
        LoadState::Ready(entries) => state.code_view.selection.top(entries.len()),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => false,
    };
    if changed {
        preview_selected_file(state)
    } else {
        Vec::new()
    }
}

pub(crate) fn activate_tree(state: &mut AppState) -> Vec<GitEffect> {
    let Some(index) = state.code_view.selection.index() else {
        return Vec::new();
    };
    let selected = match &state.code_view.visible {
        LoadState::Ready(entries) => entries.get(index).cloned(),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
    };
    let Some(selected) = selected else {
        return Vec::new();
    };
    if selected.kind() == CodeEntryKind::File {
        let effects = if state.code_view.path.as_ref() == Some(selected.path()) {
            Vec::new()
        } else {
            load_file(state, selected.path().clone(), SourcePosition::new(0, 0))
        };
        state.overlay = Overlay::CodeContent;
        return effects;
    }
    if selected.expanded() {
        collapse(state, index);
    } else {
        expand(state, index, &selected);
    }
    Vec::new()
}

pub(crate) fn open_content(state: &mut AppState) {
    if state.code_view.path.is_some() {
        state.search.clear();
        state.overlay = Overlay::CodeContent;
    }
}

pub(crate) fn content_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    match action {
        Action::CloseOverlay | Action::Activate => {
            state.overlay = Overlay::None;
            state.search.clear();
        }
        Action::MoveUp => move_content_cursor(state, -1),
        Action::MoveDown => move_content_cursor(state, 1),
        Action::MoveTop => set_cursor_line(state, 0, false),
        Action::MoveBottom => set_cursor_line(state, last_line(state), false),
        Action::HalfPageUp => move_content_cursor(state, -10),
        Action::HalfPageDown => move_content_cursor(state, 10),
        Action::ScrollLeft => {
            state.code_view.viewport_horizontal =
                state.code_view.viewport_horizontal.saturating_sub(4);
        }
        Action::ScrollRight => {
            state.code_view.viewport_horizontal =
                state.code_view.viewport_horizontal.saturating_add(4);
        }
        Action::StartSearch(direction) => state.search.begin(direction),
        Action::InsertSearch(character) => state.search.push(character),
        Action::DeleteSearch => state.search.pop(),
        Action::ConfirmSearch => confirm_search(state),
        Action::CancelSearch => state.search.cancel_input(),
        Action::NextMatch => {
            let direction = state.search.direction();
            if let Some(line) = state.search.select_next(direction) {
                set_cursor_line(state, line, true);
            }
        }
        Action::PreviousMatch => {
            let direction = state.search.direction().reversed();
            if let Some(line) = state.search.select_next(direction) {
                set_cursor_line(state, line, true);
            }
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn move_content_cursor(state: &mut AppState, delta: isize) {
    let current = usize::try_from(state.code_view.cursor.line()).unwrap_or(usize::MAX);
    let next = current.saturating_add_signed(delta);
    let line = match &state.code_view.content {
        LoadState::Loading { .. } => next,
        LoadState::Ready(_) => next.min(last_line(state)),
        LoadState::Idle | LoadState::Failed(_) => 0,
    };
    set_cursor_line(state, line, false);
}

pub(crate) fn last_line(state: &AppState) -> usize {
    match &state.code_view.content {
        LoadState::Ready(document) if document.message().is_some() => 0,
        LoadState::Ready(document) => document
            .lines()
            .len()
            .saturating_add(usize::from(document.is_truncated()))
            .saturating_sub(1),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => 0,
    }
}

pub(crate) fn reveal_and_load(
    state: &mut AppState,
    path: RepoPath,
    line: Option<u32>,
) -> Vec<GitEffect> {
    if matches!(state.code_view.visible, LoadState::Loading { .. }) {
        state.code_view.pending_reveal = Some(path.clone());
    } else {
        reveal_path(state, &path);
    }
    state.focus = crate::app::FocusedPane::Diff;
    load_file(
        state,
        path,
        SourcePosition::new(line.unwrap_or(1).saturating_sub(1), 0),
    )
}

pub(crate) fn reveal_location(
    state: &mut AppState,
    path: RepoPath,
    position: SourcePosition,
) -> Vec<GitEffect> {
    if matches!(state.code_view.visible, LoadState::Loading { .. }) {
        state.code_view.pending_reveal = Some(path.clone());
    } else {
        reveal_path(state, &path);
    }
    state.focus = crate::app::FocusedPane::Diff;
    state.overlay = Overlay::CodeContent;
    load_file(state, path, position)
}

fn preview_selected_file(state: &mut AppState) -> Vec<GitEffect> {
    let path = match (&state.code_view.visible, state.code_view.selection.index()) {
        (LoadState::Ready(entries), Some(index)) => entries
            .get(index)
            .filter(|entry| entry.kind() == CodeEntryKind::File)
            .map(|entry| entry.path().clone()),
        _ => None,
    };
    path.map(|path| load_file(state, path, SourcePosition::new(0, 0)))
        .unwrap_or_default()
}

fn load_file(state: &mut AppState, path: RepoPath, cursor: SourcePosition) -> Vec<GitEffect> {
    let request_id = state.request_id();
    state.code_view.path = Some(path.clone());
    state.code_view.content = LoadState::Loading { request_id };
    state.code_view.document_revision = state.code_view.document_revision.saturating_add(1);
    state.code_view.cursor = cursor;
    state.code_view.viewport_vertical = usize::try_from(cursor.line()).unwrap_or(usize::MAX);
    state.code_view.viewport_horizontal = 0;
    state.search.clear();
    vec![GitEffect::LoadCodeFile { request_id, path }]
}

fn confirm_search(state: &mut AppState) {
    let anchor = usize::try_from(state.code_view.cursor.line()).unwrap_or(usize::MAX);
    let line = match &state.code_view.content {
        LoadState::Ready(document) if document.message().is_some() => {
            state.search.confirm(document.message(), anchor)
        }
        LoadState::Ready(document) => state
            .search
            .confirm(document.lines().iter().map(String::as_str), anchor),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => {
            state.search.cancel_input();
            None
        }
    };
    if let Some(line) = line {
        set_cursor_line(state, line, true);
    }
}

pub(crate) fn move_cursor_horizontally(state: &mut AppState, right: bool) {
    let line_index = usize::try_from(state.code_view.cursor.line()).unwrap_or(usize::MAX);
    let Some(line) = current_lines(state)
        .and_then(|lines| lines.get(line_index))
        .cloned()
    else {
        state.code_view.cursor = SourcePosition::new(state.code_view.cursor.line(), 0);
        return;
    };
    let column = if right {
        crate::lsp::next_byte_column(&line, state.code_view.cursor.byte_column())
    } else {
        crate::lsp::previous_byte_column(&line, state.code_view.cursor.byte_column())
    };
    state.code_view.cursor = SourcePosition::new(state.code_view.cursor.line(), column);
    let display = crate::lsp::display_column(&line, column);
    if display < state.code_view.viewport_horizontal {
        state.code_view.viewport_horizontal = display;
    }
}

fn set_cursor_line(state: &mut AppState, line: usize, reset_column: bool) {
    let line = line.min(last_line(state));
    let byte_column = if reset_column {
        0
    } else {
        let requested = state.code_view.cursor.byte_column();
        current_lines(state)
            .and_then(|lines| lines.get(line))
            .map_or(0, |content| clamp_byte_column(content, requested))
    };
    state.code_view.cursor =
        SourcePosition::new(u32::try_from(line).unwrap_or(u32::MAX), byte_column);
    if line < state.code_view.viewport_vertical {
        state.code_view.viewport_vertical = line;
    }
}

fn clamp_cursor(state: &mut AppState) {
    let line = usize::try_from(state.code_view.cursor.line())
        .unwrap_or(usize::MAX)
        .min(last_line(state));
    set_cursor_line(state, line, false);
}

fn current_lines(state: &AppState) -> Option<&[String]> {
    match &state.code_view.content {
        LoadState::Ready(document) if document.message().is_none() => Some(document.lines()),
        LoadState::Idle
        | LoadState::Loading { .. }
        | LoadState::Ready(_)
        | LoadState::Failed(_) => None,
    }
}

fn clamp_byte_column(line: &str, requested: usize) -> usize {
    let mut column = requested.min(line.len());
    while !line.is_char_boundary(column) {
        column = column.saturating_sub(1);
    }
    column
}

fn expand(state: &mut AppState, index: usize, selected: &VisibleCodeEntry) {
    let children = direct_children(
        &state.code_view.files,
        Some(selected.path()),
        selected.depth().saturating_add(1),
    );
    let LoadState::Ready(entries) = &mut state.code_view.visible else {
        return;
    };
    if let Some(entry) = entries.get_mut(index) {
        entry.set_expanded(true);
    }
    entries.splice(index + 1..index + 1, children);
}

fn collapse(state: &mut AppState, index: usize) {
    let LoadState::Ready(entries) = &mut state.code_view.visible else {
        return;
    };
    let Some(depth) = entries.get(index).map(VisibleCodeEntry::depth) else {
        return;
    };
    if let Some(entry) = entries.get_mut(index) {
        entry.set_expanded(false);
    }
    let end = entries[index + 1..]
        .iter()
        .position(|entry| entry.depth() <= depth)
        .map_or(entries.len(), |offset| index + 1 + offset);
    entries.drain(index + 1..end);
    state.code_view.selection.clamp(entries.len());
}

fn reveal_path(state: &mut AppState, path: &RepoPath) {
    let roots = direct_children(&state.code_view.files, None, 0);
    state.code_view.visible = LoadState::Ready(roots);
    let mut component_end = 0;
    let bytes = path.as_bytes();
    while component_end < bytes.len() {
        component_end = bytes[component_end..]
            .iter()
            .position(|byte| *byte == b'/')
            .map_or(bytes.len(), |offset| component_end + offset);
        let Ok(component_path) = RepoPath::from_bytes(bytes[..component_end].to_vec()) else {
            return;
        };
        let index = match &state.code_view.visible {
            LoadState::Ready(entries) => entries
                .iter()
                .position(|entry| entry.path() == &component_path),
            _ => None,
        };
        let Some(index) = index else {
            return;
        };
        if component_end == bytes.len() {
            let len = match &state.code_view.visible {
                LoadState::Ready(entries) => entries.len(),
                _ => 0,
            };
            state.code_view.selection.reset_to(len, Some(index));
            return;
        }
        let selected = match &state.code_view.visible {
            LoadState::Ready(entries) => entries.get(index).cloned(),
            _ => None,
        };
        if let Some(selected) = selected {
            expand(state, index, &selected);
        }
        component_end = component_end.saturating_add(1);
    }
}

fn direct_children(
    files: &[RepoPath],
    parent: Option<&RepoPath>,
    depth: usize,
) -> Vec<VisibleCodeEntry> {
    let prefix = parent.map(RepoPath::as_bytes).unwrap_or_default();
    let mut children = BTreeMap::<Vec<u8>, CodeEntryKind>::new();
    for file in files {
        let bytes = file.as_bytes();
        let remainder = if prefix.is_empty() {
            bytes
        } else if bytes.starts_with(prefix)
            && bytes.get(prefix.len()) == Some(&b'/')
            && bytes.len() > prefix.len().saturating_add(1)
        {
            &bytes[prefix.len() + 1..]
        } else {
            continue;
        };
        let separator = remainder.iter().position(|byte| *byte == b'/');
        let name = separator.map_or(remainder, |index| &remainder[..index]);
        let kind = if separator.is_some() {
            CodeEntryKind::Directory
        } else {
            CodeEntryKind::File
        };
        children
            .entry(name.to_vec())
            .and_modify(|existing| {
                if kind == CodeEntryKind::Directory {
                    *existing = kind;
                }
            })
            .or_insert(kind);
    }
    let mut entries = children
        .into_iter()
        .filter_map(|(bytes, kind)| {
            let name = RepoPath::from_bytes(bytes).ok()?;
            let path = parent.map_or_else(|| name.clone(), |parent| parent.join(&name));
            Some(VisibleCodeEntry::new(path, name, depth, kind))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_is_file = left.kind() == CodeEntryKind::File;
        let right_is_file = right.kind() == CodeEntryKind::File;
        left_is_file
            .cmp(&right_is_file)
            .then_with(|| left.name().as_bytes().cmp(right.name().as_bytes()))
    });
    entries
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{direct_children, request_tree, reveal_and_load, tree_loaded};
    use crate::app::{AppState, AppView, CodeEntryKind, LoadState};
    use crate::domain::{RepoPath, RepositoryRoot};

    #[test]
    fn projects_direct_children_with_directories_before_files() {
        let files = [
            path("Cargo.toml"),
            path("README.md"),
            path("docs/guide.md"),
            path("src/commands/run.rs"),
            path("src/domain/path.rs"),
            path("src/lib.rs"),
            path("src/main.rs"),
        ];
        let roots = direct_children(&files, None, 0);
        assert_eq!(roots.len(), 4);
        assert_eq!(roots[0].path().display(), "docs");
        assert_eq!(roots[0].kind(), CodeEntryKind::Directory);
        assert_eq!(roots[1].path().display(), "src");
        assert_eq!(roots[1].kind(), CodeEntryKind::Directory);
        assert_eq!(roots[2].path().display(), "Cargo.toml");
        assert_eq!(roots[2].kind(), CodeEntryKind::File);
        assert_eq!(roots[3].path().display(), "README.md");

        let children = direct_children(&files, Some(roots[1].path()), 1);
        assert_eq!(children.len(), 4);
        assert_eq!(children[0].path().display(), "src/commands");
        assert_eq!(children[0].kind(), CodeEntryKind::Directory);
        assert_eq!(children[1].path().display(), "src/domain");
        assert_eq!(children[1].kind(), CodeEntryKind::Directory);
        assert_eq!(children[2].path().display(), "src/lib.rs");
        assert_eq!(children[2].kind(), CodeEntryKind::File);
        assert_eq!(children[3].path().display(), "src/main.rs");
    }

    #[test]
    fn defers_revealing_a_search_result_until_the_tree_finishes_loading() {
        let root = RepositoryRoot::new(PathBuf::from("/tmp/repo"))
            .unwrap_or_else(|error| panic!("invalid fixture root: {error}"));
        let mut state = AppState::new(root, AppView::Code);
        let _loading = request_tree(&mut state);
        let target = path("src/app/model.rs");

        let _file = reveal_and_load(&mut state, target.clone(), Some(3));
        assert!(matches!(state.code_view.visible, LoadState::Loading { .. }));
        assert_eq!(state.code_view.pending_reveal.as_ref(), Some(&target));

        let follow_up = tree_loaded(&mut state, Ok(vec![path("README.md"), target.clone()]));
        assert!(follow_up.is_empty());
        assert!(state.code_view.pending_reveal.is_none());
        assert!(matches!(
            &state.code_view.visible,
            LoadState::Ready(entries)
                if state.code_view.selection.index()
                    .and_then(|index| entries.get(index))
                    .is_some_and(|entry| entry.path() == &target)
        ));
        assert_eq!(state.code_view.cursor.line(), 2);
    }

    fn path(value: &str) -> RepoPath {
        RepoPath::from_bytes(value.as_bytes().to_vec())
            .unwrap_or_else(|error| panic!("invalid fixture path: {error}"))
    }
}
