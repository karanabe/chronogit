//! Pure Vim normal-mode cursor motion over read-only display text.

use std::cmp::Ordering;
use unicode_width::UnicodeWidthChar;

use crate::app::{VimMotion, VimMotionKind};
use crate::domain::SourcePosition;

/// The visible part of a text pane, including any rendered gutter columns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Viewport {
    pub(crate) top: usize,
    pub(crate) left: usize,
    pub(crate) height: usize,
    pub(crate) width: usize,
    pub(crate) gutter: usize,
    pub(crate) desired_column: Option<usize>,
}

impl Viewport {
    pub(crate) fn new(top: usize, left: usize, height: usize, width: usize, gutter: usize) -> Self {
        Self {
            top,
            left,
            height: height.max(1),
            width: width.max(1),
            gutter,
            desired_column: None,
        }
    }

    pub(crate) fn with_desired_column(mut self, desired_column: Option<usize>) -> Self {
        self.desired_column = desired_column;
        self
    }
}

/// Applies one completed motion and returns the new cursor position.
pub(crate) fn apply(
    lines: &[&str],
    position: SourcePosition,
    viewport: &mut Viewport,
    motion: VimMotion,
) -> SourcePosition {
    let buffer = TextBuffer::new(lines);
    let mut cursor = buffer.clamp(position);
    let count = motion.count().max(1);
    let desired_column = viewport
        .desired_column
        .unwrap_or_else(|| crate::lsp::display_column(buffer.line(cursor.line), cursor.column));
    match motion.kind() {
        VimMotionKind::Left => {
            cursor = repeated_boundary(cursor, count, |mut at| {
                at.column = previous_column(buffer.line(at.line), at.column);
                at
            });
        }
        VimMotionKind::LeftWrap => {
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
        VimMotionKind::Right => {
            cursor = repeated_boundary(cursor, count, |mut at| {
                at.column = next_column(buffer.line(at.line), at.column);
                at
            });
        }
        VimMotionKind::RightWrap => {
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
        VimMotionKind::Up => {
            cursor = buffer.move_lines_to_display(cursor, -(count_as_isize(count)), desired_column)
        }
        VimMotionKind::Down => {
            cursor = buffer.move_lines_to_display(cursor, count_as_isize(count), desired_column)
        }
        VimMotionKind::LineStart => cursor.column = 0,
        VimMotionKind::FirstNonBlank => cursor.column = first_non_blank(buffer.line(cursor.line)),
        VimMotionKind::LineEnd => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = last_column(buffer.line(cursor.line));
        }
        VimMotionKind::LastNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = last_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::ScreenLineStart => {
            let (source_left, _) = visible_source_columns(*viewport);
            cursor.column = byte_at_display(buffer.line(cursor.line), source_left);
        }
        VimMotionKind::ScreenFirstNonBlank => {
            let (source_left, _) = visible_source_columns(*viewport);
            let screen = byte_at_display(buffer.line(cursor.line), source_left);
            cursor.column = first_non_blank_from(buffer.line(cursor.line), screen);
        }
        VimMotionKind::ScreenLineEnd => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = screen_end_column(buffer.line(cursor.line), *viewport);
        }
        VimMotionKind::ScreenLastNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = screen_last_non_blank(buffer.line(cursor.line), *viewport);
        }
        VimMotionKind::ScreenMiddle => {
            let (source_left, source_end) = visible_source_columns(*viewport);
            cursor.column = byte_at_display(
                buffer.line(cursor.line),
                source_left.saturating_add(source_end.saturating_sub(source_left) / 2),
            );
        }
        VimMotionKind::LineMiddle => {
            let percent = if motion.has_explicit_count() {
                count.min(100)
            } else {
                50
            };
            let line = buffer.line(cursor.line);
            cursor.column = byte_at_display(
                line,
                crate::lsp::display_column(line, line.len()).saturating_mul(percent) / 100,
            );
        }
        VimMotionKind::Column => {
            cursor.column = byte_at_display(buffer.line(cursor.line), count.saturating_sub(1));
        }
        VimMotionKind::ByteOffset => cursor = buffer.byte_offset(count),
        VimMotionKind::WordForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartForward, false)
        }
        VimMotionKind::BigWordForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartForward, true)
        }
        VimMotionKind::WordEndForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndForward, false)
        }
        VimMotionKind::BigWordEndForward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndForward, true)
        }
        VimMotionKind::WordBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartBackward, false)
        }
        VimMotionKind::BigWordBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::StartBackward, true)
        }
        VimMotionKind::WordEndBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndBackward, false)
        }
        VimMotionKind::BigWordEndBackward => {
            cursor = buffer.word_motion(cursor, count, WordMotion::EndBackward, true)
        }
        VimMotionKind::FindForward
        | VimMotionKind::FindBackward
        | VimMotionKind::TillForward
        | VimMotionKind::TillBackward => {
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
        VimMotionKind::PreviousLineFirstNonBlank => {
            cursor = buffer.move_lines(cursor, -(count_as_isize(count)));
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::NextLineFirstNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count));
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::CountedLineFirstNonBlank => {
            cursor = buffer.move_lines(cursor, count_as_isize(count.saturating_sub(1)));
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::BufferTop => {
            cursor.line = if motion.has_explicit_count() {
                count.saturating_sub(1).min(buffer.last_line())
            } else {
                0
            };
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::BufferBottom => {
            cursor.line = buffer.last_line();
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::BufferBottomEnd => {
            cursor.line = if motion.has_explicit_count() {
                count.saturating_sub(1).min(buffer.last_line())
            } else {
                buffer.last_line()
            };
            cursor.column = last_column(buffer.line(cursor.line));
        }
        VimMotionKind::BufferPercentage => {
            let one_based = count
                .min(100)
                .saturating_mul(buffer.len())
                .saturating_add(99)
                / 100;
            cursor.line = one_based.saturating_sub(1).min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::WindowTop => {
            cursor.line = viewport
                .top
                .saturating_add(count.saturating_sub(1))
                .min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::WindowMiddle => {
            cursor.line = viewport
                .top
                .saturating_add(viewport.height.saturating_sub(1) / 2)
                .min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::WindowBottom => {
            cursor.line = viewport
                .top
                .saturating_add(viewport.height.saturating_sub(count))
                .min(buffer.last_line());
            cursor.column = first_non_blank(buffer.line(cursor.line));
        }
        VimMotionKind::SentenceBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.sentence_backward(at));
        }
        VimMotionKind::SentenceForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.sentence_forward(at));
        }
        VimMotionKind::ParagraphBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.paragraph_backward(at));
        }
        VimMotionKind::ParagraphForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.paragraph_forward(at));
        }
        VimMotionKind::SectionStartBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, false, '{'));
        }
        VimMotionKind::SectionStartForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, true, '{'));
        }
        VimMotionKind::SectionEndBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, false, '}'));
        }
        VimMotionKind::SectionEndForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.section(at, true, '}'));
        }
        VimMotionKind::MatchingPair => {
            if let Some(found) = buffer.matching_pair(cursor, false) {
                cursor = found;
            }
        }
        VimMotionKind::MatchingPairBackward => {
            if let Some(found) = buffer.matching_pair(cursor, true) {
                cursor = found;
            }
        }
        VimMotionKind::UnmatchedOpenBackward => {
            if let Some(target) = motion.target() {
                cursor = repeated_boundary(cursor, count, |at| buffer.unmatched_open(at, target));
            }
        }
        VimMotionKind::UnmatchedCloseForward => {
            if let Some(target) = motion.target() {
                cursor = repeated_boundary(cursor, count, |at| buffer.unmatched_close(at, target));
            }
        }
        VimMotionKind::MethodBackward => {
            let target = if motion.target() == Some('M') {
                '}'
            } else {
                '{'
            };
            cursor = repeated_boundary(cursor, count, |at| buffer.brace(at, false, target));
        }
        VimMotionKind::MethodForward => {
            let target = if motion.target() == Some('M') {
                '}'
            } else {
                '{'
            };
            cursor = repeated_boundary(cursor, count, |at| buffer.brace(at, true, target));
        }
        VimMotionKind::PreprocessorBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.preprocessor(at, false));
        }
        VimMotionKind::PreprocessorForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.preprocessor(at, true));
        }
        VimMotionKind::CommentBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.comment(at, false));
        }
        VimMotionKind::CommentForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.comment(at, true));
        }
        VimMotionKind::DiffChangeBackward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.diff_change(at, false));
        }
        VimMotionKind::DiffChangeForward => {
            cursor = repeated_boundary(cursor, count, |at| buffer.diff_change(at, true));
        }
        VimMotionKind::HalfPageDown => {
            let distance = if motion.has_explicit_count() {
                count
            } else {
                viewport.height.saturating_div(2).max(1)
            };
            cursor = buffer.move_lines_to_display(cursor, count_as_isize(distance), desired_column);
        }
        VimMotionKind::HalfPageUp => {
            let distance = if motion.has_explicit_count() {
                count
            } else {
                viewport.height.saturating_div(2).max(1)
            };
            cursor =
                buffer.move_lines_to_display(cursor, -(count_as_isize(distance)), desired_column);
        }
        VimMotionKind::PageDown => {
            let page = viewport.height.saturating_sub(2).max(1);
            cursor = buffer.move_lines_to_display(
                cursor,
                count_as_isize(page.saturating_mul(count)),
                desired_column,
            );
            viewport.top = viewport
                .top
                .saturating_add(page.saturating_mul(count))
                .min(buffer.last_line());
        }
        VimMotionKind::PageUp => {
            let page = viewport.height.saturating_sub(2).max(1);
            cursor = buffer.move_lines_to_display(
                cursor,
                -(count_as_isize(page.saturating_mul(count))),
                desired_column,
            );
            viewport.top = viewport.top.saturating_sub(page.saturating_mul(count));
        }
        VimMotionKind::ScrollLineDown => {
            viewport.top = viewport.top.saturating_add(count).min(buffer.last_line());
            if cursor.line < viewport.top {
                cursor = buffer.move_to_line_display(cursor, viewport.top, desired_column);
            }
        }
        VimMotionKind::ScrollLineUp => {
            viewport.top = viewport.top.saturating_sub(count);
            let bottom = viewport
                .top
                .saturating_add(viewport.height.saturating_sub(1))
                .min(buffer.last_line());
            if cursor.line > bottom {
                cursor = buffer.move_to_line_display(cursor, bottom, desired_column);
            }
        }
        VimMotionKind::CursorToWindowTop => {
            if motion.has_explicit_count() {
                cursor =
                    buffer.move_to_line_display(cursor, count.saturating_sub(1), desired_column);
            }
            viewport.top = cursor.line;
        }
        VimMotionKind::CursorToWindowTopFirstNonBlank => {
            if motion.has_explicit_count() {
                cursor.line = count.saturating_sub(1).min(buffer.last_line());
            }
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = cursor.line;
        }
        VimMotionKind::CursorToWindowMiddle => {
            if motion.has_explicit_count() {
                cursor =
                    buffer.move_to_line_display(cursor, count.saturating_sub(1), desired_column);
            }
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1) / 2);
        }
        VimMotionKind::CursorToWindowMiddleFirstNonBlank => {
            if motion.has_explicit_count() {
                cursor.line = count.saturating_sub(1).min(buffer.last_line());
            }
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1) / 2);
        }
        VimMotionKind::CursorToWindowBottom => {
            if motion.has_explicit_count() {
                cursor =
                    buffer.move_to_line_display(cursor, count.saturating_sub(1), desired_column);
            }
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1));
        }
        VimMotionKind::CursorToWindowBottomFirstNonBlank => {
            if motion.has_explicit_count() {
                cursor.line = count.saturating_sub(1).min(buffer.last_line());
            }
            cursor.column = first_non_blank(buffer.line(cursor.line));
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1));
        }
        VimMotionKind::NextWindowTop => {
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
        VimMotionKind::PreviousWindowBottom => {
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
        VimMotionKind::ScrollColumnLeft => {
            viewport.left = viewport.left.saturating_sub(count);
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        VimMotionKind::ScrollColumnRight => {
            viewport.left = viewport.left.saturating_add(count);
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        VimMotionKind::ScrollHalfScreenLeft => {
            viewport.left = viewport
                .left
                .saturating_sub(viewport.width.saturating_div(2).saturating_mul(count));
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        VimMotionKind::ScrollHalfScreenRight => {
            viewport.left = viewport
                .left
                .saturating_add(viewport.width.saturating_div(2).saturating_mul(count));
            follow_horizontal_scroll(&buffer, &mut cursor, viewport);
        }
        VimMotionKind::CursorToWindowLeft => {
            viewport.left =
                display_with_gutter(buffer.line(cursor.line), cursor.column, viewport.gutter);
        }
        VimMotionKind::CursorToWindowRight => {
            viewport.left =
                display_with_gutter(buffer.line(cursor.line), cursor.column, viewport.gutter)
                    .saturating_sub(viewport.width.saturating_sub(1));
        }
        VimMotionKind::RepeatCharacterSearch
        | VimMotionKind::ReverseCharacterSearch
        | VimMotionKind::SearchNext
        | VimMotionKind::SearchPrevious
        | VimMotionKind::SearchWordForward
        | VimMotionKind::SearchWordBackward
        | VimMotionKind::SearchPartialWordForward
        | VimMotionKind::SearchPartialWordBackward
        | VimMotionKind::PreviousMarkLine
        | VimMotionKind::PreviousMarkExact
        | VimMotionKind::NextMarkLine
        | VimMotionKind::NextMarkExact => {}
    }
    cursor = buffer.clamp(cursor.into());
    viewport.desired_column = if preserves_desired_column(motion.kind()) {
        Some(desired_column)
    } else if motion.kind() == VimMotionKind::LineEnd {
        Some(usize::MAX)
    } else if is_viewport_only_motion(motion.kind()) {
        viewport.desired_column
    } else {
        Some(crate::lsp::display_column(
            buffer.line(cursor.line),
            cursor.column,
        ))
    };
    keep_cursor_visible(&buffer, cursor, viewport, motion.kind());
    cursor.into()
}

fn preserves_desired_column(kind: VimMotionKind) -> bool {
    matches!(
        kind,
        VimMotionKind::Up
            | VimMotionKind::Down
            | VimMotionKind::HalfPageUp
            | VimMotionKind::HalfPageDown
            | VimMotionKind::PageUp
            | VimMotionKind::PageDown
            | VimMotionKind::ScrollLineUp
            | VimMotionKind::ScrollLineDown
    )
}

fn is_viewport_only_motion(kind: VimMotionKind) -> bool {
    matches!(
        kind,
        VimMotionKind::CursorToWindowTop
            | VimMotionKind::CursorToWindowMiddle
            | VimMotionKind::CursorToWindowBottom
            | VimMotionKind::ScrollColumnLeft
            | VimMotionKind::ScrollColumnRight
            | VimMotionKind::ScrollHalfScreenLeft
            | VimMotionKind::ScrollHalfScreenRight
            | VimMotionKind::CursorToWindowLeft
            | VimMotionKind::CursorToWindowRight
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

/// Adjusts a viewport so an externally selected search location is visible.
pub(crate) fn reveal(lines: &[&str], position: SourcePosition, viewport: &mut Viewport) {
    let buffer = TextBuffer::new(lines);
    let cursor = buffer.clamp(position);
    keep_cursor_visible(&buffer, cursor, viewport, VimMotionKind::Left);
}

fn repeated_boundary(
    mut cursor: Position,
    count: usize,
    mut step: impl FnMut(Position) -> Position,
) -> Position {
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
    kind: VimMotionKind,
) {
    if !matches!(
        kind,
        VimMotionKind::CursorToWindowTop
            | VimMotionKind::CursorToWindowTopFirstNonBlank
            | VimMotionKind::CursorToWindowMiddle
            | VimMotionKind::CursorToWindowMiddleFirstNonBlank
            | VimMotionKind::CursorToWindowBottom
            | VimMotionKind::CursorToWindowBottomFirstNonBlank
            | VimMotionKind::NextWindowTop
            | VimMotionKind::PreviousWindowBottom
            | VimMotionKind::ScrollLineDown
            | VimMotionKind::ScrollLineUp
    ) {
        if cursor.line < viewport.top {
            viewport.top = cursor.line;
        } else if cursor.line >= viewport.top.saturating_add(viewport.height) {
            viewport.top = cursor
                .line
                .saturating_sub(viewport.height.saturating_sub(1));
        }
    }
    viewport.top = viewport.top.min(buffer.last_line());

    if matches!(
        kind,
        VimMotionKind::ScrollColumnLeft
            | VimMotionKind::ScrollColumnRight
            | VimMotionKind::ScrollHalfScreenLeft
            | VimMotionKind::ScrollHalfScreenRight
            | VimMotionKind::CursorToWindowLeft
            | VimMotionKind::CursorToWindowRight
    ) {
        return;
    }
    let display = display_with_gutter(buffer.line(cursor.line), cursor.column, viewport.gutter);
    let cursor_width = buffer.line(cursor.line)[cursor.column..]
        .chars()
        .next()
        .map_or(1, |character| {
            if character == '\t' {
                4
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Position {
    line: usize,
    column: usize,
}

impl From<Position> for SourcePosition {
    fn from(value: Position) -> Self {
        Self::new(u32::try_from(value.line).unwrap_or(u32::MAX), value.column)
    }
}

#[derive(Clone, Copy)]
enum WordMotion {
    StartForward,
    EndForward,
    StartBackward,
    EndBackward,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    start: Position,
    end: Position,
    empty_line: bool,
}

#[derive(Clone, Copy, Debug)]
enum MatchItem {
    Delimiter(usize),
    CommentStart(Position),
    CommentEnd(Position),
    Preprocessor(PreprocessorDirective),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreprocessorDirective {
    If,
    Else,
    EndIf,
}

struct TextBuffer<'a> {
    lines: &'a [&'a str],
}

impl<'a> TextBuffer<'a> {
    fn new(lines: &'a [&'a str]) -> Self {
        Self { lines }
    }

    fn len(&self) -> usize {
        self.lines.len().max(1)
    }

    fn last_line(&self) -> usize {
        self.len().saturating_sub(1)
    }

    fn line(&self, index: usize) -> &str {
        self.lines.get(index).copied().unwrap_or("")
    }

    fn clamp(&self, position: SourcePosition) -> Position {
        let line = usize::try_from(position.line())
            .unwrap_or(usize::MAX)
            .min(self.last_line());
        let text = self.line(line);
        let mut column = position.byte_column().min(text.len());
        while !text.is_char_boundary(column) {
            column = column.saturating_sub(1);
        }
        if column == text.len() && !text.is_empty() {
            column = last_column(text);
        }
        Position { line, column }
    }

    fn move_lines(&self, cursor: Position, delta: isize) -> Position {
        let line = cursor
            .line
            .saturating_add_signed(delta)
            .min(self.last_line());
        self.move_to_line(cursor, line)
    }

    fn move_to_line(&self, cursor: Position, line: usize) -> Position {
        let line = line.min(self.last_line());
        let text = self.line(line);
        let mut column = cursor.column.min(last_column(text));
        while !text.is_char_boundary(column) {
            column = column.saturating_sub(1);
        }
        Position { line, column }
    }

    fn move_lines_to_display(
        &self,
        cursor: Position,
        delta: isize,
        display_column: usize,
    ) -> Position {
        let line = cursor
            .line
            .saturating_add_signed(delta)
            .min(self.last_line());
        self.move_to_line_display(cursor, line, display_column)
    }

    fn move_to_line_display(
        &self,
        _cursor: Position,
        line: usize,
        display_column: usize,
    ) -> Position {
        let line = line.min(self.last_line());
        let text = self.line(line);
        let column = if display_column == usize::MAX {
            last_column(text)
        } else {
            byte_at_display(text, display_column)
        };
        Position {
            line,
            column: column.min(last_column(text)),
        }
    }

    fn byte_offset(&self, one_based: usize) -> Position {
        let mut remaining = one_based.saturating_sub(1);
        for line in 0..self.len() {
            let text = self.line(line);
            if remaining < text.len() {
                return Position {
                    line,
                    column: clamp_boundary(text, remaining),
                };
            }
            if line == self.last_line() {
                return Position {
                    line,
                    column: last_column(text),
                };
            }
            remaining = remaining.saturating_sub(text.len().saturating_add(1));
        }
        Position {
            line: self.last_line(),
            column: last_column(self.line(self.last_line())),
        }
    }

    fn word_motion(
        &self,
        cursor: Position,
        count: usize,
        motion: WordMotion,
        big: bool,
    ) -> Position {
        let tokens = self.tokens(big);
        if tokens.is_empty() {
            return cursor;
        }
        let forward = matches!(motion, WordMotion::StartForward | WordMotion::EndForward);
        let mut positions = tokens.iter().filter_map(|token| {
            let position = match motion {
                WordMotion::StartForward | WordMotion::StartBackward => token.start,
                WordMotion::EndForward if token.empty_line => return None,
                WordMotion::EndForward | WordMotion::EndBackward => token.end,
            };
            let ordering = compare(position, cursor);
            ((forward && ordering.is_gt()) || (!forward && ordering.is_lt())).then_some(position)
        });
        if forward {
            positions.nth(count.saturating_sub(1)).unwrap_or(Position {
                line: self.last_line(),
                column: last_column(self.line(self.last_line())),
            })
        } else {
            positions
                .rev()
                .nth(count.saturating_sub(1))
                .unwrap_or(Position { line: 0, column: 0 })
        }
    }

    fn tokens(&self, big: bool) -> Vec<Token> {
        let mut tokens = Vec::new();
        for line_index in 0..self.len() {
            let line = self.line(line_index);
            if line.is_empty() {
                let position = Position {
                    line: line_index,
                    column: 0,
                };
                tokens.push(Token {
                    start: position,
                    end: position,
                    empty_line: true,
                });
                continue;
            }
            let mut active: Option<(usize, CharacterClass)> = None;
            for (column, character) in line.char_indices() {
                let class = CharacterClass::of(character, big);
                match (active, class) {
                    (Some((start, _)), CharacterClass::Space) => {
                        tokens.push(Token {
                            start: Position {
                                line: line_index,
                                column: start,
                            },
                            end: Position {
                                line: line_index,
                                column: previous_column(line, column),
                            },
                            empty_line: false,
                        });
                        active = None;
                    }
                    (Some((start, previous)), current) if previous != current => {
                        tokens.push(Token {
                            start: Position {
                                line: line_index,
                                column: start,
                            },
                            end: Position {
                                line: line_index,
                                column: previous_column(line, column),
                            },
                            empty_line: false,
                        });
                        active = Some((column, current));
                    }
                    (None, current) if current != CharacterClass::Space => {
                        active = Some((column, current));
                    }
                    _ => {}
                }
            }
            if let Some((start, _)) = active {
                tokens.push(Token {
                    start: Position {
                        line: line_index,
                        column: start,
                    },
                    end: Position {
                        line: line_index,
                        column: last_column(line),
                    },
                    empty_line: false,
                });
            }
        }
        tokens
    }

    fn sentence_starts(&self) -> Vec<Position> {
        let mut starts = Vec::new();
        if let Some(first) = self.next_non_blank(Position { line: 0, column: 0 }, true) {
            starts.push(first);
        }
        for line_index in 0..self.len() {
            let line = self.line(line_index);
            let characters = line.char_indices().collect::<Vec<_>>();
            for (index, (column, character)) in characters.iter().copied().enumerate() {
                if !matches!(character, '.' | '!' | '?') {
                    continue;
                }
                let mut next = index + 1;
                while characters
                    .get(next)
                    .is_some_and(|(_, value)| matches!(value, ')' | ']' | '"' | '\''))
                {
                    next += 1;
                }
                let boundary = characters
                    .get(next)
                    .is_none_or(|(_, value)| value.is_whitespace());
                if !boundary {
                    continue;
                }
                let after = Position {
                    line: line_index,
                    column: column.saturating_add(character.len_utf8()),
                };
                if let Some(start) = self.next_non_blank(after, false)
                    && starts.last() != Some(&start)
                {
                    starts.push(start);
                }
            }
            if line.is_empty() {
                let empty = Position {
                    line: line_index,
                    column: 0,
                };
                if starts.last() != Some(&empty) {
                    starts.push(empty);
                }
            }
        }
        starts.sort_by(|left, right| compare(*left, *right));
        starts
    }

    fn sentence_forward(&self, cursor: Position) -> Position {
        self.sentence_starts()
            .into_iter()
            .find(|position| compare(*position, cursor) == Ordering::Greater)
            .unwrap_or(cursor)
    }

    fn sentence_backward(&self, cursor: Position) -> Position {
        self.sentence_starts()
            .into_iter()
            .rev()
            .find(|position| compare(*position, cursor) == Ordering::Less)
            .unwrap_or(cursor)
    }

    fn paragraph_forward(&self, cursor: Position) -> Position {
        let mut line = cursor.line;
        if self.line(line).is_empty() {
            while line < self.len() && self.line(line).is_empty() {
                line = line.saturating_add(1);
            }
        } else {
            line = line.saturating_add(1);
        }
        while line < self.len() && !self.line(line).is_empty() {
            line = line.saturating_add(1);
        }
        if line < self.len() {
            Position { line, column: 0 }
        } else {
            let line = self.last_line();
            Position {
                line,
                column: last_column(self.line(line)),
            }
        }
    }

    fn paragraph_backward(&self, cursor: Position) -> Position {
        let mut line = cursor.line;
        if self.line(line).is_empty() {
            while line > 0 && self.line(line.saturating_sub(1)).is_empty() {
                line = line.saturating_sub(1);
            }
            line = line.saturating_sub(1);
        } else if line > 0 && self.line(line.saturating_sub(1)).is_empty() {
            return Position {
                line: line.saturating_sub(1),
                column: 0,
            };
        }
        while line > 0 && !self.line(line.saturating_sub(1)).is_empty() {
            line = line.saturating_sub(1);
        }
        Position { line, column: 0 }
    }

    fn section(&self, cursor: Position, forward: bool, target: char) -> Position {
        let found = if forward {
            ((cursor.line + 1)..self.len()).find(|line| self.line(*line).starts_with(target))
        } else {
            (0..cursor.line)
                .rev()
                .find(|line| self.line(*line).starts_with(target))
        };
        found.map_or(cursor, |line| Position { line, column: 0 })
    }

    fn matching_pair(&self, cursor: Position, backward: bool) -> Option<Position> {
        let chars = self.characters();
        let in_direction = |position: Position| {
            position.line == cursor.line
                && if backward {
                    position.column <= cursor.column
                } else {
                    position.column >= cursor.column
                }
        };
        let mut candidates = chars
            .iter()
            .enumerate()
            .filter(|(_, (position, character))| {
                in_direction(*position) && is_pair_character(*character)
            })
            .map(|(index, (position, _))| (*position, MatchItem::Delimiter(index)))
            .collect::<Vec<_>>();
        for (needle, start) in [("/*", true), ("*/", false)] {
            candidates.extend(
                self.line(cursor.line)
                    .match_indices(needle)
                    .map(|(column, _)| Position {
                        line: cursor.line,
                        column,
                    })
                    .filter(|position| in_direction(*position))
                    .map(|position| {
                        (
                            position,
                            if start {
                                MatchItem::CommentStart(position)
                            } else {
                                MatchItem::CommentEnd(position)
                            },
                        )
                    }),
            );
        }
        if let Some((column, directive)) = preprocessor_directive(self.line(cursor.line)) {
            let position = Position {
                line: cursor.line,
                column,
            };
            if in_direction(position) {
                candidates.push((position, MatchItem::Preprocessor(directive)));
            }
        }
        let (_, item) = if backward {
            candidates
                .into_iter()
                .max_by(|(left, _), (right, _)| compare(*left, *right))
        } else {
            candidates
                .into_iter()
                .min_by(|(left, _), (right, _)| compare(*left, *right))
        }?;

        match item {
            MatchItem::Delimiter(start) => matching_delimiter(chars, start),
            MatchItem::CommentStart(origin) => self.comment_match(origin, true),
            MatchItem::CommentEnd(origin) => self.comment_match(origin, false),
            MatchItem::Preprocessor(directive) => self.preprocessor_match(cursor.line, directive),
        }
    }

    fn comment_match(&self, origin: Position, forward: bool) -> Option<Position> {
        let needle = if forward { "*/" } else { "/*" };
        let mut candidates = (0..self.len()).flat_map(|line| {
            self.line(line)
                .match_indices(needle)
                .map(move |(column, _)| Position { line, column })
        });
        if forward {
            candidates
                .find(|position| compare(*position, origin) == Ordering::Greater)
                .map(|mut position| {
                    position.column = position.column.saturating_add(1);
                    position
                })
        } else {
            candidates
                .filter(|position| compare(*position, origin) == Ordering::Less)
                .last()
        }
    }

    fn preprocessor_match(
        &self,
        origin_line: usize,
        origin: PreprocessorDirective,
    ) -> Option<Position> {
        if origin == PreprocessorDirective::EndIf {
            let mut depth = 0usize;
            for line in (0..origin_line).rev() {
                let Some((column, directive)) = preprocessor_directive(self.line(line)) else {
                    continue;
                };
                match directive {
                    PreprocessorDirective::EndIf => depth = depth.saturating_add(1),
                    PreprocessorDirective::If if depth == 0 => {
                        return Some(Position { line, column });
                    }
                    PreprocessorDirective::If => depth = depth.saturating_sub(1),
                    PreprocessorDirective::Else => {}
                }
            }
            return None;
        }

        let mut depth = 0usize;
        for line in origin_line.saturating_add(1)..self.len() {
            let Some((column, directive)) = preprocessor_directive(self.line(line)) else {
                continue;
            };
            match directive {
                PreprocessorDirective::If => depth = depth.saturating_add(1),
                PreprocessorDirective::EndIf if depth == 0 => {
                    return Some(Position { line, column });
                }
                PreprocessorDirective::EndIf => depth = depth.saturating_sub(1),
                PreprocessorDirective::Else if depth == 0 => {
                    return Some(Position { line, column });
                }
                PreprocessorDirective::Else => {}
            }
        }
        None
    }

    fn unmatched_open(&self, cursor: Position, open: char) -> Position {
        let close = match open {
            '(' => ')',
            '{' => '}',
            '[' => ']',
            _ => return cursor,
        };
        let mut depth = 0usize;
        for (position, character) in self
            .characters()
            .into_iter()
            .filter(|(position, _)| compare(*position, cursor) == Ordering::Less)
            .rev()
        {
            if character == close {
                depth = depth.saturating_add(1);
            } else if character == open {
                if depth == 0 {
                    return position;
                }
                depth = depth.saturating_sub(1);
            }
        }
        cursor
    }

    fn unmatched_close(&self, cursor: Position, close: char) -> Position {
        let open = match close {
            ')' => '(',
            '}' => '{',
            ']' => '[',
            _ => return cursor,
        };
        let mut depth = 0usize;
        for (position, character) in self
            .characters()
            .into_iter()
            .filter(|(position, _)| compare(*position, cursor) == Ordering::Greater)
        {
            if character == open {
                depth = depth.saturating_add(1);
            } else if character == close {
                if depth == 0 {
                    return position;
                }
                depth = depth.saturating_sub(1);
            }
        }
        cursor
    }

    fn brace(&self, cursor: Position, forward: bool, target: char) -> Position {
        let chars = self.characters();
        if forward {
            chars
                .into_iter()
                .find(|(position, character)| {
                    compare(*position, cursor) == Ordering::Greater && *character == target
                })
                .map_or(cursor, |(position, _)| position)
        } else {
            chars
                .into_iter()
                .rev()
                .find(|(position, character)| {
                    compare(*position, cursor) == Ordering::Less && *character == target
                })
                .map_or(cursor, |(position, _)| position)
        }
    }

    fn preprocessor(&self, cursor: Position, forward: bool) -> Position {
        let mut depth = 0usize;
        if forward {
            for line in cursor.line.saturating_add(1)..self.len() {
                let Some((column, directive)) = preprocessor_directive(self.line(line)) else {
                    continue;
                };
                match directive {
                    PreprocessorDirective::If => depth = depth.saturating_add(1),
                    PreprocessorDirective::EndIf if depth == 0 => {
                        return Position { line, column };
                    }
                    PreprocessorDirective::EndIf => depth = depth.saturating_sub(1),
                    PreprocessorDirective::Else if depth == 0 => {
                        return Position { line, column };
                    }
                    PreprocessorDirective::Else => {}
                }
            }
        } else {
            for line in (0..cursor.line).rev() {
                let Some((column, directive)) = preprocessor_directive(self.line(line)) else {
                    continue;
                };
                match directive {
                    PreprocessorDirective::EndIf => depth = depth.saturating_add(1),
                    PreprocessorDirective::If if depth == 0 => {
                        return Position { line, column };
                    }
                    PreprocessorDirective::If => depth = depth.saturating_sub(1),
                    PreprocessorDirective::Else if depth == 0 => {
                        return Position { line, column };
                    }
                    PreprocessorDirective::Else => {}
                }
            }
        }
        cursor
    }

    fn comment(&self, cursor: Position, forward: bool) -> Position {
        let needle = if forward { "*/" } else { "/*" };
        let mut candidates = Vec::new();
        for line in 0..self.len() {
            let text = self.line(line);
            for (column, _) in text.match_indices(needle) {
                candidates.push(Position { line, column });
            }
        }
        if forward {
            candidates
                .into_iter()
                .find(|position| compare(*position, cursor) == Ordering::Greater)
                .unwrap_or(cursor)
        } else {
            candidates
                .into_iter()
                .rev()
                .find(|position| compare(*position, cursor) == Ordering::Less)
                .unwrap_or(cursor)
        }
    }

    fn diff_change(&self, cursor: Position, forward: bool) -> Position {
        let mut starts = (0..self.len()).filter(|line| {
            is_diff_change_line(self.line(*line))
                && (*line == 0 || !is_diff_change_line(self.line(line.saturating_sub(1))))
        });
        let found = if forward {
            starts.find(|line| *line > cursor.line)
        } else {
            starts.rfind(|line| *line < cursor.line)
        };
        found.map_or(cursor, |line| Position { line, column: 0 })
    }

    fn characters(&self) -> Vec<(Position, char)> {
        self.lines
            .iter()
            .enumerate()
            .flat_map(|(line, text)| {
                text.char_indices()
                    .map(move |(column, character)| (Position { line, column }, character))
            })
            .collect()
    }

    fn next_non_blank(&self, after: Position, include_current: bool) -> Option<Position> {
        for line_index in after.line..self.len() {
            let line = self.line(line_index);
            let start = if line_index == after.line {
                after.column.min(line.len())
            } else {
                0
            };
            for (column, character) in line.char_indices() {
                let after_start = if include_current {
                    column >= start
                } else {
                    column > start || (line_index > after.line && column >= start)
                };
                if after_start && !character.is_whitespace() {
                    return Some(Position {
                        line: line_index,
                        column,
                    });
                }
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharacterClass {
    Space,
    Keyword,
    Other,
}

impl CharacterClass {
    fn of(character: char, big: bool) -> Self {
        if character.is_whitespace() {
            Self::Space
        } else if big || character.is_alphanumeric() || character == '_' {
            Self::Keyword
        } else {
            Self::Other
        }
    }
}

fn compare(left: Position, right: Position) -> Ordering {
    (left.line, left.column).cmp(&(right.line, right.column))
}

fn previous_column(line: &str, column: usize) -> usize {
    let end = clamp_boundary(line, column);
    line[..end]
        .char_indices()
        .next_back()
        .map_or(0, |(byte, _)| byte)
}

fn next_column(line: &str, column: usize) -> usize {
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

fn last_column(line: &str) -> usize {
    line.char_indices().next_back().map_or(0, |(byte, _)| byte)
}

fn first_non_blank(line: &str) -> usize {
    line.char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(0, |(column, _)| column)
}

fn first_non_blank_from(line: &str, start: usize) -> usize {
    line.char_indices()
        .find(|(column, character)| *column >= start && !character.is_whitespace())
        .map_or_else(|| last_column(line), |(column, _)| column)
}

fn last_non_blank(line: &str) -> usize {
    line.char_indices()
        .rev()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(0, |(column, _)| column)
}

fn clamp_boundary(line: &str, requested: usize) -> usize {
    let mut column = requested.min(line.len());
    while !line.is_char_boundary(column) {
        column = column.saturating_sub(1);
    }
    column
}

fn byte_at_display(line: &str, display: usize) -> usize {
    let mut best = 0usize;
    let mut cells = 0usize;
    for (column, character) in line.char_indices() {
        if cells > display {
            break;
        }
        best = column;
        cells = cells.saturating_add(if character == '\t' {
            4
        } else {
            UnicodeWidthChar::width(character).unwrap_or(0)
        });
    }
    best
}

fn screen_end_column(line: &str, viewport: Viewport) -> usize {
    let (_, source_end) = visible_source_columns(viewport);
    byte_at_display(line, source_end.saturating_sub(1))
}

fn screen_last_non_blank(line: &str, viewport: Viewport) -> usize {
    let (start, end) = visible_source_columns(viewport);
    line.char_indices()
        .rev()
        .find(|(column, character)| {
            let display = crate::lsp::display_column(line, *column);
            display >= start && display < end && !character.is_whitespace()
        })
        .map_or_else(|| byte_at_display(line, start), |(column, _)| column)
}

fn visible_source_columns(viewport: Viewport) -> (usize, usize) {
    let start = viewport.left.saturating_sub(viewport.gutter);
    let end = viewport
        .left
        .saturating_add(viewport.width)
        .saturating_sub(viewport.gutter)
        .max(start.saturating_add(1));
    (start, end)
}

fn display_with_gutter(line: &str, column: usize, gutter: usize) -> usize {
    crate::lsp::display_column(line, column).saturating_add(gutter)
}

fn find_character(
    line: &str,
    column: usize,
    target: char,
    kind: VimMotionKind,
    count: usize,
    repeated: bool,
) -> usize {
    let forward = matches!(
        kind,
        VimMotionKind::FindForward | VimMotionKind::TillForward
    );
    let till = matches!(
        kind,
        VimMotionKind::TillForward | VimMotionKind::TillBackward
    );
    let adjacent = if forward {
        next_column(line, column)
    } else {
        previous_column(line, column)
    };
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

fn is_pair_character(character: char) -> bool {
    matches!(character, '(' | ')' | '[' | ']' | '{' | '}')
}

fn is_diff_change_line(line: &str) -> bool {
    (line.starts_with('+') && !line.starts_with("+++"))
        || (line.starts_with('-') && !line.starts_with("---"))
}

fn matching_delimiter(chars: Vec<(Position, char)>, start: usize) -> Option<Position> {
    let (_, character) = chars[start];
    let (target, direction) = pair_for(character)?;
    let mut depth = 0usize;
    if direction > 0 {
        for (position, current) in chars.into_iter().skip(start + 1) {
            if current == character {
                depth = depth.saturating_add(1);
            } else if current == target {
                if depth == 0 {
                    return Some(position);
                }
                depth = depth.saturating_sub(1);
            }
        }
    } else {
        for (position, current) in chars.into_iter().take(start).rev() {
            if current == character {
                depth = depth.saturating_add(1);
            } else if current == target {
                if depth == 0 {
                    return Some(position);
                }
                depth = depth.saturating_sub(1);
            }
        }
    }
    None
}

fn preprocessor_directive(line: &str) -> Option<(usize, PreprocessorDirective)> {
    let column = first_non_blank(line);
    let directive = line.get(column..)?.strip_prefix('#')?.trim_start();
    let kind = if directive.starts_with("if") {
        PreprocessorDirective::If
    } else if directive.starts_with("else") || directive.starts_with("elif") {
        PreprocessorDirective::Else
    } else if directive.starts_with("endif") {
        PreprocessorDirective::EndIf
    } else {
        return None;
    };
    Some((column, kind))
}

fn pair_for(character: char) -> Option<(char, isize)> {
    match character {
        '(' => Some((')', 1)),
        '[' => Some((']', 1)),
        '{' => Some(('}', 1)),
        ')' => Some(('(', -1)),
        ']' => Some(('[', -1)),
        '}' => Some(('{', -1)),
        _ => None,
    }
}

fn count_as_isize(count: usize) -> isize {
    isize::try_from(count).unwrap_or(isize::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Viewport, apply};
    use crate::app::{VimMotion, VimMotionKind};
    use crate::domain::SourcePosition;

    fn motion(kind: VimMotionKind, count: usize) -> VimMotion {
        VimMotion::new(kind).counted(count, count != 1)
    }

    #[test]
    fn word_and_big_word_motions_follow_vim_boundaries() {
        let lines = ["one.two  three", "", "four-five"];
        let mut viewport = Viewport::new(0, 0, 10, 80, 0);
        let mut cursor = SourcePosition::new(0, 0);
        cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::WordForward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(0, 3));
        cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::WordForward, 2),
        );
        assert_eq!(cursor, SourcePosition::new(0, 9));
        cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::BigWordForward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(1, 0));
        cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::WordEndForward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(2, 3));
        cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::WordEndBackward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(1, 0));
    }

    #[test]
    fn oversized_counts_stop_at_text_boundaries() {
        let lines = ["one 界", "two"];
        for kind in [
            VimMotionKind::Left,
            VimMotionKind::Right,
            VimMotionKind::LeftWrap,
            VimMotionKind::RightWrap,
            VimMotionKind::WordForward,
            VimMotionKind::WordBackward,
            VimMotionKind::WordEndForward,
            VimMotionKind::WordEndBackward,
        ] {
            let mut viewport = Viewport::new(0, 0, 10, 80, 0);
            let cursor = apply(
                &lines,
                SourcePosition::new(0, 0),
                &mut viewport,
                motion(kind, usize::MAX),
            );
            let expected = match kind {
                VimMotionKind::Right => SourcePosition::new(0, 4),
                VimMotionKind::RightWrap
                | VimMotionKind::WordForward
                | VimMotionKind::WordEndForward => SourcePosition::new(1, 2),
                _ => SourcePosition::new(0, 0),
            };
            assert_eq!(cursor, expected, "{kind:?}");
        }
    }

    #[test]
    fn till_repeats_skip_only_an_adjacent_target_that_would_not_move() {
        use crate::app::Action;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let lines = ["axbx cxdx"];
        let mut mapper = crate::tui::keymap::KeyMapper::new();
        let mut viewport = Viewport::new(0, 0, 10, 80, 0);
        let mut cursor = SourcePosition::new(0, 0);
        for (keys, expected) in [("tx", 0), (";", 2), (";", 5), (",", 4), ("0;", 2)] {
            for key in keys.chars() {
                if let Some(Action::VimMotion(motion)) =
                    mapper.map(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE), false)
                {
                    cursor = apply(&lines, cursor, &mut viewport, motion);
                }
            }
            assert_eq!(cursor.byte_column(), expected, "{keys}");
        }
    }

    #[test]
    fn character_search_and_matching_pairs_keep_utf8_boundaries() {
        let lines = ["a界(b(c)d)e"];
        let mut viewport = Viewport::new(0, 0, 4, 20, 0);
        let find = VimMotion::new(VimMotionKind::FindForward)
            .counted(2, true)
            .targeting('c');
        let cursor = apply(&lines, SourcePosition::new(0, 0), &mut viewport, find);
        assert_eq!(cursor, SourcePosition::new(0, 0));
        let cursor = apply(
            &lines,
            SourcePosition::new(0, "a界".len()),
            &mut viewport,
            motion(VimMotionKind::MatchingPair, 1),
        );
        assert_eq!(cursor, SourcePosition::new(0, "a界(b(c)d".len()));

        let cursor = apply(
            &lines,
            SourcePosition::new(0, "a界(b(c)".len()),
            &mut viewport,
            motion(VimMotionKind::MatchingPairBackward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(0, "a界(b".len()));
    }

    #[test]
    fn space_and_backspace_wrap_lines_without_changing_h_and_l() {
        let lines = ["ab", "", "界x"];
        let mut viewport = Viewport::new(0, 0, 4, 20, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 1),
            &mut viewport,
            motion(VimMotionKind::RightWrap, 2),
        );
        assert_eq!(cursor, SourcePosition::new(2, 0));
        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::LeftWrap, 2),
        );
        assert_eq!(cursor, SourcePosition::new(0, 1));

        assert_eq!(
            apply(
                &lines,
                SourcePosition::new(0, 1),
                &mut viewport,
                motion(VimMotionKind::Right, 1),
            ),
            SourcePosition::new(0, 1)
        );
        assert_eq!(
            apply(
                &lines,
                SourcePosition::new(2, 0),
                &mut viewport,
                motion(VimMotionKind::Left, 1),
            ),
            SourcePosition::new(2, 0)
        );
    }

    #[test]
    fn percent_matches_comments_and_nested_preprocessor_conditionals() {
        let comments = ["a /* one", "two */ b"];
        let mut viewport = Viewport::new(0, 0, 8, 40, 0);
        let cursor = apply(
            &comments,
            SourcePosition::new(0, 2),
            &mut viewport,
            motion(VimMotionKind::MatchingPair, 1),
        );
        assert_eq!(cursor, SourcePosition::new(1, 5));
        let cursor = apply(
            &comments,
            SourcePosition::new(1, 4),
            &mut viewport,
            motion(VimMotionKind::MatchingPair, 1),
        );
        assert_eq!(cursor, SourcePosition::new(0, 2));

        let directives = ["#if A", "#if B", "#else", "#endif", "#else", "#endif"];
        let cursor = apply(
            &directives,
            SourcePosition::new(0, 0),
            &mut viewport,
            motion(VimMotionKind::MatchingPair, 1),
        );
        assert_eq!(cursor, SourcePosition::new(4, 0));
        let cursor = apply(
            &directives,
            SourcePosition::new(5, 0),
            &mut viewport,
            motion(VimMotionKind::MatchingPair, 1),
        );
        assert_eq!(cursor, SourcePosition::new(0, 0));

        let cursor = apply(
            &directives,
            SourcePosition::new(3, 0),
            &mut viewport,
            motion(VimMotionKind::PreprocessorBackward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(2, 0));
        let cursor = apply(
            &directives,
            SourcePosition::new(1, 0),
            &mut viewport,
            motion(VimMotionKind::PreprocessorForward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(2, 0));
    }

    #[test]
    fn an_explicit_half_page_count_is_a_line_count() {
        let owned = (0..30).map(|line| line.to_string()).collect::<Vec<_>>();
        let lines = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let mut viewport = Viewport::new(0, 0, 10, 80, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 0),
            &mut viewport,
            VimMotion::new(VimMotionKind::HalfPageDown).counted(2, true),
        );
        assert_eq!(cursor, SourcePosition::new(2, 0));

        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            VimMotion::new(VimMotionKind::HalfPageDown),
        );
        assert_eq!(cursor, SourcePosition::new(7, 0));
    }

    #[test]
    fn paragraph_motions_cross_runs_of_empty_lines_like_vim() {
        let lines = ["one", "two", "", "", "three", "four", "", "five"];
        let mut viewport = Viewport::new(0, 0, 8, 40, 0);
        let forward = |line, viewport: &mut Viewport| {
            apply(
                &lines,
                SourcePosition::new(line, 0),
                viewport,
                motion(VimMotionKind::ParagraphForward, 1),
            )
        };
        assert_eq!(forward(0, &mut viewport), SourcePosition::new(2, 0));
        assert_eq!(forward(2, &mut viewport), SourcePosition::new(6, 0));
        let backward = |line, viewport: &mut Viewport| {
            apply(
                &lines,
                SourcePosition::new(line, 0),
                viewport,
                motion(VimMotionKind::ParagraphBackward, 1),
            )
        };
        assert_eq!(backward(4, &mut viewport), SourcePosition::new(3, 0));
        assert_eq!(backward(3, &mut viewport), SourcePosition::new(0, 0));

        let no_trailing_blank = ["one", "two"];
        let cursor = apply(
            &no_trailing_blank,
            SourcePosition::new(0, 0),
            &mut viewport,
            motion(VimMotionKind::ParagraphForward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(1, 2));
    }

    #[test]
    fn page_and_window_motions_use_the_actual_viewport() {
        let owned = (0..40)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>();
        let lines = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let mut viewport = Viewport::new(10, 0, 8, 40, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(12, 0),
            &mut viewport,
            motion(VimMotionKind::WindowBottom, 1),
        );
        assert_eq!(cursor.line(), 17);
        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::PageDown, 1),
        );
        assert_eq!(cursor.line(), 23);
        assert_eq!(viewport.top, 16);
    }

    #[test]
    fn z_commands_preserve_or_reset_columns_and_horizontal_scroll_follows_cursor() {
        let lines = ["  zero", "    one", "  two", "three", "four", "five"];
        let mut viewport = Viewport::new(0, 0, 5, 4, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 4),
            &mut viewport,
            VimMotion::new(VimMotionKind::CursorToWindowTop).counted(2, true),
        );
        assert_eq!(cursor, SourcePosition::new(1, 4));
        assert_eq!(viewport.top, 1);

        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            VimMotion::new(VimMotionKind::CursorToWindowMiddleFirstNonBlank).counted(3, true),
        );
        assert_eq!(cursor, SourcePosition::new(2, 2));
        assert_eq!(viewport.top, 0);

        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            VimMotion::new(VimMotionKind::PreviousWindowBottom).counted(6, true),
        );
        assert_eq!(cursor, SourcePosition::new(1, 4));
        assert_eq!(viewport.top, 0);

        let horizontal = ["abcdefghij"];
        let mut viewport = Viewport::new(0, 0, 1, 4, 0);
        let cursor = apply(
            &horizontal,
            SourcePosition::new(0, 0),
            &mut viewport,
            VimMotion::new(VimMotionKind::ScrollColumnRight).counted(3, true),
        );
        assert_eq!(viewport.left, 3);
        assert_eq!(cursor, SourcePosition::new(0, 3));
    }

    #[test]
    fn vertical_motions_preserve_the_wanted_display_column_across_short_lines() {
        let lines = ["abcdef", "x", "abcdef"];
        let mut viewport = Viewport::new(0, 0, 3, 20, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 4),
            &mut viewport,
            motion(VimMotionKind::Down, 1),
        );
        assert_eq!(cursor, SourcePosition::new(1, 0));
        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::Down, 1),
        );
        assert_eq!(cursor, SourcePosition::new(2, 4));

        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            motion(VimMotionKind::LineEnd, 1),
        );
        let cursor = apply(&lines, cursor, &mut viewport, motion(VimMotionKind::Up, 1));
        assert_eq!(cursor, SourcePosition::new(1, 0));
        let cursor = apply(&lines, cursor, &mut viewport, motion(VimMotionKind::Up, 1));
        assert_eq!(cursor, SourcePosition::new(0, 5));
    }

    #[test]
    fn counted_screen_end_motions_move_down_before_selecting_the_column() {
        let lines = ["first", "second   ", "third"];
        let mut viewport = Viewport::new(0, 0, 3, 20, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 0),
            &mut viewport,
            VimMotion::new(VimMotionKind::ScreenLineEnd).counted(2, true),
        );
        assert_eq!(cursor, SourcePosition::new(1, 8));

        let cursor = apply(
            &lines,
            SourcePosition::new(0, 0),
            &mut viewport,
            VimMotion::new(VimMotionKind::ScreenLastNonBlank).counted(2, true),
        );
        assert_eq!(cursor, SourcePosition::new(1, 5));
    }

    #[test]
    fn screen_column_motions_account_for_a_partly_scrolled_gutter() {
        let lines = ["  abcdefghijklmnop  "];
        let mut viewport = Viewport::new(0, 10, 1, 10, 8);
        let start = apply(
            &lines,
            SourcePosition::new(0, 8),
            &mut viewport,
            motion(VimMotionKind::ScreenLineStart, 1),
        );
        assert_eq!(start, SourcePosition::new(0, 2));
        let middle = apply(
            &lines,
            start,
            &mut viewport,
            motion(VimMotionKind::ScreenMiddle, 1),
        );
        assert_eq!(middle, SourcePosition::new(0, 7));
        let end = apply(
            &lines,
            middle,
            &mut viewport,
            motion(VimMotionKind::ScreenLineEnd, 1),
        );
        assert_eq!(end, SourcePosition::new(0, 11));
        let non_blank = apply(
            &lines,
            middle,
            &mut viewport,
            motion(VimMotionKind::ScreenLastNonBlank, 1),
        );
        assert_eq!(non_blank, SourcePosition::new(0, 11));
    }

    #[test]
    fn byte_offsets_and_diff_change_motions_are_count_aware() {
        let lines = ["abc", "+first", "+more", " context", "-second"];
        let mut viewport = Viewport::new(0, 0, 4, 40, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 0),
            &mut viewport,
            motion(VimMotionKind::ByteOffset, 6),
        );
        assert_eq!(cursor, SourcePosition::new(1, 1));
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 0),
            &mut viewport,
            motion(VimMotionKind::DiffChangeForward, 2),
        );
        assert_eq!(cursor, SourcePosition::new(4, 0));

        let cursor = apply(
            &lines,
            SourcePosition::new(2, 2),
            &mut viewport,
            motion(VimMotionKind::DiffChangeBackward, 1),
        );
        assert_eq!(cursor, SourcePosition::new(1, 0));
    }
}
