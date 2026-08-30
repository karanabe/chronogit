use crate::app::{
    Action, AppState, AppView, ErrorNotice, Event, FocusedPane, GitEffect, HistoryPanel, LoadState,
    Overlay, VisibleTreeEntry,
};
use crate::domain::{CommitSummary, DiffTarget, RepoPath, TreeKind};

pub(crate) fn apply_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    if action == Action::Quit {
        state.should_quit = true;
        return Vec::new();
    }
    if state.overlay != Overlay::None {
        return overlay_action(state, action);
    }
    match action {
        Action::ShowChanges => switch_view(state, AppView::Changes),
        Action::ShowHistory => switch_view(state, AppView::History),
        Action::FocusLeft => {
            state.focus = previous_pane(state.view, state.focus);
            Vec::new()
        }
        Action::FocusRight => {
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
            state.diff.horizontal = state.diff.horizontal.saturating_sub(4);
            Vec::new()
        }
        Action::ScrollRight => {
            state.diff.horizontal = state.diff.horizontal.saturating_add(4);
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
        Action::StartSearch(_)
        | Action::InsertSearch(_)
        | Action::DeleteSearch
        | Action::ConfirmSearch
        | Action::CancelSearch
        | Action::NextMatch
        | Action::PreviousMatch
        | Action::CloseOverlay
        | Action::Tick
        | Action::Quit => Vec::new(),
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
                }
                Err(error) => {
                    state.diff.content = LoadState::Failed(ErrorNotice::new(error.to_string()));
                    state.diff.vertical = 0;
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
        _ => Vec::new(),
    }
}

fn overlay_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    match state.overlay {
        Overlay::Diff => diff_overlay_action(state, action),
        Overlay::CommitMessage => message_overlay_action(state, action),
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
    }
}

fn switch_view(state: &mut AppState, view: AppView) -> Vec<GitEffect> {
    state.view = view;
    state.focus = FocusedPane::Primary;
    match view {
        AppView::Changes if matches!(state.changes, LoadState::Idle) => state.request_changes(),
        AppView::History | AppView::CommitDetails if matches!(state.commits, LoadState::Idle) => {
            state.request_commits(false)
        }
        _ => Vec::new(),
    }
}

fn move_selection(state: &mut AppState, delta: isize) -> Vec<GitEffect> {
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
        (AppView::History | AppView::CommitDetails, FocusedPane::Primary, _) => {
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
        _ => Vec::new(),
    }
}

fn move_to_edge(state: &mut AppState, bottom: bool) -> Vec<GitEffect> {
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
        (AppView::History | AppView::CommitDetails, FocusedPane::Primary, _) => {
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
        _ => false,
    };
    if !moved {
        return maybe_load_more(state);
    }
    match (state.view, state.focus, state.history_panel) {
        (AppView::Changes, _, _) => selected_change_diff(state),
        (AppView::History | AppView::CommitDetails, FocusedPane::Primary, _) => {
            let mut effects = selected_commit_context(state);
            effects.extend(maybe_load_more(state));
            effects
        }
        (AppView::History, _, HistoryPanel::ChangedFiles) => selected_file_diff(state),
        (AppView::CommitDetails, FocusedPane::Diff, _) => selected_file_diff(state),
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
    if state.view == AppView::CommitDetails && state.focus == FocusedPane::Secondary {
        move_message_cursor(state, delta);
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
        AppView::History | AppView::CommitDetails => {
            if let Some(commit) = selected_commit(state) {
                state.preferred_commit = Some(commit.id().clone());
            }
        }
    }
    state.clear_cache();
    state.diff = crate::app::model::DiffViewState {
        target: None,
        content: LoadState::Idle,
        vertical: 0,
        horizontal: 0,
    };
    match state.view {
        AppView::Changes => state.request_changes(),
        AppView::History | AppView::CommitDetails => state.request_commits(false),
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
    request_message(state, commit)
}

fn toggle_message(state: &mut AppState) -> Vec<GitEffect> {
    if !matches!(state.view, AppView::History | AppView::CommitDetails) {
        return Vec::new();
    }
    let Some(commit) = selected_commit(state).map(|summary| summary.id().clone()) else {
        return Vec::new();
    };
    state.message.scroll = 0;
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
    if state.view == AppView::CommitDetails {
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
    if !matches!(state.view, AppView::History | AppView::CommitDetails)
        || state.focus != FocusedPane::Primary
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
        SearchDirection,
    };
    use crate::domain::{
        ChangeKind, ChangedFile, CommitMessage, CommitSummary, DiffDocument, DiffLine,
        DiffLineKind, DiffTarget, ObjectId, RepoPath, RepositoryRoot, TreeEntry, TreeKind,
        WorktreeChange,
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

        let _none = apply_action(&mut state, Action::Activate);
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
