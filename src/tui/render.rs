//! Pure rendering of application state into ratatui frames.
//!
//! Rendering never initiates repository work or mutates [`AppState`]. Layouts
//! adapt at 110 columns, and terminals smaller than 80 by 24 cells receive a
//! stable resize message instead of partially rendered panes.
//!
//! [`AppState`]: crate::app::AppState

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::{
    AppState, AppView, CodeEntryKind, FocusedPane, HistoryPanel, LoadState, Overlay,
    RepositorySearchKind, VisibleCodeEntry, VisibleTreeEntry,
};
use crate::domain::{DiffDocument, DiffLine, DiffLineKind, DiffTarget, FileDocument, TreeKind};
use crate::tui::graph::graph_prefixes;
use crate::tui::highlight::{highlight_code, source_is_too_large};

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;
const WIDE_WIDTH: u16 = 110;

/// Renders one complete frame from an immutable application-state snapshot.
///
/// The function sanitizes repository-provided text before placing it in terminal
/// cells and replaces unsupported terminal sizes with a resize message.
pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(frame, area);
        return;
    }
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    render_main(frame, sections[0], state);
    render_footer(frame, sections[1], state);
    render_overlay(frame, area, state);
}

fn render_main(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.view {
        AppView::Changes if area.width < WIDE_WIDTH => match state.focus {
            FocusedPane::Primary | FocusedPane::Secondary => render_changes(frame, area, state),
            FocusedPane::Diff => render_diff(frame, area, state),
        },
        AppView::Changes => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
                .split(area);
            render_changes(frame, columns[0], state);
            render_diff(frame, columns[1], state);
        }
        AppView::History => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(50),
                ])
                .split(area);
            render_commits(frame, rows[0], state);
            render_history_middle(frame, rows[1], state);
            render_diff(frame, rows[2], state);
        }
        AppView::CommitDetails => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(45),
                    Constraint::Percentage(30),
                ])
                .split(area);
            render_commits(frame, rows[0], state);
            render_commit_body(frame, rows[1], state);
            render_detail_files(frame, rows[2], state);
        }
        AppView::Graph => render_graph(frame, area, state),
        AppView::GraphDetails => {
            render_graph(frame, area, state);
            let popup = centered(area, 90, 88);
            frame.render_widget(Clear, popup);
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(popup);
            render_file_list(
                frame,
                rows[0],
                state,
                "Changed files [q/Esc: graph, Enter: full diff]",
                state.focus == FocusedPane::Secondary,
            );
            render_diff(frame, rows[1], state);
        }
        AppView::FileHistory => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(area);
            render_file_history(frame, rows[0], state);
            if state.file_view.showing_history_diff {
                render_diff(frame, rows[1], state);
            } else {
                render_file_content(frame, rows[1], state, "Current working tree content");
            }
        }
        AppView::Code => render_code_view(frame, area, state),
    }
}

fn render_code_view(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    render_code_tree(frame, rows[0], state);
    render_code_content(frame, rows[1], state);
}

fn render_code_tree(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = pane_block(
        "Working tree [Enter: expand/open]",
        state.focus == FocusedPane::Primary,
    );
    let lines = match &state.code_view.visible {
        LoadState::Idle => vec![plain("Not loaded")],
        LoadState::Loading { .. } => vec![plain("Loading files…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(entries) if entries.is_empty() => vec![plain("No files.")],
        LoadState::Ready(entries) => entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                code_tree_line(entry, state.code_view.selection.index() == Some(index))
            })
            .collect(),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((list_scroll(state.code_view.selection.index(), area), 0)),
        area,
    );
}

fn code_tree_line(entry: &VisibleCodeEntry, selected: bool) -> Line<'static> {
    let marker = match entry.kind() {
        CodeEntryKind::Directory if entry.expanded() => "▾",
        CodeEntryKind::Directory => "▸",
        CodeEntryKind::File => "·",
    };
    selected_line(
        selected,
        format!(
            "{}{} {}",
            "  ".repeat(entry.depth()),
            marker,
            sanitize_inline(&entry.name().display())
        ),
    )
}

fn render_graph(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = pane_block("Git graph [Enter: commit details]", true);
    let mut lines = match &state.commits {
        LoadState::Idle => vec![plain("Not loaded")],
        LoadState::Loading { .. } => vec![plain("Loading graph…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(commits) if commits.is_empty() => vec![plain("No commits yet.")],
        LoadState::Ready(commits) => graph_prefixes(commits)
            .into_iter()
            .zip(commits)
            .enumerate()
            .map(|(index, (prefix, commit))| {
                let date = commit
                    .authored_at()
                    .get(..10)
                    .unwrap_or(commit.authored_at());
                selected_line(
                    state.commit_selection.index() == Some(index),
                    format!(
                        "{prefix}{} {date} {} — {}",
                        commit.id().short(),
                        sanitize_inline(commit.author()),
                        sanitize_inline(commit.subject())
                    ),
                )
            })
            .collect(),
    };
    if state.history_page.loading_more.is_some() {
        lines.push(plain("Loading more…"));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((list_scroll(state.commit_selection.index(), area), 0)),
        area,
    );
}

fn render_file_history(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let path = state
        .file_view
        .path
        .as_ref()
        .map(|path| sanitize_inline(&path.display()))
        .unwrap_or_else(|| "file".to_owned());
    let title = format!("History — {path} [j/k: show commit diff, q/Esc: back]");
    let block = pane_block(&title, state.focus == FocusedPane::Primary);
    let lines = match &state.file_view.commits {
        LoadState::Idle => vec![plain("Not loaded")],
        LoadState::Loading { .. } => vec![plain("Loading file history…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(commits) if commits.is_empty() => vec![plain("No committed history.")],
        LoadState::Ready(commits) => commits
            .iter()
            .enumerate()
            .map(|(index, commit)| {
                let date = commit
                    .authored_at()
                    .get(..10)
                    .unwrap_or(commit.authored_at());
                selected_line(
                    state.file_view.selection.index() == Some(index),
                    format!(
                        "{} {date} {} — {}",
                        commit.id().short(),
                        sanitize_inline(commit.author()),
                        sanitize_inline(commit.subject())
                    ),
                )
            })
            .collect(),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((list_scroll(state.file_view.selection.index(), area), 0)),
        area,
    );
}

fn render_changes(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = pane_block("Unstaged changes", state.focus == FocusedPane::Primary);
    let lines = match &state.changes {
        LoadState::Idle => vec![plain("Not loaded")],
        LoadState::Loading { .. } => vec![plain("Loading changes…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(changes) if changes.is_empty() => {
            vec![plain("No unstaged changes. Staged-only files are hidden.")]
        }
        LoadState::Ready(changes) => changes
            .iter()
            .enumerate()
            .map(|(index, change)| {
                let rename = change
                    .original_path()
                    .map(|path| format!("{} → ", sanitize_inline(&path.display())))
                    .unwrap_or_default();
                selected_line(
                    state.change_selection.index() == Some(index),
                    format!(
                        "{} {rename}{}",
                        change.kind().label(),
                        sanitize_inline(&change.path().display())
                    ),
                )
            })
            .collect(),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((list_scroll(state.change_selection.index(), area), 0)),
        area,
    );
}

fn render_commits(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = pane_block("Commits", state.focus == FocusedPane::Primary);
    let mut lines = match &state.commits {
        LoadState::Idle => vec![plain("Not loaded")],
        LoadState::Loading { .. } => vec![plain("Loading history…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(commits) if commits.is_empty() => vec![plain("No commits yet.")],
        LoadState::Ready(commits) => commits
            .iter()
            .enumerate()
            .map(|(index, commit)| {
                let date = commit
                    .authored_at()
                    .get(..10)
                    .unwrap_or(commit.authored_at());
                selected_line(
                    state.commit_selection.index() == Some(index),
                    format!(
                        "{} {date} {} — {}",
                        commit.id().short(),
                        sanitize_inline(commit.author()),
                        sanitize_inline(commit.subject())
                    ),
                )
            })
            .collect(),
    };
    if state.history_page.loading_more.is_some() {
        lines.push(plain("Loading more…"));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((list_scroll(state.commit_selection.index(), area), 0)),
        area,
    );
}

fn render_commit_body(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = pane_block("Commit body", state.focus == FocusedPane::Secondary);
    let text = match &state.message.content {
        LoadState::Idle => "Select a commit in History.".to_owned(),
        LoadState::Loading { .. } => "Loading commit body…".to_owned(),
        LoadState::Failed(error) => format!("Error: {}", sanitize_inline(error.message())),
        LoadState::Ready(message) if message.body().is_empty() => "No commit body.".to_owned(),
        LoadState::Ready(message) => message.body().to_owned(),
    };
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let last = text.lines().count().saturating_sub(1);
    let cursor = state.message.scroll.min(last);
    let vertical = followed_scroll(cursor, state.message.viewport_vertical, visible);
    let lines = message_cursor_lines(&text, cursor, state.message.byte_column);
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((
            vertical,
            state.message.horizontal.min(usize::from(u16::MAX)) as u16,
        )),
        area,
    );
}

fn render_history_middle(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.history_panel {
        HistoryPanel::ChangedFiles => render_files(frame, area, state),
        HistoryPanel::Tree => render_tree(frame, area, state),
    }
}

fn render_files(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    render_file_list(
        frame,
        area,
        state,
        "Changed files [\\t: tree]",
        state.focus == FocusedPane::Secondary,
    );
}

fn render_detail_files(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    render_file_list(
        frame,
        area,
        state,
        "Changed files [Enter: diff]",
        state.focus == FocusedPane::Diff,
    );
}

fn render_file_list(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    title: &str,
    focused: bool,
) {
    let block = pane_block(title, focused);
    let lines = match &state.files {
        LoadState::Idle => vec![plain("Select a commit")],
        LoadState::Loading { .. } => vec![plain("Loading files…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(files) if files.is_empty() => vec![plain("No changed files.")],
        LoadState::Ready(files) => files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let rename = file
                    .original_path()
                    .map(|path| format!("{} → ", sanitize_inline(&path.display())))
                    .unwrap_or_default();
                selected_line(
                    state.file_selection.index() == Some(index),
                    format!(
                        "{} {rename}{}",
                        file.kind().label(),
                        sanitize_inline(&file.path().display())
                    ),
                )
            })
            .collect(),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((list_scroll(state.file_selection.index(), area), 0)),
        area,
    );
}

fn render_tree(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = pane_block(
        "Commit tree [\\t: files]",
        state.focus == FocusedPane::Secondary,
    );
    let lines = match &state.tree.visible {
        LoadState::Idle => vec![plain("Select a commit")],
        LoadState::Loading { .. } => vec![plain("Loading tree…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(entries) if entries.is_empty() => vec![plain("Empty tree.")],
        LoadState::Ready(entries) => entries
            .iter()
            .enumerate()
            .map(|(index, entry)| tree_line(entry, state.tree.selection.index() == Some(index)))
            .collect(),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((list_scroll(state.tree.selection.index(), area), 0)),
        area,
    );
}

fn tree_line(entry: &VisibleTreeEntry, selected: bool) -> Line<'static> {
    let marker = match entry.entry().kind() {
        TreeKind::Directory if entry.expanded() => "▾",
        TreeKind::Directory => "▸",
        TreeKind::File => "·",
        TreeKind::Symlink => "↗",
        TreeKind::Submodule => "◆",
    };
    selected_line(
        selected,
        format!(
            "{}{} {} {}",
            "  ".repeat(entry.depth()),
            marker,
            entry.entry().mode(),
            sanitize_inline(&entry.entry().name().display())
        ),
    )
}

fn render_diff(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let baseline = selected_baseline(state);
    let title = baseline.map_or_else(|| "Diff".to_owned(), |value| format!("Diff — {value}"));
    render_diff_pane(frame, area, state, &title, state.focus == FocusedPane::Diff);
}

fn render_diff_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    title: &str,
    focused: bool,
) {
    let block = pane_block(title, focused);
    let lines = match &state.diff.content {
        LoadState::Idle => vec![plain("Select a file to view its diff.")],
        LoadState::Loading { .. } => vec![plain("Loading diff…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(document) => diff_lines(document, state),
    };
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let cursor = state.diff.vertical.min(lines.len().saturating_sub(1));
    let vertical = followed_scroll(cursor, state.diff.viewport_vertical, visible);
    let horizontal = state.diff.horizontal.min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((vertical, horizontal)),
        area,
    );
}

fn render_file_content(frame: &mut Frame<'_>, area: Rect, state: &AppState, title: &str) {
    let block = pane_block(
        title,
        state.focus == FocusedPane::Diff || state.overlay == Overlay::FileContent,
    );
    let lines = match &state.file_view.content {
        LoadState::Idle => vec![plain("Select a file to view its current content.")],
        LoadState::Loading { .. } => vec![plain("Loading current content…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(document) => file_document_lines(document, state),
    };
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let cursor = state.file_view.vertical.min(lines.len().saturating_sub(1));
    let vertical = followed_scroll(cursor, state.file_view.viewport_vertical, visible);
    let horizontal = state.file_view.horizontal.min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((vertical, horizontal)),
        area,
    );
}

fn render_code_content(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let path = state
        .code_view
        .path
        .as_ref()
        .map(|path| sanitize_inline(&path.display()))
        .unwrap_or_else(|| "Code".to_owned());
    let block = pane_block(&path, state.focus == FocusedPane::Diff);
    let lines = match &state.code_view.content {
        LoadState::Idle => vec![plain("Select a file to view its current content.")],
        LoadState::Loading { .. } => vec![plain("Loading current content…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(document) => code_document_lines(document, state),
    };
    let (vertical, horizontal) = code_scroll(state, area, lines.len());
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((vertical, horizontal)),
        area,
    );
}

fn file_document_lines(document: &FileDocument, state: &AppState) -> Vec<Line<'static>> {
    document_lines(document, state.file_view.path.as_ref(), |line, index| {
        current_file_line(line, index, state)
    })
}

fn code_document_lines(document: &FileDocument, state: &AppState) -> Vec<Line<'static>> {
    document_lines(document, state.code_view.path.as_ref(), |line, index| {
        code_file_line(line, index, state)
    })
}

fn document_lines(
    document: &FileDocument,
    path: Option<&crate::domain::RepoPath>,
    mut decorate: impl FnMut(Line<'static>, usize) -> Line<'static>,
) -> Vec<Line<'static>> {
    if let Some(message) = document.message() {
        return vec![decorate(plain(sanitize_inline(message)), 0)];
    }
    let highlighted = source_spans(document.lines(), path);
    let mut lines = document
        .lines()
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let code = highlighted
                .as_ref()
                .and_then(|lines| lines.get(index))
                .cloned()
                .unwrap_or_else(|| vec![Span::raw(sanitize_inline(line))]);
            let mut spans = vec![Span::styled(format!("{:>6} ", index + 1), gutter_style())];
            spans.extend(code);
            decorate(Line::from(spans), index)
        })
        .collect::<Vec<_>>();
    if document.is_truncated() {
        let index = lines.len();
        lines.push(decorate(
            Line::styled(
                "… file truncated at the safe output limit …",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            index,
        ));
    }
    if lines.is_empty() {
        lines.push(decorate(plain("Empty file."), 0));
    }
    lines
}

fn code_file_line(mut line: Line<'static>, index: usize, state: &AppState) -> Line<'static> {
    let selected = usize::try_from(state.code_view.cursor.line()).ok() == Some(index)
        && (state.overlay == Overlay::CodeContent || state.focus == FocusedPane::Diff);
    if selected
        && let Some(source) = match &state.code_view.content {
            LoadState::Ready(document) => document.lines().get(index),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
        }
    {
        highlight_source_cursor(&mut line, source, state.code_view.cursor.byte_column(), 1);
    }
    line.spans.insert(0, navigation_marker(selected));
    if state.search.current_line() == Some(index) {
        line.style = line.style.patch(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    } else if state.search.is_match(index) {
        line.style = line.style.add_modifier(Modifier::UNDERLINED);
    }
    line
}

fn current_file_line(mut line: Line<'static>, index: usize, state: &AppState) -> Line<'static> {
    let selected = state.file_view.vertical == index
        && (state.overlay == Overlay::FileContent || state.focus == FocusedPane::Diff);
    line.spans.insert(0, navigation_marker(selected));
    if selected
        && let Some(source) = match &state.file_view.content {
            LoadState::Ready(document) => document.lines().get(index),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
        }
    {
        highlight_source_cursor(&mut line, source, state.file_view.byte_column, 2);
    }
    line
}

fn diff_lines(document: &DiffDocument, state: &AppState) -> Vec<Line<'static>> {
    if let Some(message) = document.message() {
        return vec![highlight_diff_line(
            plain(sanitize_inline(message)),
            0,
            state,
        )];
    }
    let mut highlighted = diff_source_spans(document, diff_target_path(state.diff.target.as_ref()));
    let mut lines = document
        .lines()
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let code = highlighted.get_mut(index).and_then(Option::take);
            highlight_diff_line(diff_line(line, code), index, state)
        })
        .collect::<Vec<_>>();
    if document.is_truncated() {
        let index = lines.len();
        lines.push(highlight_diff_line(
            Line::styled(
                "… diff truncated at the safe output limit …",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            index,
            state,
        ));
    }
    lines
}

fn highlight_diff_line(mut line: Line<'static>, index: usize, state: &AppState) -> Line<'static> {
    let selected = state.diff.vertical == index
        && (state.overlay == Overlay::Diff || state.focus == FocusedPane::Diff);
    line.spans.insert(0, navigation_marker(selected));
    if selected
        && let Some(source) = match &state.diff.content {
            LoadState::Ready(document) => document.lines().get(index).map(DiffLine::text),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
        }
    {
        highlight_source_cursor(&mut line, source, state.diff.byte_column, 2);
    }
    if state.search.current_line() == Some(index) {
        line.style = line.style.patch(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    } else if state.search.is_match(index) {
        line.style = line.style.add_modifier(Modifier::UNDERLINED);
    }
    line
}

fn diff_line(line: &DiffLine, syntax_spans: Option<Vec<Span<'static>>>) -> Line<'static> {
    let old = line
        .old_line()
        .map(|value| value.value().to_string())
        .unwrap_or_default();
    let new = line
        .new_line()
        .map(|value| value.value().to_string())
        .unwrap_or_default();
    let mut spans = vec![Span::styled(format!("{old:>5} {new:>5} "), gutter_style())];
    let line_style = match line.kind() {
        DiffLineKind::Added => {
            let (marker, code) = split_diff_code(line);
            spans.push(Span::styled(
                marker,
                Style::default()
                    .fg(Color::Rgb(166, 227, 161))
                    .add_modifier(Modifier::BOLD),
            ));
            spans.extend(syntax_spans.unwrap_or_else(|| vec![Span::raw(sanitize_inline(code))]));
            Style::default().bg(Color::Rgb(33, 58, 43))
        }
        DiffLineKind::Removed => {
            let (marker, code) = split_diff_code(line);
            spans.push(Span::styled(
                marker,
                Style::default()
                    .fg(Color::Rgb(243, 139, 168))
                    .add_modifier(Modifier::BOLD),
            ));
            spans.extend(syntax_spans.unwrap_or_else(|| vec![Span::raw(sanitize_inline(code))]));
            Style::default().bg(Color::Rgb(74, 34, 29))
        }
        DiffLineKind::Context => {
            let (marker, code) = split_diff_code(line);
            spans.push(Span::raw(marker));
            spans.extend(syntax_spans.unwrap_or_else(|| vec![Span::raw(sanitize_inline(code))]));
            Style::default()
        }
        DiffLineKind::Hunk => {
            spans.push(Span::styled(
                sanitize_inline(line.text()),
                Style::default()
                    .fg(Color::Rgb(137, 180, 250))
                    .add_modifier(Modifier::BOLD),
            ));
            Style::default().bg(Color::Rgb(49, 50, 68))
        }
        DiffLineKind::Header => {
            spans.push(Span::styled(
                sanitize_inline(line.text()),
                Style::default()
                    .fg(Color::Rgb(137, 180, 250))
                    .add_modifier(Modifier::BOLD),
            ));
            Style::default()
        }
        DiffLineKind::Meta => {
            spans.push(Span::styled(
                sanitize_inline(line.text()),
                Style::default().fg(Color::Rgb(249, 226, 175)),
            ));
            Style::default()
        }
    };
    Line::from(spans).style(line_style)
}

fn source_spans(
    lines: &[String],
    path: Option<&crate::domain::RepoPath>,
) -> Option<Vec<Vec<Span<'static>>>> {
    let bytes = lines
        .iter()
        .fold(0usize, |total, line| total.saturating_add(line.len()));
    if source_is_too_large(bytes, lines.len()) {
        return None;
    }
    let mut source = String::with_capacity(bytes.saturating_add(lines.len()));
    for line in lines {
        source.push_str(&sanitize_inline(line));
        source.push('\n');
    }
    highlight_code(&source, path).filter(|highlighted| highlighted.len() == lines.len())
}

fn diff_source_spans(
    document: &DiffDocument,
    path: Option<&crate::domain::RepoPath>,
) -> Vec<Option<Vec<Span<'static>>>> {
    let lines = document.lines();
    let mut result = vec![None; lines.len()];
    let bytes = lines.iter().fold(0usize, |total, line| {
        total.saturating_add(diff_code_text(line).map_or(0, str::len))
    });
    let code_lines = lines
        .iter()
        .filter(|line| diff_code_text(line).is_some())
        .count();
    if source_is_too_large(bytes, code_lines) {
        return result;
    }

    let mut start = 0;
    while start < lines.len() {
        while start < lines.len() && diff_code_text(&lines[start]).is_none() {
            start += 1;
        }
        let mut end = start;
        while end < lines.len() && diff_code_text(&lines[end]).is_some() {
            end += 1;
        }
        if start == end {
            continue;
        }
        let mut source = String::new();
        for line in &lines[start..end] {
            if let Some(code) = diff_code_text(line) {
                source.push_str(&sanitize_inline(code));
                source.push('\n');
            }
        }
        if let Some(highlighted) = highlight_code(&source, path)
            && highlighted.len() == end - start
            && let Some(slots) = result.get_mut(start..end)
        {
            for (slot, spans) in slots.iter_mut().zip(highlighted) {
                *slot = Some(spans);
            }
        }
        start = end;
    }
    result
}

fn diff_code_text(line: &DiffLine) -> Option<&str> {
    match line.kind() {
        DiffLineKind::Added => Some(line.text().strip_prefix('+').unwrap_or(line.text())),
        DiffLineKind::Removed => Some(line.text().strip_prefix('-').unwrap_or(line.text())),
        DiffLineKind::Context => Some(line.text().strip_prefix(' ').unwrap_or(line.text())),
        DiffLineKind::Header | DiffLineKind::Hunk | DiffLineKind::Meta => None,
    }
}

fn split_diff_code(line: &DiffLine) -> (&'static str, &str) {
    match line.kind() {
        DiffLineKind::Added => ("+", line.text().strip_prefix('+').unwrap_or(line.text())),
        DiffLineKind::Removed => ("-", line.text().strip_prefix('-').unwrap_or(line.text())),
        DiffLineKind::Context => (" ", line.text().strip_prefix(' ').unwrap_or(line.text())),
        DiffLineKind::Header | DiffLineKind::Hunk | DiffLineKind::Meta => ("", line.text()),
    }
}

fn diff_target_path(target: Option<&DiffTarget>) -> Option<&crate::domain::RepoPath> {
    match target {
        Some(DiffTarget::Worktree { path, .. } | DiffTarget::Commit { path, .. }) => Some(path),
        None => None,
    }
}

fn navigation_marker(selected: bool) -> Span<'static> {
    if selected {
        Span::styled("▌", Style::default().fg(Color::Rgb(137, 180, 250)))
    } else {
        Span::raw(" ")
    }
}

fn gutter_style() -> Style {
    Style::default().fg(Color::Rgb(108, 112, 134))
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if state.search.prompt_text().is_some() {
        render_search_bar(frame, area, state);
        return;
    }
    let view = match state.view {
        AppView::Changes => "CHANGES",
        AppView::History => "HISTORY",
        AppView::CommitDetails => "DETAILS",
        AppView::Graph => "GRAPH",
        AppView::GraphDetails => "GRAPH DETAILS",
        AppView::FileHistory => "FILE HISTORY",
        AppView::Code => "CODE",
    };
    let notice = state
        .notice
        .as_ref()
        .map(|notice| format!(" | notice: {}", sanitize_inline(notice.message())))
        .unwrap_or_default();
    let lsp = match (
        &state.semantic_navigation.targets,
        state.semantic_navigation.kind,
    ) {
        (LoadState::Loading { .. }, Some(kind)) => state
            .semantic_navigation
            .status
            .as_deref()
            .map(|status| format!(" | LSP: {}: {}", kind.label(), sanitize_inline(status)))
            .unwrap_or_else(|| format!(" | LSP: locating {}…", kind.label())),
        _ => match &state.lsp_hover.content {
            LoadState::Loading { .. } => state
                .lsp_hover
                .status
                .as_deref()
                .map(|status| format!(" | LSP: hover: {}", sanitize_inline(status)))
                .unwrap_or_else(|| " | LSP: loading hover…".to_owned()),
            LoadState::Idle | LoadState::Ready(_) | LoadState::Failed(_) => String::new(),
        },
    };
    let comparison = selected_baseline(state).unwrap_or_else(|| "comparison pending".to_owned());
    let controls = match (state.view, area.width >= WIDE_WIDTH) {
        (AppView::CommitDetails, true) => {
            "q/Esc History  \\m message  ^w h/j pane  j/k move  Enter diff  Q quit"
        }
        (AppView::CommitDetails, false) => "q/Esc History  \\m msg  Enter diff  Q quit",
        (AppView::GraphDetails, true) => {
            "q/Esc Graph  \\m message  ^w h/j pane  j/k move  Enter diff  \\f/g search  Q quit"
        }
        (AppView::GraphDetails, false) => "q/Esc Graph  j/k file  Enter diff  Q quit",
        (AppView::FileHistory, true) => {
            "q/Esc back  ^w h/j pane  j/k history  Enter full  \\f/g search  Q quit"
        }
        (AppView::FileHistory, false) => "q/Esc back  j/k history  Enter full  Q quit",
        (AppView::Code, true) => {
            "\\4 Code  h/j/k/l move  ^w h/j pane  w/b word  K hover  gd definition  ^o/^i jump  Q quit"
        }
        (AppView::Code, false) => "h/l cursor/pane  j/k line  K hover  gd definition  Q quit",
        (_, true) => {
            "\\1/2/3 Git  \\4 Code  h/j/k/l move  ^w h/j pane  Enter open  \\f/g search  r refresh  \\m message  F1 help  Q quit"
        }
        (_, false) => "\\1-3 Git  \\4 Code  \\f/g find  Q quit",
    };
    let root = if area.width >= 180 {
        format!(" | {}", sanitize_inline(&state.root.to_string()))
    } else {
        String::new()
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" {view} "),
            Style::default().bg(Color::Blue).fg(Color::White),
        ),
        Span::raw(format!(
            " {}{notice}{lsp} | {controls}{root}",
            sanitize_inline(&comparison),
        )),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.overlay {
        Overlay::None => {}
        Overlay::Help => {
            let popup = centered(area, 76, 88);
            frame.render_widget(Clear, popup);
            let text = vec![
                plain("ChronoGit keys"),
                plain("\\1..\\4    Changes / History / Graph / Code"),
                plain("\\f / \\g    Search files / repository content"),
                plain("Ctrl-w h/j  Focus previous / next pane (k/l/w/W also work)"),
                plain("h j k l     Character / line motions; Space/Backspace may wrap"),
                plain("w/W e/E b/B ge/gE   Word / WORD motions"),
                plain("0 ^ $ g_    Line start / first nonblank / end / last nonblank"),
                plain("f F t T     Find/till a character; ;/, repeat/reverse"),
                plain("gg G % g% () {} [[ ]]   Buffer and structural motions"),
                plain("Ctrl-u/d    Half page; Ctrl-b/f or PageUp/Down full page"),
                plain("H M L; zt zz zb       Window motions and cursor placement"),
                plain("zh/zl zH/zL zs/ze     Horizontal viewport motions"),
                plain("/ ? n N; * # g* g#    Search and search word at cursor"),
                plain("m{c} 'c/`c; ['/`[ ]'/`]   Mark jumps and scans"),
                plain("K; gd/gi/gy/gD   LSP hover and target navigation"),
                plain("Ctrl-o/i     Older / newer Vim, search, or LSP jump"),
                plain("r; \\m/\\b/\\t  Refresh; message / layout / commit tree"),
                plain("Enter       Open selection; move down in an opened document"),
                plain("F1 help; q/Esc close/back; Q/Ctrl-C quit"),
                plain("ChronoGit is read-only and never stages or commits changes."),
            ];
            frame.render_widget(
                Paragraph::new(text)
                    .block(Block::default().title(" Help ").borders(Borders::ALL))
                    .wrap(Wrap { trim: false }),
                popup,
            );
        }
        Overlay::CommitMessage => {
            let popup = centered(area, 82, 78);
            frame.render_widget(Clear, popup);
            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(popup);
            let text = match &state.message.content {
                LoadState::Idle => "Select a commit.".to_owned(),
                LoadState::Loading { .. } => "Loading commit message…".to_owned(),
                LoadState::Failed(error) => {
                    format!("Error: {}", sanitize_inline(error.message()))
                }
                LoadState::Ready(message) => message.as_str().to_owned(),
            };
            let visible = usize::from(sections[0].height.saturating_sub(2)).max(1);
            let last = text.lines().count().saturating_sub(1);
            let cursor = state.message.scroll.min(last);
            let vertical = followed_scroll(cursor, state.message.viewport_vertical, visible);
            let lines = message_cursor_lines(&text, cursor, state.message.byte_column);
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .title(" Commit message [q/Esc: close, Enter: next line] ")
                            .borders(Borders::ALL),
                    )
                    .scroll((
                        vertical,
                        state.message.horizontal.min(usize::from(u16::MAX)) as u16,
                    )),
                sections[0],
            );
            render_search_bar(frame, sections[1], state);
        }
        Overlay::Diff => render_diff_overlay(frame, area, state),
        Overlay::RepositorySearch => render_repository_search_overlay(frame, area, state),
        Overlay::FileContent => render_file_content_overlay(frame, area, state),
        Overlay::CodeContent => render_code_content_overlay(frame, area, state),
        Overlay::SemanticTargets => render_semantic_targets(frame, area, state),
        Overlay::LspHover => {
            if state.lsp_hover.return_overlay == Overlay::CodeContent {
                render_code_content_overlay(frame, area, state);
            }
            render_lsp_hover(frame, area, state);
        }
    }
}

fn render_lsp_hover(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = centered(area, 82, 62);
    frame.render_widget(Clear, popup);
    let text = match &state.lsp_hover.content {
        LoadState::Idle => "No hover request.".to_owned(),
        LoadState::Loading { .. } => "Loading hover information…".to_owned(),
        LoadState::Failed(error) => format!("Error: {}", sanitize_inline(error.message())),
        LoadState::Ready(None) => "No hover information at the current cursor.".to_owned(),
        LoadState::Ready(Some(content)) => sanitize_multiline(content),
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .title(" LSP hover [j/k: scroll, K/q/Esc: close] ")
                    .borders(Borders::ALL),
            )
            .scroll((state.lsp_hover.scroll.min(usize::from(u16::MAX)) as u16, 0))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn render_repository_search_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = centered(area, 86, 82);
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(popup);
    let mode = state.repository_search.kind.label();
    let query = state
        .repository_search
        .prompt
        .as_deref()
        .unwrap_or(&state.repository_search.query);
    let cursor = if state.repository_search.prompt.is_some() {
        "█"
    } else {
        ""
    };
    let prompt_active = state.repository_search.prompt.is_some();
    let search_title = if prompt_active {
        format!("Search {mode} [live; Enter/Ctrl-j: results, Esc: close]")
    } else {
        format!("Search {mode} [Ctrl-w k: edit again, q/Esc: close]")
    };
    frame.render_widget(
        Paragraph::new(format!("> {}{cursor}", sanitize_inline(query)))
            .block(pane_block(&search_title, prompt_active)),
        sections[0],
    );
    let lines = match &state.repository_search.results {
        LoadState::Idle => vec![plain(match state.repository_search.kind {
            RepositorySearchKind::Files => "Type part of a path; an empty query lists all files.",
            RepositorySearchKind::Content => "Type a fixed text string to grep the working tree.",
        })],
        LoadState::Loading { .. } => vec![plain("Searching…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(results) if results.is_empty() => vec![plain("No matches.")],
        LoadState::Ready(results) => results
            .iter()
            .enumerate()
            .map(|(index, hit)| {
                let suffix = hit.line().map_or_else(String::new, |line| {
                    format!(":{line}: {}", sanitize_inline(hit.preview()))
                });
                selected_line(
                    state.repository_search.selection.index() == Some(index),
                    format!("{}{suffix}", sanitize_inline(&hit.path().display())),
                )
            })
            .collect(),
    };
    let results_title = if prompt_active {
        "Results [live preview; Enter/Ctrl-j: focus]"
    } else {
        "Results [j/k: move, Enter: open, Ctrl-w k: search, q/Esc: close]"
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(results_title, !prompt_active))
            .scroll((
                list_scroll(state.repository_search.selection.index(), sections[1]),
                0,
            )),
        sections[1],
    );
}

fn render_file_content_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(popup);
    let path = state
        .file_view
        .path
        .as_ref()
        .map(|path| sanitize_inline(&path.display()))
        .unwrap_or_else(|| "file".to_owned());
    render_file_content(
        frame,
        sections[0],
        state,
        &format!("{path} [q/Esc: close, Enter: next line]"),
    );
    render_search_bar(frame, sections[1], state);
}

fn render_code_content_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(popup);
    let path = state
        .code_view
        .path
        .as_ref()
        .map(|path| sanitize_inline(&path.display()))
        .unwrap_or_else(|| "Code".to_owned());
    render_code_content_with_title(
        frame,
        sections[0],
        state,
        &format!("{path} [q/Esc: close, Enter: next line]"),
    );
    render_search_bar(frame, sections[1], state);
}

fn render_code_content_with_title(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    title: &str,
) {
    let block = pane_block(title, true);
    let lines = match &state.code_view.content {
        LoadState::Idle => vec![plain("Select a file to view its current content.")],
        LoadState::Loading { .. } => vec![plain("Loading current content…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(document) => code_document_lines(document, state),
    };
    let (vertical, horizontal) = code_scroll(state, area, lines.len());
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((vertical, horizontal)),
        area,
    );
}

fn render_semantic_targets(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = centered(area, 82, 72);
    frame.render_widget(Clear, popup);
    let operation = state
        .semantic_navigation
        .kind
        .map_or("semantic", crate::domain::SemanticNavigationKind::label);
    let lines = match &state.semantic_navigation.targets {
        LoadState::Ready(targets) => targets
            .iter()
            .enumerate()
            .map(|(index, target)| {
                selected_line(
                    state.semantic_navigation.selection.index() == Some(index),
                    sanitize_inline(&target.display()),
                )
            })
            .collect(),
        LoadState::Idle => vec![plain("No targets.")],
        LoadState::Loading { .. } => vec![plain("Loading semantic targets…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(
                        " {operation} targets [j/k: move, Enter: open, q/Esc: close] "
                    ))
                    .borders(Borders::ALL),
            )
            .scroll((
                list_scroll(state.semantic_navigation.selection.index(), popup),
                0,
            )),
        popup,
    );
}

fn code_scroll(state: &AppState, area: Rect, line_count: usize) -> (u16, u16) {
    let visible_lines = usize::from(area.height.saturating_sub(2)).max(1);
    let cursor_line = usize::try_from(state.code_view.cursor.line())
        .unwrap_or(usize::MAX)
        .min(line_count.saturating_sub(1));
    let mut vertical = state.code_view.viewport_vertical;
    if cursor_line < vertical {
        vertical = cursor_line;
    } else if cursor_line >= vertical.saturating_add(visible_lines) {
        vertical = cursor_line.saturating_sub(visible_lines.saturating_sub(1));
    }

    let source_line = match &state.code_view.content {
        LoadState::Ready(document) => document.lines().get(cursor_line),
        LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
    };
    let cursor_display = source_line.map_or(0, |line| {
        crate::lsp::display_column(line, state.code_view.cursor.byte_column()).saturating_add(8)
    });
    let visible_columns = usize::from(area.width.saturating_sub(2)).max(1);
    let mut horizontal = state.code_view.viewport_horizontal;
    if cursor_display < horizontal {
        horizontal = cursor_display;
    } else if cursor_display >= horizontal.saturating_add(visible_columns) {
        horizontal = cursor_display.saturating_sub(visible_columns.saturating_sub(1));
    }
    (
        vertical.min(u16::MAX as usize) as u16,
        horizontal.min(u16::MAX as usize) as u16,
    )
}

fn render_diff_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(popup);
    let baseline = selected_baseline(state);
    let title = baseline.map_or_else(
        || "Diff [q/Esc: close, Enter: next line]".to_owned(),
        |value| format!("Diff — {value} [q/Esc: close, Enter: next line]"),
    );
    render_diff_pane(frame, sections[0], state, &title, true);
    render_search_bar(frame, sections[1], state);
}

fn render_search_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let line = if let Some((direction, input)) = state.search.prompt_text() {
        Line::from(vec![
            Span::styled(
                direction.prompt().to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(sanitize_inline(input)),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ])
    } else if state.search.query().is_empty() {
        Line::raw(" / forward search  ? backward search  n/N next/previous")
    } else {
        let position = state.search.current_ordinal().unwrap_or(0);
        Line::raw(format!(
            " {}{}  {position}/{}  [n/N: next/previous]",
            state.search.direction().prompt(),
            sanitize_inline(state.search.query()),
            state.search.match_count(),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn selected_baseline(state: &AppState) -> Option<String> {
    match state.view {
        AppView::Changes => return Some("index → working tree".to_owned()),
        AppView::FileHistory if !state.file_view.showing_history_diff => {
            return Some("current working tree file".to_owned());
        }
        AppView::FileHistory => {
            return match (&state.file_view.commits, state.file_view.selection.index()) {
                (LoadState::Ready(commits), Some(index)) => commits
                    .get(index)
                    .map(|commit| commit.baseline().to_string()),
                _ => None,
            };
        }
        AppView::Code => return Some("current working tree file".to_owned()),
        AppView::History | AppView::CommitDetails | AppView::Graph | AppView::GraphDetails => {}
    }
    match (&state.commits, state.commit_selection.index()) {
        (LoadState::Ready(commits), Some(index)) => commits
            .get(index)
            .map(|commit| commit.baseline().to_string()),
        _ => None,
    }
}

fn render_too_small(frame: &mut Frame<'_>, area: Rect) {
    let message = format!(
        "Terminal too small: {}x{}. ChronoGit needs at least {MIN_WIDTH}x{MIN_HEIGHT}. Press Q to quit.",
        area.width, area.height
    );
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .block(Block::default().title(" ChronoGit ").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn pane_block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(style)
}

fn selected_line(selected: bool, value: String) -> Line<'static> {
    if selected {
        Line::styled(
            format!("> {value}"),
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Line::raw(format!("  {value}"))
    }
}

fn plain(value: impl Into<String>) -> Line<'static> {
    Line::raw(value.into())
}

fn error_line(value: &str) -> Line<'static> {
    Line::styled(
        format!("Error: {}", sanitize_inline(value)),
        Style::default().fg(Color::Red),
    )
}

fn sanitize_inline(value: &str) -> String {
    sanitize(value, false)
}

fn sanitize_multiline(value: &str) -> String {
    sanitize(value, true)
}

fn sanitize(value: &str, preserve_newlines: bool) -> String {
    let mut safe = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' if preserve_newlines => safe.push('\n'),
            '\t' => safe.push_str("    "),
            character if character.is_control() => {
                safe.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
            }
            character => safe.push(character),
        }
    }
    safe
}

fn highlight_source_cursor(
    line: &mut Line<'static>,
    source: &str,
    requested: usize,
    prefix_spans: usize,
) {
    let mut column = requested.min(source.len());
    while !source.is_char_boundary(column) {
        column = column.saturating_sub(1);
    }
    if column == source.len() && !source.is_empty() {
        column = source
            .char_indices()
            .next_back()
            .map_or(0, |(byte, _)| byte);
    }
    let end = source[column..].chars().next().map_or(column, |character| {
        column.saturating_add(character.len_utf8())
    });
    let rendered_start = sanitize_inline(&source[..column]).len();
    let rendered_target = sanitize_inline(&source[column..end]);
    let rendered_end = rendered_start.saturating_add(rendered_target.len());
    let cursor_style = Style::default()
        .fg(Color::Black)
        .bg(Color::LightCyan)
        .add_modifier(Modifier::BOLD);
    let original = std::mem::take(&mut line.spans);
    let prefix_spans = prefix_spans.min(original.len());
    let mut spans = original[..prefix_spans].to_vec();
    let mut offset = 0usize;
    let mut inserted_width_marker = false;
    for span in &original[prefix_spans..] {
        let content = span.content.as_ref();
        let span_end = offset.saturating_add(content.len());
        let overlap_start = rendered_start.max(offset).min(span_end);
        let overlap_end = rendered_end.max(offset).min(span_end);
        if overlap_start >= overlap_end {
            spans.push(span.clone());
        } else {
            let local_start = overlap_start.saturating_sub(offset);
            let local_end = overlap_end.saturating_sub(offset);
            if local_start > 0 {
                spans.push(Span::styled(content[..local_start].to_owned(), span.style));
            }
            if !inserted_width_marker && UnicodeWidthStr::width(&source[column..end]) == 0 {
                spans.push(Span::styled("▏", span.style.patch(cursor_style)));
                inserted_width_marker = true;
            }
            spans.push(Span::styled(
                content[local_start..local_end].to_owned(),
                span.style.patch(cursor_style),
            ));
            if local_end < content.len() {
                spans.push(Span::styled(content[local_end..].to_owned(), span.style));
            }
        }
        offset = span_end;
    }
    if rendered_start == rendered_end {
        spans.push(Span::styled(" ", cursor_style));
    }
    line.spans = spans;
}

fn message_cursor_lines(text: &str, cursor: usize, byte_column: usize) -> Vec<Line<'static>> {
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(index, source)| {
            if index == cursor {
                let mut line = Line::raw(sanitize_inline(source));
                highlight_source_cursor(&mut line, source, byte_column, 0);
                line
            } else {
                Line::raw(sanitize_inline(source))
            }
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        let mut line = Line::raw("");
        highlight_source_cursor(&mut line, "", 0, 0);
        lines.push(line);
    }
    lines
}

fn followed_scroll(cursor: usize, requested: usize, visible: usize) -> u16 {
    let top = if cursor < requested {
        cursor
    } else if cursor >= requested.saturating_add(visible) {
        cursor.saturating_sub(visible.saturating_sub(1))
    } else {
        requested
    };
    top.min(usize::from(u16::MAX)) as u16
}

fn list_scroll(selection: Option<usize>, area: Rect) -> u16 {
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    selection
        .unwrap_or(0)
        .saturating_sub(visible.saturating_sub(1))
        .min(usize::from(u16::MAX)) as u16
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    use super::{
        code_document_lines, diff_line, diff_lines, file_document_lines, render, sanitize_inline,
        sanitize_multiline,
    };
    use crate::app::{
        Action, AppState, AppView, ErrorNotice, Event, FocusedPane, GitEffect, LoadState, Overlay,
        RepositorySearchKind,
    };
    use crate::domain::{
        ChangeKind, ChangedFile, CommitBaseline, CommitMessage, CommitSummary, DiffDocument,
        DiffLine, DiffLineKind, DiffTarget, FileDocument, ObjectId, RepoPath, RepositoryRoot,
        SearchHit, SourcePosition, WorktreeChange,
    };

    fn state() -> AppState {
        let root = RepositoryRoot::new(PathBuf::from("/tmp/repo"))
            .unwrap_or_else(|error| panic!("{error}"));
        AppState::new(root, AppView::Changes)
    }

    #[test]
    fn renders_supported_and_too_small_sizes() {
        for (width, height) in [(80, 24), (140, 40), (40, 10)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend)
                .unwrap_or_else(|error| panic!("could not create terminal: {error}"));
            let state = state();
            terminal
                .draw(|frame| render(frame, &state))
                .unwrap_or_else(|error| panic!("could not draw: {error}"));
            let text = buffer_text(terminal.backend());
            if width < 80 || height < 24 {
                assert!(text.contains("Terminal too small"));
            } else {
                assert!(text.contains("Unstaged changes"));
            }
        }
    }

    #[test]
    fn message_motion_cursor_stays_visible_after_tabs_and_long_lines() {
        for overlay in [Overlay::CommitMessage, Overlay::None] {
            let mut state = state();
            state.view = AppView::CommitDetails;
            state.focus = FocusedPane::Secondary;
            state.overlay = overlay;
            state.set_terminal_size(80, 24);
            let body = format!("\t{}界\nnext", "x".repeat(120));
            state.message.content =
                LoadState::Ready(CommitMessage::new(format!("subject\n\n{body}")));
            state.message.scroll = if overlay == Overlay::None { 0 } else { 2 };
            let _none = state.handle_action(Action::VimMotion(crate::app::VimMotion::new(
                crate::app::VimMotionKind::LineEnd,
            )));
            assert!(state.message.horizontal > 0);
            let backend = TestBackend::new(80, 24);
            let mut terminal = Terminal::new(backend).unwrap_or_else(|error| panic!("{error}"));
            terminal
                .draw(|frame| render(frame, &state))
                .unwrap_or_else(|error| panic!("{error}"));
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .any(|cell| cell.symbol() == "界" && cell.bg == Color::LightCyan)
            );
            let _none = state.handle_action(Action::VimMotion(crate::app::VimMotion::new(
                crate::app::VimMotionKind::NextLineFirstNonBlank,
            )));
            terminal
                .draw(|frame| render(frame, &state))
                .unwrap_or_else(|error| panic!("{error}"));
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .any(|cell| cell.symbol() == "n" && cell.bg == Color::LightCyan)
            );
        }
    }

    #[test]
    fn renders_scrollable_lsp_hover_over_code_content() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)
            .unwrap_or_else(|error| panic!("could not create terminal: {error}"));
        let mut state = AppState::new(
            RepositoryRoot::new(PathBuf::from("/tmp/repo"))
                .unwrap_or_else(|error| panic!("root: {error}")),
            AppView::Code,
        );
        state.focus = FocusedPane::Diff;
        state.code_view.path = Some(
            RepoPath::from_bytes(b"src/main.rs".to_vec())
                .unwrap_or_else(|error| panic!("path: {error}")),
        );
        state.code_view.content = LoadState::Ready(FileDocument::Text {
            source: "struct Action;".to_owned(),
            lines: vec!["struct Action;".to_owned()],
            valid_utf8: true,
            truncated: false,
        });
        state.lsp_hover.return_overlay = Overlay::CodeContent;
        state.lsp_hover.content =
            LoadState::Ready(Some("struct Action\n\nA semantic action.".to_owned()));
        state.overlay = Overlay::LspHover;

        terminal
            .draw(|frame| render(frame, &state))
            .unwrap_or_else(|error| panic!("could not draw: {error}"));
        let text = buffer_text(terminal.backend());
        assert!(text.contains("LSP hover"));
        assert!(text.contains("A semantic action."));
        assert!(text.contains("src/main.rs"));
    }

    #[test]
    fn renders_code_tree_preview_and_full_content_overlay() {
        let mut state = AppState::new(
            RepositoryRoot::new(PathBuf::from("/tmp/repo"))
                .unwrap_or_else(|error| panic!("{error}")),
            AppView::Code,
        );
        let tree_request = match state.start().first() {
            Some(GitEffect::LoadCodeTree { request_id }) => *request_id,
            other => panic!("expected code-tree request, got {other:?}"),
        };
        let path = RepoPath::from_bytes(b"README.md".to_vec())
            .unwrap_or_else(|error| panic!("invalid fixture path: {error}"));
        let file_effects = state.handle_event(Event::CodeTreeLoaded {
            request_id: tree_request,
            result: Ok(vec![path.clone()]),
        });
        let file_request = match file_effects.first() {
            Some(GitEffect::LoadCodeFile { request_id, .. }) => *request_id,
            other => panic!("expected code-file request, got {other:?}"),
        };
        let _none = state.handle_event(Event::CodeFileLoaded {
            request_id: file_request,
            path,
            result: Ok(FileDocument::Text {
                lines: vec!["code viewer content".to_owned()],
                source: "code viewer content".to_owned(),
                valid_utf8: true,
                truncated: false,
            }),
        });

        let text = rendered_text(&state, 100, 30);
        assert!(text.contains("Working tree"));
        assert!(text.contains("README.md"));
        assert!(text.contains("code viewer content"));

        let _none = state.handle_action(Action::Activate);
        assert_eq!(state.overlay, Overlay::CodeContent);
        let overlay = rendered_text(&state, 100, 30);
        assert!(overlay.contains("q/Esc: close, Enter: next line"));
        assert!(overlay.contains("forward search"));
    }

    #[test]
    fn renders_help_overlay() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend)
            .unwrap_or_else(|error| panic!("could not create terminal: {error}"));
        let mut state = state();
        state.overlay = Overlay::Help;
        terminal
            .draw(|frame| render(frame, &state))
            .unwrap_or_else(|error| panic!("could not draw: {error}"));
        let text = buffer_text(terminal.backend());
        assert!(text.contains("ChronoGit keys"));
        assert!(text.contains("F1 help; q/Esc close/back; Q/Ctrl-C quit"));
    }

    #[test]
    fn renders_the_complete_commit_message_overlay() {
        let mut state = state();
        state.overlay = Overlay::CommitMessage;
        state.message.content = LoadState::Ready(CommitMessage::new(
            "overlay subject\n\noverlay body\nTrailer: value\n".to_owned(),
        ));

        let text = rendered_text(&state, 100, 30);
        assert!(text.contains("Commit message"));
        assert!(text.contains("overlay subject"));
        assert!(text.contains("overlay body"));
        assert!(text.contains("Trailer: value"));
    }

    #[test]
    fn renders_loading_empty_error_and_truncated_states() {
        let mut loading = state();
        let _effects = loading.start();
        assert!(rendered_text(&loading, 100, 30).contains("Loading changes"));

        let mut empty = state();
        empty.changes = LoadState::Ready(Vec::new());
        assert!(rendered_text(&empty, 100, 30).contains("No unstaged changes"));

        let mut failed = state();
        failed.changes = LoadState::Failed(ErrorNotice::new("Git read failed"));
        assert!(rendered_text(&failed, 100, 30).contains("Error: Git read failed"));

        let mut truncated = state();
        truncated.focus = FocusedPane::Diff;
        truncated.diff.content = LoadState::Ready(DiffDocument::Truncated {
            lines: vec![DiffLine::new(
                DiffLineKind::Added,
                None,
                None,
                "+new line".to_owned(),
            )],
            bytes: 8 * 1024 * 1024,
        });
        assert!(
            rendered_text(&truncated, 100, 30).contains("diff truncated at the safe output limit")
        );
    }

    #[test]
    fn renders_the_selected_commit_baseline_in_the_footer() {
        let mut state = AppState::new(
            RepositoryRoot::new(PathBuf::from("/tmp/repo"))
                .unwrap_or_else(|error| panic!("{error}")),
            AppView::History,
        );
        state.commits = LoadState::Ready(vec![CommitSummary::new(
            ObjectId::parse("a".repeat(40)).unwrap_or_else(|error| panic!("{error}")),
            Vec::new(),
            "Author".to_owned(),
            "2026-08-29T00:00:00Z".to_owned(),
            "root".to_owned(),
        )]);
        state.commit_selection.reset(1);

        let text = rendered_text(&state, 80, 24);
        assert!(
            text.contains("empty tree (root commit)"),
            "the footer must make the root-commit comparison explicit"
        );
        assert!(
            text.contains("Q quit"),
            "the minimum-width footer needs a quit hint"
        );
    }

    #[test]
    fn history_uses_three_full_width_rows_even_at_minimum_width() {
        let mut state = AppState::new(
            RepositoryRoot::new(PathBuf::from("/tmp/repo"))
                .unwrap_or_else(|error| panic!("{error}")),
            AppView::History,
        );
        let commit = CommitSummary::new(
            ObjectId::parse("a".repeat(40)).unwrap_or_else(|error| panic!("{error}")),
            Vec::new(),
            "Author".to_owned(),
            "2026-08-29T00:00:00Z".to_owned(),
            "a readable commit subject".to_owned(),
        );
        let path = RepoPath::from_bytes(b"src/readable_file_name.rs".to_vec())
            .unwrap_or_else(|error| panic!("{error}"));
        state.commits = LoadState::Ready(vec![commit.clone()]);
        state.commit_selection.reset(1);
        state.files = LoadState::Ready(vec![ChangedFile::new(
            path.clone(),
            None,
            ChangeKind::Modified,
        )]);
        state.file_selection.reset(1);
        state.diff.target = Some(DiffTarget::Commit {
            commit: commit.id().clone(),
            baseline: CommitBaseline::EmptyTree,
            path,
        });
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: vec![DiffLine::new(
                DiffLineKind::Added,
                None,
                None,
                "+readable diff content".to_owned(),
            )],
            bytes: 22,
        });

        let text = rendered_text(&state, 80, 24);
        assert!(text.contains("a readable commit subject"));
        assert!(text.contains("src/readable_file_name.rs"));
        assert!(text.contains("readable diff content"));
    }

    #[test]
    fn commit_details_uses_commit_list_body_and_changed_file_rows() {
        let mut state = AppState::new(
            RepositoryRoot::new(PathBuf::from("/tmp/repo"))
                .unwrap_or_else(|error| panic!("{error}")),
            AppView::CommitDetails,
        );
        let commit = CommitSummary::new(
            ObjectId::parse("b".repeat(40)).unwrap_or_else(|error| panic!("{error}")),
            Vec::new(),
            "Author".to_owned(),
            "2026-08-30T00:00:00Z".to_owned(),
            "details page subject".to_owned(),
        );
        let path = RepoPath::from_bytes(b"src/details.rs".to_vec())
            .unwrap_or_else(|error| panic!("{error}"));
        state.commits = LoadState::Ready(vec![commit]);
        state.commit_selection.reset(1);
        state.message.content = LoadState::Ready(CommitMessage::new(
            "details page subject\n\nbody displayed in the middle row\n".to_owned(),
        ));
        state.files = LoadState::Ready(vec![ChangedFile::new(path, None, ChangeKind::Modified)]);
        state.file_selection.reset(1);

        let text = rendered_text(&state, 80, 24);
        assert!(text.contains("Commits"));
        assert!(text.contains("details page subject"));
        assert!(text.contains("Commit body"));
        assert!(text.contains("body displayed in the middle row"));
        assert!(text.contains("src/details.rs"));
    }

    #[test]
    fn assigns_subtle_backgrounds_and_distinct_semantic_colors_to_diff_lines() {
        let added = diff_line(
            &DiffLine::new(DiffLineKind::Added, None, None, "+added".to_owned()),
            None,
        );
        assert_eq!(added.style.bg, Some(Color::Rgb(33, 58, 43)));
        assert!(added.spans.iter().any(|span| {
            span.content == "+" && span.style.fg == Some(Color::Rgb(166, 227, 161))
        }));

        let removed = diff_line(
            &DiffLine::new(DiffLineKind::Removed, None, None, "-removed".to_owned()),
            None,
        );
        assert_eq!(removed.style.bg, Some(Color::Rgb(74, 34, 29)));
        assert!(removed.spans.iter().any(|span| {
            span.content == "-" && span.style.fg == Some(Color::Rgb(243, 139, 168))
        }));

        let hunk = diff_line(
            &DiffLine::new(DiffLineKind::Hunk, None, None, "@@ -1 +1 @@".to_owned()),
            None,
        );
        assert_eq!(hunk.style.bg, Some(Color::Rgb(49, 50, 68)));
        assert_eq!(hunk.spans[1].style.fg, Some(Color::Rgb(137, 180, 250)));

        for kind in [DiffLineKind::Header, DiffLineKind::Meta] {
            let line = diff_line(
                &DiffLine::new(kind, None, None, "metadata".to_owned()),
                None,
            );
            assert!(line.spans[1].style.fg.is_some());
        }
        let context = diff_line(
            &DiffLine::new(DiffLineKind::Context, None, None, " context".to_owned()),
            None,
        );
        assert_eq!(context.style.bg, None);
    }

    #[test]
    fn syntax_highlights_code_inside_diff_hunks() {
        let mut state = state();
        state.diff.target = Some(DiffTarget::Worktree {
            path: RepoPath::from_bytes(b"src/example.rs".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
            untracked: false,
        });
        let document = DiffDocument::Text {
            lines: vec![
                DiffLine::new(DiffLineKind::Hunk, None, None, "@@ -1 +1 @@".to_owned()),
                DiffLine::new(
                    DiffLineKind::Added,
                    None,
                    None,
                    "+pub fn answer() -> u32 { 42 }".to_owned(),
                ),
            ],
            bytes: 45,
        };
        state.focus = FocusedPane::Diff;
        state.diff.vertical = 1;
        state.diff.byte_column = 5;
        state.diff.content = LoadState::Ready(document.clone());

        let lines = diff_lines(&document, &state);
        let colors = lines[1]
            .spans
            .iter()
            .filter_map(|span| span.style.fg)
            .collect::<std::collections::HashSet<_>>();
        assert!(colors.len() > 3, "diff code should retain token colors");
    }

    #[test]
    fn marks_the_diff_line_and_character_selected_by_vim_motions() {
        let mut state = state();
        state.focus = FocusedPane::Diff;
        let document = DiffDocument::Text {
            lines: vec![
                DiffLine::new(DiffLineKind::Context, None, None, "first".to_owned()),
                DiffLine::new(DiffLineKind::Context, None, None, "second".to_owned()),
            ],
            bytes: 11,
        };
        state.diff.content = LoadState::Ready(document.clone());

        let initial = diff_lines(&document, &state);
        assert_eq!(initial[0].spans[0].content, "▌");
        assert_eq!(initial[0].style.bg, None);
        assert_eq!(
            initial[0]
                .spans
                .iter()
                .filter(|span| span.style.bg == Some(Color::LightCyan))
                .count(),
            1
        );
        assert_eq!(initial[1].spans[0].content, " ");
        assert_eq!(initial[1].style.bg, None);

        let _none = state.handle_action(Action::MoveDown);
        let moved = diff_lines(&document, &state);
        assert_eq!(moved[0].spans[0].content, " ");
        assert_eq!(moved[1].spans[0].content, "▌");
        assert_eq!(moved[1].style.bg, None);

        let _none = state.handle_action(Action::MoveUp);
        let returned = diff_lines(&document, &state);
        assert_eq!(returned[0].spans[0].content, "▌");
    }

    #[test]
    fn marks_the_selected_current_file_line_without_a_background_override() {
        let mut state = state();
        state.focus = FocusedPane::Diff;
        state.file_view.path = Some(
            RepoPath::from_bytes(b"src/example.rs".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
        );
        let document = FileDocument::Text {
            lines: vec![
                "pub fn first() {}".to_owned(),
                "pub fn second() {}".to_owned(),
            ],
            source: "pub fn first() {}\npub fn second() {}".to_owned(),
            valid_utf8: true,
            truncated: false,
        };

        let first = file_document_lines(&document, &state);
        assert_eq!(first[0].spans[0].content, "▌");
        assert!(first[0].spans.iter().all(|span| span.style.bg.is_none()));

        state.file_view.vertical = 1;
        let second = file_document_lines(&document, &state);
        assert_eq!(second[0].spans[0].content, " ");
        assert_eq!(second[1].spans[0].content, "▌");
        assert!(second[1].spans.iter().all(|span| span.style.bg.is_none()));
    }

    #[test]
    fn renders_the_code_cursor_at_a_multibyte_and_combining_position() {
        let mut state = state();
        state.view = AppView::Code;
        state.focus = FocusedPane::Diff;
        state.overlay = Overlay::CodeContent;
        state.code_view.path = Some(
            RepoPath::from_bytes(b"src/example.rs".to_vec())
                .unwrap_or_else(|error| panic!("path: {error}")),
        );
        state.code_view.content = LoadState::Ready(FileDocument::Text {
            source: "a界e\u{301}".to_owned(),
            lines: vec!["a界e\u{301}".to_owned()],
            valid_utf8: true,
            truncated: false,
        });
        state.code_view.cursor = SourcePosition::new(0, 1);
        let wide = code_document_lines(
            match &state.code_view.content {
                LoadState::Ready(document) => document,
                _ => panic!("document must be ready"),
            },
            &state,
        );
        assert!(
            wide[0]
                .spans
                .iter()
                .any(|span| { span.content == "界" && span.style.bg == Some(Color::LightCyan) })
        );

        state.code_view.cursor = SourcePosition::new(0, "a界e".len());
        let combining = code_document_lines(
            match &state.code_view.content {
                LoadState::Ready(document) => document,
                _ => panic!("document must be ready"),
            },
            &state,
        );
        assert!(
            combining[0]
                .spans
                .iter()
                .any(|span| span.content.starts_with('▏'))
        );
    }

    #[test]
    fn keeps_the_selected_list_row_inside_the_viewport() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)
            .unwrap_or_else(|error| panic!("could not create terminal: {error}"));
        let mut state = state();
        state.changes = LoadState::Ready(
            (0..40)
                .map(|index| {
                    let path = RepoPath::from_bytes(format!("file-{index:02}").into_bytes())
                        .unwrap_or_else(|error| panic!("invalid fixture path: {error}"));
                    WorktreeChange::new(path, None, ChangeKind::Modified)
                })
                .collect(),
        );
        let _effects = state.handle_action(Action::MoveBottom);
        terminal
            .draw(|frame| render(frame, &state))
            .unwrap_or_else(|error| panic!("could not draw: {error}"));
        assert!(buffer_text(terminal.backend()).contains("file-39"));
    }

    #[test]
    fn floating_diff_can_scroll_to_the_last_line() {
        let mut state = state();
        state.diff.target = Some(DiffTarget::Worktree {
            path: RepoPath::from_bytes(b"long.txt".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
            untracked: false,
        });
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: (0..60)
                .map(|index| {
                    DiffLine::new(
                        DiffLineKind::Context,
                        None,
                        None,
                        format!("line {index:02}"),
                    )
                })
                .collect(),
            bytes: 480,
        });
        state.overlay = Overlay::Diff;
        let _none = state.handle_action(Action::MoveBottom);

        let text = rendered_text(&state, 80, 24);
        assert!(text.contains("line 59"));
        assert!(text.contains("? backward search"));

        let _none = state.handle_action(Action::MoveUp);
        let moved_up = rendered_text(&state, 80, 24);
        assert!(moved_up.contains("line 40"));
    }

    #[test]
    fn sanitizes_terminal_control_characters_but_preserves_message_lines() {
        assert_eq!(
            sanitize_inline("path\u{1b}[2J\tname"),
            "path\\u{1b}[2J    name"
        );
        assert_eq!(
            sanitize_multiline("line one\nline\u{7} two"),
            "line one\nline\\u{7} two"
        );
    }

    #[test]
    fn renders_graph_graph_details_and_file_history_views() {
        let first = CommitSummary::new(
            ObjectId::parse("a".repeat(40)).unwrap_or_else(|error| panic!("{error}")),
            vec![ObjectId::parse("b".repeat(40)).unwrap_or_else(|error| panic!("{error}"))],
            "Ada".to_owned(),
            "2026-09-01T00:00:00Z".to_owned(),
            "graph subject".to_owned(),
        );
        let mut graph = AppState::new(
            RepositoryRoot::new(PathBuf::from("/tmp/repo"))
                .unwrap_or_else(|error| panic!("{error}")),
            AppView::Graph,
        );
        graph.commits = LoadState::Ready(vec![first.clone()]);
        graph.commit_selection.reset(1);
        let graph_text = rendered_text(&graph, 100, 30);
        assert!(graph_text.contains("Git graph"));
        assert!(graph_text.contains("●"));
        assert!(graph_text.contains("graph subject"));

        graph.view = AppView::GraphDetails;
        graph.focus = FocusedPane::Secondary;
        graph.files = LoadState::Ready(vec![ChangedFile::new(
            RepoPath::from_bytes(b"src/graph.rs".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
            None,
            ChangeKind::Modified,
        )]);
        graph.file_selection.reset(1);
        let details = rendered_text(&graph, 100, 30);
        assert!(details.contains("q/Esc: graph"));
        assert!(details.contains("Changed files"));

        graph.view = AppView::FileHistory;
        graph.focus = FocusedPane::Primary;
        graph.file_view.path = Some(
            RepoPath::from_bytes(b"src/search.rs".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
        );
        graph.file_view.commits = LoadState::Ready(vec![first]);
        graph.file_view.selection.reset(1);
        graph.file_view.content = LoadState::Ready(FileDocument::Text {
            lines: vec!["current source line".to_owned()],
            source: "current source line".to_owned(),
            valid_utf8: true,
            truncated: false,
        });
        let file_text = rendered_text(&graph, 100, 30);
        assert!(file_text.contains("History"));
        assert!(file_text.contains("Current working tree content"));
        assert!(file_text.contains("current source line"));
    }

    #[test]
    fn renders_repository_search_prompt_and_results() {
        let mut state = state();
        state.overlay = Overlay::RepositorySearch;
        state.repository_search.kind = RepositorySearchKind::Content;
        state.repository_search.prompt = None;
        state.repository_search.query = "needle".to_owned();
        state.repository_search.results = LoadState::Ready(vec![SearchHit::content(
            RepoPath::from_bytes(b"src/lib.rs".to_vec()).unwrap_or_else(|error| panic!("{error}")),
            42,
            "let needle = true;".to_owned(),
        )]);
        state.repository_search.selection.reset(1);

        let text = rendered_text(&state, 100, 30);
        assert!(text.contains("Search content"));
        assert!(text.contains("Ctrl-w k: edit again"));
        assert!(text.contains("src/lib.rs:42"));
        assert!(text.contains("let needle = true"));

        state.repository_search.prompt = Some("needle".to_owned());
        let prompt_text = rendered_text(&state, 100, 30);
        assert!(prompt_text.contains("Enter/Ctrl-j: results"));
        assert!(prompt_text.contains("Enter/Ctrl-j: focus"));
    }

    fn buffer_text(backend: &TestBackend) -> String {
        backend
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    fn rendered_text(state: &AppState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend)
            .unwrap_or_else(|error| panic!("could not create terminal: {error}"));
        terminal
            .draw(|frame| render(frame, state))
            .unwrap_or_else(|error| panic!("could not draw: {error}"));
        buffer_text(terminal.backend())
    }
}
