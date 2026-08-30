use crate::domain::{CommitBaseline, ObjectId, RepoPath};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DiffTarget {
    Worktree {
        path: RepoPath,
        untracked: bool,
    },
    Commit {
        commit: ObjectId,
        baseline: CommitBaseline,
        path: RepoPath,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineNumber(u32);

impl LineNumber {
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffLineKind {
    Header,
    Hunk,
    Added,
    Removed,
    Context,
    Meta,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    kind: DiffLineKind,
    old_line: Option<LineNumber>,
    new_line: Option<LineNumber>,
    text: String,
}

impl DiffLine {
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

    #[must_use]
    pub fn kind(&self) -> DiffLineKind {
        self.kind
    }

    #[must_use]
    pub fn old_line(&self) -> Option<LineNumber> {
        self.old_line
    }

    #[must_use]
    pub fn new_line(&self) -> Option<LineNumber> {
        self.new_line
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffDocument {
    Text { lines: Vec<DiffLine>, bytes: usize },
    Binary { summary: String },
    Empty { message: String },
    Truncated { lines: Vec<DiffLine>, bytes: usize },
}

impl DiffDocument {
    #[must_use]
    pub fn lines(&self) -> &[DiffLine] {
        match self {
            Self::Text { lines, .. } | Self::Truncated { lines, .. } => lines,
            Self::Binary { .. } | Self::Empty { .. } => &[],
        }
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Binary { summary } => Some(summary),
            Self::Empty { message } => Some(message),
            Self::Text { .. } | Self::Truncated { .. } => None,
        }
    }

    #[must_use]
    pub fn approximate_bytes(&self) -> usize {
        match self {
            Self::Text { bytes, .. } | Self::Truncated { bytes, .. } => *bytes,
            Self::Binary { summary } => summary.len(),
            Self::Empty { message } => message.len(),
        }
    }

    #[must_use]
    pub fn is_truncated(&self) -> bool {
        matches!(self, Self::Truncated { .. })
    }
}
