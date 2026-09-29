//! Keyboard-to-render regression tests for retained viewport positions.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout, Margin, Rect};

use super::model::{FullFileDeletion, Selection};
use super::*;
use crate::domain::{
    ChangeKind, ChangedFile, CommitSummary, DiffDocument, DiffLine, DiffLineKind, FileDocument,
    GitTreeMode, ObjectId, RepoPath, RepositoryRoot, SearchHit, SourcePosition, TreeEntry,
    WorktreeChange,
};
use crate::tui::keymap::{KeyInputContext, KeyMapper};

fn path(index: usize) -> RepoPath {
    RepoPath::from_bytes(format!("row-{index:03}").into_bytes())
        .unwrap_or_else(|error| panic!("{error}"))
}

fn fixture(view: AppView, focus: FocusedPane, width: u16, height: u16) -> AppState {
    let root =
        RepositoryRoot::new(PathBuf::from("/tmp/repo")).unwrap_or_else(|error| panic!("{error}"));
    let mut state = AppState::new(root, view);
    state.focus = focus;
    state.history_preview = HistoryPreview::Diff;
    let commits = (0..100)
        .map(|index| {
            CommitSummary::new(
                ObjectId::parse(format!("{index:040x}")).unwrap_or_else(|error| panic!("{error}")),
                Vec::new(),
                "author".to_owned(),
                "2026-09-30".to_owned(),
                format!("row-{index:03}"),
            )
        })
        .collect::<Vec<_>>();
    state.commits = LoadState::Ready(commits.clone());
    state.file_view.commits = LoadState::Ready(commits);
    state.changes = LoadState::Ready(
        (0..100)
            .map(|i| WorktreeChange::new(path(i), None, ChangeKind::Modified))
            .collect(),
    );
    state.files = LoadState::Ready(
        (0..100)
            .map(|i| ChangedFile::new(path(i), None, ChangeKind::Modified))
            .collect(),
    );
    state.code_view.visible = LoadState::Ready(
        (0..100)
            .map(|i| VisibleCodeEntry::new(path(i), path(i), 0, CodeEntryKind::File))
            .collect(),
    );
    state.tree.visible = LoadState::Ready(
        (0..100)
            .map(|i| {
                VisibleTreeEntry::new(
                    TreeEntry::new(
                        ObjectId::parse(format!("{i:040x}"))
                            .unwrap_or_else(|error| panic!("{error}")),
                        GitTreeMode::RegularFile,
                        path(i),
                    ),
                    path(i),
                    0,
                )
            })
            .collect(),
    );
    state.repository_search.results =
        LoadState::Ready((0..100).map(|i| SearchHit::file(path(i))).collect());
    for selection in [
        &mut state.commit_selection,
        &mut state.file_selection,
        &mut state.change_selection,
        &mut state.tree.selection,
        &mut state.code_view.selection,
        &mut state.file_view.selection,
        &mut state.repository_search.selection,
    ] {
        selection.reset_to(100, Some(40));
    }
    let text = (0..100)
        .map(|i| format!("row-{i:03}"))
        .collect::<Vec<_>>()
        .join("\n");
    state.code_view.content = LoadState::Ready(FileDocument::exact_text(text.clone()));
    state.file_view.content = LoadState::Ready(FileDocument::exact_text(text.clone()));
    state.full_file.content = LoadState::Ready(FileDocument::exact_text(text));
    state.diff.content = LoadState::Ready(DiffDocument::Text {
        lines: (0..100)
            .map(|i| DiffLine::new(DiffLineKind::Context, None, None, format!("row-{i:03}")))
            .collect(),
        bytes: 800,
    });
    state.set_terminal_size(width, height);
    state
}

fn keys(state: &mut AppState, keys: &str) -> Vec<AppEffect> {
    let mut mapper = KeyMapper::new();
    let mut effects = Vec::new();
    for character in keys.chars() {
        if let Some(action) = mapper.map(
            KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            KeyInputContext::Normal,
        ) {
            effects.extend(state.handle_app_action(action));
        }
    }
    effects
}

fn selection(state: &AppState) -> &Selection {
    if state.overlay == Overlay::RepositorySearch {
        return &state.repository_search.selection;
    }
    match (state.view, state.focus, state.history_panel) {
        (AppView::Code, _, _) => &state.code_view.selection,
        (AppView::Changes, _, _) => &state.change_selection,
        (AppView::History, FocusedPane::Secondary, HistoryPanel::Tree) => &state.tree.selection,
        (AppView::History, FocusedPane::Secondary, _)
        | (AppView::CommitDetails, FocusedPane::Diff, _)
        | (AppView::GraphDetails, _, _) => &state.file_selection,
        (AppView::FileHistory, _, _) => &state.file_view.selection,
        _ => &state.commit_selection,
    }
}

fn content_area(state: &AppState) -> Rect {
    let screen = Rect::new(0, 0, state.terminal_width, state.terminal_height);
    let area = match state.overlay {
        Overlay::RepositorySearch => {
            let popup = crate::layout::centered(
                screen,
                crate::layout::REPOSITORY_SEARCH_WIDTH_PERCENT,
                crate::layout::REPOSITORY_SEARCH_HEIGHT_PERCENT,
            );
            Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(popup)[1]
        }
        Overlay::Diff | Overlay::CodeContent | Overlay::FileContent | Overlay::FullFile => {
            Rect::new(1, 1, screen.width - 2, screen.height - 3)
        }
        _ => {
            let panes = crate::layout::main_panes(
                crate::layout::main_area(screen),
                state.view,
                state.focus,
            );
            panes[match state.focus {
                FocusedPane::Primary => 0,
                FocusedPane::Secondary => 1,
                FocusedPane::Diff => 2,
            }]
        }
    };
    area.inner(Margin::new(1, 1))
}

fn rendered_rows(state: &AppState) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(
        state.terminal_width,
        state.terminal_height,
    ))
    .unwrap_or_else(|error| panic!("{error}"));
    terminal
        .draw(|frame| crate::tui::render::render(frame, state))
        .unwrap_or_else(|error| panic!("{error}"));
    let buffer = terminal.backend().buffer();
    let area = content_area(state);
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

fn assert_row(state: &AppState, index: usize, row: usize) {
    let rows = rendered_rows(state);
    assert!(
        rows[row].contains(&format!("row-{index:03}")),
        "{:?}/{:?}/{:?} row {row}: {rows:?}",
        state.view,
        state.focus,
        state.overlay
    );
}

#[test]
fn list_positioning_uses_the_actual_pane_and_preserves_the_selection() {
    for (view, focus, tree, search) in [
        (AppView::Changes, FocusedPane::Primary, false, false),
        (AppView::History, FocusedPane::Primary, false, false),
        (AppView::History, FocusedPane::Secondary, false, false),
        (AppView::History, FocusedPane::Secondary, true, false),
        (AppView::CommitDetails, FocusedPane::Primary, false, false),
        (AppView::CommitDetails, FocusedPane::Diff, false, false),
        (AppView::Graph, FocusedPane::Primary, false, false),
        (AppView::GraphDetails, FocusedPane::Secondary, false, false),
        (AppView::FileHistory, FocusedPane::Primary, false, false),
        (AppView::Code, FocusedPane::Primary, false, false),
        (AppView::Code, FocusedPane::Primary, false, true),
    ] {
        for (width, height) in [(80, 24), (140, 40)] {
            for scrolloff in [0, 2, 3, usize::MAX] {
                let mut state = fixture(view, focus, width, height);
                if tree {
                    state.history_panel = HistoryPanel::Tree;
                }
                if search {
                    state.overlay = Overlay::RepositorySearch;
                    state.repository_search.prompt = None;
                }
                state.set_scrolloff(scrolloff);
                let height = usize::from(content_area(&state).height);
                let margin = scrolloff.min((height - 1) / 2);
                for (command, row) in [
                    ("zt", margin),
                    ("zz", (height - 1) / 2),
                    ("zb", height - 1 - margin),
                ] {
                    assert!(
                        keys(&mut state, command).is_empty(),
                        "positioning must not reload previews"
                    );
                    assert_eq!(selection(&state).index(), Some(40));
                    assert_eq!(selection(&state).viewport_top, 40 - row);
                    assert_row(&state, 40, row);
                }
                keys(&mut state, "55zt");
                assert_eq!(selection(&state).index(), Some(54));
                assert_row(&state, 54, margin);
            }
        }
    }
}

#[test]
fn lists_keep_the_window_still_when_reversing_and_keep_independent_origins() {
    let mut state = fixture(AppView::History, FocusedPane::Primary, 140, 60);
    keys(&mut state, "zb");
    let top = state.commit_selection.viewport_top;
    keys(&mut state, "k");
    assert_eq!(state.commit_selection.viewport_top, top);
    // Refill the changed files after the new commit requested its own context.
    state.files = LoadState::Ready(
        (0..100)
            .map(|i| ChangedFile::new(path(i), None, ChangeKind::Modified))
            .collect(),
    );
    state.file_selection.reset_to(100, Some(40));
    state.handle_app_action(Action::FocusRight);
    keys(&mut state, "zt");
    let file_top = state.file_selection.viewport_top;
    assert_eq!(file_top, 38);
    state.handle_app_action(Action::FocusLeft);
    assert_eq!(state.commit_selection.viewport_top, top);
    assert_eq!(state.file_selection.viewport_top, file_top);
    keys(&mut state, "zzH");
    assert_eq!(
        state.commit_selection.index(),
        Some(state.commit_selection.viewport_top + 2)
    );
}

#[test]
fn list_boundaries_short_lists_and_resize_keep_the_selected_item_visible() {
    let mut state = fixture(AppView::Code, FocusedPane::Primary, 140, 40);
    keys(&mut state, "ggzt");
    assert_row(&state, 0, 0);
    keys(&mut state, "G");
    assert!(
        rendered_rows(&state)
            .last()
            .is_some_and(|row| row.contains("row-099"))
    );
    keys(&mut state, "zz");
    state.set_terminal_size(80, 24);
    assert!(
        rendered_rows(&state)
            .iter()
            .any(|row| row.contains("row-099"))
    );
    if let LoadState::Ready(items) = &mut state.code_view.visible {
        items.truncate(2);
    }
    state.code_view.selection.clamp(2);
    keys(&mut state, "zzztzb");
    assert_eq!(state.code_view.selection.index(), Some(1));
    assert_row(&state, 0, 0);
    assert_row(&state, 1, 1);
    state.code_view.visible = LoadState::Ready(Vec::new());
    state.code_view.selection.reset(0);
    assert!(keys(&mut state, "ztzzzb").is_empty());
    assert_eq!(state.code_view.selection.viewport_top, 0);
}

#[test]
fn diff_and_source_panes_and_floats_follow_the_configured_context_band() {
    for (view, overlay) in [
        (AppView::Changes, Overlay::None),
        (AppView::History, Overlay::None),
        (AppView::GraphDetails, Overlay::None),
        (AppView::Code, Overlay::None),
        (AppView::FileHistory, Overlay::None),
        (AppView::Changes, Overlay::Diff),
        (AppView::Code, Overlay::CodeContent),
        (AppView::Code, Overlay::FullFile),
        (AppView::FileHistory, Overlay::FileContent),
    ] {
        for scrolloff in [0, 2, 3] {
            let mut state = fixture(view, FocusedPane::Diff, 140, 40);
            state.overlay = overlay;
            state.set_scrolloff(scrolloff);
            let height = usize::from(content_area(&state).height);
            let margin = scrolloff.min((height - 1) / 2);
            keys(&mut state, "40j");
            assert_row(&state, 40, height - 1 - margin);
            let before = rendered_rows(&state);
            keys(&mut state, "k");
            assert_row(&state, 39, height - 2 - margin);
            assert_eq!(rendered_rows(&state)[0], before[0]);
            for (command, row) in [
                ("zt", margin),
                ("zz", (height - 1) / 2),
                ("zb", height - 1 - margin),
            ] {
                keys(&mut state, command);
                assert_row(&state, 39, row);
            }
        }
    }
}

#[test]
fn full_source_context_counts_removed_display_rows_and_survives_mode_changes() {
    let mut state = fixture(AppView::Code, FocusedPane::Diff, 80, 24);
    state.overlay = Overlay::FullFile;
    state.full_file.mode = FullFileMode::Changes;
    state.full_file.deleted_lines = (0..30)
        .map(|_| FullFileDeletion {
            anchor: 39,
            old_line: None,
            content: "deleted".to_owned(),
        })
        .collect();
    state.full_file.cursor = SourcePosition::new(40, 0);
    keys(&mut state, "zb");
    let height = usize::from(content_area(&state).height);
    assert_row(&state, 40, height - 3);
    let top = state.full_file.viewport_vertical;
    keys(&mut state, "k");
    assert_eq!(state.full_file.viewport_vertical, top);
    assert_row(&state, 39, height - 4);
    keys(&mut state, "zz");
    assert_row(&state, 39, (height - 1) / 2);
    state.handle_app_action(Action::ToggleFullFileMode);
    assert_row(&state, 39, (height - 1) / 2);
    state.handle_app_action(Action::ToggleFullFileMode);
    assert_row(&state, 39, (height - 1) / 2);
}

#[test]
fn searches_reveal_context_in_diff_source_and_full_source() {
    for (view, overlay) in [
        (AppView::Changes, Overlay::Diff),
        (AppView::Code, Overlay::None),
        (AppView::Code, Overlay::FullFile),
        (AppView::FileHistory, Overlay::FileContent),
    ] {
        let mut state = fixture(view, FocusedPane::Diff, 140, 40);
        state.overlay = overlay;
        state.set_scrolloff(3);
        state.handle_app_action(Action::StartSearch(SearchDirection::Forward));
        for character in "row-070".chars() {
            state.handle_app_action(Action::InsertSearch(character));
        }
        state.handle_app_action(Action::ConfirmSearch);
        assert_row(&state, 70, usize::from(content_area(&state).height) - 4);
    }
}

#[test]
fn appending_commits_and_refreshing_files_preserve_deliberate_placement() {
    let mut state = fixture(AppView::History, FocusedPane::Primary, 140, 40);
    state.history_page.continuation = HistoryContinuation::Available;
    let effects = keys(&mut state, "Gzz");
    let (request_id, page, mode) = effects
        .into_iter()
        .find_map(|effect| match effect {
            AppEffect::Git(GitEffect::LoadCommits {
                request_id,
                page,
                mode,
            }) => Some((request_id, page, mode)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected next history page"));
    let top = state.commit_selection.viewport_top;
    state.handle_app_event(Event::CommitsLoaded {
        request_id,
        page,
        mode,
        result: Ok((100..105)
            .map(|index| {
                CommitSummary::new(
                    ObjectId::parse(format!("{index:040x}"))
                        .unwrap_or_else(|error| panic!("{error}")),
                    Vec::new(),
                    "author".to_owned(),
                    "2026-09-30".to_owned(),
                    format!("row-{index:03}"),
                )
            })
            .collect()),
    });
    assert_eq!(state.commit_selection.index(), Some(99));
    assert_eq!(state.commit_selection.viewport_top, top);
    assert!(
        rendered_rows(&state)
            .iter()
            .any(|row| row.contains("row-100"))
    );

    let mut state = fixture(AppView::Changes, FocusedPane::Primary, 140, 40);
    keys(&mut state, "zt");
    let top = state.change_selection.viewport_top;
    for len in [100, 2] {
        let effects = state.handle_app_action(Action::Refresh);
        let request_id = effects
            .into_iter()
            .find_map(|effect| match effect {
                AppEffect::Git(GitEffect::LoadChanges { request_id }) => Some(request_id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected changes request"));
        state.handle_app_event(Event::ChangesLoaded {
            request_id,
            result: Ok((0..len)
                .map(|i| WorktreeChange::new(path(i), None, ChangeKind::Modified))
                .collect()),
        });
        if len == 100 {
            assert_eq!(state.change_selection.viewport_top, top);
            assert_row(&state, 40, 2);
        } else {
            assert_eq!(state.change_selection.viewport_top, 0);
            assert_row(&state, 0, 0);
        }
    }
}
