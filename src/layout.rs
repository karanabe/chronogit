//! Shared terminal-layout measurements used by rendering and viewport motion.

pub(crate) const FULL_PERCENT: u16 = 100;
pub(crate) const MIN_TERMINAL_WIDTH: u16 = 80;
pub(crate) const MIN_TERMINAL_HEIGHT: u16 = 24;
pub(crate) const WIDE_LAYOUT_WIDTH: u16 = 110;
pub(crate) const ROOT_DISPLAY_WIDTH: u16 = 180;

pub(crate) const FOOTER_ROWS: u16 = 1;
pub(crate) const MIN_CONTENT_ROWS: u16 = 1;
pub(crate) const PANE_BORDER_CELLS: u16 = 2;
pub(crate) const DOCUMENT_OVERLAY_MARGIN: u16 = 1;
pub(crate) const DOCUMENT_OVERLAY_INSET: u16 = DOCUMENT_OVERLAY_MARGIN * 2;
pub(crate) const SEARCH_BAR_ROWS: u16 = 1;
pub(crate) const SEARCH_INPUT_ROWS: u16 = 3;
pub(crate) const DOCUMENT_OVERLAY_RESERVED_ROWS: u16 = DOCUMENT_OVERLAY_INSET + SEARCH_BAR_ROWS;

pub(crate) const CHANGES_LIST_PERCENT: u16 = 32;
pub(crate) const CHANGES_DIFF_PERCENT: u16 = 68;
pub(crate) const HISTORY_LIST_PERCENT: u16 = 25;
pub(crate) const HISTORY_MIDDLE_PERCENT: u16 = 25;
pub(crate) const HISTORY_DIFF_PERCENT: u16 = 50;
pub(crate) const COMMIT_DETAILS_LIST_PERCENT: u16 = 25;
pub(crate) const COMMIT_DETAILS_BODY_PERCENT: u16 = 45;
pub(crate) const COMMIT_DETAILS_FILES_PERCENT: u16 = 30;
pub(crate) const GRAPH_DETAILS_WIDTH_PERCENT: u16 = 90;
pub(crate) const GRAPH_DETAILS_HEIGHT_PERCENT: u16 = 88;
pub(crate) const GRAPH_DETAILS_FILES_PERCENT: u16 = 38;
pub(crate) const GRAPH_DETAILS_DIFF_PERCENT: u16 = 62;
pub(crate) const FILE_HISTORY_LIST_PERCENT: u16 = 38;
pub(crate) const FILE_HISTORY_CONTENT_PERCENT: u16 = 62;
pub(crate) const CODE_TREE_PERCENT: u16 = 42;
pub(crate) const CODE_CONTENT_PERCENT: u16 = 58;

pub(crate) const HELP_WIDTH_PERCENT: u16 = 76;
pub(crate) const HELP_HEIGHT_PERCENT: u16 = 88;
pub(crate) const MESSAGE_WIDTH_PERCENT: u16 = 82;
pub(crate) const MESSAGE_HEIGHT_PERCENT: u16 = 78;
pub(crate) const SYMBOL_WIDTH_PERCENT: u16 = 82;
pub(crate) const SYMBOL_HEIGHT_PERCENT: u16 = 72;
pub(crate) const HOVER_WIDTH_PERCENT: u16 = 82;
pub(crate) const HOVER_HEIGHT_PERCENT: u16 = 62;
pub(crate) const REPOSITORY_SEARCH_WIDTH_PERCENT: u16 = 86;
pub(crate) const REPOSITORY_SEARCH_HEIGHT_PERCENT: u16 = 82;

pub(crate) const SOURCE_GUTTER_COLUMNS: usize = 8;
// Navigation marker, two five-column line numbers, and two separating spaces.
pub(crate) const DIFF_GUTTER_COLUMNS: usize = 13;

use crate::app::{AppView, FocusedPane};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub(crate) fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
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

/// Outer rectangles in primary, secondary, diff order. Code and file-history
/// layouts leave the unused secondary slot empty.
pub(crate) fn main_panes(area: Rect, view: AppView, focus: FocusedPane) -> [Rect; 3] {
    let empty = Rect::default();
    match view {
        AppView::Changes if area.width < WIDE_LAYOUT_WIDTH => {
            if focus == FocusedPane::Diff {
                [empty, empty, area]
            } else {
                [area, empty, empty]
            }
        }
        AppView::Changes => {
            let rows = Layout::horizontal([
                Constraint::Percentage(CHANGES_LIST_PERCENT),
                Constraint::Percentage(CHANGES_DIFF_PERCENT),
            ])
            .split(area);
            [rows[0], empty, rows[1]]
        }
        AppView::History | AppView::CommitDetails => {
            let percentages = if view == AppView::History {
                [
                    HISTORY_LIST_PERCENT,
                    HISTORY_MIDDLE_PERCENT,
                    HISTORY_DIFF_PERCENT,
                ]
            } else {
                [
                    COMMIT_DETAILS_LIST_PERCENT,
                    COMMIT_DETAILS_BODY_PERCENT,
                    COMMIT_DETAILS_FILES_PERCENT,
                ]
            };
            let rows = Layout::vertical(percentages.map(Constraint::Percentage)).split(area);
            [rows[0], rows[1], rows[2]]
        }
        AppView::Graph => [area, empty, empty],
        AppView::GraphDetails => {
            let popup = centered(
                area,
                GRAPH_DETAILS_WIDTH_PERCENT,
                GRAPH_DETAILS_HEIGHT_PERCENT,
            );
            let rows = Layout::vertical([
                Constraint::Percentage(GRAPH_DETAILS_FILES_PERCENT),
                Constraint::Percentage(GRAPH_DETAILS_DIFF_PERCENT),
            ])
            .split(popup);
            [area, rows[0], rows[1]]
        }
        AppView::FileHistory | AppView::Code => {
            let percentages = if view == AppView::Code {
                [CODE_TREE_PERCENT, CODE_CONTENT_PERCENT]
            } else {
                [FILE_HISTORY_LIST_PERCENT, FILE_HISTORY_CONTENT_PERCENT]
            };
            let rows = Layout::vertical(percentages.map(Constraint::Percentage)).split(area);
            [rows[0], empty, rows[1]]
        }
    }
}

pub(crate) fn main_area(area: Rect) -> Rect {
    Layout::vertical([
        Constraint::Min(MIN_CONTENT_ROWS),
        Constraint::Length(FOOTER_ROWS),
    ])
    .split(area)[0]
}
