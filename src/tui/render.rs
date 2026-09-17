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
    AppState, AppView, CodeEntryKind, FocusedPane, FullFileDeletion, FullFileMode, HistoryPanel,
    LoadState, Overlay, RepositorySearchKind, VisibleCodeEntry, VisibleTreeEntry,
};
use crate::domain::{DiffDocument, DiffLine, DiffLineKind, DiffTarget, FileDocument, TreeKind};
use crate::layout::{
    CHANGES_DIFF_PERCENT, CHANGES_LIST_PERCENT, CODE_CONTENT_PERCENT, CODE_TREE_PERCENT,
    COMMIT_DETAILS_BODY_PERCENT, COMMIT_DETAILS_FILES_PERCENT, COMMIT_DETAILS_LIST_PERCENT,
    DOCUMENT_OVERLAY_INSET, DOCUMENT_OVERLAY_MARGIN, FILE_HISTORY_CONTENT_PERCENT,
    FILE_HISTORY_LIST_PERCENT, FOOTER_ROWS, FULL_PERCENT, GRAPH_DETAILS_DIFF_PERCENT,
    GRAPH_DETAILS_FILES_PERCENT, GRAPH_DETAILS_HEIGHT_PERCENT, GRAPH_DETAILS_WIDTH_PERCENT,
    HELP_HEIGHT_PERCENT, HELP_WIDTH_PERCENT, HISTORY_DIFF_PERCENT, HISTORY_LIST_PERCENT,
    HISTORY_MIDDLE_PERCENT, HOVER_HEIGHT_PERCENT, HOVER_WIDTH_PERCENT, MESSAGE_HEIGHT_PERCENT,
    MESSAGE_WIDTH_PERCENT, MIN_CONTENT_ROWS, MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH,
    PANE_BORDER_CELLS, REPOSITORY_SEARCH_HEIGHT_PERCENT, REPOSITORY_SEARCH_WIDTH_PERCENT,
    ROOT_DISPLAY_WIDTH, SEARCH_BAR_ROWS, SEARCH_INPUT_ROWS, SOURCE_GUTTER_COLUMNS,
    SYMBOL_HEIGHT_PERCENT, SYMBOL_WIDTH_PERCENT, WIDE_LAYOUT_WIDTH,
};
use crate::tui::graph::graph_prefixes;
use crate::tui::highlight::{highlight_code, source_is_too_large};

const ISO_DATE_PREFIX_BYTES: usize = 10;
const ADDED_FOREGROUND: Color = Color::Rgb(166, 227, 161);
const ADDED_BACKGROUND: Color = Color::Rgb(33, 58, 43);
const CHANGED_LINE_BACKGROUND: Color = Color::Rgb(33, 53, 43);
const REMOVED_FOREGROUND: Color = Color::Rgb(243, 139, 168);
const REMOVED_BACKGROUND: Color = Color::Rgb(74, 34, 29);
const DIFF_ACCENT: Color = Color::Rgb(137, 180, 250);
const DIFF_HUNK_BACKGROUND: Color = Color::Rgb(49, 50, 68);
const DIFF_META_FOREGROUND: Color = Color::Rgb(249, 226, 175);
const GUTTER_FOREGROUND: Color = Color::Rgb(108, 112, 134);

#[derive(Clone, Copy)]
struct PrefixSpanCount(usize);

impl PrefixSpanCount {
    const NONE: Self = Self(0);
    const LINE_NUMBER: Self = Self(1);
    const NAVIGATION: Self = Self(1);
    const NAVIGATION_AND_LINE_NUMBER: Self = Self(2);

    const fn value(self) -> usize {
        self.0
    }
}

/// Renders one complete frame from an immutable application-state snapshot.
///
/// The function sanitizes repository-provided text before placing it in terminal
/// cells and replaces unsupported terminal sizes with a resize message.
pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
        render_too_small(frame, area);
        return;
    }
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(MIN_CONTENT_ROWS),
            Constraint::Length(FOOTER_ROWS),
        ])
        .split(area);
    render_main(frame, sections[0], state);
    render_footer(frame, sections[1], state);
    render_overlay(frame, area, state);
    if state.branch_picker.is_some() {
        render_branches(frame, area, state);
    }
}

fn render_main(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.view {
        AppView::Changes if area.width < WIDE_LAYOUT_WIDTH => match state.focus {
            FocusedPane::Primary | FocusedPane::Secondary => render_changes(frame, area, state),
            FocusedPane::Diff => render_diff(frame, area, state),
        },
        AppView::Changes => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(CHANGES_LIST_PERCENT),
                    Constraint::Percentage(CHANGES_DIFF_PERCENT),
                ])
                .split(area);
            render_changes(frame, columns[0], state);
            render_diff(frame, columns[1], state);
        }
        AppView::History => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(HISTORY_LIST_PERCENT),
                    Constraint::Percentage(HISTORY_MIDDLE_PERCENT),
                    Constraint::Percentage(HISTORY_DIFF_PERCENT),
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
                    Constraint::Percentage(COMMIT_DETAILS_LIST_PERCENT),
                    Constraint::Percentage(COMMIT_DETAILS_BODY_PERCENT),
                    Constraint::Percentage(COMMIT_DETAILS_FILES_PERCENT),
                ])
                .split(area);
            render_commits(frame, rows[0], state);
            render_commit_body(frame, rows[1], state);
            render_detail_files(frame, rows[2], state);
        }
        AppView::Graph => render_graph(frame, area, state),
        AppView::GraphDetails => {
            render_graph(frame, area, state);
            let popup = centered(
                area,
                GRAPH_DETAILS_WIDTH_PERCENT,
                GRAPH_DETAILS_HEIGHT_PERCENT,
            );
            frame.render_widget(Clear, popup);
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(GRAPH_DETAILS_FILES_PERCENT),
                    Constraint::Percentage(GRAPH_DETAILS_DIFF_PERCENT),
                ])
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
                .constraints([
                    Constraint::Percentage(FILE_HISTORY_LIST_PERCENT),
                    Constraint::Percentage(FILE_HISTORY_CONTENT_PERCENT),
                ])
                .split(area);
            render_file_history(frame, rows[0], state);
            if state.file_view.mode.shows_history_diff() {
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
        .constraints([
            Constraint::Percentage(CODE_TREE_PERCENT),
            Constraint::Percentage(CODE_CONTENT_PERCENT),
        ])
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
                    .get(..ISO_DATE_PREFIX_BYTES)
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
                    .get(..ISO_DATE_PREFIX_BYTES)
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
                    .get(..ISO_DATE_PREFIX_BYTES)
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
    let visible = usize::from(area.height.saturating_sub(PANE_BORDER_CELLS)).max(1);
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
        "Changed files [Space t: tree]",
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
        "Commit tree [Space t: files]",
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
    let content_area = block.inner(area);
    let lines = match &state.diff.content {
        LoadState::Idle => vec![plain("Select a file to view its diff.")],
        LoadState::Loading { .. } => vec![plain("Loading diff…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(document) => diff_lines(document, state),
    };
    let visible = usize::from(content_area.height).max(1);
    let cursor = state.diff.vertical.min(lines.len().saturating_sub(1));
    let vertical = followed_scroll(cursor, state.diff.viewport_vertical, visible);
    let horizontal = state.diff.horizontal.min(u16::MAX as usize) as u16;
    render_diff_line_backgrounds(frame, content_area, &lines, vertical);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((vertical, horizontal)),
        area,
    );
}

/// Extends row-level diff backgrounds through cells that contain no text.
///
/// Painting the visible buffer rows separately keeps that visual padding out of
/// the document text used by horizontal scrolling, search, and cursor motion.
fn render_diff_line_backgrounds(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: &[Line<'_>],
    vertical: u16,
) {
    for (row, line) in area.rows().zip(lines.iter().skip(usize::from(vertical))) {
        if let Some(background) = line.style.bg {
            frame
                .buffer_mut()
                .set_style(row, Style::default().bg(background));
        }
    }
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
    let visible = usize::from(area.height.saturating_sub(PANE_BORDER_CELLS)).max(1);
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
    if let LoadState::Ready(document) = &state.code_view.content
        && let Some(source) = document
            .lines()
            .get(index)
            .map(String::as_str)
            .or(document.message())
    {
        highlight_search_ranges(
            &mut line,
            source,
            index,
            if document.message().is_none() {
                PrefixSpanCount::LINE_NUMBER
            } else {
                PrefixSpanCount::NONE
            },
            state,
        );
    }
    let selected = usize::try_from(state.code_view.cursor.line()).ok() == Some(index)
        && (state.overlay == Overlay::CodeContent || state.focus == FocusedPane::Diff);
    if selected
        && let Some(source) = match &state.code_view.content {
            LoadState::Ready(document) => document.lines().get(index),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
        }
    {
        highlight_source_cursor(
            &mut line,
            source,
            state.code_view.cursor.byte_column(),
            PrefixSpanCount::LINE_NUMBER,
        );
    }
    line.spans.insert(0, navigation_marker(selected));
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
        highlight_source_cursor(
            &mut line,
            source,
            state.file_view.byte_column,
            PrefixSpanCount::NAVIGATION_AND_LINE_NUMBER,
        );
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
    if let LoadState::Ready(document) = &state.diff.content
        && let Some(source) = document
            .lines()
            .get(index)
            .map(DiffLine::text)
            .or(document.message())
    {
        highlight_search_ranges(
            &mut line,
            source,
            index,
            if document.message().is_none() {
                PrefixSpanCount::LINE_NUMBER
            } else {
                PrefixSpanCount::NONE
            },
            state,
        );
    }
    let selected = state.diff.vertical == index
        && (state.overlay == Overlay::Diff || state.focus == FocusedPane::Diff);
    line.spans.insert(0, navigation_marker(selected));
    if selected
        && let Some(source) = match &state.diff.content {
            LoadState::Ready(document) => document.lines().get(index).map(DiffLine::text),
            LoadState::Idle | LoadState::Loading { .. } | LoadState::Failed(_) => None,
        }
    {
        highlight_source_cursor(
            &mut line,
            source,
            state.diff.byte_column,
            PrefixSpanCount::NAVIGATION_AND_LINE_NUMBER,
        );
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
                    .fg(ADDED_FOREGROUND)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.extend(syntax_spans.unwrap_or_else(|| vec![Span::raw(sanitize_inline(code))]));
            Style::default().bg(ADDED_BACKGROUND)
        }
        DiffLineKind::Removed => {
            let (marker, code) = split_diff_code(line);
            spans.push(Span::styled(
                marker,
                Style::default()
                    .fg(REMOVED_FOREGROUND)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.extend(syntax_spans.unwrap_or_else(|| vec![Span::raw(sanitize_inline(code))]));
            Style::default().bg(REMOVED_BACKGROUND)
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
                    .fg(DIFF_ACCENT)
                    .add_modifier(Modifier::BOLD),
            ));
            Style::default().bg(DIFF_HUNK_BACKGROUND)
        }
        DiffLineKind::Header => {
            spans.push(Span::styled(
                sanitize_inline(line.text()),
                Style::default()
                    .fg(DIFF_ACCENT)
                    .add_modifier(Modifier::BOLD),
            ));
            Style::default()
        }
        DiffLineKind::Meta => {
            spans.push(Span::styled(
                sanitize_inline(line.text()),
                Style::default().fg(DIFF_META_FOREGROUND),
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
        Span::styled("▌", Style::default().fg(DIFF_ACCENT))
    } else {
        Span::raw(" ")
    }
}

fn gutter_style() -> Style {
    Style::default().fg(GUTTER_FOREGROUND)
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    // Text overlays render their own search bar inside the floating window.
    if state.overlay == Overlay::None
        && (state.search.prompt_text().is_some() || state.has_active_search_highlights())
    {
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
        _ if matches!(state.symbol_context.symbols, LoadState::Loading { .. }) => state
            .symbol_context
            .status
            .as_deref()
            .map(|status| format!(" | LSP: symbols: {}", sanitize_inline(status)))
            .unwrap_or_else(|| " | LSP: loading symbols…".to_owned()),
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
    let controls = match (state.view, area.width >= WIDE_LAYOUT_WIDTH) {
        (AppView::CommitDetails, true) => {
            "q/Esc History  Space m message  ^w h/j pane  j/k move  Enter diff  Q quit"
        }
        (AppView::CommitDetails, false) => "q/Esc History  Space m msg  Enter diff  Q quit",
        (AppView::GraphDetails, true) => {
            "q/Esc Graph  Space m message  ^w h/j pane  j/k move  Enter diff  Space f/g search  Q quit"
        }
        (AppView::GraphDetails, false) => "q/Esc Graph  j/k file  Enter diff  Q quit",
        (AppView::FileHistory, true) => {
            "q/Esc back  ^w h/j pane  j/k history  Enter full  Space f/g search  Q quit"
        }
        (AppView::FileHistory, false) => "q/Esc back  j/k history  Enter full  Q quit",
        (AppView::Code, true) => {
            "Space 4 Code  h/j/k/l move  ^w h/j pane  w/b word  K hover  gd definition  ^o/^i jump  Q quit"
        }
        (AppView::Code, false) => "h/l cursor/pane  j/k line  K hover  gd definition  Q quit",
        (_, true) => {
            "Space 1/2/3 Git  Space 4 Code  h/j/k/l move  ^w h/j pane  Enter open  Space f/g search  r refresh  Space m message  F1 help  Q quit"
        }
        (_, false) => "Q quit  Space 1-4 views  Space f/g find",
    };
    let root = if area.width >= ROOT_DISPLAY_WIDTH {
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
            " Space b branches | {}{notice}{lsp} | {controls}{root}",
            sanitize_inline(&comparison),
        )),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_branches(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let Some(picker) = &state.branch_picker else {
        return;
    };
    let popup = centered(area, 85, 80);
    frame.render_widget(Clear, popup);
    let sections = Layout::vertical([Constraint::Min(5), Constraint::Length(7)]).split(popup);
    let lines = match &picker.branches {
        LoadState::Idle | LoadState::Loading { .. } => vec![plain("Loading local branches…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(branches) if branches.is_empty() => vec![plain(
            "No local branches. Create a commit or branch with Git first.",
        )],
        LoadState::Ready(branches) => branches
            .iter()
            .enumerate()
            .map(|(index, branch)| {
                selected_line(
                    picker.selection.index() == Some(index),
                    format!(
                        "{} {}",
                        if branch.is_current() { "*" } else { " " },
                        sanitize_inline(&branch.display())
                    ),
                )
            })
            .collect(),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Local branches [* current] ")
                    .borders(Borders::ALL),
            )
            .scroll((list_scroll(picker.selection.index(), sections[0]), 0)),
        sections[0],
    );
    let message = match &picker.switching {
        LoadState::Loading { .. } => "Switching branch… Please wait.".to_owned(),
        LoadState::Failed(error) => format!("{}\n\nj/k: select  Enter: retry  r: reload  q/Esc: close", sanitize_multiline(error.message())),
        _ => "j/k: select  Enter: switch  r: reload  q/Esc: cancel\nSwitching updates HEAD, the index, and working-tree files.\nConflicting local changes are preserved and reported as an error.".to_owned(),
    };
    frame.render_widget(
        Paragraph::new(message).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Switch branch ")
                .borders(Borders::ALL),
        ),
        sections[1],
    );
}

fn render_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.overlay {
        Overlay::None => {}
        Overlay::Help => {
            let popup = centered(area, HELP_WIDTH_PERCENT, HELP_HEIGHT_PERCENT);
            frame.render_widget(Clear, popup);
            let text = vec![
                plain("ChronoGit keys"),
                plain("Space 1..4  Changes / History / Graph / Code"),
                plain("Space b     Switch local branch (j/k select, Enter switch)"),
                plain("Space f/g   Search files / repository content"),
                plain("Ctrl-h/k/j/l Focus previous / next pane; Ctrl-w forms also work"),
                plain("h j k l     Character / line motions; Backspace wraps left"),
                plain("w/W e/E b/B ge/gE   Word / WORD motions"),
                plain("0 ^ $ g_    Line start / first nonblank / end / last nonblank"),
                plain("f F t T     Find/till a character; ;/, repeat/reverse"),
                plain("gg G % g% () {} [[ ]]   Buffer and structural motions"),
                plain("Ctrl-u/d    Half page; Ctrl-b/f or PageUp/Down full page"),
                plain("H M L; zt zz zb       Window motions and cursor placement"),
                plain("zh/zl zH/zL zs/ze     Horizontal viewport motions"),
                plain("/ ? n N; * # g* g#    Search and search word at cursor"),
                plain("Esc: clear text search, then close/back; q: close now"),
                plain("m{c} 'c/`c; ['/`[ ]'/`]   Mark jumps and scans"),
                plain("K; gd/gi/gy/gD   LSP hover and target navigation"),
                plain("Space s/v/d  Symbols / full file / changes-new toggle"),
                plain("Ctrl-o/i     Older / newer Vim, search, or LSP jump"),
                plain("r; Space m/B/t  Refresh; message / layout / commit tree"),
                plain("Space is the app leader; l/Right moves right"),
                plain("Enter       Open selection; move down in an opened document"),
                plain("F1 help; q close/back immediately; Q/Ctrl-C quit"),
                plain("Branch switching updates the worktree; documents are read-only."),
            ];
            frame.render_widget(
                Paragraph::new(text)
                    .block(Block::default().title(" Help ").borders(Borders::ALL))
                    .wrap(Wrap { trim: false }),
                popup,
            );
        }
        Overlay::CommitMessage => {
            let popup = centered(area, MESSAGE_WIDTH_PERCENT, MESSAGE_HEIGHT_PERCENT);
            frame.render_widget(Clear, popup);
            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(MIN_CONTENT_ROWS),
                    Constraint::Length(SEARCH_BAR_ROWS),
                ])
                .split(popup);
            let text = match &state.message.content {
                LoadState::Idle => "Select a commit.".to_owned(),
                LoadState::Loading { .. } => "Loading commit message…".to_owned(),
                LoadState::Failed(error) => {
                    format!("Error: {}", sanitize_inline(error.message()))
                }
                LoadState::Ready(message) => message.as_str().to_owned(),
            };
            let visible = usize::from(sections[0].height.saturating_sub(PANE_BORDER_CELLS)).max(1);
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
        Overlay::FullFile => render_full_file_overlay(frame, area, state),
        Overlay::SymbolContext => {
            match state.symbol_context.return_overlay {
                Overlay::Diff => render_diff_overlay(frame, area, state),
                Overlay::CodeContent => render_code_content_overlay(frame, area, state),
                Overlay::FileContent => render_file_content_overlay(frame, area, state),
                Overlay::FullFile => render_full_file_overlay(frame, area, state),
                _ => {}
            }
            render_symbol_context(frame, area, state);
        }
    }
}

fn render_full_file_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = document_overlay(area);
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(MIN_CONTENT_ROWS),
            Constraint::Length(SEARCH_BAR_ROWS),
        ])
        .split(popup);
    let identity = state.full_file.identity.as_ref();
    let path = identity
        .map(|identity| sanitize_inline(&identity.path.display()))
        .unwrap_or_else(|| "file".to_owned());
    let revision = identity
        .map(|identity| identity.revision.display())
        .unwrap_or_else(|| "source".to_owned());
    let mode = match state.full_file.mode {
        FullFileMode::Changes => "changes",
        FullFileMode::New => "new state",
    };
    let title =
        format!("{path} — {revision} — {mode} [Space d: toggle, Space s: symbols, q/Esc: back]");
    let block = pane_block(&title, true);
    let content_area = block.inner(sections[0]);
    let lines = match &state.full_file.content {
        LoadState::Idle => vec![plain("Select a diff or file first.")],
        LoadState::Loading { .. } => vec![plain("Loading complete source…")],
        LoadState::Failed(error) => vec![error_line(error.message())],
        LoadState::Ready(document) => full_file_document_lines(document, state),
    };
    let visible = usize::from(content_area.height).max(1);
    let source_cursor = usize::try_from(state.full_file.cursor.line())
        .unwrap_or(usize::MAX)
        .min(lines.len().saturating_sub(1));
    let cursor = full_file_display_cursor(state, source_cursor);
    let requested = full_file_display_viewport(state, state.full_file.viewport_vertical);
    let vertical = followed_scroll(cursor, requested, visible);
    if state.full_file.mode == FullFileMode::Changes {
        render_diff_line_backgrounds(frame, content_area, &lines, vertical);
    }
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((
            vertical,
            state
                .full_file
                .viewport_horizontal
                .min(usize::from(u16::MAX)) as u16,
        )),
        sections[0],
    );
    render_search_bar(frame, sections[1], state);
}

fn full_file_document_lines(document: &FileDocument, state: &AppState) -> Vec<Line<'static>> {
    let path = state
        .full_file
        .identity
        .as_ref()
        .map(|identity| &identity.path);
    let mut source_lines = document_lines(document, path, |mut line, index| {
        let selected = usize::try_from(state.full_file.cursor.line()).ok() == Some(index);
        line.spans.insert(0, navigation_marker(selected));
        if let Some(source) = document
            .lines()
            .get(index)
            .map(String::as_str)
            .or(document.message())
        {
            highlight_search_ranges(
                &mut line,
                source,
                index,
                if document.message().is_none() {
                    PrefixSpanCount::NAVIGATION_AND_LINE_NUMBER
                } else {
                    PrefixSpanCount::NAVIGATION
                },
                state,
            );
        }
        if selected && let Some(source) = document.lines().get(index) {
            highlight_source_cursor(
                &mut line,
                source,
                state.full_file.cursor.byte_column(),
                PrefixSpanCount::NAVIGATION_AND_LINE_NUMBER,
            );
        }
        if state.full_file.mode == FullFileMode::Changes
            && state
                .full_file
                .changed_lines
                .contains(&u32::try_from(index).unwrap_or(u32::MAX))
        {
            line.style = line
                .style
                .patch(Style::default().bg(CHANGED_LINE_BACKGROUND));
        }
        line
    });
    if state.full_file.mode != FullFileMode::Changes || state.full_file.deleted_lines.is_empty() {
        return source_lines;
    }

    let source_len = document.lines().len();
    let suffix = source_lines.split_off(source_len.min(source_lines.len()));
    let deleted_content = state
        .full_file
        .deleted_lines
        .iter()
        .map(|line| line.content.clone())
        .collect::<Vec<_>>();
    let highlighted = source_spans(&deleted_content, path);
    let mut deletions = state.full_file.deleted_lines.iter().enumerate().peekable();
    let mut source_lines = source_lines.into_iter();
    let mut lines = Vec::with_capacity(
        source_len
            .saturating_add(state.full_file.deleted_lines.len())
            .saturating_add(suffix.len()),
    );
    for anchor in 0..=source_len {
        while let Some((index, deletion)) = deletions.peek().copied() {
            let deletion_anchor = usize::try_from(deletion.anchor)
                .unwrap_or(usize::MAX)
                .min(source_len);
            if deletion_anchor != anchor {
                break;
            }
            let syntax_spans = highlighted
                .as_ref()
                .and_then(|lines| lines.get(index))
                .cloned();
            lines.push(full_file_deleted_line(deletion, syntax_spans));
            deletions.next();
        }
        if let Some(line) = source_lines.next() {
            lines.push(line);
        }
    }
    lines.extend(suffix);
    lines
}

fn full_file_deleted_line(
    deletion: &FullFileDeletion,
    syntax_spans: Option<Vec<Span<'static>>>,
) -> Line<'static> {
    let old_line = deletion
        .old_line
        .map_or_else(String::new, |line| line.to_string());
    let mut spans = vec![
        navigation_marker(false),
        Span::styled(format!("{old_line:>5} "), gutter_style()),
        Span::styled(
            "-",
            Style::default()
                .fg(REMOVED_FOREGROUND)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    spans.extend(
        syntax_spans.unwrap_or_else(|| vec![Span::raw(sanitize_inline(&deletion.content))]),
    );
    Line::from(spans).style(Style::default().bg(REMOVED_BACKGROUND))
}

fn full_file_display_cursor(state: &AppState, source_line: usize) -> usize {
    if state.full_file.mode != FullFileMode::Changes
        || !matches!(state.full_file.content, LoadState::Ready(_))
    {
        return source_line;
    }
    source_line.saturating_add(
        state
            .full_file
            .deleted_lines
            .iter()
            .filter(|line| usize::try_from(line.anchor).unwrap_or(usize::MAX) <= source_line)
            .count(),
    )
}

fn full_file_display_viewport(state: &AppState, source_line: usize) -> usize {
    if state.full_file.mode != FullFileMode::Changes
        || !matches!(state.full_file.content, LoadState::Ready(_))
    {
        return source_line;
    }
    source_line.saturating_add(
        state
            .full_file
            .deleted_lines
            .iter()
            .filter(|line| usize::try_from(line.anchor).unwrap_or(usize::MAX) < source_line)
            .count(),
    )
}

fn render_symbol_context(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = centered(area, SYMBOL_WIDTH_PERCENT, SYMBOL_HEIGHT_PERCENT);
    frame.render_widget(Clear, popup);
    let mut lines = vec![selected_line(
        state.symbol_context.selection.index() == Some(0),
        "Open full file without selecting a symbol".to_owned(),
    )];
    match &state.symbol_context.symbols {
        LoadState::Idle => lines.push(plain("No symbol request.")),
        LoadState::Loading { .. } => lines.push(plain("Loading symbol context…")),
        LoadState::Failed(error) => lines.push(error_line(error.message())),
        LoadState::Ready(symbols) if symbols.is_empty() => {
            lines.push(plain("No symbols in this context."));
        }
        LoadState::Ready(symbols) => {
            lines.extend(symbols.iter().enumerate().map(|(index, symbol)| {
                let detail = symbol
                    .detail()
                    .map(|detail| format!(" — {}", sanitize_inline(detail)))
                    .unwrap_or_default();
                selected_line(
                    state.symbol_context.selection.index() == Some(index.saturating_add(1)),
                    format!(
                        "{}{} {}  line {}{detail}",
                        "  ".repeat(symbol.depth()),
                        symbol.kind(),
                        sanitize_inline(symbol.name()),
                        symbol.selection().line().saturating_add(1),
                    ),
                )
            }))
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Symbol/context [j/k: move, Enter: open and jump, q/Esc: back] ")
                    .borders(Borders::ALL),
            )
            .scroll((
                list_scroll(state.symbol_context.selection.index(), popup),
                0,
            )),
        popup,
    );
}

fn render_lsp_hover(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let popup = centered(area, HOVER_WIDTH_PERCENT, HOVER_HEIGHT_PERCENT);
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
    let popup = centered(
        area,
        REPOSITORY_SEARCH_WIDTH_PERCENT,
        REPOSITORY_SEARCH_HEIGHT_PERCENT,
    );
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(SEARCH_INPUT_ROWS),
            Constraint::Min(MIN_CONTENT_ROWS),
        ])
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
        format!("Search {mode} [Ctrl-h/k: edit again, q/Esc: close]")
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
        "Results [j/k: move, Enter: open, Ctrl-h/k: search, q/Esc: close]"
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
    let popup = document_overlay(area);
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(MIN_CONTENT_ROWS),
            Constraint::Length(SEARCH_BAR_ROWS),
        ])
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
    let popup = document_overlay(area);
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(MIN_CONTENT_ROWS),
            Constraint::Length(SEARCH_BAR_ROWS),
        ])
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
        &format!("{path} [{}]", document_close_hint(state)),
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
    let popup = centered(area, SYMBOL_WIDTH_PERCENT, SYMBOL_HEIGHT_PERCENT);
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
    let visible_lines = usize::from(area.height.saturating_sub(PANE_BORDER_CELLS)).max(1);
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
        crate::lsp::display_column(line, state.code_view.cursor.byte_column())
            .saturating_add(SOURCE_GUTTER_COLUMNS)
    });
    let visible_columns = usize::from(area.width.saturating_sub(PANE_BORDER_CELLS)).max(1);
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
    let popup = document_overlay(area);
    frame.render_widget(Clear, popup);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(MIN_CONTENT_ROWS),
            Constraint::Length(SEARCH_BAR_ROWS),
        ])
        .split(popup);
    let baseline = selected_baseline(state);
    let hint = document_close_hint(state);
    let title = baseline.map_or_else(
        || format!("Diff [{hint}]"),
        |value| format!("Diff — {value} [{hint}]"),
    );
    render_diff_pane(frame, sections[0], state, &title, true);
    render_search_bar(frame, sections[1], state);
}

fn document_close_hint(state: &AppState) -> &'static str {
    if state.has_active_search_highlights() {
        "Esc: clear search, q: close, Enter: next line"
    } else {
        "q/Esc: close, Enter: next line"
    }
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
            Span::raw(if input.is_empty() {
                "  [Backspace/Esc: cancel]"
            } else {
                "  [Esc: cancel]"
            }),
        ])
    } else if state.search.query().is_empty() {
        Line::raw(" / forward search  ? backward search  n/N next/previous")
    } else {
        let position = state.search.current_ordinal().unwrap_or(0);
        let controls = if state.has_active_search_highlights() {
            "n/N: next/previous, Esc: clear search, q: close/back"
        } else {
            "n/N: next/previous"
        };
        Line::raw(format!(
            " {}{}  {position}/{}  [{controls}]",
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
        AppView::FileHistory if !state.file_view.mode.shows_history_diff() => {
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
        "Terminal too small: {}x{}. ChronoGit needs at least {MIN_TERMINAL_WIDTH}x{MIN_TERMINAL_HEIGHT}. Press Q to quit.",
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
    sanitize(value, SanitizationMode::Inline)
}

fn sanitize_multiline(value: &str) -> String {
    sanitize(value, SanitizationMode::Multiline)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SanitizationMode {
    Inline,
    Multiline,
}

fn sanitize(value: &str, mode: SanitizationMode) -> String {
    let mut safe = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' if mode == SanitizationMode::Multiline => safe.push('\n'),
            '\t' => safe.push_str("    "),
            character if character.is_control() => {
                safe.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
            }
            character => safe.push(character),
        }
    }
    safe
}

// Search offsets refer to original UTF-8 bytes. Convert only the matched
// boundaries through the same sanitization as the source spans; Ratatui then
// handles display widths and horizontal clipping. Cursor styling is applied last.
fn highlight_search_ranges(
    line: &mut Line<'static>,
    source: &str,
    index: usize,
    prefix_spans: PrefixSpanCount,
    state: &AppState,
) {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut current = None;
    for (range, selected) in state.search.highlighted_ranges(index) {
        if source.get(range.clone()).is_none() {
            continue;
        }
        if selected {
            current = Some(range.clone());
        }
        if let Some(previous) = ranges.last_mut()
            && previous.end >= range.start
        {
            previous.end = previous.end.max(range.end);
        } else {
            ranges.push(range);
        }
    }
    let mut source_offset = 0;
    let mut rendered_offset = 0;
    let rendered = ranges.into_iter().map(|range| {
        rendered_offset += sanitize_inline(&source[source_offset..range.start]).len();
        let start = rendered_offset;
        rendered_offset += sanitize_inline(&source[range.clone()]).len();
        source_offset = range.end;
        start..rendered_offset
    });
    style_source_ranges(
        line,
        prefix_spans,
        rendered,
        Style::default().add_modifier(Modifier::UNDERLINED),
    );
    if let Some(range) = current {
        let start = sanitize_inline(&source[..range.start]).len();
        let end = start + sanitize_inline(&source[range]).len();
        style_source_ranges(
            line,
            prefix_spans,
            std::iter::once(start..end),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
}

// Sorted, non-overlapping rendered byte ranges are painted in one pass through
// the syntax spans, preserving every style outside the ranges and the gutters.
fn style_source_ranges(
    line: &mut Line<'static>,
    prefix_spans: PrefixSpanCount,
    ranges: impl Iterator<Item = std::ops::Range<usize>>,
    style: Style,
) {
    let mut ranges = ranges.peekable();
    if ranges.peek().is_none() {
        return;
    }
    let original = std::mem::take(&mut line.spans);
    let mut spans = Vec::with_capacity(original.len());
    let mut offset = 0;
    for (index, span) in original.into_iter().enumerate() {
        if index < prefix_spans.value() {
            spans.push(span);
            continue;
        }
        let content = span.content.as_ref();
        let end = offset + content.len();
        let mut position = offset;
        while position < end {
            while ranges.peek().is_some_and(|range| range.end <= position) {
                ranges.next();
            }
            let (boundary, selected) = match ranges.peek() {
                Some(range) if range.start <= position => (end.min(range.end), true),
                Some(range) => (end.min(range.start), false),
                None => (end, false),
            };
            spans.push(Span::styled(
                content[position - offset..boundary - offset].to_owned(),
                if selected {
                    span.style.patch(style)
                } else {
                    span.style
                },
            ));
            position = boundary;
        }
        offset = end;
    }
    line.spans = spans;
}

fn highlight_source_cursor(
    line: &mut Line<'static>,
    source: &str,
    requested: usize,
    prefix_spans: PrefixSpanCount,
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
    let prefix_spans = prefix_spans.value().min(original.len());
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
                highlight_source_cursor(&mut line, source, byte_column, PrefixSpanCount::NONE);
                line
            } else {
                Line::raw(sanitize_inline(source))
            }
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        let mut line = Line::raw("");
        highlight_source_cursor(&mut line, "", 0, PrefixSpanCount::NONE);
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
    let visible = usize::from(area.height.saturating_sub(PANE_BORDER_CELLS)).max(1);
    selection
        .unwrap_or(0)
        .saturating_sub(visible.saturating_sub(1))
        .min(usize::from(u16::MAX)) as u16
}

fn document_overlay(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(DOCUMENT_OVERLAY_MARGIN),
        y: area.y.saturating_add(DOCUMENT_OVERLAY_MARGIN),
        width: area.width.saturating_sub(DOCUMENT_OVERLAY_INSET),
        height: area.height.saturating_sub(DOCUMENT_OVERLAY_INSET),
    }
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((FULL_PERCENT - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((FULL_PERCENT - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((FULL_PERCENT - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((FULL_PERCENT - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    use super::{
        code_document_lines, diff_line, diff_lines, file_document_lines, render, render_diff_pane,
        sanitize_inline, sanitize_multiline,
    };
    use crate::app::{
        Action, AppEffect, AppState, AppView, ErrorNotice, Event, FocusedPane, GitEffect,
        LoadState, LspEffect, Overlay, RepositorySearchKind, SearchDirection,
    };
    use crate::domain::{
        ChangeKind, ChangedFile, CommitBaseline, CommitMessage, CommitSummary, DiffDocument,
        DiffLine, DiffLineKind, DiffTarget, DocumentSymbol, DocumentSymbolKind, FileDocument,
        FileRevision, LineNumber, ObjectId, RepoPath, RepositoryRoot, SearchHit, SourcePosition,
        SourceRange, WorktreeChange,
    };

    fn state() -> AppState {
        let root = RepositoryRoot::new(PathBuf::from("/tmp/repo"))
            .unwrap_or_else(|error| panic!("{error}"));
        AppState::new(root, AppView::Changes)
    }

    #[test]
    fn search_highlights_only_matching_cells_and_restores_original_styles() {
        use crate::app::SearchDirection;
        use ratatui::style::Modifier;
        // Includes smart-case expansion, tabs, matched spaces, multiple/overlapping
        // occurrences, and matches crossing syntax spans (the quoted string).
        for query in [
            "needle",
            "日本",
            "\tlet",
            " needle",
            "\"日本 needle\"",
            "ana",
            "i\u{307}",
        ] {
            for code in [true, false] {
                let source = "\tlet needle = \"日本 needle\"; banana İ // needle";
                let texts: Vec<String> = if code {
                    vec![source.to_owned(), source.to_owned()]
                } else {
                    vec![format!("+{source}"), format!("-{source}")]
                };
                let path = RepoPath::from_bytes(b"example.rs".to_vec())
                    .unwrap_or_else(|error| panic!("{error}"));
                let mut state = state();
                state.view = if code {
                    AppView::Code
                } else {
                    AppView::Changes
                };
                state.focus = FocusedPane::Diff;
                state.code_view.path = Some(path.clone());
                state.diff.target = Some(DiffTarget::Worktree {
                    path,
                    kind: crate::domain::WorktreeDiffKind::Tracked,
                });
                state.code_view.content =
                    LoadState::Ready(FileDocument::exact_text(texts.join("\n")));
                state.diff.content = LoadState::Ready(DiffDocument::Text {
                    lines: texts
                        .iter()
                        .enumerate()
                        .map(|(index, text)| {
                            DiffLine::new(
                                if index == 0 {
                                    DiffLineKind::Added
                                } else {
                                    DiffLineKind::Removed
                                },
                                crate::domain::LineNumber::new(1),
                                crate::domain::LineNumber::new(1),
                                text.clone(),
                            )
                        })
                        .collect(),
                    bytes: 200,
                });
                state.search.begin(SearchDirection::Forward);
                for character in query.chars() {
                    state.search.push(character);
                }
                let current = state
                    .search
                    .confirm_position(texts.iter().map(String::as_str), SourcePosition::new(0, 0))
                    .unwrap_or_else(|| panic!("expected match for {query:?}"));
                state.code_view.cursor = current;
                state.diff.vertical = current.line() as usize;
                state.diff.byte_column = current.byte_column();
                let draw = |state: &AppState, horizontal: u16| {
                    let lines = if code {
                        let LoadState::Ready(document) = &state.code_view.content else {
                            unreachable!()
                        };
                        code_document_lines(document, state)
                    } else {
                        let LoadState::Ready(document) = &state.diff.content else {
                            unreachable!()
                        };
                        diff_lines(document, state)
                    };
                    let mut terminal = Terminal::new(TestBackend::new(90, 3))
                        .unwrap_or_else(|error| panic!("{error}"));
                    terminal
                        .draw(|frame| {
                            frame.render_widget(
                                ratatui::widgets::Paragraph::new(lines).scroll((0, horizontal)),
                                frame.area(),
                            )
                        })
                        .unwrap_or_else(|error| panic!("{error}"));
                    terminal.backend().buffer().clone()
                };
                // Expected ranges use independently selected literal byte ranges;
                // smart-case expansion maps the three-byte query back to the İ.
                let literal = if query == "i\u{307}" { "İ" } else { query };
                let expected: Vec<Vec<std::ops::Range<usize>>> = texts
                    .iter()
                    .map(|text| {
                        text.char_indices()
                            .filter_map(|(byte, _)| {
                                text[byte..]
                                    .starts_with(literal)
                                    .then_some(byte..byte + literal.len())
                            })
                            .collect()
                    })
                    .collect();
                for horizontal in [0, 13, 29, 38] {
                    let highlighted = draw(&state, horizontal);
                    assert!(state.search.dismiss_highlights());
                    let dismissed = draw(&state, horizontal);
                    for (line, text) in texts.iter().enumerate() {
                        let prefix = if code { 8 } else { 13 };
                        // A scroll into a wide glyph retains that whole glyph;
                        // the visible source starts at its leading cell.
                        let mut effective_scroll = 0;
                        let displayed = format!("{}{}", " ".repeat(prefix), sanitize_inline(text));
                        for character in displayed.chars() {
                            let width =
                                unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
                            if effective_scroll + width > usize::from(horizontal) {
                                break;
                            }
                            effective_scroll += width;
                        }
                        let cells: Vec<_> = expected[line]
                            .iter()
                            .map(|range| {
                                let start = prefix
                                    + unicode_width::UnicodeWidthStr::width(
                                        sanitize_inline(&text[..range.start]).as_str(),
                                    );
                                let end = start
                                    + unicode_width::UnicodeWidthStr::width(
                                        sanitize_inline(&text[range.clone()]).as_str(),
                                    );
                                start..end
                            })
                            .collect();
                        for x in 0..90 {
                            let before = &highlighted[(x, line as u16)];
                            let after = &dismissed[(x, line as u16)];
                            assert_eq!(before.symbol(), after.symbol());
                            // Ratatui resets continuation cells for wide glyphs.
                            if before.symbol() == " "
                                && x > 0
                                && unicode_width::UnicodeWidthStr::width(
                                    highlighted[(x - 1, line as u16)].symbol(),
                                ) == 2
                            {
                                continue;
                            }
                            let matched = cells
                                .iter()
                                .any(|range| range.contains(&(usize::from(x) + effective_scroll)));
                            assert_eq!(
                                before.modifier.contains(Modifier::UNDERLINED),
                                matched,
                                "{query:?}, code={code}, scroll={horizontal}, line={line}, x={x}"
                            );
                            assert!(!after.modifier.contains(Modifier::UNDERLINED));
                            if !matched {
                                assert_eq!(before, after, "non-match changed at {x}");
                            }
                        }
                    }
                    // Showing the retained query again must reproduce the same cells.
                    state.search.begin(SearchDirection::Forward);
                    state.search.confirm_position(
                        texts.iter().map(String::as_str),
                        SourcePosition::new(0, 0),
                    );
                    assert_eq!(highlighted, draw(&state, horizontal));
                }
                let highlighted = draw(&state, 0);
                let cursor_cells = highlighted
                    .content
                    .iter()
                    .filter(|cell| cell.bg == Color::LightCyan)
                    .count();
                assert!(cursor_cells > 0);
                if query == "needle" {
                    assert!(
                        highlighted
                            .content
                            .iter()
                            .any(|cell| cell.bg == Color::Yellow)
                    );
                }
                assert!(state.search.dismiss_highlights());
                let dismissed = draw(&state, 0);
                assert_eq!(
                    cursor_cells,
                    dismissed
                        .content
                        .iter()
                        .filter(|cell| cell.bg == Color::LightCyan)
                        .count()
                );
                if !code {
                    assert_eq!(dismissed[(13, 0)].bg, Color::Rgb(33, 58, 43));
                    assert_eq!(dismissed[(13, 1)].bg, Color::Rgb(74, 34, 29));
                }
                state.search.clear();
                assert_eq!(
                    dismissed,
                    draw(&state, 0),
                    "dismissal must restore syntax/diff/cursor styles exactly"
                );
            }
        }
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
    fn renders_full_file_modes_and_symbol_context_choices() {
        let mut state = state();
        state.focus = FocusedPane::Diff;
        let path = RepoPath::from_bytes(b"src/example.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        state.diff.target = Some(DiffTarget::Worktree {
            path: path.clone(),
            kind: crate::domain::WorktreeDiffKind::Tracked,
        });
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: vec![
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
            bytes: 32,
        });
        let effects = state.handle_app_action(Action::OpenFullFile);
        let request_id = match effects[0] {
            AppEffect::Git(GitEffect::LoadFullFile { request_id, .. }) => request_id,
            ref other => panic!("unexpected effect: {other:?}"),
        };
        state.handle_app_event(Event::FullFileLoaded {
            request_id,
            revision: FileRevision::WorkingTree,
            path: path.clone(),
            result: Ok(FileDocument::exact_text("fn first() {}\nfn changed() {}\n")),
        });
        let changes = rendered_text(&state, 100, 30);
        assert!(changes.contains("working tree — changes"));
        assert!(changes.contains("-fn removed()"));
        assert!(changes.contains("fn changed()"));
        assert!(
            rendered_buffer(&state, 100, 30)
                .content()
                .iter()
                .any(|cell| cell.bg == Color::Rgb(74, 34, 29))
        );

        state.handle_app_action(Action::ToggleFullFileMode);
        let new_state = rendered_text(&state, 100, 30);
        assert!(new_state.contains("new state"));
        assert!(!new_state.contains("fn removed()"));

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
            path,
            document_revision,
            result: Ok(vec![DocumentSymbol::new(
                "changed".to_owned(),
                None,
                DocumentSymbolKind::Function,
                SourceRange::new(SourcePosition::new(1, 0), SourcePosition::new(1, 15)),
                SourcePosition::new(1, 3),
                0,
            )]),
        });
        let symbols = rendered_text(&state, 100, 30);
        assert!(symbols.contains("Open full file without selecting a symbol"));
        assert!(symbols.contains("function changed"));
    }

    #[test]
    fn changes_mode_renders_removed_rows_when_the_new_file_is_absent() {
        let mut state = state();
        state.focus = FocusedPane::Diff;
        let path = RepoPath::from_bytes(b"src/deleted.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        state.diff.target = Some(DiffTarget::Worktree {
            path: path.clone(),
            kind: crate::domain::WorktreeDiffKind::Tracked,
        });
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: vec![
                DiffLine::new(DiffLineKind::Hunk, None, None, "@@ -1 +0,0 @@".to_owned()),
                DiffLine::new(
                    DiffLineKind::Removed,
                    LineNumber::new(1),
                    None,
                    "-fn removed_with_file() {}".to_owned(),
                ),
            ],
            bytes: 32,
        });

        let effects = state.handle_app_action(Action::OpenFullFile);
        let request_id = match effects[0] {
            AppEffect::Git(GitEffect::LoadFullFile { request_id, .. }) => request_id,
            ref other => panic!("unexpected effect: {other:?}"),
        };
        state.handle_app_event(Event::FullFileLoaded {
            request_id,
            revision: FileRevision::WorkingTree,
            path,
            result: Ok(FileDocument::Unavailable {
                summary: "File does not exist in the current working tree".to_owned(),
            }),
        });

        let changes = rendered_text(&state, 100, 30);
        assert!(changes.contains("-fn removed_with_file()"));
        assert!(changes.contains("File does not exist"));

        state.handle_app_action(Action::ToggleFullFileMode);
        let new_state = rendered_text(&state, 100, 30);
        assert!(!new_state.contains("fn removed_with_file()"));
        assert!(new_state.contains("File does not exist"));
    }

    #[test]
    fn standalone_control_focus_switches_narrow_changes_and_wide_borders() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::layout::{Constraint, Direction, Layout};

        let press = |state: &mut AppState, key: char| {
            let mut mapper = crate::tui::keymap::KeyMapper::new();
            let context = if state.is_search_input_active() {
                crate::tui::keymap::KeyInputContext::SearchInput
            } else {
                crate::tui::keymap::KeyInputContext::Normal
            };
            let action = mapper
                .map(
                    KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL),
                    context,
                )
                .unwrap_or_else(|| panic!("expected Ctrl-{key} focus action"));
            assert!(state.handle_app_action(action).is_empty());
        };

        for width in [80, 109] {
            let mut state = state();
            state.set_terminal_size(width, 24);
            let primary = rendered_text(&state, width, 24);
            assert!(primary.contains("Unstaged changes"));
            assert!(!primary.contains("Select a file to view its diff."));

            press(&mut state, 'l');
            let diff = rendered_text(&state, width, 24);
            assert!(!diff.contains("Unstaged changes"));
            assert!(diff.contains("Select a file to view its diff."));

            press(&mut state, 'h');
            assert_eq!(state.focus, FocusedPane::Primary);
            assert!(rendered_text(&state, width, 24).contains("Unstaged changes"));
        }

        for width in [110, 140] {
            let mut state = state();
            state.set_terminal_size(width, 24);
            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
                .split(Rect::new(0, 0, width, 23));
            let diff_x = panes[1].x;

            let primary = rendered_buffer(&state, width, 24);
            assert_eq!(primary[(0, 0)].fg, Color::Yellow);
            assert_eq!(primary[(diff_x, 0)].fg, Color::DarkGray);
            let primary_text = rendered_text(&state, width, 24);
            assert!(primary_text.contains("Unstaged changes"));
            assert!(primary_text.contains("Select a file to view its diff."));

            press(&mut state, 'j');
            let diff = rendered_buffer(&state, width, 24);
            assert_eq!(diff[(0, 0)].fg, Color::DarkGray);
            assert_eq!(diff[(diff_x, 0)].fg, Color::Yellow);
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
        state.code_view.content = LoadState::Ready(FileDocument::exact_text("struct Action;"));
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
            result: Ok(FileDocument::exact_text("code viewer content")),
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
    fn document_search_bar_is_rendered_once_in_panes_and_overlays() {
        use crate::app::SearchDirection;

        for (view, overlay) in [
            (AppView::Changes, Overlay::None),
            (AppView::Code, Overlay::None),
            (AppView::Changes, Overlay::Diff),
            (AppView::Code, Overlay::CodeContent),
            (AppView::FileHistory, Overlay::FileContent),
            (AppView::Changes, Overlay::FullFile),
            (AppView::History, Overlay::CommitMessage),
        ] {
            for direction in [SearchDirection::Forward, SearchDirection::Backward] {
                for (width, height) in [(80, 24), (140, 40)] {
                    let mut state = state();
                    state.view = view;
                    state.overlay = overlay;
                    state.focus = FocusedPane::Diff;
                    state.search.begin(direction);
                    for character in "needle".chars() {
                        state.search.push(character);
                    }
                    let query = format!("{}needle", direction.prompt());
                    let text = rendered_text(&state, width, height);
                    assert_eq!(
                        text.matches(&query).count(),
                        1,
                        "input: {view:?}/{overlay:?} at {width}x{height}"
                    );

                    state
                        .search
                        .confirm_position(["needle"], SourcePosition::new(0, 0));
                    let text = rendered_text(&state, width, height);
                    assert_eq!(
                        text.matches(&query).count(),
                        1,
                        "confirmed: {view:?}/{overlay:?} at {width}x{height}"
                    );
                }
            }
        }
    }

    #[test]
    fn search_backspace_removes_the_input_cursor_and_restores_the_previous_frame() {
        use crate::app::SearchDirection;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        for (view, overlay, focus) in [
            (AppView::Changes, Overlay::None, FocusedPane::Diff),
            (AppView::Code, Overlay::None, FocusedPane::Diff),
            (AppView::Changes, Overlay::Diff, FocusedPane::Primary),
            (AppView::Code, Overlay::CodeContent, FocusedPane::Primary),
            (AppView::FileHistory, Overlay::None, FocusedPane::Diff),
            (
                AppView::FileHistory,
                Overlay::FileContent,
                FocusedPane::Primary,
            ),
            (
                AppView::CommitDetails,
                Overlay::None,
                FocusedPane::Secondary,
            ),
            (
                AppView::History,
                Overlay::CommitMessage,
                FocusedPane::Primary,
            ),
        ] {
            for (width, height) in [(80, 24), (140, 40)] {
                for previous in [None, Some(true), Some(false)] {
                    let mut state = state();
                    state.view = view;
                    state.overlay = overlay;
                    state.focus = focus;
                    if let Some(visible) = previous {
                        state.search.begin(SearchDirection::Backward);
                        state.search.push('旧');
                        state
                            .search
                            .confirm_position(["旧"], SourcePosition::new(0, 0));
                        if !visible {
                            state.search.dismiss_highlights();
                        }
                    }
                    let mut terminal = Terminal::new(TestBackend::new(width, height))
                        .unwrap_or_else(|error| panic!("{error}"));
                    terminal
                        .draw(|frame| render(frame, &state))
                        .unwrap_or_else(|error| panic!("{error}"));
                    let before = terminal.backend().buffer().clone();
                    let mut mapper = crate::tui::keymap::KeyMapper::new();
                    for prompt in ['/', '?'] {
                        for (key, expected_input) in [
                            (KeyCode::Char(prompt), true),
                            (KeyCode::Char('新'), true),
                            (KeyCode::Backspace, true),
                            (KeyCode::Backspace, false),
                        ] {
                            let context = if state.is_search_input_active() {
                                crate::tui::keymap::KeyInputContext::SearchInput
                            } else {
                                crate::tui::keymap::KeyInputContext::Normal
                            };
                            let action = mapper
                                .map(KeyEvent::new(key, KeyModifiers::NONE), context)
                                .unwrap_or_else(|| panic!("expected search key"));
                            assert!(state.handle_app_action(action).is_empty());
                            terminal
                                .draw(|frame| render(frame, &state))
                                .unwrap_or_else(|error| panic!("{error}"));
                            let text = buffer_text(terminal.backend());
                            assert_eq!(text.contains('█'), expected_input);
                            assert_eq!(
                                text.contains("Backspace/Esc: cancel"),
                                expected_input && key != KeyCode::Char('新')
                            );
                        }
                        assert_eq!(
                            terminal.backend().buffer(),
                            &before,
                            "{view:?}/{overlay:?} {width}x{height}, previous={previous:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn branch_picker_renders_above_all_views_and_scrolls_to_the_selection() {
        use crate::app::{Action, Event, GitEffect};
        use crate::domain::LocalBranch;
        for view in [
            AppView::Changes,
            AppView::History,
            AppView::Graph,
            AppView::Code,
        ] {
            for (width, height) in [(80, 24), (140, 40)] {
                let mut state = state();
                state.view = view;
                let effects = state.handle_action(Action::OpenBranches);
                let [GitEffect::LoadBranches { request_id }] = effects.as_slice() else {
                    panic!("missing load")
                };
                assert!(rendered_text(&state, width, height).contains("Loading local branches"));
                let branches = (0..60)
                    .map(|index| {
                        LocalBranch::from_ref(
                            format!("refs/heads/topic-{index:02}").as_bytes(),
                            index == 0,
                        )
                        .unwrap_or_else(|| panic!("test branch"))
                    })
                    .collect();
                state.handle_event(Event::BranchesLoaded {
                    request_id: *request_id,
                    result: Ok(branches),
                });
                let text = rendered_text(&state, width, height);
                assert!(text.contains("* topic-00"));
                assert!(text.contains("Enter: switch"));
                state.handle_action(Action::MoveBottom);
                assert!(rendered_text(&state, width, height).contains("topic-59"));
                let effects = state.handle_action(Action::Activate);
                let [GitEffect::SwitchBranch { request_id, .. }] = effects.as_slice() else {
                    panic!("missing switch")
                };
                assert!(rendered_text(&state, width, height).contains("Switching branch"));
                state.handle_event(Event::BranchSwitched {
                    request_id: *request_id,
                    result: Err(crate::git::GitError::Unsupported(
                        "Local changes would be overwritten".into(),
                    )),
                });
                assert!(
                    rendered_text(&state, width, height)
                        .contains("Local changes would be overwritten")
                );
            }
        }
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
        assert!(text.contains("Space 1..4  Changes / History / Graph / Code"));
        assert!(text.contains("Space f/g   Search files / repository content"));
        assert!(text.contains("Ctrl-h/k/j/l Focus previous / next pane"));
        assert!(text.contains("Backspace wraps left"));
        assert!(!text.contains("Backspace/Ctrl-H wraps left"));
        assert!(text.contains("r; Space m/B/t"));
        assert!(text.contains("Space is the app leader; l/Right moves right"));
        assert!(text.contains("F1 help; q close/back immediately; Q/Ctrl-C quit"));
        assert!(text.contains("Esc: clear text search, then close/back; q: close now"));
        let compact = rendered_text(&state, 80, 24);
        assert!(compact.contains("Ctrl-h/k/j/l Focus previous / next pane"));
        assert!(compact.contains("Esc: clear text search, then close/back; q: close now"));

        state.overlay = Overlay::None;
        let footer = rendered_text(&state, 180, 30);
        assert!(footer.contains("Space 1/2/3 Git"));
        assert!(footer.contains("Space 4 Code"));
        assert!(footer.contains("Space f/g search"));
        assert!(!footer.contains("\\1/2/3"));
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
    fn fills_diff_line_backgrounds_to_the_content_edge_at_each_width_and_scroll() {
        let mut state = state();
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: vec![
                DiffLine::new(DiffLineKind::Hunk, None, None, "@@ -1 +1 @@".to_owned()),
                DiffLine::new(DiffLineKind::Added, None, None, "+short".to_owned()),
                DiffLine::new(DiffLineKind::Removed, None, None, "-\t界".to_owned()),
                DiffLine::new(DiffLineKind::Context, None, None, " context".to_owned()),
                DiffLine::new(DiffLineKind::Header, None, None, "diff --git".to_owned()),
                DiffLine::new(DiffLineKind::Meta, None, None, "\\ No newline".to_owned()),
                DiffLine::new(DiffLineKind::Added, None, None, "+".to_owned()),
                DiffLine::new(
                    DiffLineKind::Added,
                    None,
                    None,
                    "+01234567890123456789012345678901234567890123456789".to_owned(),
                ),
            ],
            bytes: 108,
        });

        let expected = [
            Color::Rgb(49, 50, 68),
            Color::Rgb(33, 58, 43),
            Color::Rgb(74, 34, 29),
            Color::Reset,
            Color::Reset,
            Color::Reset,
            Color::Rgb(33, 58, 43),
            Color::Rgb(33, 58, 43),
        ];
        for pane_width in [34, 44] {
            let pane = Rect::new(2, 1, pane_width, 10);
            for horizontal in [0, 17] {
                state.diff.horizontal = horizontal;
                let buffer = rendered_diff_pane_buffer(&state, pane, 50, 12);
                let content_start = pane.x + 1;
                let content_end = pane.x + pane.width - 1;

                for (line, background) in expected.iter().enumerate() {
                    let y = pane.y + 1 + line as u16;
                    for x in content_start..content_end {
                        // Ratatui clears a wide glyph's continuation cell; the
                        // leading cell carries the style for both columns.
                        if x > content_start
                            && unicode_width::UnicodeWidthStr::width(buffer[(x - 1, y)].symbol())
                                == 2
                        {
                            continue;
                        }
                        assert_eq!(
                            buffer[(x, y)].bg,
                            *background,
                            "width={pane_width}, scroll={horizontal}, line={line}, x={x}"
                        );
                    }
                    assert_eq!(buffer[(pane.x, y)].symbol(), "│");
                    assert_eq!(buffer[(content_end, y)].symbol(), "│");
                    if *background != Color::Reset {
                        assert_ne!(buffer[(pane.x, y)].bg, *background);
                        assert_ne!(buffer[(content_end, y)].bg, *background);
                    }
                }
            }
        }
    }

    #[test]
    fn normal_and_floating_diffs_bound_full_width_backgrounds_by_their_own_borders() {
        let mut state = state();
        state.focus = FocusedPane::Diff;
        state.diff.content = LoadState::Ready(DiffDocument::Text {
            lines: vec![
                DiffLine::new(DiffLineKind::Context, None, None, " context".to_owned()),
                DiffLine::new(DiffLineKind::Added, None, None, "+short".to_owned()),
            ],
            bytes: 15,
        });
        let added_background = Color::Rgb(33, 58, 43);

        for width in [80, 120] {
            let normal = rendered_buffer(&state, width, 30);
            assert_background_row_reaches_bordered_content_edges(&normal, added_background);
        }

        state.overlay = Overlay::Diff;
        for width in [80, 120] {
            let floating = rendered_buffer(&state, width, 30);
            assert_background_row_reaches_bordered_content_edges(&floating, added_background);
        }
    }

    #[test]
    fn full_width_diff_backgrounds_preserve_text_token_search_and_cursor_styles() {
        let mut state = state();
        state.focus = FocusedPane::Diff;
        state.diff.target = Some(DiffTarget::Worktree {
            path: RepoPath::from_bytes(b"src/example.rs".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
            kind: crate::domain::WorktreeDiffKind::Tracked,
        });
        let source = "+pub fn needle() { let other = \"needle\"; }";
        let document = DiffDocument::Text {
            lines: vec![DiffLine::new(
                DiffLineKind::Added,
                None,
                None,
                source.to_owned(),
            )],
            bytes: source.len(),
        };
        state.diff.content = LoadState::Ready(document.clone());
        state.diff.byte_column = 1;
        state.search.begin(SearchDirection::Forward);
        for character in "needle".chars() {
            state.search.push(character);
        }
        state.search.confirm_position(
            document.lines().iter().map(DiffLine::text),
            SourcePosition::new(0, 0),
        );

        let logical = diff_lines(&document, &state);
        assert_eq!(
            logical[0].width(),
            13 + unicode_width::UnicodeWidthStr::width(source)
        );
        assert_eq!(document.lines()[0].text(), source);

        let buffer = rendered_diff_pane_buffer(&state, Rect::new(1, 1, 60, 3), 62, 5);
        let backgrounds =
            buffer
                .content()
                .iter()
                .fold(std::collections::HashMap::new(), |mut counts, cell| {
                    *counts.entry(cell.bg).or_insert(0usize) += 1;
                    counts
                });
        assert!(backgrounds[&Color::Rgb(33, 58, 43)] > 40);
        assert!(backgrounds[&Color::Yellow] >= "needle".len());
        assert_eq!(backgrounds[&Color::LightCyan], 1);
        let foregrounds = buffer
            .content()
            .iter()
            .map(|cell| cell.fg)
            .collect::<std::collections::HashSet<_>>();
        assert!(foregrounds.len() > 3, "syntax token colors were flattened");
    }

    #[test]
    fn syntax_highlights_code_inside_diff_hunks() {
        let mut state = state();
        state.diff.target = Some(DiffTarget::Worktree {
            path: RepoPath::from_bytes(b"src/example.rs".to_vec())
                .unwrap_or_else(|error| panic!("{error}")),
            kind: crate::domain::WorktreeDiffKind::Tracked,
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
        let document = FileDocument::exact_text("pub fn first() {}\npub fn second() {}");

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
        state.code_view.content = LoadState::Ready(FileDocument::exact_text("a界e\u{301}"));
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
            kind: crate::domain::WorktreeDiffKind::Tracked,
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
        graph.file_view.content = LoadState::Ready(FileDocument::exact_text("current source line"));
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
            LineNumber::new(42).unwrap_or_else(|| panic!("fixture line must be nonzero")),
            "let needle = true;".to_owned(),
        )]);
        state.repository_search.selection.reset(1);

        let text = rendered_text(&state, 100, 30);
        assert!(text.contains("Search content"));
        assert!(text.contains("Ctrl-h/k: edit again"));
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

    fn rendered_diff_pane_buffer(state: &AppState, pane: Rect, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend)
            .unwrap_or_else(|error| panic!("could not create terminal: {error}"));
        terminal
            .draw(|frame| render_diff_pane(frame, pane, state, "Diff", false))
            .unwrap_or_else(|error| panic!("could not draw diff pane: {error}"));
        terminal.backend().buffer().clone()
    }

    fn assert_background_row_reaches_bordered_content_edges(buffer: &Buffer, background: Color) {
        let (y, cells) = (0..buffer.area.height)
            .filter_map(|y| {
                let cells = (0..buffer.area.width)
                    .filter(|x| buffer[(*x, y)].bg == background)
                    .collect::<Vec<_>>();
                (!cells.is_empty()).then_some((y, cells))
            })
            .max_by_key(|(_, cells)| cells.len())
            .unwrap_or_else(|| panic!("expected a row with background {background:?}"));
        let start = *cells.first().unwrap_or_else(|| unreachable!());
        let end = *cells.last().unwrap_or_else(|| unreachable!());

        assert!(cells.len() > 20, "background stopped at rendered text");
        assert!(cells.windows(2).all(|pair| pair[1] == pair[0] + 1));
        assert_eq!(buffer[(start - 1, y)].symbol(), "│");
        assert_eq!(buffer[(end + 1, y)].symbol(), "│");
        assert_ne!(buffer[(start - 1, y)].bg, background);
        assert_ne!(buffer[(end + 1, y)].bg, background);
    }

    fn rendered_buffer(state: &AppState, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend)
            .unwrap_or_else(|error| panic!("could not create terminal: {error}"));
        terminal
            .draw(|frame| render(frame, state))
            .unwrap_or_else(|error| panic!("could not draw: {error}"));
        terminal.backend().buffer().clone()
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
