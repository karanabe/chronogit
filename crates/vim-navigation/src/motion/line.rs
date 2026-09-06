//! UTF-8 byte-boundary and terminal display-cell calculations.
//!
//! Cursor columns stay as byte offsets at the public boundary while screen
//! motions and viewport visibility operate in display cells. Centralizing the
//! conversion here prevents Unicode width and tab-stop rules from leaking into
//! semantic text scans.

use unicode_width::UnicodeWidthChar;

use super::{MotionKind, Viewport};

pub(super) fn previous_column(line: &str, column: usize) -> usize {
    let end = clamp_boundary(line, column);
    line[..end]
        .char_indices()
        .next_back()
        .map_or(0, |(byte, _)| byte)
}

pub(super) fn next_column(line: &str, column: usize) -> usize {
    let start = clamp_boundary(line, column);
    let next = line[start..].chars().next().map_or(start, |character| {
        start.saturating_add(character.len_utf8())
    });
    if next >= line.len() {
        last_column(line)
    } else {
        next
    }
}

pub(super) fn last_column(line: &str) -> usize {
    line.char_indices().next_back().map_or(0, |(byte, _)| byte)
}

pub(super) fn first_non_blank(line: &str) -> usize {
    line.char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(0, |(column, _)| column)
}

pub(super) fn first_non_blank_from(line: &str, start: usize) -> usize {
    line.char_indices()
        .find(|(column, character)| *column >= start && !character.is_whitespace())
        .map_or_else(|| last_column(line), |(column, _)| column)
}

pub(super) fn last_non_blank(line: &str) -> usize {
    line.char_indices()
        .rev()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(0, |(column, _)| column)
}

pub(super) fn clamp_boundary(line: &str, requested: usize) -> usize {
    let mut column = requested.min(line.len());
    // External cursors can land inside a multibyte scalar. Rounding backward
    // preserves a position at or before the requested byte without slicing an
    // invalid UTF-8 range.
    while !line.is_char_boundary(column) {
        column = column.saturating_sub(1);
    }
    column
}

pub(super) fn byte_at_display(line: &str, display: usize) -> usize {
    let mut best = 0usize;
    let mut cells = 0usize;
    for (column, character) in line.char_indices() {
        if cells > display {
            break;
        }
        // A display coordinate inside a wide scalar or tab cell selects that
        // scalar's first byte, which is the only representable text position.
        best = column;
        cells = cells.saturating_add(if character == '\t' {
            next_tabstop(cells)
        } else {
            UnicodeWidthChar::width(character).unwrap_or(0)
        });
    }
    best
}

pub(super) fn display_column(line: &str, byte_column: usize) -> usize {
    let end = clamp_boundary(line, byte_column);
    line[..end].chars().fold(0usize, |column, character| {
        if character == '\t' {
            column.saturating_add(next_tabstop(column))
        } else {
            column.saturating_add(UnicodeWidthChar::width(character).unwrap_or(0))
        }
    })
}

pub(super) const fn next_tabstop(column: usize) -> usize {
    4usize.saturating_sub(column % 4)
}

pub(super) fn screen_end_column(line: &str, viewport: Viewport) -> usize {
    let (_, source_end) = visible_source_columns(viewport);
    byte_at_display(line, source_end.saturating_sub(1))
}

pub(super) fn screen_last_non_blank(line: &str, viewport: Viewport) -> usize {
    let (start, end) = visible_source_columns(viewport);
    line.char_indices()
        .rev()
        .find(|(column, character)| {
            let display = display_column(line, *column);
            display >= start && display < end && !character.is_whitespace()
        })
        .map_or_else(|| byte_at_display(line, start), |(column, _)| column)
}

pub(super) fn visible_source_columns(viewport: Viewport) -> (usize, usize) {
    let start = viewport.left.saturating_sub(viewport.gutter);
    let end = viewport
        .left
        .saturating_add(viewport.width)
        .saturating_sub(viewport.gutter)
        .max(start.saturating_add(1));
    (start, end)
}

pub(super) fn display_with_gutter(line: &str, column: usize, gutter: usize) -> usize {
    display_column(line, column).saturating_add(gutter)
}

pub(super) fn find_character(
    line: &str,
    column: usize,
    target: char,
    kind: MotionKind,
    count: usize,
    repeated: bool,
) -> usize {
    let forward = matches!(kind, MotionKind::FindForward | MotionKind::TillForward);
    let till = matches!(kind, MotionKind::TillForward | MotionKind::TillBackward);
    let adjacent = if forward {
        next_column(line, column)
    } else {
        previous_column(line, column)
    };
    // Vim's `;`/`,` repetition for `t`/`T` skips the adjacent occurrence that
    // would otherwise leave the cursor in place and repeats the useful search.
    let skip_adjacent = repeated && till;
    let found = if forward {
        line.char_indices()
            .filter(|(byte, character)| *byte > column && *character == target)
            .filter(|(byte, _)| !skip_adjacent || *byte != adjacent)
            .nth(count.saturating_sub(1))
            .map(|(byte, _)| byte)
    } else {
        line.char_indices()
            .rev()
            .filter(|(byte, character)| *byte < column && *character == target)
            .filter(|(byte, _)| !skip_adjacent || *byte != adjacent)
            .nth(count.saturating_sub(1))
            .map(|(byte, _)| byte)
    };
    let Some(found) = found else {
        return column;
    };
    if !till {
        found
    } else if forward {
        previous_column(line, found)
    } else {
        next_column(line, found)
    }
}

pub(super) fn is_pair_character(character: char) -> bool {
    matches!(character, '(' | ')' | '[' | ']' | '{' | '}')
}

pub(super) fn is_diff_change_line(line: &str) -> bool {
    (line.starts_with('+') && !line.starts_with("+++"))
        || (line.starts_with('-') && !line.starts_with("---"))
}

pub(super) fn count_as_isize(count: usize) -> isize {
    isize::try_from(count).unwrap_or(isize::MAX)
}
