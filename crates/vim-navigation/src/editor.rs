//! Explicitly mutable Normal/Insert buffer state.
//!
//! [`EditableBuffer`] is an opt-in owner of text, mode, cursor, viewport, and
//! byte-limit invariants. It is intentionally separate from read-only
//! [`crate::motion::apply`]: an application can reuse every motion without
//! granting editing capability to its documents.
//!
//! In [`Mode::Normal`], the cursor identifies a Unicode scalar value or column
//! zero on an empty line. In [`Mode::Insert`], it is an insertion point and may
//! equal the line's byte length. Columns are always UTF-8 byte boundaries;
//! viewport columns are display cells. Every successful move or edit reveals
//! the cursor. Wrong-mode commands, line separators passed as
//! [`EditorInput::Character`], boundary corrections, and input exceeding the
//! configured byte limit return [`EditOutcome::Ignored`] rather than panicking.
//!
//! The caller remains responsible for mapping keys, displaying the active
//! mode, choosing an input limit, and deciding whether a resource is editable.
//! Search prompts with their own confirm/cancel semantics need not use this
//! buffer. The default Insert escape sequence is `jj`; [`EditableBuffer`]
//! inserts a possible prefix immediately, so a lone prefix is never hidden
//! behind a timeout.
//!
//! # Example
//!
//! ```
//! use vim_navigation::editor::{EditOutcome, EditableBuffer, EditorInput};
//! use vim_navigation::motion::Viewport;
//!
//! let mut buffer =
//!     EditableBuffer::with_byte_limit("a", Viewport::new(0, 0, 1, 10, 0), 2);
//! assert_eq!(buffer.handle(EditorInput::Append), EditOutcome::ModeChanged);
//! assert_eq!(buffer.handle(EditorInput::Character('界')), EditOutcome::Ignored);
//! assert_eq!(buffer.handle(EditorInput::Character('b')), EditOutcome::Changed);
//! assert_eq!(buffer.handle(EditorInput::Escape), EditOutcome::ModeChanged);
//! assert_eq!(buffer.text(), "ab");
//! ```

use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{Cursor, Motion, Viewport, apply};

/// The active modal input state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    /// Motions and insertion-start commands are accepted; text is not changed.
    #[default]
    Normal,
    /// Character input, newline, and basic corrections may mutate the buffer.
    Insert,
}

/// Configures the character sequence that leaves Insert mode.
///
/// [`Default`] is `jj`. A prefix is inserted into the buffer immediately and
/// remembered only as a possible escape, so a lone `j`, a mismatch, context
/// switch, or caller that never supplies another input cannot lose text. When
/// the complete sequence arrives, its already-inserted prefix is removed and
/// the buffer follows the same cursor-normalization contract as
/// [`EditorInput::Escape`].
///
/// Use [`Self::disabled`] to permit literal `jj`, or [`Self::new`] to replace
/// the default with another sequence such as `jk`.
///
/// ```
/// use vim_navigation::InsertEscapeSequence;
///
/// assert_eq!(InsertEscapeSequence::default().as_str(), Some("jj"));
/// assert_eq!(InsertEscapeSequence::new("jk")?.as_str(), Some("jk"));
/// assert_eq!(InsertEscapeSequence::disabled().as_str(), None);
/// # Ok::<(), vim_navigation::InsertEscapeSequenceError>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsertEscapeSequence {
    sequence: String,
}

impl InsertEscapeSequence {
    /// Creates a non-empty escape sequence containing no line separators.
    ///
    /// The sequence is matched as Unicode scalar values in insertion order.
    /// Use [`Self::disabled`] instead of an empty string.
    ///
    /// # Errors
    ///
    /// Returns [`InsertEscapeSequenceError`] for an empty sequence or one
    /// containing `\n` or `\r`.
    pub fn new(sequence: &str) -> Result<Self, InsertEscapeSequenceError> {
        if sequence.is_empty() || sequence.contains(['\n', '\r']) {
            return Err(InsertEscapeSequenceError { _private: () });
        }
        Ok(Self {
            sequence: sequence.to_owned(),
        })
    }

    /// Disables Insert-mode character escape recognition.
    ///
    /// With this setting, `jj` and every other character sequence are inserted
    /// literally; [`EditorInput::Escape`] remains available.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            sequence: String::new(),
        }
    }

    /// Returns the configured sequence, or `None` when recognition is disabled.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        (!self.sequence.is_empty()).then_some(self.sequence.as_str())
    }
}

impl Default for InsertEscapeSequence {
    fn default() -> Self {
        Self {
            sequence: "jj".to_owned(),
        }
    }
}

/// An invalid Insert-mode escape sequence.
///
/// Sequences must be non-empty and contain no line separators. The type keeps
/// its representation private so validation can evolve without exposing
/// additional exhaustive error variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InsertEscapeSequenceError {
    _private: (),
}

impl Display for InsertEscapeSequenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .write_str("insert escape sequence must be non-empty and contain no line separator")
    }
}

impl Error for InsertEscapeSequenceError {}

/// Framework-independent input accepted by [`EditableBuffer`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorInput {
    /// Apply a completed Vim motion in Normal mode; ignored in Insert mode.
    Motion(Motion),
    /// Start insertion at the cursor (`i`).
    Insert,
    /// Start insertion after the cursor (`a`).
    Append,
    /// Start insertion at the first non-blank byte (`I`).
    InsertLineStart,
    /// Start insertion after the line (`A`).
    AppendLineEnd,
    /// Open a new line below and enter Insert mode (`o`).
    OpenBelow,
    /// Open a new line above and enter Insert mode (`O`).
    OpenAbove,
    /// Insert one non-line-separator Unicode scalar value while in Insert mode.
    ///
    /// Use [`EditorInput::Newline`] for `\n` or `\r` input. The byte limit is
    /// charged by [`char::len_utf8`], not by character or display width.
    Character(char),
    /// Split the current line while in Insert mode, charging one separator byte.
    Newline,
    /// Delete the preceding scalar value, joining lines at column zero.
    ///
    /// At the start of the first line this is ignored.
    Backspace,
    /// Delete the next scalar value, joining with the next line at line end.
    ///
    /// At the end of the final line this is ignored.
    Delete,
    /// Leave Insert mode while preserving content, or do nothing in Normal mode.
    ///
    /// On a non-empty line, leaving Insert moves the insertion-point cursor one
    /// Unicode scalar left to satisfy the Normal-mode cursor invariant.
    Escape,
}

/// The observable effect of one editor input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditOutcome {
    /// The input has no meaning in the current mode or exceeds the byte limit.
    Ignored,
    /// Only the cursor or viewport changed.
    Moved,
    /// Buffer content changed; the mode or cursor may also have changed.
    Changed,
    /// The mode changed; a recognized Insert escape prefix may also be removed.
    ModeChanged,
}

/// A text buffer that opts into mutation separately from read-only motion.
///
/// The buffer always contains at least one line. Cursor columns remain UTF-8
/// boundaries. In Normal mode the cursor is on a character (or column zero for
/// an empty line); in Insert mode it may also be at the byte length of the line.
/// The safe public operations clamp external coordinates and report
/// unsupported or boundary input as [`EditOutcome::Ignored`]; they do not panic
/// for caller-controlled positions or input.
#[derive(Debug)]
pub struct EditableBuffer {
    lines: Vec<String>,
    cursor: Cursor,
    viewport: Viewport,
    mode: Mode,
    byte_limit: usize,
    byte_len: usize,
    insert_escape_sequence: InsertEscapeSequence,
    pending_insert_escape: Option<PendingInsertEscape>,
}

#[derive(Clone, Copy, Debug)]
struct PendingInsertEscape {
    matched_sequence_bytes: usize,
    start: Cursor,
    inserted_text_bytes: usize,
}

impl EditableBuffer {
    /// Creates an editable buffer without a library-enforced insertion limit.
    ///
    /// Applications accepting untrusted or long-lived input should normally
    /// choose [`Self::with_byte_limit`] instead.
    #[must_use]
    pub fn new(text: &str, viewport: Viewport) -> Self {
        Self::with_byte_limit(text, viewport, usize::MAX)
    }

    /// Creates an editable buffer whose future content cannot exceed `byte_limit` bytes.
    ///
    /// Existing input is retained even when it already exceeds the limit; the
    /// limit only prevents additional characters or newlines until corrections
    /// bring the content below it. The size is UTF-8 content bytes plus one byte
    /// for each logical `\n` separator. Initial text is preserved verbatim:
    /// only `\n` splits lines, and `\r` remains content. Callers wanting CRLF
    /// normalization should perform it before construction.
    #[must_use]
    pub fn with_byte_limit(text: &str, viewport: Viewport, byte_limit: usize) -> Self {
        let lines = text.split('\n').map(str::to_owned).collect::<Vec<_>>();
        Self {
            lines,
            cursor: Cursor::new(0, 0),
            viewport,
            mode: Mode::Normal,
            byte_limit,
            byte_len: text.len(),
            insert_escape_sequence: InsertEscapeSequence::default(),
            pending_insert_escape: None,
        }
    }

    /// Replaces the default `jj` Insert escape sequence for this buffer.
    ///
    /// This builder is intended for construction-time configuration. Use
    /// [`Self::set_insert_escape_sequence`] to replace it later.
    #[must_use]
    pub fn with_insert_escape_sequence(mut self, sequence: InsertEscapeSequence) -> Self {
        let _ = self.flush_pending_input();
        self.insert_escape_sequence = sequence;
        self
    }

    /// Returns the active Insert escape sequence configuration.
    #[must_use]
    pub const fn insert_escape_sequence(&self) -> &InsertEscapeSequence {
        &self.insert_escape_sequence
    }

    /// Replaces the Insert escape sequence without discarding a typed prefix.
    ///
    /// A pending prefix has already been inserted literally, so changing the
    /// configuration commits it simply by forgetting its escape-candidate
    /// status.
    pub fn set_insert_escape_sequence(&mut self, sequence: InsertEscapeSequence) {
        let _ = self.flush_pending_input();
        self.insert_escape_sequence = sequence;
    }

    /// Reports whether inserted text still matches a prefix of the escape sequence.
    #[must_use]
    pub const fn has_pending_input(&self) -> bool {
        self.pending_insert_escape.is_some()
    }

    /// Commits a possible escape prefix as literal input.
    ///
    /// The prefix is already present in the text, so this operation only clears
    /// recognition state and returns whether any state was cleared. Call it
    /// before an application changes focus or associates this buffer with a
    /// different input target. Dropping or abandoning the resolver without a
    /// flush still cannot lose the typed characters.
    #[must_use]
    pub fn flush_pending_input(&mut self) -> bool {
        self.pending_insert_escape.take().is_some()
    }

    /// Returns the active mode.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// Returns the current cursor.
    #[must_use]
    pub const fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Returns the current viewport after any automatic cursor reveal.
    #[must_use]
    pub const fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Returns the logical lines, including a final empty line after a trailing newline.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Returns the content size, including newline separators.
    #[must_use]
    pub const fn byte_len(&self) -> usize {
        self.byte_len
    }

    /// Allocates and returns the complete buffer content joined with `\n`.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Selects a cursor, clamps it according to the active mode, and reveals it.
    ///
    /// A line beyond the buffer selects the final line. A byte inside a UTF-8
    /// scalar rounds backward to that scalar's start. Subsequent vertical
    /// motions derive their desired column from this selection.
    pub fn set_cursor(&mut self, cursor: Cursor) {
        let _ = self.flush_pending_input();
        self.cursor = cursor;
        self.clamp_cursor();
        self.viewport = self.viewport.with_desired_column(None);
        self.reveal_cursor();
    }

    /// Applies one framework-independent input according to the active mode.
    ///
    /// [`EditOutcome::Changed`] takes precedence when an input also enters
    /// Insert mode or moves the cursor, as `o` and `O` do. An ignored input
    /// leaves content, cursor, viewport, and mode unchanged. It may still
    /// advance escape-sequence recognition when the byte limit prevents a
    /// possible prefix from being inserted.
    pub fn handle(&mut self, input: EditorInput) -> EditOutcome {
        let outcome = match self.mode {
            Mode::Normal => self.handle_normal(input),
            Mode::Insert => self.handle_insert(input),
        };
        if outcome != EditOutcome::Ignored {
            if !matches!(input, EditorInput::Motion(_)) {
                self.viewport = self.viewport.with_desired_column(None);
            }
            self.reveal_cursor();
        }
        outcome
    }

    fn handle_normal(&mut self, input: EditorInput) -> EditOutcome {
        match input {
            EditorInput::Motion(motion) => {
                let before = (self.cursor, self.viewport);
                let lines = self.lines.iter().map(String::as_str).collect::<Vec<_>>();
                self.cursor = apply(&lines, self.cursor, &mut self.viewport, motion);
                if before == (self.cursor, self.viewport) {
                    EditOutcome::Ignored
                } else {
                    EditOutcome::Moved
                }
            }
            EditorInput::Insert => self.enter_insert(self.cursor.byte_column()),
            EditorInput::Append => {
                let column = next_boundary(self.current_line(), self.cursor.byte_column());
                self.enter_insert(column)
            }
            EditorInput::InsertLineStart => {
                let column = self
                    .current_line()
                    .char_indices()
                    .find(|(_, character)| !character.is_whitespace())
                    .map_or(self.current_line().len(), |(column, _)| column);
                self.enter_insert(column)
            }
            EditorInput::AppendLineEnd => {
                let column = self.current_line().len();
                self.enter_insert(column)
            }
            EditorInput::OpenBelow => self.open_line(OpenLinePlacement::Below),
            EditorInput::OpenAbove => self.open_line(OpenLinePlacement::Above),
            EditorInput::Character(_)
            | EditorInput::Newline
            | EditorInput::Backspace
            | EditorInput::Delete
            | EditorInput::Escape => EditOutcome::Ignored,
        }
    }

    fn handle_insert(&mut self, input: EditorInput) -> EditOutcome {
        if let EditorInput::Character(character) = input {
            return self.handle_insert_character(character);
        }
        // Non-character input terminates recognition, but the possible prefix
        // is already literal text. The editing operation then observes it in
        // exactly the order the caller submitted inputs.
        let _ = self.flush_pending_input();
        match input {
            EditorInput::Newline => self.insert_newline(),
            EditorInput::Backspace => self.backspace(),
            EditorInput::Delete => self.delete(),
            EditorInput::Escape => self.leave_insert_mode(),
            EditorInput::Character(_) => unreachable!("characters return before this match"),
            EditorInput::Motion(_)
            | EditorInput::Insert
            | EditorInput::Append
            | EditorInput::InsertLineStart
            | EditorInput::AppendLineEnd
            | EditorInput::OpenBelow
            | EditorInput::OpenAbove => EditOutcome::Ignored,
        }
    }

    fn enter_insert(&mut self, column: usize) -> EditOutcome {
        self.pending_insert_escape = None;
        self.cursor = Cursor::new(self.cursor.line(), column);
        self.mode = Mode::Insert;
        self.clamp_cursor();
        EditOutcome::ModeChanged
    }

    fn insert_character(&mut self, character: char) -> EditOutcome {
        if matches!(character, '\n' | '\r') {
            return EditOutcome::Ignored;
        }
        if self
            .byte_len
            .checked_add(character.len_utf8())
            .is_none_or(|next| next > self.byte_limit)
        {
            return EditOutcome::Ignored;
        }
        let line = self.cursor.line();
        let column = self.cursor.byte_column();
        self.lines[line].insert(column, character);
        self.cursor = Cursor::new(line, column.saturating_add(character.len_utf8()));
        self.byte_len = self.byte_len.saturating_add(character.len_utf8());
        EditOutcome::Changed
    }

    fn handle_insert_character(&mut self, character: char) -> EditOutcome {
        if let Some(mut pending) = self.pending_insert_escape.take() {
            let expected = self.insert_escape_sequence.sequence[pending.matched_sequence_bytes..]
                .chars()
                .next();
            if expected == Some(character) {
                let matched_sequence_bytes = pending
                    .matched_sequence_bytes
                    .saturating_add(character.len_utf8());
                if matched_sequence_bytes == self.insert_escape_sequence.sequence.len() {
                    self.remove_pending_escape_prefix(pending);
                    return self.leave_insert_mode();
                }
                let outcome = self.insert_character(character);
                pending.matched_sequence_bytes = matched_sequence_bytes;
                if outcome == EditOutcome::Changed {
                    pending.inserted_text_bytes = pending
                        .inserted_text_bytes
                        .saturating_add(character.len_utf8());
                }
                self.pending_insert_escape = Some(pending);
                return outcome;
            }
            // The existing candidate stays in the buffer as literal text. The
            // mismatching character may itself start a new candidate, which
            // matters for replaceable sequences with overlapping prefixes.
        }

        let Some(first) = self.insert_escape_sequence.sequence.chars().next() else {
            return self.insert_character(character);
        };
        if character != first {
            return self.insert_character(character);
        }
        if character.len_utf8() == self.insert_escape_sequence.sequence.len() {
            return self.leave_insert_mode();
        }

        let start = self.cursor;
        let outcome = self.insert_character(character);
        self.pending_insert_escape = Some(PendingInsertEscape {
            matched_sequence_bytes: character.len_utf8(),
            start,
            inserted_text_bytes: if outcome == EditOutcome::Changed {
                character.len_utf8()
            } else {
                0
            },
        });
        outcome
    }

    fn remove_pending_escape_prefix(&mut self, pending: PendingInsertEscape) {
        let line = pending.start.line();
        let start = pending.start.byte_column();
        let end = start.saturating_add(pending.inserted_text_bytes);
        if start < end {
            self.lines[line].drain(start..end);
            self.byte_len = self.byte_len.saturating_sub(pending.inserted_text_bytes);
        }
        self.cursor = pending.start;
    }

    fn leave_insert_mode(&mut self) -> EditOutcome {
        self.pending_insert_escape = None;
        // Vim leaves Insert mode on the character preceding the insertion
        // point; byte subtraction would split UTF-8.
        if self.cursor.byte_column() > 0 {
            let column = previous_boundary(self.current_line(), self.cursor.byte_column());
            self.cursor = Cursor::new(self.cursor.line(), column);
        }
        self.mode = Mode::Normal;
        self.clamp_cursor();
        EditOutcome::ModeChanged
    }

    fn insert_newline(&mut self) -> EditOutcome {
        if self
            .byte_len
            .checked_add(1)
            .is_none_or(|next| next > self.byte_limit)
        {
            return EditOutcome::Ignored;
        }
        let line = self.cursor.line();
        let column = self.cursor.byte_column();
        let suffix = self.lines[line].split_off(column);
        self.lines.insert(line.saturating_add(1), suffix);
        self.cursor = Cursor::new(line.saturating_add(1), 0);
        self.byte_len = self.byte_len.saturating_add(1);
        EditOutcome::Changed
    }

    fn open_line(&mut self, placement: OpenLinePlacement) -> EditOutcome {
        if self
            .byte_len
            .checked_add(1)
            .is_none_or(|next| next > self.byte_limit)
        {
            return EditOutcome::Ignored;
        }
        let line = if placement == OpenLinePlacement::Above {
            self.cursor.line().min(self.lines.len().saturating_sub(1))
        } else {
            self.cursor.line().saturating_add(1).min(self.lines.len())
        };
        self.lines.insert(line, String::new());
        self.byte_len = self.byte_len.saturating_add(1);
        self.cursor = Cursor::new(line, 0);
        self.mode = Mode::Insert;
        self.pending_insert_escape = None;
        EditOutcome::Changed
    }

    fn backspace(&mut self) -> EditOutcome {
        let line = self.cursor.line();
        let column = self.cursor.byte_column();
        if column > 0 {
            let previous = previous_boundary(&self.lines[line], column);
            self.lines[line].drain(previous..column);
            self.byte_len = self
                .byte_len
                .saturating_sub(column.saturating_sub(previous));
            self.cursor = Cursor::new(line, previous);
            return EditOutcome::Changed;
        }
        if line == 0 {
            return EditOutcome::Ignored;
        }
        let current = self.lines.remove(line);
        let previous = line.saturating_sub(1);
        let column = self.lines[previous].len();
        self.lines[previous].push_str(&current);
        self.byte_len = self.byte_len.saturating_sub(1);
        self.cursor = Cursor::new(previous, column);
        EditOutcome::Changed
    }

    fn delete(&mut self) -> EditOutcome {
        let line = self.cursor.line();
        let column = self.cursor.byte_column();
        if column < self.lines[line].len() {
            let next = next_boundary(&self.lines[line], column);
            self.lines[line].drain(column..next);
            self.byte_len = self.byte_len.saturating_sub(next.saturating_sub(column));
            return EditOutcome::Changed;
        }
        if line.saturating_add(1) >= self.lines.len() {
            return EditOutcome::Ignored;
        }
        let next = self.lines.remove(line.saturating_add(1));
        self.lines[line].push_str(&next);
        self.byte_len = self.byte_len.saturating_sub(1);
        EditOutcome::Changed
    }

    fn current_line(&self) -> &str {
        &self.lines[self.cursor.line()]
    }

    fn clamp_cursor(&mut self) {
        let line = self.cursor.line().min(self.lines.len().saturating_sub(1));
        let text = &self.lines[line];
        let mut column = self.cursor.byte_column().min(text.len());
        while !text.is_char_boundary(column) {
            column = column.saturating_sub(1);
        }
        if self.mode == Mode::Normal && column == text.len() && !text.is_empty() {
            column = previous_boundary(text, text.len());
        }
        self.cursor = Cursor::new(line, column);
    }

    fn reveal_cursor(&mut self) {
        let line = self.cursor.line();
        crate::motion::reveal_cursor_line(
            &self.lines[line],
            self.lines.len().saturating_sub(1),
            self.cursor,
            &mut self.viewport,
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenLinePlacement {
    Above,
    Below,
}

fn previous_boundary(line: &str, byte_column: usize) -> usize {
    let end = byte_column.min(line.len());
    line[..end]
        .char_indices()
        .next_back()
        .map_or(0, |(column, _)| column)
}

fn next_boundary(line: &str, byte_column: usize) -> usize {
    let start = byte_column.min(line.len());
    line[start..].chars().next().map_or(start, |character| {
        start.saturating_add(character.len_utf8())
    })
}

#[cfg(test)]
mod tests {
    use super::{EditOutcome, EditableBuffer, EditorInput, InsertEscapeSequence, Mode};
    use crate::{Cursor, Motion, MotionKind, Viewport};

    #[test]
    fn selecting_a_cursor_resets_the_column_for_vertical_motion() {
        let mut buffer = EditableBuffer::new("abcdef\nx\nabcdef", Viewport::new(0, 0, 3, 20, 0));
        buffer.set_cursor(Cursor::new(0, 4));
        let _ = buffer.handle(EditorInput::Motion(Motion::new(MotionKind::Down)));
        buffer.set_cursor(Cursor::new(1, 0));
        let _ = buffer.handle(EditorInput::Motion(Motion::new(MotionKind::Down)));
        assert_eq!(buffer.cursor(), Cursor::new(2, 0));
    }

    #[test]
    fn leaving_insert_mode_resets_the_column_for_vertical_motion() {
        for escape in [EditorInput::Escape, EditorInput::Character('j')] {
            let mut buffer = EditableBuffer::new("abcdef\nabcdef", Viewport::new(0, 0, 2, 20, 0));
            let _ = buffer.handle(EditorInput::Motion(Motion::new(MotionKind::LineEnd)));
            let _ = buffer.handle(EditorInput::InsertLineStart);
            let _ = buffer.handle(EditorInput::Character('界'));
            if escape == EditorInput::Character('j') {
                let _ = buffer.handle(escape);
            }
            let _ = buffer.handle(escape);
            let _ = buffer.handle(EditorInput::Motion(Motion::new(MotionKind::Down)));
            assert_eq!(buffer.cursor(), Cursor::new(1, 0), "{escape:?}");
        }
    }

    #[test]
    fn normal_insert_correction_newline_and_escape_preserve_content_and_cursor() {
        let mut buffer = EditableBuffer::new("abcd", Viewport::new(0, 0, 4, 20, 0));
        assert_eq!(
            buffer.handle(EditorInput::Motion(Motion::new(MotionKind::Right))),
            EditOutcome::Moved
        );
        assert_eq!(buffer.handle(EditorInput::Insert), EditOutcome::ModeChanged);
        assert_eq!(
            buffer.handle(EditorInput::Character('x')),
            EditOutcome::Changed
        );
        assert_eq!(buffer.handle(EditorInput::Backspace), EditOutcome::Changed);
        for character in "j3qQ".chars() {
            assert_eq!(
                buffer.handle(EditorInput::Character(character)),
                EditOutcome::Changed
            );
        }
        assert_eq!(buffer.handle(EditorInput::Newline), EditOutcome::Changed);
        assert_eq!(
            buffer.handle(EditorInput::Character('界')),
            EditOutcome::Changed
        );
        assert_eq!(buffer.handle(EditorInput::Escape), EditOutcome::ModeChanged);

        assert_eq!(buffer.mode(), Mode::Normal);
        assert_eq!(buffer.text(), "aj3qQ\n界bcd");
        assert_eq!(buffer.cursor(), Cursor::new(1, 0));
    }

    #[test]
    fn insert_mode_treats_motion_letters_digits_and_quit_letters_as_text() {
        let mut buffer = EditableBuffer::new("", Viewport::new(0, 0, 2, 10, 0));
        assert_eq!(buffer.handle(EditorInput::Insert), EditOutcome::ModeChanged);
        for character in "jk123qQ".chars() {
            let _ = buffer.handle(EditorInput::Character(character));
        }
        assert_eq!(buffer.text(), "jk123qQ");
        assert_eq!(buffer.handle(EditorInput::Escape), EditOutcome::ModeChanged);
        assert_eq!(buffer.cursor(), Cursor::new(0, 6));
    }

    #[test]
    fn byte_limit_and_unicode_corrections_keep_boundaries() {
        let mut buffer =
            EditableBuffer::with_byte_limit("界", Viewport::new(0, 0, 2, 10, 0), "界x".len());
        let _ = buffer.handle(EditorInput::AppendLineEnd);
        assert_eq!(
            buffer.handle(EditorInput::Character('x')),
            EditOutcome::Changed
        );
        assert_eq!(
            buffer.handle(EditorInput::Character('y')),
            EditOutcome::Ignored
        );
        assert_eq!(buffer.handle(EditorInput::Backspace), EditOutcome::Changed);
        assert_eq!(buffer.handle(EditorInput::Backspace), EditOutcome::Changed);
        assert_eq!(buffer.text(), "");
        assert_eq!(buffer.cursor(), Cursor::new(0, 0));
    }

    #[test]
    fn line_joining_corrections_are_symmetric() {
        let mut buffer = EditableBuffer::new("one\ntwo", Viewport::new(0, 0, 3, 20, 0));
        buffer.set_cursor(Cursor::new(1, 0));
        let _ = buffer.handle(EditorInput::Insert);
        assert_eq!(buffer.handle(EditorInput::Backspace), EditOutcome::Changed);
        assert_eq!(buffer.text(), "onetwo");
        assert_eq!(buffer.cursor(), Cursor::new(0, 3));

        let _ = buffer.handle(EditorInput::Newline);
        let _ = buffer.handle(EditorInput::Escape);
        buffer.set_cursor(Cursor::new(0, 2));
        let _ = buffer.handle(EditorInput::AppendLineEnd);
        assert_eq!(buffer.handle(EditorInput::Delete), EditOutcome::Changed);
        assert_eq!(buffer.text(), "onetwo");
    }

    #[test]
    fn new_lines_respect_the_byte_limit() {
        let mut buffer =
            EditableBuffer::with_byte_limit("one", Viewport::new(0, 0, 3, 20, 0), "one".len());
        assert_eq!(buffer.handle(EditorInput::OpenBelow), EditOutcome::Ignored);
        assert_eq!(buffer.handle(EditorInput::OpenAbove), EditOutcome::Ignored);

        let _ = buffer.handle(EditorInput::AppendLineEnd);
        assert_eq!(buffer.handle(EditorInput::Newline), EditOutcome::Ignored);
        assert_eq!(buffer.text(), "one");
    }

    #[test]
    fn insert_cursor_stays_visible_and_line_separators_use_newline_input() {
        let mut buffer = EditableBuffer::new("ab", Viewport::new(0, 0, 1, 2, 0));
        let _ = buffer.handle(EditorInput::AppendLineEnd);
        assert_eq!(
            buffer.handle(EditorInput::Character('\n')),
            EditOutcome::Ignored
        );
        assert_eq!(
            buffer.handle(EditorInput::Character('\r')),
            EditOutcome::Ignored
        );
        assert_eq!(buffer.text(), "ab");

        assert_eq!(buffer.handle(EditorInput::Newline), EditOutcome::Changed);
        assert_eq!(buffer.cursor(), Cursor::new(1, 0));
        assert_eq!(buffer.viewport().top(), 1);

        assert_eq!(
            buffer.handle(EditorInput::Character('界')),
            EditOutcome::Changed
        );
        assert_eq!(buffer.viewport().left(), 1);
        assert_eq!(buffer.text(), "ab\n界");
    }

    #[test]
    fn default_jj_returns_to_normal_without_inserting_the_sequence() {
        let mut buffer = EditableBuffer::new("ab", Viewport::new(0, 0, 2, 20, 0));
        let mut escaped = EditableBuffer::new("ab", Viewport::new(0, 0, 2, 20, 0));
        assert_eq!(
            buffer.handle(EditorInput::AppendLineEnd),
            EditOutcome::ModeChanged
        );
        let _ = escaped.handle(EditorInput::AppendLineEnd);
        let _ = escaped.handle(EditorInput::Escape);
        assert_eq!(
            buffer.handle(EditorInput::Character('j')),
            EditOutcome::Changed
        );
        assert_eq!(buffer.text(), "abj");
        assert!(buffer.has_pending_input());

        assert_eq!(
            buffer.handle(EditorInput::Character('j')),
            EditOutcome::ModeChanged
        );
        assert_eq!(buffer.text(), "ab");
        assert_eq!(buffer.mode(), Mode::Normal);
        assert_eq!(buffer.cursor(), Cursor::new(0, 1));
        assert_eq!(buffer.text(), escaped.text());
        assert_eq!(buffer.cursor(), escaped.cursor());
        assert_eq!(buffer.viewport(), escaped.viewport());
        assert!(!buffer.has_pending_input());
    }

    #[test]
    fn lone_j_mismatch_and_context_flush_keep_literal_input_in_order() {
        let mut buffer = EditableBuffer::new("", Viewport::new(0, 0, 2, 20, 0));
        let _ = buffer.handle(EditorInput::Insert);
        assert_eq!(
            buffer.handle(EditorInput::Character('j')),
            EditOutcome::Changed
        );
        assert_eq!(buffer.text(), "j");
        assert!(buffer.has_pending_input());

        assert_eq!(
            buffer.handle(EditorInput::Character('界')),
            EditOutcome::Changed
        );
        assert_eq!(buffer.text(), "j界");
        assert!(!buffer.has_pending_input());

        let _ = buffer.handle(EditorInput::Character('j'));
        assert!(buffer.flush_pending_input());
        assert_eq!(buffer.text(), "j界j");
        assert!(!buffer.flush_pending_input());

        let _ = buffer.handle(EditorInput::Character('j'));
        buffer.set_cursor(Cursor::new(0, 0));
        assert_eq!(buffer.text(), "j界jj");
        assert!(!buffer.has_pending_input());

        let _ = buffer.handle(EditorInput::Character('j'));
        let before_context_input = buffer.text();
        assert_eq!(
            buffer.handle(EditorInput::Motion(Motion::new(MotionKind::Right))),
            EditOutcome::Ignored
        );
        assert_eq!(buffer.text(), before_context_input);
        assert!(!buffer.has_pending_input());

        let _ = buffer.handle(EditorInput::Character('j'));
        let before_invalid_character = buffer.text();
        assert_eq!(
            buffer.handle(EditorInput::Character('\n')),
            EditOutcome::Ignored
        );
        assert_eq!(buffer.text(), before_invalid_character);
        assert!(!buffer.has_pending_input());
    }

    #[test]
    fn editing_keys_after_j_observe_the_inserted_prefix() {
        let mut backspace = EditableBuffer::new("ab", Viewport::new(0, 0, 2, 20, 0));
        let _ = backspace.handle(EditorInput::Insert);
        let _ = backspace.handle(EditorInput::Character('j'));
        assert_eq!(
            backspace.handle(EditorInput::Backspace),
            EditOutcome::Changed
        );
        assert_eq!(backspace.text(), "ab");
        assert_eq!(backspace.cursor(), Cursor::new(0, 0));

        let mut delete = EditableBuffer::new("ab", Viewport::new(0, 0, 2, 20, 0));
        let _ = delete.handle(EditorInput::Insert);
        let _ = delete.handle(EditorInput::Character('j'));
        assert_eq!(delete.handle(EditorInput::Delete), EditOutcome::Changed);
        assert_eq!(delete.text(), "jb");
        assert_eq!(delete.cursor(), Cursor::new(0, 1));

        let mut newline = EditableBuffer::new("", Viewport::new(0, 0, 2, 20, 0));
        let _ = newline.handle(EditorInput::Insert);
        let _ = newline.handle(EditorInput::Character('j'));
        assert_eq!(newline.handle(EditorInput::Newline), EditOutcome::Changed);
        assert_eq!(newline.text(), "j\n");
        assert_eq!(newline.cursor(), Cursor::new(1, 0));

        let mut escape = EditableBuffer::new("", Viewport::new(0, 0, 2, 20, 0));
        let _ = escape.handle(EditorInput::Insert);
        let _ = escape.handle(EditorInput::Character('j'));
        assert_eq!(escape.handle(EditorInput::Escape), EditOutcome::ModeChanged);
        assert_eq!(escape.text(), "j");
        assert_eq!(escape.mode(), Mode::Normal);
        assert_eq!(escape.cursor(), Cursor::new(0, 0));
    }

    #[test]
    fn insert_escape_can_be_disabled_or_replaced_for_literal_jj() {
        let mut literal = EditableBuffer::new("", Viewport::new(0, 0, 2, 20, 0))
            .with_insert_escape_sequence(InsertEscapeSequence::disabled());
        let _ = literal.handle(EditorInput::Insert);
        let _ = literal.handle(EditorInput::Character('j'));
        let _ = literal.handle(EditorInput::Character('j'));
        assert_eq!(literal.text(), "jj");
        assert_eq!(literal.mode(), Mode::Insert);

        let sequence = InsertEscapeSequence::new("jk")
            .unwrap_or_else(|error| panic!("valid sequence: {error}"));
        let mut replaced = EditableBuffer::new("", Viewport::new(0, 0, 2, 20, 0))
            .with_insert_escape_sequence(sequence);
        let _ = replaced.handle(EditorInput::Insert);
        let _ = replaced.handle(EditorInput::Character('j'));
        let _ = replaced.handle(EditorInput::Character('j'));
        assert_eq!(replaced.text(), "jj");
        assert!(replaced.flush_pending_input());
        let _ = replaced.handle(EditorInput::Character('j'));
        assert_eq!(
            replaced.handle(EditorInput::Character('k')),
            EditOutcome::ModeChanged
        );
        assert_eq!(replaced.text(), "jj");
        assert_eq!(replaced.mode(), Mode::Normal);

        let mut reconfigured = EditableBuffer::new("", Viewport::new(0, 0, 2, 20, 0));
        let _ = reconfigured.handle(EditorInput::Insert);
        let _ = reconfigured.handle(EditorInput::Character('j'));
        assert!(reconfigured.has_pending_input());
        reconfigured.set_insert_escape_sequence(InsertEscapeSequence::disabled());
        assert_eq!(reconfigured.text(), "j");
        assert!(!reconfigured.has_pending_input());
        let _ = reconfigured.handle(EditorInput::Character('j'));
        assert_eq!(reconfigured.text(), "jj");
        assert_eq!(reconfigured.mode(), Mode::Insert);
    }

    #[test]
    fn insert_escape_respects_byte_limits_and_unicode_sequences() {
        let mut full =
            EditableBuffer::with_byte_limit("x", Viewport::new(0, 0, 2, 20, 0), "x".len());
        let _ = full.handle(EditorInput::AppendLineEnd);
        assert_eq!(
            full.handle(EditorInput::Character('j')),
            EditOutcome::Ignored
        );
        assert!(full.has_pending_input());
        assert_eq!(
            full.handle(EditorInput::Character('j')),
            EditOutcome::ModeChanged
        );
        assert_eq!(full.text(), "x");
        assert_eq!(full.mode(), Mode::Normal);

        let sequence = InsertEscapeSequence::new("界界")
            .unwrap_or_else(|error| panic!("valid Unicode sequence: {error}"));
        let mut unicode = EditableBuffer::new("a", Viewport::new(0, 0, 2, 20, 0))
            .with_insert_escape_sequence(sequence);
        let _ = unicode.handle(EditorInput::AppendLineEnd);
        let _ = unicode.handle(EditorInput::Character('界'));
        assert_eq!(unicode.text(), "a界");
        assert_eq!(
            unicode.handle(EditorInput::Character('界')),
            EditOutcome::ModeChanged
        );
        assert_eq!(unicode.text(), "a");
        assert_eq!(unicode.cursor(), Cursor::new(0, 0));

        assert!(InsertEscapeSequence::new("").is_err());
        assert!(InsertEscapeSequence::new("j\nj").is_err());
    }
}
