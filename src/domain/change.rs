use crate::domain::RepoPath;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChangeKind {
    Modified,
    Added,
    Untracked,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
}

impl ChangeKind {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeChange {
    path: RepoPath,
    original_path: Option<RepoPath>,
    kind: ChangeKind,
}

impl WorktreeChange {
    #[must_use]
    pub fn new(path: RepoPath, original_path: Option<RepoPath>, kind: ChangeKind) -> Self {
        Self {
            path,
            original_path,
            kind,
        }
    }

    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    #[must_use]
    pub fn original_path(&self) -> Option<&RepoPath> {
        self.original_path.as_ref()
    }

    #[must_use]
    pub fn kind(&self) -> ChangeKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedFile {
    path: RepoPath,
    original_path: Option<RepoPath>,
    kind: ChangeKind,
}

impl ChangedFile {
    #[must_use]
    pub fn new(path: RepoPath, original_path: Option<RepoPath>, kind: ChangeKind) -> Self {
        Self {
            path,
            original_path,
            kind,
        }
    }

    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    #[must_use]
    pub fn original_path(&self) -> Option<&RepoPath> {
        self.original_path.as_ref()
    }

    #[must_use]
    pub fn kind(&self) -> ChangeKind {
        self.kind
    }
}
