//! Typed diff targets and display-oriented unified-diff documents.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU32;

use crate::domain::{ChangeKind, CommitBaseline, ObjectId, RepoPath};

/// The Git mechanism used to produce an index-to-working-tree comparison.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorktreeDiffKind {
    /// Compare an indexed path with the working tree.
    Tracked,
    /// Compare an untracked path with an empty file through `--no-index`.
    Untracked,
}

impl From<ChangeKind> for WorktreeDiffKind {
    fn from(value: ChangeKind) -> Self {
        if value == ChangeKind::Untracked {
            Self::Untracked
        } else {
            Self::Tracked
        }
    }
}

/// A repository comparison that can be requested from the Git service.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DiffTarget {
    /// Compare the index with one working-tree path.
    Worktree {
        /// Repository-relative path to compare.
        path: RepoPath,
        /// Whether Git reads an indexed path or compares an untracked file.
        kind: WorktreeDiffKind,
    },
    /// Compare one commit path with its explicit baseline.
    Commit {
        /// Commit shown on the newer side of the comparison.
        commit: ObjectId,
        /// Empty-tree or first-parent older side.
        baseline: CommitBaseline,
        /// Repository-relative path to restrict the comparison to.
        path: RepoPath,
    },
}

/// A one-based source or destination line number from a diff hunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineNumber(NonZeroU32);

/// Error returned when zero is used where a one-based line is required.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineNumberError;

impl Display for LineNumberError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("line number must be one or greater")
    }
}

impl Error for LineNumberError {}

impl LineNumber {
    /// Creates a line number as reported by a unified-diff hunk.
    ///
    /// Returns `None` for zero, which is not a valid one-based source line.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the numeric line value.
    #[must_use]
    pub fn value(self) -> u32 {
        self.0.get()
    }
}

impl Display for LineNumber {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<NonZeroU32> for LineNumber {
    fn from(value: NonZeroU32) -> Self {
        Self(value)
    }
}

impl From<LineNumber> for u32 {
    fn from(value: LineNumber) -> Self {
        value.value()
    }
}

impl TryFrom<u32> for LineNumber {
    type Error = LineNumberError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(LineNumberError)
    }
}

/// The semantic role of a parsed unified-diff line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffLineKind {
    /// File header such as `diff --git`, `---`, or `+++`.
    Header,
    /// A hunk range header beginning with `@@`.
    Hunk,
    /// A line present only in the newer file.
    Added,
    /// A line present only in the older file.
    Removed,
    /// An unchanged line included for context.
    Context,
    /// Diff metadata that is not assigned source line numbers.
    Meta,
}

/// One parsed line of a unified diff with optional old and new positions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    kind: DiffLineKind,
    old_line: Option<LineNumber>,
    new_line: Option<LineNumber>,
    text: String,
}

impl DiffLine {
    /// Creates a classified diff line.
    ///
    /// Line numbers are absent on whichever side does not contain the line and
    /// on headers or metadata that do not refer to file contents.
    #[must_use]
    pub fn new(
        kind: DiffLineKind,
        old_line: Option<LineNumber>,
        new_line: Option<LineNumber>,
        text: String,
    ) -> Self {
        Self {
            kind,
            old_line,
            new_line,
            text,
        }
    }

    /// Returns the line classification used by the renderer.
    #[must_use]
    pub fn kind(&self) -> DiffLineKind {
        self.kind
    }

    /// Returns the position in the older file, when applicable.
    #[must_use]
    pub fn old_line(&self) -> Option<LineNumber> {
        self.old_line
    }

    /// Returns the position in the newer file, when applicable.
    #[must_use]
    pub fn new_line(&self) -> Option<LineNumber> {
        self.new_line
    }

    /// Returns the original line text, including its diff marker.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// A bounded result suitable for the diff viewer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffDocument {
    /// A complete text patch.
    Text {
        /// Parsed lines in display order.
        lines: Vec<DiffLine>,
        /// Approximate source bytes retained for cache accounting.
        bytes: usize,
    },
    /// A binary comparison represented by a human-readable summary.
    Binary {
        /// Summary returned to the renderer.
        summary: String,
    },
    /// A valid comparison that produced no patch.
    Empty {
        /// Explanation returned to the renderer.
        message: String,
    },
    /// The prefix of a text patch that crossed the configured output limit.
    Truncated {
        /// Complete parsed lines retained before truncation.
        lines: Vec<DiffLine>,
        /// Approximate retained bytes for cache accounting.
        bytes: usize,
    },
}

impl DiffDocument {
    /// Returns parsed text lines, or an empty slice for non-text outcomes.
    #[must_use]
    pub fn lines(&self) -> &[DiffLine] {
        match self {
            Self::Text { lines, .. } | Self::Truncated { lines, .. } => lines,
            Self::Binary { .. } | Self::Empty { .. } => &[],
        }
    }

    /// Returns the explanatory text for binary or empty outcomes.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Binary { summary } => Some(summary),
            Self::Empty { message } => Some(message),
            Self::Text { .. } | Self::Truncated { .. } => None,
        }
    }

    /// Estimates the memory cost used by the bounded application cache.
    #[must_use]
    pub fn approximate_bytes(&self) -> usize {
        match self {
            Self::Text { bytes, .. } | Self::Truncated { bytes, .. } => *bytes,
            Self::Binary { summary } => summary.len(),
            Self::Empty { message } => message.len(),
        }
    }

    /// Reports whether only a bounded prefix of the patch is available.
    #[must_use]
    pub fn is_truncated(&self) -> bool {
        matches!(self, Self::Truncated { .. })
    }
}
