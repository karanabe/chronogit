use crate::domain::{ObjectId, RepoPath};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeKind {
    Directory,
    File,
    Symlink,
    Submodule,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeEntry {
    object_id: ObjectId,
    mode: String,
    kind: TreeKind,
    name: RepoPath,
}

impl TreeEntry {
    #[must_use]
    pub fn new(object_id: ObjectId, mode: String, kind: TreeKind, name: RepoPath) -> Self {
        Self {
            object_id,
            mode,
            kind,
            name,
        }
    }

    #[must_use]
    pub fn object_id(&self) -> &ObjectId {
        &self.object_id
    }

    #[must_use]
    pub fn mode(&self) -> &str {
        &self.mode
    }

    #[must_use]
    pub fn kind(&self) -> TreeKind {
        self.kind
    }

    #[must_use]
    pub fn name(&self) -> &RepoPath {
        &self.name
    }
}
