//! Borrowed multi-line text model and semantic Vim scans.
//!
//! The model normalizes only observation: it treats an empty input slice as a
//! virtual empty line and clamps positions without allocating or changing the
//! caller's text. Word, sentence, paragraph, structural, comment,
//! preprocessor, and diff movement live here because they scan across logical
//! lines rather than terminal cells.

use std::cmp::Ordering;

use super::Cursor;
use super::line::{
    byte_at_display, clamp_boundary, first_non_blank, is_diff_change_line, is_pair_character,
    last_column, next_column, previous_column,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Position {
    pub(super) line: usize,
    pub(super) column: usize,
}

impl From<Position> for Cursor {
    fn from(value: Position) -> Self {
        Self::new(value.line, value.column)
    }
}

#[derive(Clone, Copy)]
pub(super) enum WordMotion {
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

pub(super) struct TextBuffer<'a> {
    lines: &'a [&'a str],
}

impl<'a> TextBuffer<'a> {
    pub(super) fn new(lines: &'a [&'a str]) -> Self {
        Self { lines }
    }

    pub(super) fn len(&self) -> usize {
        // A cursor always needs one addressable logical line, even when an
        // adapter supplies no lines for an empty document.
        self.lines.len().max(1)
    }

    pub(super) fn last_line(&self) -> usize {
        self.len().saturating_sub(1)
    }

    pub(super) fn line(&self, index: usize) -> &str {
        self.lines.get(index).copied().unwrap_or("")
    }

    pub(super) fn clamp(&self, position: Cursor) -> Position {
        let line = position.line().min(self.last_line());
        let text = self.line(line);
        let mut column = position.byte_column().min(text.len());
        while !text.is_char_boundary(column) {
            column = column.saturating_sub(1);
        }
        // Normal-mode motion rests on a scalar, never on the insertion point
        // after it. EditableBuffer performs its separate Insert-mode clamping.
        if column == text.len() && !text.is_empty() {
            column = last_column(text);
        }
        Position { line, column }
    }

    pub(super) fn move_lines(&self, cursor: Position, delta: isize) -> Position {
        let line = cursor
            .line
            .saturating_add_signed(delta)
            .min(self.last_line());
        self.move_to_line(cursor, line)
    }

    pub(super) fn move_to_line(&self, cursor: Position, line: usize) -> Position {
        let line = line.min(self.last_line());
        let text = self.line(line);
        let mut column = cursor.column.min(last_column(text));
        while !text.is_char_boundary(column) {
            column = column.saturating_sub(1);
        }
        Position { line, column }
    }

    pub(super) fn move_lines_to_display(
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

    pub(super) fn move_to_line_display(
        &self,
        _cursor: Position,
        line: usize,
        display_column: usize,
    ) -> Position {
        let line = line.min(self.last_line());
        let text = self.line(line);
        // The motion engine uses MAX only for Vim's `$` desired-column rule:
        // subsequent vertical motions follow each destination line's end.
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

    pub(super) fn byte_offset(&self, one_based: usize) -> Position {
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

    pub(super) fn word_motion(
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
        if let Some(first) = self.next_non_blank(Position { line: 0, column: 0 }) {
            starts.push(first);
        }
        for line_index in 0..self.len() {
            let line = self.line(line_index);
            let characters = line.char_indices().collect::<Vec<_>>();
            for (index, (_, character)) in characters.iter().copied().enumerate() {
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
                    column: characters
                        .get(next)
                        .map_or(line.len(), |(column, _)| *column),
                };
                if let Some(start) = self.next_non_blank(after)
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

    pub(super) fn sentence_forward(&self, cursor: Position) -> Position {
        self.sentence_starts()
            .into_iter()
            .find(|position| compare(*position, cursor) == Ordering::Greater)
            .unwrap_or(cursor)
    }

    pub(super) fn sentence_backward(&self, cursor: Position) -> Position {
        self.sentence_starts()
            .into_iter()
            .rev()
            .find(|position| compare(*position, cursor) == Ordering::Less)
            .unwrap_or(cursor)
    }

    pub(super) fn paragraph_forward(&self, cursor: Position) -> Position {
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

    pub(super) fn paragraph_backward(&self, cursor: Position) -> Position {
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
        if line > 0 {
            line = line.saturating_sub(1);
        }
        Position { line, column: 0 }
    }

    pub(super) fn section(&self, cursor: Position, forward: bool, target: char) -> Position {
        let found = if forward {
            ((cursor.line + 1)..self.len()).find(|line| self.line(*line).starts_with(target))
        } else {
            (0..cursor.line)
                .rev()
                .find(|line| self.line(*line).starts_with(target))
        };
        found.map_or(cursor, |line| Position { line, column: 0 })
    }

    pub(super) fn matching_pair(&self, cursor: Position, backward: bool) -> Option<Position> {
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

    pub(super) fn unmatched_open(&self, cursor: Position, open: char) -> Position {
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

    pub(super) fn unmatched_close(&self, cursor: Position, close: char) -> Position {
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

    pub(super) fn brace(&self, cursor: Position, forward: bool, target: char) -> Position {
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

    pub(super) fn preprocessor(&self, cursor: Position, forward: bool) -> Position {
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

    pub(super) fn comment(&self, cursor: Position, forward: bool) -> Position {
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
                .map(|mut position| {
                    position.column = next_column(self.line(position.line), position.column);
                    position
                })
                .unwrap_or(cursor)
        } else {
            candidates
                .into_iter()
                .rev()
                .find(|position| compare(*position, cursor) == Ordering::Less)
                .unwrap_or(cursor)
        }
    }

    pub(super) fn diff_change(&self, cursor: Position, forward: bool) -> Position {
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

    fn next_non_blank(&self, after: Position) -> Option<Position> {
        for line_index in after.line..self.len() {
            let line = self.line(line_index);
            let start = if line_index == after.line {
                after.column.min(line.len())
            } else {
                0
            };
            for (column, character) in line.char_indices() {
                if column >= start && !character.is_whitespace() {
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
