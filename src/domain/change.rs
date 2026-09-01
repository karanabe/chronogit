//! Change classifications reported for the worktree and committed snapshots.

use crate::domain::RepoPath;

/// The semantic kind of a path change reported by Git.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChangeKind {
    /// File contents or metadata changed in place.
    Modified,
    /// A new tracked path was added.
    Added,
    /// A worktree path is not tracked by Git.
    Untracked,
    /// A tracked path was deleted.
    Deleted,
    /// A path was renamed from [`WorktreeChange::original_path`] or
    /// [`ChangedFile::original_path`].
    Renamed,
    /// A path was copied from [`WorktreeChange::original_path`] or
    /// [`ChangedFile::original_path`].
    Copied,
    /// The Git object type or file mode changed.
    TypeChanged,
    /// Git reports an unresolved merge state for the path.
    Unmerged,
}

impl ChangeKind {
    /// Returns the compact status marker used in the terminal file list.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Untracked => "?",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Copied => "C",
            Self::TypeChanged => "T",
            Self::Unmerged => "U",
        }
    }
}

/// An unstaged change between the index and the current working tree.
///
/// Staged-only changes are deliberately absent from this type's data source.
/// For renames and copies, `original_path` identifies the source path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeChange {
    path: RepoPath,
    original_path: Option<RepoPath>,
    kind: ChangeKind,
}

impl WorktreeChange {
    /// Creates a worktree change returned by the porcelain-status parser.
    #[must_use]
    pub fn new(path: RepoPath, original_path: Option<RepoPath>, kind: ChangeKind) -> Self {
        Self {
            path,
            original_path,
            kind,
        }
    }

    /// Returns the current or destination repository-relative path.
    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// Returns the source path for a rename or copy, when Git supplied one.
    #[must_use]
    pub fn original_path(&self) -> Option<&RepoPath> {
        self.original_path.as_ref()
    }

    /// Returns the semantic change classification.
    #[must_use]
    pub fn kind(&self) -> ChangeKind {
        self.kind
    }
}

/// A path changed by a commit relative to its selected baseline.
///
/// Unlike [`WorktreeChange`], this value comes from a commit-to-tree
/// comparison. Root commits use the empty tree and other commits use their
/// first parent as that baseline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedFile {
    path: RepoPath,
    original_path: Option<RepoPath>,
    kind: ChangeKind,
}

impl ChangedFile {
    /// Creates a committed-file change returned by the name-status parser.
    #[must_use]
    pub fn new(path: RepoPath, original_path: Option<RepoPath>, kind: ChangeKind) -> Self {
        Self {
            path,
            original_path,
            kind,
        }
    }

    /// Returns the current or destination repository-relative path.
    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// Returns the source path for a rename or copy, when Git supplied one.
    #[must_use]
    pub fn original_path(&self) -> Option<&RepoPath> {
        self.original_path.as_ref()
    }

    /// Returns the semantic change classification.
    #[must_use]
    pub fn kind(&self) -> ChangeKind {
        self.kind
    }
}
