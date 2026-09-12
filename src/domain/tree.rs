//! Entries returned while lazily expanding a commit tree.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::domain::{ObjectId, RepoPath};

/// The object or filesystem role represented by a tree entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeKind {
    /// A Git tree that can be expanded lazily.
    Directory,
    /// A regular blob.
    File,
    /// A blob whose mode identifies a symbolic link.
    Symlink,
    /// A gitlink that names another repository commit.
    Submodule,
}

/// A validated Git tree-entry mode.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitTreeMode {
    /// A tree object (`040000`).
    Directory,
    /// A non-executable blob (`100644`).
    RegularFile,
    /// An executable blob (`100755`).
    ExecutableFile,
    /// A symbolic-link blob (`120000`).
    Symlink,
    /// A gitlink commit (`160000`).
    Submodule,
}

impl GitTreeMode {
    /// Validates the object-type and mode pair emitted by `git ls-tree`.
    ///
    /// # Errors
    ///
    /// Returns [`GitTreeModeError`] when the pair is not one of Git's supported
    /// tree-entry representations.
    pub fn parse(object_type: &str, mode: &str) -> Result<Self, GitTreeModeError> {
        match (object_type, mode) {
            ("tree", "040000") => Ok(Self::Directory),
            ("blob", "100644") => Ok(Self::RegularFile),
            ("blob", "100755") => Ok(Self::ExecutableFile),
            ("blob", "120000") => Ok(Self::Symlink),
            ("commit", "160000") => Ok(Self::Submodule),
            _ => Err(GitTreeModeError {
                object_type: object_type.to_owned(),
                mode: mode.to_owned(),
            }),
        }
    }

    /// Returns the semantic role implied by this validated mode.
    #[must_use]
    pub const fn kind(self) -> TreeKind {
        match self {
            Self::Directory => TreeKind::Directory,
            Self::RegularFile | Self::ExecutableFile => TreeKind::File,
            Self::Symlink => TreeKind::Symlink,
            Self::Submodule => TreeKind::Submodule,
        }
    }

    /// Returns Git's canonical six-digit octal representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "040000",
            Self::RegularFile => "100644",
            Self::ExecutableFile => "100755",
            Self::Symlink => "120000",
            Self::Submodule => "160000",
        }
    }
}

impl Display for GitTreeMode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// An unsupported object-type and mode pair from `git ls-tree`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitTreeModeError {
    object_type: String,
    mode: String,
}

impl GitTreeModeError {
    /// Returns the unrecognized Git object type.
    #[must_use]
    pub fn object_type(&self) -> &str {
        &self.object_type
    }

    /// Returns the unrecognized octal mode text.
    #[must_use]
    pub fn mode(&self) -> &str {
        &self.mode
    }
}

impl Display for GitTreeModeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unsupported tree entry type {:?} with mode {:?}",
            self.object_type, self.mode
        )
    }
}

impl Error for GitTreeModeError {}

/// One direct child of a Git tree object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeEntry {
    object_id: ObjectId,
    mode: GitTreeMode,
    name: RepoPath,
}

impl TreeEntry {
    /// Creates an entry from a validated `ls-tree` record.
    #[must_use]
    pub fn new(object_id: ObjectId, mode: GitTreeMode, name: RepoPath) -> Self {
        Self {
            object_id,
            mode,
            name,
        }
    }

    /// Returns the object ID needed to load a directory's children.
    #[must_use]
    pub fn object_id(&self) -> &ObjectId {
        &self.object_id
    }

    /// Returns Git's octal mode text.
    #[must_use]
    pub fn mode(&self) -> GitTreeMode {
        self.mode
    }

    /// Returns the classified entry role.
    #[must_use]
    pub fn kind(&self) -> TreeKind {
        self.mode.kind()
    }

    /// Returns the name relative to the queried tree, not the repository root.
    #[must_use]
    pub fn name(&self) -> &RepoPath {
        &self.name
    }
}
