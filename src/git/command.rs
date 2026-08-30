use std::path::PathBuf;

use crate::domain::{CommitBaseline, ObjectId, RepoPath};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitCommand {
    Discover {
        start: PathBuf,
    },
    IsBare,
    HasHead,
    Status,
    WorktreeDiff {
        path: RepoPath,
    },
    UntrackedDiff {
        path: RepoPath,
    },
    Commits {
        skip: usize,
        limit: usize,
    },
    CommitMessage {
        commit: ObjectId,
    },
    ChangedFiles {
        commit: ObjectId,
        baseline: CommitBaseline,
    },
    CommitDiff {
        commit: ObjectId,
        baseline: CommitBaseline,
        path: RepoPath,
    },
    TreeEntries {
        treeish: ObjectId,
    },
}

impl GitCommand {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Discover { .. } => "discover repository",
            Self::IsBare => "check bare repository",
            Self::HasHead => "check HEAD",
            Self::Status => "read worktree status",
            Self::WorktreeDiff { .. } => "read worktree diff",
            Self::UntrackedDiff { .. } => "read untracked diff",
            Self::Commits { .. } => "read commit history",
            Self::CommitMessage { .. } => "read commit message",
            Self::ChangedFiles { .. } => "read changed files",
            Self::CommitDiff { .. } => "read commit diff",
            Self::TreeEntries { .. } => "read tree entries",
        }
    }
}
