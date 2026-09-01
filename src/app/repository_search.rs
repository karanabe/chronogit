use crate::app::{
    Action, AppState, AppView, FocusedPane, GitEffect, LoadState, Overlay, RepositorySearchKind,
};
use crate::domain::{DiffTarget, RepoPath};

pub(crate) fn open(state: &mut AppState, kind: RepositorySearchKind) {
    let return_view = if state.view == AppView::FileHistory {
        state.file_view.return_view
    } else {
        state.view
    };
    state.repository_search.kind = kind;
    state.repository_search.prompt = Some(String::new());
    state.repository_search.query.clear();
    state.repository_search.results = LoadState::Idle;
    state.repository_search.selection.reset(0);
    state.repository_search.return_view = return_view;
    state.overlay = Overlay::RepositorySearch;
}

pub(crate) fn overlay_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    if state.repository_search.prompt.is_some() {
        match action {
            Action::InsertSearch(character) => {
                if let Some(prompt) = &mut state.repository_search.prompt {
                    prompt.push(character);
                }
            }
            Action::DeleteSearch => {
                if let Some(prompt) = &mut state.repository_search.prompt {
                    prompt.pop();
                }
            }
            Action::ConfirmSearch => return confirm(state),
            Action::CancelSearch | Action::CloseOverlay => {
                state.overlay = Overlay::None;
                state.repository_search.prompt = None;
            }
            _ => {}
        }
        return Vec::new();
    }
    match action {
        Action::MoveUp => {
            if let LoadState::Ready(results) = &state.repository_search.results {
                state.repository_search.selection.move_by(-1, results.len());
            }
            Vec::new()
        }
        Action::MoveDown => {
            if let LoadState::Ready(results) = &state.repository_search.results {
                state.repository_search.selection.move_by(1, results.len());
            }
            Vec::new()
        }
        Action::MoveTop => {
            if let LoadState::Ready(results) = &state.repository_search.results {
                state.repository_search.selection.top(results.len());
            }
            Vec::new()
        }
        Action::MoveBottom => {
            if let LoadState::Ready(results) = &state.repository_search.results {
                state.repository_search.selection.bottom(results.len());
            }
            Vec::new()
        }
        Action::Activate => open_selected_file(state),
        Action::CloseOverlay | Action::CancelSearch => {
            state.overlay = Overlay::None;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn file_content_overlay_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    match action {
        Action::CloseOverlay | Action::Activate => state.overlay = Overlay::None,
        Action::MoveUp => move_file_content_cursor(state, -1),
        Action::MoveDown => move_file_content_cursor(state, 1),
        Action::MoveTop => state.file_view.vertical = 0,
        Action::MoveBottom => state.file_view.vertical = file_content_last_line(state),
        Action::HalfPageUp => move_file_content_cursor(state, -10),
        Action::HalfPageDown => move_file_content_cursor(state, 10),
        Action::ScrollLeft => {
            state.file_view.horizontal = state.file_view.horizontal.saturating_sub(4);
        }
        Action::ScrollRight => {
            state.file_view.horizontal = state.file_view.horizontal.saturating_add(4);
        }
        _ => {}
    }
    Vec::new()
}

fn confirm(state: &mut AppState) -> Vec<GitEffect> {
    let Some(query) = state.repository_search.prompt.take() else {
        return Vec::new();
    };
    state.repository_search.query.clone_from(&query);
    let request_id = state.request_id();
    state.repository_search.results = LoadState::Loading { request_id };
    state.repository_search.selection.reset(0);
    match state.repository_search.kind {
        RepositorySearchKind::Files => vec![GitEffect::SearchFiles { request_id, query }],
        RepositorySearchKind::Content => vec![GitEffect::SearchContent { request_id, query }],
    }
}

fn open_selected_file(state: &mut AppState) -> Vec<GitEffect> {
    let hit = match (
        &state.repository_search.results,
        state.repository_search.selection.index(),
    ) {
        (LoadState::Ready(results), Some(index)) => results.get(index).cloned(),
        _ => None,
    };
    let Some(hit) = hit else {
        return Vec::new();
    };
    state.file_view.return_view = state.repository_search.return_view;
    state.file_view.vertical = hit.line().unwrap_or(1).saturating_sub(1) as usize;
    state.overlay = Overlay::None;
    state.view = AppView::FileHistory;
    state.focus = FocusedPane::Primary;
    load_file_view(state, hit.path().clone())
}

pub(crate) fn load_file_view(state: &mut AppState, path: RepoPath) -> Vec<GitEffect> {
    const FILE_HISTORY_LIMIT: usize = 200;
    state.file_view.path = Some(path.clone());
    state.file_view.selection.reset(0);
    state.file_view.showing_history_diff = false;
    state.file_view.horizontal = 0;
    state.diff.target = None;
    state.diff.content = LoadState::Idle;

    let history_request = state.request_id();
    state.file_view.commits = LoadState::Loading {
        request_id: history_request,
    };
    let content_request = state.request_id();
    state.file_view.content = LoadState::Loading {
        request_id: content_request,
    };
    vec![
        GitEffect::LoadFileHistory {
            request_id: history_request,
            path: path.clone(),
            limit: FILE_HISTORY_LIMIT,
        },
        GitEffect::LoadFileContent {
            request_id: content_request,
            path,
        },
    ]
}

pub(crate) fn selected_file_history_diff(state: &mut AppState) -> Vec<GitEffect> {
    let commit = match (&state.file_view.commits, state.file_view.selection.index()) {
        (LoadState::Ready(commits), Some(index)) => commits.get(index).cloned(),
        _ => None,
    };
    let Some((commit, path)) = commit.zip(state.file_view.path.clone()) else {
        return Vec::new();
    };
    state.file_view.showing_history_diff = true;
    state.request_diff(DiffTarget::Commit {
        commit: commit.id().clone(),
        baseline: commit.baseline(),
        path,
    })
}

pub(crate) fn move_file_content_cursor(state: &mut AppState, delta: isize) {
    state.file_view.vertical = state
        .file_view
        .vertical
        .saturating_add_signed(delta)
        .min(file_content_last_line(state));
}

pub(crate) fn file_content_last_line(state: &AppState) -> usize {
    match &state.file_view.content {
        LoadState::Ready(document) if document.message().is_some() => 0,
        LoadState::Ready(document) => document
            .lines()
            .len()
            .saturating_add(usize::from(document.is_truncated()))
            .saturating_sub(1),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => 0,
    }
}
