//! Read-only Vim cursor and viewport motion.
//!
//! This module owns the coordinate types and pure movement vocabulary used by
//! both direct callers and the crate's optional editable buffer. [`apply`]
//! borrows a logical line snapshot, clamps the supplied [`Cursor`] to a valid
//! UTF-8 boundary, applies one completed [`Motion`], and updates [`Viewport`]
//! so the result remains visible. It never changes the input text.
//!
//! # Coordinates
//!
//! [`Cursor::byte_column`] is a zero-based UTF-8 byte offset, not a character
//! index or screen column. [`Viewport::left`] and [`Viewport::width`] are
//! terminal display cells. A tab advances to the next four-cell tab stop;
//! wide and combining Unicode characters use their terminal widths. Empty
//! input and empty lines have the single valid cursor position `(line, 0)`.
//!
//! Count parsing, incomplete commands, and character-search repetition belong
//! to [`crate::command`]. Text mutation and Insert-mode cursor rules belong to
//! [`crate::editor`]. Search histories, marks, jump lists, terminal keys, and
//! application resources remain caller-owned.
//!
//! # Example
//!
//! ```
//! use vim_navigation::motion::{Cursor, Motion, MotionKind, Viewport, apply};
//!
//! let lines = ["\t界"];
//! let mut viewport = Viewport::new(0, 0, 1, 8, 0);
//! let cursor = apply(
//!     &lines,
//!     Cursor::new(99, 2), // invalid line and inside the three-byte `界`
//!     &mut viewport,
//!     Motion::new(MotionKind::LineEnd),
//! );
//! assert_eq!(cursor, Cursor::new(0, 1));
//! ```

mod buffer;
mod engine;
mod line;
#[cfg(test)]
mod tests;

pub(crate) use engine::reveal_cursor_line;
pub use engine::{apply, reveal};

pub(crate) const FULL_PERCENT: usize = 100;

/// A zero-based text position whose column is a UTF-8 byte offset.
///
/// Construction is deliberately unchecked so adapters can store a cursor
/// before their next text snapshot is available. [`apply`] and [`reveal`]
/// clamp an out-of-range line to the final logical line and round a byte inside
/// a UTF-8 scalar backward to its start. On non-empty lines a byte column at or
/// beyond the line length clamps to the final scalar; empty lines use column
/// zero.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Cursor {
    line: usize,
    byte_column: usize,
}

impl Cursor {
    /// Creates a cursor without consulting any text.
    ///
    /// The next call to [`apply`] or [`reveal`] performs the documented
    /// boundary clamping.
    #[must_use]
    pub const fn new(line: usize, byte_column: usize) -> Self {
        Self { line, byte_column }
    }

    /// Returns the zero-based line.
    #[must_use]
    pub const fn line(self) -> usize {
        self.line
    }

    /// Returns the zero-based UTF-8 byte column.
    #[must_use]
    pub const fn byte_column(self) -> usize {
        self.byte_column
    }
}

/// Origin of a motion count, including the semantically distinct implicit one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountSource {
    /// No decimal count was supplied; the effective count defaults to one.
    Implicit,
    /// The user supplied a decimal count, including an explicit one.
    Explicit,
}

impl CountSource {
    /// Reports whether the count was explicitly supplied.
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Explicit)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Repetition {
    Original,
    Repeated,
}

/// A completed Vim movement with its count and optional character argument.
///
/// Use [`crate::MotionState`] when counts and character arguments arrive as
/// separate key events. Constructing a `Motion` directly is useful when an
/// adapter has already resolved its command language. A target is meaningful
/// only to character find/till, unmatched-delimiter, and method motions; other
/// motions safely ignore it. Method motions use target `'M'` for closing braces
/// (`[M`/`]M`) and otherwise select opening braces (`[m`/`]m`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Motion {
    kind: MotionKind,
    count: usize,
    count_source: CountSource,
    target: Option<char>,
    repetition: Repetition,
}

impl Motion {
    /// Creates an uncounted movement template with an effective count of one.
    #[must_use]
    pub const fn new(kind: MotionKind) -> Self {
        Self {
            kind,
            count: 1,
            count_source: CountSource::Implicit,
            target: None,
            repetition: Repetition::Original,
        }
    }

    /// Returns the operation.
    #[must_use]
    pub const fn kind(self) -> MotionKind {
        self.kind
    }

    /// Returns the normalized count, which is always at least one.
    #[must_use]
    pub const fn count(self) -> usize {
        self.count
    }

    /// Reports whether the user supplied a count.
    ///
    /// This is semantically distinct from [`Self::count`]: Vim interprets an
    /// explicit `1G`, `1%`, or `1z` command differently from its uncounted
    /// form even though both carry the numeric value one.
    #[must_use]
    pub const fn has_explicit_count(self) -> bool {
        self.count_source.is_explicit()
    }

    /// Returns the character argument for find, till, delimiter, or method movement.
    #[must_use]
    pub const fn target(self) -> Option<char> {
        self.target
    }

    /// Attaches a normalized count and its explicit or implicit [`CountSource`].
    ///
    /// A zero count is normalized to one. Counts produced from decimal input
    /// should normally come from [`crate::MotionState`], which also saturates
    /// overflow and applies Vim's counted-command reinterpretations.
    #[must_use]
    pub const fn counted(mut self, count: usize, source: CountSource) -> Self {
        self.count = if count == 0 { 1 } else { count };
        self.count_source = source;
        self
    }

    /// Attaches a Unicode scalar character or delimiter argument.
    #[must_use]
    pub const fn targeting(mut self, target: char) -> Self {
        self.target = Some(target);
        self
    }

    /// Marks a find/till movement as a `;` or `,` repetition.
    ///
    /// Repeated till motions have a Vim-specific adjacent-match exception;
    /// callers should set this through [`crate::MotionState`] rather than
    /// emulating it by issuing the original motion again.
    #[must_use]
    pub const fn repeating(mut self) -> Self {
        self.repetition = Repetition::Repeated;
        self
    }

    /// Reports whether this movement repeats a prior character search.
    #[must_use]
    pub const fn is_repeated(self) -> bool {
        matches!(self.repetition, Repetition::Repeated)
    }

    pub(super) const fn repetition(self) -> Repetition {
        self.repetition
    }
}

/// Vim's standalone cursor and viewport movement vocabulary.
///
/// Most variants are handled entirely by [`apply`]. Search and mark variants
/// are vocabulary signals for adapters because their resources intentionally
/// remain outside this crate; direct `apply` calls leave the cursor unchanged
/// for those variants. [`crate::MotionState`] resolves the two character-search
/// repetition variants to a concrete find/till motion before application.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MotionKind {
    /// `h` or Left.
    Left,
    /// Backspace or Ctrl-H when `'whichwrap'` permits crossing lines.
    LeftWrap,
    /// `l` or Right.
    Right,
    /// Space when `'whichwrap'` permits crossing lines.
    RightWrap,
    /// `k`, Up, or Ctrl-P.
    Up,
    /// `j`, Down, Ctrl-J, Ctrl-N, or newline.
    Down,
    /// `0` or Home.
    LineStart,
    /// `^`.
    FirstNonBlank,
    /// `$` or End.
    LineEnd,
    /// `g_`.
    LastNonBlank,
    /// `g0`.
    ScreenLineStart,
    /// `g^`.
    ScreenFirstNonBlank,
    /// `g$`.
    ScreenLineEnd,
    /// `g<End>`.
    ScreenLastNonBlank,
    /// `gm`.
    ScreenMiddle,
    /// `gM`.
    LineMiddle,
    /// `|`.
    Column,
    /// `go`.
    ByteOffset,
    /// `w`.
    WordForward,
    /// `W`.
    BigWordForward,
    /// `e`.
    WordEndForward,
    /// `E`.
    BigWordEndForward,
    /// `b`.
    WordBackward,
    /// `B`.
    BigWordBackward,
    /// `ge`.
    WordEndBackward,
    /// `gE`.
    BigWordEndBackward,
    /// `f{char}`.
    FindForward,
    /// `F{char}`.
    FindBackward,
    /// `t{char}`.
    TillForward,
    /// `T{char}`.
    TillBackward,
    /// `;`, resolved by [`crate::MotionState`].
    RepeatCharacterSearch,
    /// `,`, resolved by [`crate::MotionState`].
    ReverseCharacterSearch,
    /// `-`.
    PreviousLineFirstNonBlank,
    /// `+` or Enter.
    NextLineFirstNonBlank,
    /// `_`.
    CountedLineFirstNonBlank,
    /// `gg` or Ctrl-Home.
    BufferTop,
    /// `G`.
    BufferBottom,
    /// Ctrl-End.
    BufferBottomEnd,
    /// `{count}%`.
    BufferPercentage,
    /// `H`.
    WindowTop,
    /// `M`.
    WindowMiddle,
    /// `L`.
    WindowBottom,
    /// `(`.
    SentenceBackward,
    /// `)`.
    SentenceForward,
    /// `{`.
    ParagraphBackward,
    /// `}`.
    ParagraphForward,
    /// `[[`.
    SectionStartBackward,
    /// `]]`.
    SectionStartForward,
    /// `[]`.
    SectionEndBackward,
    /// `][`.
    SectionEndForward,
    /// `%` without a count.
    MatchingPair,
    /// `g%`.
    MatchingPairBackward,
    /// `[(` or `[{`; the delimiter is stored in [`Motion::target`].
    UnmatchedOpenBackward,
    /// `])` or `]}`; the delimiter is stored in [`Motion::target`].
    UnmatchedCloseForward,
    /// `[m` or `[M`.
    MethodBackward,
    /// `]m` or `]M`.
    MethodForward,
    /// `[#`.
    PreprocessorBackward,
    /// `]#`.
    PreprocessorForward,
    /// `[*` or `[/`.
    CommentBackward,
    /// `]*` or `]/`.
    CommentForward,
    /// `[c` in diff text.
    DiffChangeBackward,
    /// `]c` in diff text.
    DiffChangeForward,
    /// `n`, applied by the caller's retained search state.
    SearchNext,
    /// `N`, applied by the caller's retained search state.
    SearchPrevious,
    /// `*`, applied by the caller's retained search state.
    SearchWordForward,
    /// `#`, applied by the caller's retained search state.
    SearchWordBackward,
    /// `g*`, applied by the caller's retained search state.
    SearchPartialWordForward,
    /// `g#`, applied by the caller's retained search state.
    SearchPartialWordBackward,
    /// `['`, applied by the caller's resource-aware mark store.
    PreviousMarkLine,
    /// `` [` ``, applied by the caller's resource-aware mark store.
    PreviousMarkExact,
    /// `]'`, applied by the caller's resource-aware mark store.
    NextMarkLine,
    /// `` ]` ``, applied by the caller's resource-aware mark store.
    NextMarkExact,
    /// Ctrl-D.
    HalfPageDown,
    /// Ctrl-U.
    HalfPageUp,
    /// Ctrl-F or `PageDown`.
    PageDown,
    /// Ctrl-B or `PageUp`.
    PageUp,
    /// Ctrl-E.
    ScrollLineDown,
    /// Ctrl-Y.
    ScrollLineUp,
    /// `zt`.
    CursorToWindowTop,
    /// `z<CR>`.
    CursorToWindowTopFirstNonBlank,
    /// `zz`.
    CursorToWindowMiddle,
    /// `z.`.
    CursorToWindowMiddleFirstNonBlank,
    /// `zb`.
    CursorToWindowBottom,
    /// `z-`.
    CursorToWindowBottomFirstNonBlank,
    /// `z+`.
    NextWindowTop,
    /// `z^`.
    PreviousWindowBottom,
    /// `zh`.
    ScrollColumnLeft,
    /// `zl`.
    ScrollColumnRight,
    /// `zH`.
    ScrollHalfScreenLeft,
    /// `zL`.
    ScrollHalfScreenRight,
    /// `zs`.
    CursorToWindowLeft,
    /// `ze`.
    CursorToWindowRight,
}

/// The visible part of a text pane, including any rendered gutter columns.
///
/// `top` is a zero-based logical line. `left`, `width`, `gutter`, and the
/// retained desired column are terminal display cells, not UTF-8 byte columns.
/// The viewport does not own wrapping or rendered text: callers supply the
/// same logical line snapshot to [`apply`] that they display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Viewport {
    top: usize,
    left: usize,
    height: usize,
    width: usize,
    gutter: usize,
    desired_column: Option<usize>,
}

impl Viewport {
    /// Creates a viewport in terminal display cells.
    ///
    /// Zero `height` or `width` is normalized to one so reveal calculations
    /// remain total. `top` and `left` may initially be beyond the supplied
    /// text; motion application clamps or adjusts them as needed.
    #[must_use]
    pub fn new(top: usize, left: usize, height: usize, width: usize, gutter: usize) -> Self {
        Self {
            top,
            left,
            height: height.max(1),
            width: width.max(1),
            gutter,
            desired_column: None,
        }
    }

    /// Restores the desired display column retained by vertical motions.
    ///
    /// `None` derives the first desired column from the current cursor. After
    /// each motion, [`apply`] stores the column needed for a later `j` or `k`;
    /// callers preserving a viewport between calls normally do not need to set
    /// this themselves.
    #[must_use]
    pub fn with_desired_column(mut self, desired_column: Option<usize>) -> Self {
        self.desired_column = desired_column;
        self
    }

    /// Returns the zero-based first visible logical line.
    #[must_use]
    pub const fn top(&self) -> usize {
        self.top
    }

    /// Returns the first visible terminal column, including the gutter.
    #[must_use]
    pub const fn left(&self) -> usize {
        self.left
    }

    /// Returns the viewport height in logical lines (always at least one).
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Returns the viewport width in terminal display cells (always at least one).
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Returns the rendered gutter width in terminal display cells.
    #[must_use]
    pub const fn gutter(&self) -> usize {
        self.gutter
    }

    /// Returns the retained source-text display column used by vertical motions.
    #[must_use]
    pub const fn desired_column(&self) -> Option<usize> {
        self.desired_column
    }
}
