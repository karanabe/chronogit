//! Motion dispatch and cursor/viewport postconditions.
//!
//! This private layer combines the borrowed text model with display-cell
//! calculations. It is kept separate from the public vocabulary so movement
//! policy, semantic scans, and Unicode display details can be maintained and
//! tested at their natural boundaries.

use unicode_width::UnicodeWidthChar;

use super::buffer::{Position, TextBuffer, WordMotion};
use super::line::{
    byte_at_display, clamp_boundary, count_as_isize, display_column, display_with_gutter,
    find_character, first_non_blank, first_non_blank_from, last_column, last_non_blank,
    next_column, next_tabstop, previous_column, screen_end_column, screen_last_non_blank,
    visible_source_columns,
};
use super::{Cursor, Motion, MotionKind, Viewport};

/// Applies one completed motion and returns the new cursor position.
///
/// `lines` is borrowed and never modified. An empty slice behaves like one
/// empty logical line. The input cursor is clamped to the final line and a
/// valid UTF-8 scalar boundary; counts and coordinates are handled with
/// saturating or content-bounded arithmetic rather than caller-visible errors.
/// The viewport is updated in place to keep the result visible and to retain
/// Vim's desired display column across vertical movement.
///
/// Search and mark variants need histories or resources that this crate does
/// not own. They are intentionally no-ops here and must be applied by a caller
/// adapter, which can then call [`reveal`] for the selected location.
pub fn apply(lines: &[&str], position: Cursor, viewport: &mut Viewport, motion: Motion) -> Cursor {
    let buffer = TextBuffer::new(lines);
    let mut cursor = buffer.clamp(position);
    let count = motion.count().max(1);
    // Vim retains the intended screen column through short lines, so a later
    // vertical motion can return to it rather than inheriting the short line.
    let desired_column = viewport
        .desired_column
        .unwrap_or_else(|| display_column(buffer.line(cursor.line), cursor.column));
    match motion.kind() {
        MotionKind::Left => {
            cursor = repeated_boundary(cursor, count, |mut at| {
                at.column = previous_column(buffer.line(at.line), at.column);
                at
            });
        }
        MotionKind::LeftWrap => {
            cursor = repeated_boundary(cursor, count, |mut at| {
                if at.column == 0 && at.line > 0 {
                    at.line = at.line.saturating_sub(1);
                    at.column = last_column(buffer.line(at.line));
                } else {
                    at.column = previous_column(buffer.line(at.line), at.column);
                }
                at
            });
        }
        MotionKind::Right => {
            cursor = repeated_boundary(cursor, count, |mut at| {
                at.column = next_column(buffer.line(at.line), at.column);
                at
            });
        }
        MotionKind::RightWrap => {
            cursor = repeated_boundary(cursor, count, |mut at| {
                let line = buffer.line(at.line);
                if at.column >= last_column(line) && at.line < buffer.last_line() {
                    at.line = at.line.saturating_add(1);
                    at.column = 0;
                } else {
                    at.column = next_column(line, at.column);
                }
                at
            });
        }
        MotionKind::Up => {
            cursor = buffer.move_lines_to_display(cursor, -(count_as_isize(count)), desired_column)
        }
        MotionKind::Down => {
            cursor = buffer.move_lines_to_display(cursor, count_as_isize(count), desired_column)
        }
        MotionKind::LineStart => cursor.column = 0,
        MotionKind::FirstNonBlank => cursor.column = first_non_blank(buffer.line(cursor.line)),
        MotionKind::LineEnd => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = last_column(buffer.line(cursor.line));
        }
        MotionKind::LastNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = last_non_blank(buffer.line(cursor.line));
        }
        MotionKind::ScreenLineStart => {
            let (source_left, _) = visible_source_columns(*viewport);
            cursor.column = byte_at_display(buffer.line(cursor.line), source_left);
        }
        MotionKind::ScreenFirstNonBlank => {
            let (source_left, _) = visible_source_columns(*viewport);
            let screen = byte_at_display(buffer.line(cursor.line), source_left);
            cursor.column = first_non_blank_from(buffer.line(cursor.line), screen);
        }
        MotionKind::ScreenLineEnd => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = screen_end_column(buffer.line(cursor.line), *viewport);
        }
        MotionKind::ScreenLastNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = screen_last_non_blank(buffer.line(cursor.line), *viewport);
        }
        MotionKind::ScreenMiddle => {
            let (source_left, source_end) = visible_source_columns(*viewport);
            cursor.column = byte_at_display(
                buffer.line(cursor.line),
                source_left.saturating_add(source_end.saturating_sub(source_left) / 2),
            );
        }
        MotionKind::LineMiddle => {
            let percent = if motion.has_explicit_count() {
                count.min(100)
            } else {
                50
            };
            let line = buffer.line(cursor.line);
            cursor.column = byte_at_display(
                line,
                display_column(line, line.len()).saturating_mul(percent) / 100,
            );
        }
        MotionKind::Column => {
            cursor.column = byte_at_display(buffer.line(cursor.line), count.saturating_sub(1));
        }
        MotionKind::ByteOffset => cursor = buffer.byte_offset(count),
        MotionKind::WordForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartForward, false)
        }
        MotionKind::BigWordForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartForward, true)
        }
        MotionKind::WordEndForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndForward, false)
        }
        MotionKind::BigWordEndForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndForward, true)
        }
        MotionKind::WordBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartBackward, false)
        }
        MotionKind::BigWordBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartBackward, true)
        }
        MotionKind::WordEndBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndBackward, false)
        }
        MotionKind::BigWordEndBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndBackward, true)
        }
        MotionKind::FindForward
        | MotionKind::FindBackward
        | MotionKind::TillForward
        | MotionKind::TillBackward => {
            if let Some(target) = motion.target() {
                cursor.column = find_character(
                    buffer.line(cursor.line),
                    cursor.column,
                    target,
                    motion.kind(),
                    count,
                    motion.is_repeated(),
                );
            }
        }
        MotionKind::PreviousLineFirstNonBlank => {
            cursor = buffer.move_lines(cursor, -(count_as_isize(count)));
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::NextLineFirstNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count));
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::CountedLineFirstNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::BufferTop => {
            cursor.line = if motion.has_explicit_count() {
                count.saturating_sub(1).min(buffer.last_line())
            } else {
                0
            };
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::BufferBottom => {
            cursor.line = buffer.last_line();
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::BufferBottomEnd => {
            cursor.line = if motion.has_explicit_count() {
                count.saturating_sub(1).min(buffer.last_line())
            } else {
                buffer.last_line()
            };
            cursor.column = last_column(buffer.line(cursor.line));
        }
        MotionKind::BufferPercentage => {
            let one_based = count
                .min(100)
                .saturating_mul(buffer.len())
                .saturating_add(99)
                / 100;
            cursor.line = one_based.saturating_sub(1).min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::WindowTop => {
            cursor.line = viewport
                .top
                .saturating_add(count.saturating_sub(1))
                .min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::WindowMiddle => {
            cursor.line = viewport
                .top
                .saturating_add(viewport.height.saturating_sub(1) / 2)
                .min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::WindowBottom => {
            cursor.line = viewport
                .top
                .saturating_add(viewport.height.saturating_sub(count))
                .min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::SentenceBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.sentence_backward(at));
        }
        MotionKind::SentenceForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.sentence_forward(at));
        }
        MotionKind::ParagraphBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.paragraph_backward(at));
        }
        MotionKind::ParagraphForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.paragraph_forward(at));
        }
        MotionKind::SectionStartBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, false, '{'));
        }
        MotionKind::SectionStartForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, true, '{'));
        }
        MotionKind::SectionEndBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, false, '}'));
        }
        MotionKind::SectionEndForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, true, '}'));
        }
        MotionKind::MatchingPair => {
            if let Some(found) = buffer.matching_pair(cursor, false) {
                cursor = found;
            }
        }
        MotionKind::MatchingPairBackward => {
            if let Some(found) = buffer.matching_pair(cursor, true) {
                cursor = found;
            }
        }
        MotionKind::UnmatchedOpenBackward => {
            if let Some(target) = motion.target() {
                cursor = repeated_boundary(cursor, count, |at| buffer.unmatched_open(at, target));
            }
        }
        MotionKind::UnmatchedCloseForward => {
            if let Some(target) = motion.target() {
                cursor = repeated_boundary(cursor, count, |at| buffer.unmatched_close(at, target));
            }
        }
        MotionKind::MethodBackward => {
            let target = if motion.target() == Some('M') {
                '}'
            } else {
                '{'
            };
            cursor = repeated_boundary(cursor, count, |at| buffer.brace(at, false, target));
        }
        MotionKind::MethodForward => {
            let target = if motion.target() == Some('M') {
                '}'
            } else {
                '{'
            };
            cursor = repeated_boundary(cursor, count, |at| buffer.brace(at, true, target));
        }
        MotionKind::PreprocessorBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.preprocessor(at, false));
        }
        MotionKind::PreprocessorForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.preprocessor(at, true));
        }
        MotionKind::CommentBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.comment(at, false));
        }
        MotionKind::CommentForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.comment(at, true));
        }
        MotionKind::DiffChangeBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.diff_change(at, false));
        }
        MotionKind::DiffChangeForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.diff_change(at, true));
        }
        MotionKind::HalfPageDown => {
            let distance = if motion.has_explicit_count() {
                count
            } else {
                viewport.height.saturating_div(2).max(1)
            };
            cursor = buffer.move_lines_to_display(cursor, count_as_isize(distance), desired_column);
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = viewport
                .top
                .saturating_add(distance)
                .min(buffer.last_line());
        }
        MotionKind::HalfPageUp => {
            let distance = if motion.has_explicit_count() {
                count
            } else {
                viewport.height.saturating_div(2).max(1)
            };
            cursor =
                buffer.move_lines_to_display(cursor, -(count_as_isize(distance)), desired_column);
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = viewport.top.saturating_sub(distance);
        }
        MotionKind::PageDown => {
            let page = viewport.height.saturating_sub(2).max(1);
            viewport.top = viewport
                .top
                .saturating_add(page.saturating_mul(count))
                .min(buffer.last_line());
            cursor.line = viewport.top;
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::PageUp => {
            let page = viewport.height.saturating_sub(2).max(1);
            viewport.top = viewport.top.saturating_sub(page.saturating_mul(count));
            cursor.line = viewport
                .top
                .saturating_add(viewport.height.saturating_sub(1))
                .min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        MotionKind::ScrollLineDown => {
            viewport.top = viewport.top.saturating_add(count).min(buffer.last_line());
            if cursor.line < viewport.top {
                cursor = buffer.move_to_line_display(cursor, viewport.top, desired_column);
            }
        }
        MotionKind::ScrollLineUp => {
            viewport.top = viewport.top.saturating_sub(count);
            let bottom = viewport
                .top
                .saturating_add(viewport.height.saturating_sub(1))
                .min(buffer.last_line());
            if cursor.line > bottom {
                cursor = buffer.move_to_line_display(cursor, bottom, desired_column);
            }
        }
        MotionKind::CursorToWindowTop => {
            if motion.has_explicit_count() {
                cursor =
                    buffer.move_to_line_display(cursor, count.saturating_sub(1), desired_column);
            }
            viewport.top = cursor.line;
        }
        MotionKind::CursorToWindowTopFirstNonBlank => {
            if motion.has_explicit_count() {
                cursor.line = count.saturating_sub(1).min(buffer.last_line());
            }
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = cursor.line;
        }
        MotionKind::CursorToWindowMiddle => {
            if motion.has_explicit_count() {
                cursor =
                    buffer.move_to_line_display(cursor, count.saturating_sub(1), desired_column);
            }
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1) / 2);
        }
        MotionKind::CursorToWindowMiddleFirstNonBlank => {
            if motion.has_explicit_count() {
                cursor.line = count.saturating_sub(1).min(buffer.last_line());
            }
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1) / 2);
        }
        MotionKind::CursorToWindowBottom => {
            if motion.has_explicit_count() {
                cursor =
                    buffer.move_to_line_display(cursor, count.saturating_sub(1), desired_column);
            }
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1));
        }
        MotionKind::CursorToWindowBottomFirstNonBlank => {
            if motion.has_explicit_count() {
                cursor.line = count.saturating_sub(1).min(buffer.last_line());
            }
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1));
        }
        MotionKind::NextWindowTop => {
            let target = if motion.has_explicit_count() {
                count.saturating_sub(1).min(buffer.last_line())
            } else {
                viewport
                    .top
                    .saturating_add(viewport.height)
                    .min(buffer.last_line())
            };
            cursor = buffer.move_to_line(cursor, target);
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = target;
        }
        MotionKind::PreviousWindowBottom => {
            let target = if motion.has_explicit_count() {
                count
                    .saturating_sub(viewport.height)
                    .min(buffer.last_line())
            } else {
                viewport.top.saturating_sub(1)
            };
            cursor = buffer.move_to_line(cursor, target);
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = target.saturating_sub(viewport.height.saturating_sub(1));
        }
        MotionKind::ScrollColumnLeft => {
            viewport.left = viewport.left.saturating_sub(count);
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        MotionKind::ScrollColumnRight => {
            viewport.left = viewport.left.saturating_add(count);
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        MotionKind::ScrollHalfScreenLeft => {
            viewport.left = viewport
                .left
                .saturating_sub(viewport.width.saturating_div(2).saturating_mul(count));
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        MotionKind::ScrollHalfScreenRight => {
            viewport.left = viewport
                .left
                .saturating_add(viewport.width.saturating_div(2).saturating_mul(count));
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        MotionKind::CursorToWindowLeft => {
            viewport.left =
                display_with_gutter(buffer.line(cursor.line), cursor.column, viewport.gutter);
        }
        MotionKind::CursorToWindowRight => {
            viewport.left =
                display_with_gutter(buffer.line(cursor.line), cursor.column, viewport.gutter)
                    .saturating_sub(viewport.width.saturating_sub(1));
        }
        // These commands depend on state outside a borrowed text snapshot.
        // MotionState resolves character repeats; application adapters own
        // search histories and marks and reveal their selected cursor later.
        MotionKind::RepeatCharacterSearch
        | MotionKind::ReverseCharacterSearch
        | MotionKind::SearchNext
        | MotionKind::SearchPrevious
        | MotionKind::SearchWordForward
        | MotionKind::SearchWordBackward
        | MotionKind::SearchPartialWordForward
        | MotionKind::SearchPartialWordBackward
        | MotionKind::PreviousMarkLine
        | MotionKind::PreviousMarkExact
        | MotionKind::NextMarkLine
        | MotionKind::NextMarkExact => {}
    }
    cursor = buffer.clamp(cursor.into());
    viewport.desired_column = if preserves_desired_column(motion.kind()) {
        Some(desired_column)
    } else if motion.kind() == MotionKind::LineEnd {
        // `$` followed by `j`/`k` tracks the end of each target line rather
        // than a finite display cell. The sentinel is private to this module.
        Some(usize::MAX)
    } else if is_viewport_only_motion(motion.kind()) {
        viewport.desired_column
    } else {
        Some(display_column(buffer.line(cursor.line), cursor.column))
    };
    keep_cursor_visible(&buffer, cursor, viewport, motion.kind());
    cursor.into()
}

fn preserves_desired_column(kind: MotionKind) -> bool {
    matches!(
        kind,
        MotionKind::Up | MotionKind::Down | MotionKind::ScrollLineUp | MotionKind::ScrollLineDown
    )
}

fn is_viewport_only_motion(kind: MotionKind) -> bool {
    matches!(
        kind,
        MotionKind::CursorToWindowTop
            | MotionKind::CursorToWindowMiddle
            | MotionKind::CursorToWindowBottom
            | MotionKind::ScrollColumnLeft
            | MotionKind::ScrollColumnRight
            | MotionKind::ScrollHalfScreenLeft
            | MotionKind::ScrollHalfScreenRight
            | MotionKind::CursorToWindowLeft
            | MotionKind::CursorToWindowRight
    )
}

fn follow_horizontal_scroll(
    buffer: &TextBuffer<'_>,
    cursor: &mut Position,
    viewport: &mut Viewport,
) {
    let line = buffer.line(cursor.line);
    let last_display = display_with_gutter(line, last_column(line), viewport.gutter);
    viewport.left = viewport.left.min(last_display);
    let display = display_with_gutter(line, cursor.column, viewport.gutter);
    let requested = if display < viewport.left {
        Some(viewport.left)
    } else if display >= viewport.left.saturating_add(viewport.width) {
        Some(
            viewport
                .left
                .saturating_add(viewport.width.saturating_sub(1)),
        )
    } else {
        None
    };
    if let Some(requested) = requested {
        cursor.column = byte_at_display(line, requested.saturating_sub(viewport.gutter));
    }
}

/// Adjusts a viewport so an externally selected location is visible.
///
/// This is the adapter counterpart to resource-aware search and mark motions:
/// after selecting a cursor with caller-owned state, use `reveal` to apply the
/// same line/UTF-8 clamping and viewport visibility rules as [`apply`]. Text is
/// borrowed, empty input is accepted, and no cursor is returned because the
/// caller already owns the selection.
pub fn reveal(lines: &[&str], position: Cursor, viewport: &mut Viewport) {
    let buffer = TextBuffer::new(lines);
    let cursor = buffer.clamp(position);
    keep_cursor_visible(&buffer, cursor, viewport, MotionKind::Left);
}

fn repeated_boundary(
    mut cursor: Position,
    count: usize,
    mut step: impl FnMut(Position) -> Position,
) -> Position {
    // Huge saturated counts must remain proportional to reachable content.
    // Vim stops repeating once an additional step cannot change the cursor.
    for _ in 0..count {
        let next = step(cursor);
        if next == cursor {
            break;
        }
        cursor = next;
    }
    cursor
}

fn keep_cursor_visible(
    buffer: &TextBuffer<'_>,
    cursor: Position,
    viewport: &mut Viewport,
    kind: MotionKind,
) {
    keep_line_cursor_visible(
        buffer.line(cursor.line),
        buffer.last_line(),
        cursor,
        viewport,
        kind,
    );
}

pub(crate) fn reveal_cursor_line(
    line: &str,
    last_line: usize,
    cursor: Cursor,
    viewport: &mut Viewport,
) {
    keep_line_cursor_visible(
        line,
        last_line,
        Position {
            line: cursor.line(),
            column: clamp_boundary(line, cursor.byte_column()),
        },
        viewport,
        MotionKind::Left,
    );
}

fn keep_line_cursor_visible(
    line: &str,
    last_line: usize,
    cursor: Position,
    viewport: &mut Viewport,
    kind: MotionKind,
) {
    if !matches!(
        kind,
        MotionKind::CursorToWindowTop
            | MotionKind::CursorToWindowTopFirstNonBlank
            | MotionKind::CursorToWindowMiddle
            | MotionKind::CursorToWindowMiddleFirstNonBlank
            | MotionKind::CursorToWindowBottom
            | MotionKind::CursorToWindowBottomFirstNonBlank
            | MotionKind::NextWindowTop
            | MotionKind::PreviousWindowBottom
            | MotionKind::ScrollLineDown
            | MotionKind::ScrollLineUp
    ) {
        if cursor.line < viewport.top {
            viewport.top = cursor.line;
        } else if cursor.line >= viewport.top.saturating_add(viewport.height) {
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1));
        }
    }
    viewport.top = viewport.top.min(last_line);

    if matches!(
        kind,
        MotionKind::ScrollColumnLeft
            | MotionKind::ScrollColumnRight
            | MotionKind::ScrollHalfScreenLeft
            | MotionKind::ScrollHalfScreenRight
            | MotionKind::CursorToWindowLeft
            | MotionKind::CursorToWindowRight
    ) {
        return;
    }
    let source_display = display_column(line, cursor.column);
    let display = source_display.saturating_add(viewport.gutter);
    let cursor_width = line[cursor.column..].chars().next().map_or(1, |character| {
        if character == '\t' {
            next_tabstop(source_display)
        } else {
            UnicodeWidthChar::width(character).unwrap_or(0).max(1)
        }
    });
    let end = display.saturating_add(cursor_width);
    if display < viewport.left {
        viewport.left = display;
    } else if end > viewport.left.saturating_add(viewport.width) {
        viewport.left = end.saturating_sub(viewport.width);
    }
}
