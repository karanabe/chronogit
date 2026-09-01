//! Typed diff targets and display-oriented unified-diff documents.

use crate::domain::{CommitBaseline, ObjectId, RepoPath};

/// A repository comparison that can be requested from the Git service.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DiffTarget {
    /// Compare the index with one working-tree path.
    Worktree {
        /// Repository-relative path to compare.
        path: RepoPath,
        /// Whether the path has no index entry and must compare from `/dev/null`.
        untracked: bool,
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
pub struct LineNumber(u32);

impl LineNumber {
    /// Creates a line number as reported by a unified-diff hunk.
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the numeric line value.
    #[must_use]
    pub fn value(self) -> u32 {
        self.0
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
