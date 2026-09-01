use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{
    AppState, AppView, FocusedPane, HistoryPanel, LoadState, Overlay, RepositorySearchKind,
    VisibleTreeEntry,
};
use crate::domain::{DiffDocument, DiffLine, DiffLineKind, FileDocument, TreeKind};
use crate::tui::graph::graph_prefixes;

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;
const WIDE_WIDTH: u16 = 110;

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
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(area);
            render_file_list(
                frame,
                rows[0],
                state,
                "Changed files [Esc: graph, Enter: full diff]",
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
    }
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
    let title = format!("History — {path} [j/k: show commit diff, Esc: back]");
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
        LoadState::Ready(message) => sanitize_multiline(message.body()),
    };
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let last = text.lines().count().saturating_sub(1);
    let cursor = state.message.scroll.min(last);
    let vertical = cursor
        .saturating_sub(visible.saturating_sub(1))
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .scroll((vertical, 0))
            .wrap(Wrap { trim: false }),
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
        "Changed files [t: tree]",
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
        "Commit tree [t: files]",
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
    let vertical = cursor
        .saturating_sub(visible.saturating_sub(1))
        .min(u16::MAX as usize) as u16;
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
    let vertical = cursor
        .saturating_sub(visible.saturating_sub(1))
        .min(u16::MAX as usize) as u16;
    let horizontal = state.file_view.horizontal.min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((vertical, horizontal)),
        area,
    );
}

fn file_document_lines(document: &FileDocument, state: &AppState) -> Vec<Line<'static>> {
    if let Some(message) = document.message() {
        return vec![current_file_line(plain(sanitize_inline(message)), 0, state)];
    }
    let mut lines = document
        .lines()
        .iter()
        .enumerate()
        .map(|(index, line)| {
            current_file_line(
                Line::raw(format!("{:>6} {}", index + 1, sanitize_inline(line))),
                index,
                state,
            )
        })
        .collect::<Vec<_>>();
    if document.is_truncated() {
        let index = lines.len();
        lines.push(current_file_line(
            Line::styled(
                "… file truncated at the safe output limit …",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            index,
            state,
        ));
    }
    if lines.is_empty() {
        lines.push(current_file_line(plain("Empty file."), 0, state));
    }
    lines
}

fn current_file_line(mut line: Line<'static>, index: usize, state: &AppState) -> Line<'static> {
    if state.file_view.vertical == index
        && (state.overlay == Overlay::FileContent || state.focus == FocusedPane::Diff)
    {
        line.style = line.style.patch(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
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
    let mut lines = document
        .lines()
        .iter()
        .enumerate()
        .map(|(index, line)| highlight_diff_line(diff_line(line), index, state))
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
    if state.search.current_line() != Some(index)
        && state.diff.vertical == index
        && (state.overlay == Overlay::Diff || state.focus == FocusedPane::Diff)
    {
        line.style = line.style.patch(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    }
    line
}

fn diff_line(line: &DiffLine) -> Line<'static> {
    let old = line
        .old_line()
        .map(|value| value.value().to_string())
        .unwrap_or_default();
    let new = line
        .new_line()
        .map(|value| value.value().to_string())
        .unwrap_or_default();
    let style = match line.kind() {
        DiffLineKind::Added => Style::default().fg(Color::Green),
        DiffLineKind::Removed => Style::default().fg(Color::Red),
        DiffLineKind::Hunk => Style::default().fg(Color::Cyan),
        DiffLineKind::Header => Style::default().fg(Color::Blue),
        DiffLineKind::Meta => Style::default().fg(Color::Yellow),
        DiffLineKind::Context => Style::default(),
    };
    Line::styled(
        format!("{old:>5} {new:>5} {}", sanitize_inline(line.text())),
        style,
    )
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let view = match state.view {
        AppView::Changes => "CHANGES",
        AppView::History => "HISTORY",
        AppView::CommitDetails => "DETAILS",
        AppView::Graph => "GRAPH",
        AppView::GraphDetails => "GRAPH DETAILS",
        AppView::FileHistory => "FILE HISTORY",
    };
    let notice = state
        .notice
        .as_ref()
        .map(|notice| format!(" | error: {}", sanitize_inline(notice.message())))
        .unwrap_or_default();
    let comparison = selected_baseline(state).unwrap_or_else(|| "comparison pending".to_owned());
    let controls = match (state.view, area.width >= WIDE_WIDTH) {
        (AppView::CommitDetails, true) => {
            "b History  m message  h/l or ^j/^k pane  j/k move  Enter diff  F1 help  q quit"
        }
        (AppView::CommitDetails, false) => "b History  m msg  ^j/^k pane  Enter diff  q quit",
        (AppView::GraphDetails, true) => {
            "Esc Graph  m message  h/l pane  j/k move  Enter full diff  Space f/g search  q quit"
        }
        (AppView::GraphDetails, false) => "Esc Graph  j/k file  Enter diff  q quit",
        (AppView::FileHistory, true) => {
            "Esc back  h/l pane  j/k history  Enter full view  Space f/g search  q quit"
        }
        (AppView::FileHistory, false) => "Esc back  j/k history  Enter full  q quit",
        (_, true) => {
            "1/2/3 view  h/l or ^j/^k pane  j/k move  Enter open  Space f/g search  r refresh  m message  F1 help  q quit"
        }
        (_, false) => "1/2/3  Space f/g find  Enter open  q quit",
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
            " {}{notice} | {controls}{root}",
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
                plain(""),
                plain("1 / 2       Changes / History"),
                plain("3           Git graph"),
                plain("Space f/g   Search files / repository content"),
                plain("h / l       Focus previous / next pane"),
                plain("Ctrl-j / k  Focus next / previous pane"),
                plain("j / k       Move or scroll"),
                plain("g / G       First / last"),
                plain("Ctrl-u/d    Half page up / down"),
                plain("zh / zl     Diff horizontal scroll"),
                plain("r           Refresh current view"),
                plain("m           Toggle full commit message"),
                plain("b           History diff / body layout"),
                plain("t           Changed files / commit tree"),
                plain("Enter       Open details/full view; close an opened view"),
                plain("/ , ?       Search diff forward / backward"),
                plain("n / N       Next / previous search match"),
                plain("F1          Toggle this help"),
                plain("Esc         Close overlay"),
                plain("q / Ctrl-C  Quit"),
                plain(""),
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
            let text = match &state.message.content {
                LoadState::Idle => "Select a commit.".to_owned(),
                LoadState::Loading { .. } => "Loading commit message…".to_owned(),
                LoadState::Failed(error) => {
                    format!("Error: {}", sanitize_inline(error.message()))
                }
                LoadState::Ready(message) => sanitize_multiline(message.as_str()),
            };
            let visible = usize::from(popup.height.saturating_sub(2)).max(1);
            let last = text.lines().count().saturating_sub(1);
            let cursor = state.message.scroll.min(last);
            let vertical = cursor
                .saturating_sub(visible.saturating_sub(1))
                .min(u16::MAX as usize) as u16;
            frame.render_widget(
                Paragraph::new(text)
                    .block(
                        Block::default()
                            .title(" Commit message [m/Esc: close] ")
                            .borders(Borders::ALL),
                    )
                    .scroll((vertical, 0))
                    .wrap(Wrap { trim: false }),
                popup,
            );
        }
        Overlay::Diff => render_diff_overlay(frame, area, state),
        Overlay::RepositorySearch => render_repository_search_overlay(frame, area, state),
        Overlay::FileContent => render_file_content_overlay(frame, area, state),
    }
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
    frame.render_widget(
        Paragraph::new(format!("> {}{cursor}", sanitize_inline(query))).block(
            Block::default()
                .title(format!(" Search {mode} [Enter: run, Esc: close] "))
                .borders(Borders::ALL),
        ),
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
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Results [j/k: move, Enter: open file] ")
                    .borders(Borders::ALL),
            )
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
    let path = state
        .file_view
        .path
        .as_ref()
        .map(|path| sanitize_inline(&path.display()))
        .unwrap_or_else(|| "file".to_owned());
    render_file_content(frame, popup, state, &format!("{path} [Enter/Esc: close]"));
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
        || "Diff [Enter/Esc: close]".to_owned(),
        |value| format!("Diff — {value} [Enter/Esc: close]"),
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
        "Terminal too small: {}x{}. ChronoGit needs at least {MIN_WIDTH}x{MIN_HEIGHT}. Press q to quit.",
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

    use super::{diff_line, diff_lines, render, sanitize_inline, sanitize_multiline};
    use crate::app::{
        Action, AppState, AppView, ErrorNotice, FocusedPane, LoadState, Overlay,
        RepositorySearchKind,
    };
    use crate::domain::{
        ChangeKind, ChangedFile, CommitBaseline, CommitMessage, CommitSummary, DiffDocument,
        DiffLine, DiffLineKind, DiffTarget, FileDocument, ObjectId, RepoPath, RepositoryRoot,
        SearchHit, WorktreeChange,
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
        assert!(text.contains("F1          Toggle this help"));
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
            text.contains("q quit"),
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
    fn assigns_distinct_semantic_colors_to_diff_lines() {
        for (kind, expected) in [
            (DiffLineKind::Added, Some(Color::Green)),
            (DiffLineKind::Removed, Some(Color::Red)),
            (DiffLineKind::Hunk, Some(Color::Cyan)),
            (DiffLineKind::Header, Some(Color::Blue)),
            (DiffLineKind::Meta, Some(Color::Yellow)),
            (DiffLineKind::Context, None),
        ] {
            let line = diff_line(&DiffLine::new(kind, None, None, "fixture".to_owned()));
            assert_eq!(line.style.fg, expected, "unexpected color for {kind:?}");
        }
    }

    #[test]
    fn highlights_the_diff_line_selected_by_j_and_k() {
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
        assert_eq!(initial[0].style.bg, Some(Color::DarkGray));
        assert_eq!(initial[1].style.bg, None);

        let _none = state.handle_action(Action::MoveDown);
        let moved = diff_lines(&document, &state);
        assert_eq!(moved[0].style.bg, None);
        assert_eq!(moved[1].style.bg, Some(Color::DarkGray));

        let _none = state.handle_action(Action::MoveUp);
        let returned = diff_lines(&document, &state);
        assert_eq!(returned[0].style.bg, Some(Color::DarkGray));
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
        assert!(rendered_text(&graph, 100, 30).contains("Esc: graph"));

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
        assert!(text.contains("src/lib.rs:42"));
        assert!(text.contains("let needle = true"));
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
