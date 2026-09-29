//! Global branch-picker policy and repository invalidation after checkout.

use crate::app::model::Selection;
use crate::app::{Action, AppState, ErrorNotice, GitEffect, LoadState, RequestId, VimMotionKind};
use crate::domain::LocalBranch;
use crate::git::GitError;

#[derive(Debug)]
pub(crate) struct BranchPicker {
    pub(crate) branches: LoadState<Vec<LocalBranch>>,
    pub(crate) selection: Selection,
    pub(crate) switching: LoadState<()>,
}

pub(crate) fn apply_action(state: &mut AppState, action: Action) -> Vec<GitEffect> {
    if action == Action::Quit {
        state.should_quit = true;
        return Vec::new();
    }
    if state
        .branch_picker
        .as_ref()
        .is_some_and(|picker| picker.switching.loading_request().is_some())
    {
        // Once started, a mutation cannot be cancelled or duplicated by input.
        return Vec::new();
    }
    if action == Action::OpenBranches && state.branch_picker.is_none() || action == Action::Refresh
    {
        let request_id = state.request_id();
        state.search.cancel_input();
        state.branch_picker = Some(BranchPicker {
            branches: LoadState::Loading { request_id },
            selection: Selection::new(),
            switching: LoadState::Idle,
        });
        return vec![GitEffect::LoadBranches { request_id }];
    }
    let Some(picker) = state.branch_picker.as_mut() else {
        return Vec::new();
    };
    let len = match &picker.branches {
        LoadState::Ready(branches) => branches.len(),
        _ => 0,
    };
    match action {
        Action::CloseOverlay
        | Action::DismissSearchOrClose
        | Action::CancelSearch
        | Action::OpenBranches => {
            state.branch_picker = None;
        }
        Action::Activate => {
            let branch = match (&picker.branches, picker.selection.index()) {
                (LoadState::Ready(branches), Some(index)) => branches.get(index).cloned(),
                _ => None,
            };
            if let Some(branch) = branch {
                let request_id = state.request_id();
                if let Some(picker) = &mut state.branch_picker {
                    picker.switching = LoadState::Loading { request_id };
                }
                return vec![GitEffect::SwitchBranch { request_id, branch }];
            }
        }
        Action::MoveUp => {
            picker.selection.move_up(1, len);
        }
        Action::MoveDown => {
            picker.selection.move_down(1, len);
        }
        Action::MoveTop => {
            picker.selection.top(len);
        }
        Action::MoveBottom => {
            picker.selection.bottom(len);
        }
        Action::HalfPageUp => {
            picker.selection.move_up(10, len);
        }
        Action::HalfPageDown => {
            picker.selection.move_down(10, len);
        }
        Action::VimMotion(motion) => match motion.kind() {
            VimMotionKind::Up => {
                picker.selection.move_up(motion.count(), len);
            }
            VimMotionKind::Down => {
                picker.selection.move_down(motion.count(), len);
            }
            VimMotionKind::BufferTop => {
                picker.selection.top(len);
            }
            VimMotionKind::BufferBottom => {
                picker.selection.bottom(len);
            }
            _ => {}
        },
        _ => {}
    }
    Vec::new()
}

pub(crate) fn loaded(
    state: &mut AppState,
    request_id: RequestId,
    result: Result<Vec<LocalBranch>, GitError>,
) -> Vec<GitEffect> {
    let Some(picker) = &mut state.branch_picker else {
        return Vec::new();
    };
    if picker.branches.loading_request() != Some(request_id) {
        return Vec::new();
    }
    picker.branches = match result {
        Ok(branches) => {
            picker.selection.reset_to(
                branches.len(),
                branches.iter().position(LocalBranch::is_current),
            );
            LoadState::Ready(branches)
        }
        Err(error) => LoadState::Failed(ErrorNotice::new(error.to_string())),
    };
    Vec::new()
}

pub(crate) fn switched(
    state: &mut AppState,
    request_id: RequestId,
    result: Result<(), GitError>,
) -> Vec<GitEffect> {
    if state
        .branch_picker
        .as_ref()
        .and_then(|picker| picker.switching.loading_request())
        != Some(request_id)
    {
        return Vec::new();
    }
    let picker = state.branch_picker.take();
    // Refresh on errors too: a timeout or filesystem error can follow a partial
    // checkout. Never keep cached repository content after a mutation attempt.
    let effects = state.reload_after_branch_switch();
    if let Some(mut picker) = picker {
        match result {
            Ok(()) => {
                let name = match (&picker.branches, picker.selection.index()) {
                    (LoadState::Ready(branches), Some(index)) => {
                        branches.get(index).map(LocalBranch::display)
                    }
                    _ => None,
                }
                .unwrap_or_default();
                state.notice = Some(ErrorNotice::new(format!("Switched to {name}")));
            }
            Err(error) => {
                picker.switching = LoadState::Failed(ErrorNotice::new(error.to_string()));
                state.branch_picker = Some(picker);
            }
        }
    }
    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppEffect, AppView, Event, FocusedPane, LspAvailability, Overlay};
    use crate::domain::{DiffDocument, RepoPath, RepositoryRoot};
    use crate::tui::keymap::{KeyInputContext, KeyMapper};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn state(view: AppView) -> AppState {
        AppState::new(
            RepositoryRoot::new("/tmp/repo".into()).unwrap_or_else(|e| panic!("{e}")),
            view,
        )
    }

    fn branch(name: &str, current: bool) -> LocalBranch {
        LocalBranch::from_ref(format!("refs/heads/{name}").as_bytes(), current)
            .unwrap_or_else(|| panic!("invalid test branch"))
    }

    fn key(state: &mut AppState, mapper: &mut KeyMapper, code: KeyCode) -> Vec<AppEffect> {
        let context = if state.is_search_input_active() {
            KeyInputContext::SearchInput
        } else {
            KeyInputContext::Normal
        };
        mapper
            .map(KeyEvent::new(code, KeyModifiers::NONE), context)
            .map(|action| state.handle_app_action(action))
            .unwrap_or_default()
    }

    fn open(state: &mut AppState, mapper: &mut KeyMapper) -> RequestId {
        assert!(key(state, mapper, KeyCode::Char(' ')).is_empty());
        let effects = key(state, mapper, KeyCode::Char('b'));
        let [AppEffect::Git(GitEffect::LoadBranches { request_id })] = effects.as_slice() else {
            panic!("missing branch load: {effects:?}")
        };
        *request_id
    }

    #[test]
    fn keys_switch_branches_from_every_main_view_and_invalidate_old_data() {
        for view in [
            AppView::Changes,
            AppView::History,
            AppView::Graph,
            AppView::Code,
        ] {
            for focus in [
                FocusedPane::Primary,
                FocusedPane::Secondary,
                FocusedPane::Diff,
            ] {
                let mut state = state(view);
                state.focus = focus;
                state.set_lsp_availability(LspAvailability::Enabled);
                state.set_terminal_size(140, 40);
                state.set_scrolloff(3);
                let old = state.request_id();
                state.diff.content = LoadState::Loading { request_id: old };
                state.code_view.path = Some(
                    RepoPath::from_bytes(b"old.rs".to_vec()).unwrap_or_else(|e| panic!("{e}")),
                );
                let mut mapper = KeyMapper::new();
                let request_id = open(&mut state, &mut mapper);
                state.handle_app_event(Event::BranchesLoaded {
                    request_id,
                    result: Ok(vec![branch("main", true), branch("topic", false)]),
                });
                assert!(key(&mut state, &mut mapper, KeyCode::Char('j')).is_empty());
                let effects = key(&mut state, &mut mapper, KeyCode::Enter);
                let [AppEffect::Git(GitEffect::SwitchBranch { request_id, branch })] =
                    effects.as_slice()
                else {
                    panic!("missing switch")
                };
                assert_eq!(branch.display(), "topic");
                assert!(key(&mut state, &mut mapper, KeyCode::Enter).is_empty());
                assert!(key(&mut state, &mut mapper, KeyCode::Esc).is_empty());
                let effects = state.handle_app_event(Event::BranchSwitched {
                    request_id: *request_id,
                    result: Ok(()),
                });
                assert_eq!(state.view(), view);
                assert!(state.branch_picker.is_none());
                assert!(matches!(state.diff.content, LoadState::Idle));
                assert!(state.code_view.path.is_none());
                assert_eq!(state.lsp_availability, LspAvailability::Enabled);
                assert_eq!((state.terminal_width, state.terminal_height), (140, 40));
                assert_eq!(state.scrolloff, 3);
                assert!(matches!(
                    (view, effects.as_slice()),
                    (
                        AppView::Changes,
                        [AppEffect::Git(GitEffect::LoadChanges { .. })]
                    ) | (
                        AppView::History | AppView::Graph,
                        [AppEffect::Git(GitEffect::LoadCommits { .. })]
                    ) | (
                        AppView::Code,
                        [AppEffect::Git(GitEffect::LoadCodeTree { .. })]
                    )
                ));
                state.handle_app_event(Event::DiffLoaded {
                    request_id: old,
                    result: Ok(DiffDocument::Empty {
                        message: "old diff".into(),
                    }),
                });
                assert!(matches!(state.diff.content, LoadState::Idle));
                assert!(state.request_id().value() > request_id.value());
            }
        }
    }

    #[test]
    fn cancellation_restores_overlays_and_rejects_obsolete_branch_lists() {
        for overlay in [
            Overlay::None,
            Overlay::Diff,
            Overlay::CodeContent,
            Overlay::RepositorySearch,
            Overlay::FullFile,
            Overlay::LspHover,
            Overlay::SemanticTargets,
            Overlay::SymbolContext,
            Overlay::Help,
        ] {
            let mut state = state(AppView::Code);
            state.overlay = overlay;
            let mut mapper = KeyMapper::new();
            let old = open(&mut state, &mut mapper);
            key(&mut state, &mut mapper, KeyCode::Esc);
            assert!(state.branch_picker.is_none());
            assert_eq!(state.overlay, overlay);
            let new = open(&mut state, &mut mapper);
            state.handle_app_event(Event::BranchesLoaded {
                request_id: old,
                result: Ok(vec![branch("obsolete", false)]),
            });
            assert_eq!(
                state
                    .branch_picker
                    .as_ref()
                    .and_then(|p| p.branches.loading_request()),
                Some(new)
            );
        }
    }

    #[test]
    fn empty_lists_errors_retry_and_quit_are_recoverable() {
        let mut state = state(AppView::Changes);
        let mut mapper = KeyMapper::new();
        let request_id = open(&mut state, &mut mapper);
        state.handle_app_event(Event::BranchesLoaded {
            request_id,
            result: Ok(Vec::new()),
        });
        assert!(key(&mut state, &mut mapper, KeyCode::Enter).is_empty());
        let effects = state.handle_app_action(Action::Refresh);
        let [AppEffect::Git(GitEffect::LoadBranches { request_id })] = effects.as_slice() else {
            panic!("missing load")
        };
        state.handle_app_event(Event::BranchesLoaded {
            request_id: *request_id,
            result: Ok(vec![branch("topic", false)]),
        });
        let effects = key(&mut state, &mut mapper, KeyCode::Enter);
        let [AppEffect::Git(GitEffect::SwitchBranch { request_id, .. })] = effects.as_slice()
        else {
            panic!("missing switch")
        };
        let effects = state.handle_app_event(Event::BranchSwitched {
            request_id: *request_id,
            result: Err(GitError::Unsupported(
                "local changes would be overwritten".into(),
            )),
        });
        assert!(
            !effects.is_empty(),
            "failures must reload potentially changed files too"
        );
        assert!(matches!(
            state.branch_picker.as_ref().map(|p| &p.switching),
            Some(LoadState::Failed(_))
        ));
        let retry = key(&mut state, &mut mapper, KeyCode::Enter);
        assert!(matches!(
            retry.as_slice(),
            [AppEffect::Git(GitEffect::SwitchBranch { .. })]
        ));
        state.handle_app_action(Action::Quit);
        assert!(state.should_quit());
    }
}
