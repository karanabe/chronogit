//! Entries returned while lazily expanding a commit tree.

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

/// One direct child of a Git tree object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeEntry {
    object_id: ObjectId,
    mode: String,
    kind: TreeKind,
    name: RepoPath,
}

impl TreeEntry {
    /// Creates an entry from a validated `ls-tree` record.
    #[must_use]
    pub fn new(object_id: ObjectId, mode: String, kind: TreeKind, name: RepoPath) -> Self {
        Self {
            object_id,
            mode,
            kind,
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
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// Returns the classified entry role.
    #[must_use]
    pub fn kind(&self) -> TreeKind {
        self.kind
    }

    /// Returns the name relative to the queried tree, not the repository root.
    #[must_use]
    pub fn name(&self) -> &RepoPath {
        &self.name
    }
}
