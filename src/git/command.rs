//! Closed descriptions of the Git reads ChronoGit is allowed to execute.

use std::path::PathBuf;

use crate::domain::{CommitBaseline, ObjectId, RepoPath};

/// A typed, read-only Git invocation.
///
/// Callers cannot provide arbitrary arguments. [`crate::git::GitRunner`]
/// translates only these variants and adds process-wide protections that
/// disable prompts, pager, color, external diff, textconv, and fsmonitor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitCommand {
    /// Resolve the worktree root containing a candidate directory.
    Discover {
        /// Root or descendant path supplied at startup.
        start: PathBuf,
    },
    /// Ask whether the discovered repository is bare.
    IsBare,
    /// Test whether `HEAD` resolves in a repository that may be unborn.
    HasHead,
    /// Read porcelain-v2 worktree status with NUL-delimited paths.
    Status,
    /// Compare one tracked path from the index to the worktree.
    WorktreeDiff {
        /// Repository-relative pathspec.
        path: RepoPath,
    },
    /// Produce a no-index patch for one untracked path.
    UntrackedDiff {
        /// Repository-relative pathspec.
        path: RepoPath,
    },
    /// Read one page of machine-formatted commit summaries.
    Commits {
        /// Number of leading commits to skip.
        skip: usize,
        /// Maximum number of summaries to return.
        limit: usize,
    },
    /// Enumerate tracked and untracked repository paths.
    RepositoryFiles,
    /// Search working-tree contents for fixed text.
    Grep {
        /// Literal query text, passed as a separate argument.
        query: String,
    },
    /// Read commits that touched one path.
    FileHistory {
        /// Repository-relative pathspec.
        path: RepoPath,
        /// Maximum number of summaries to return.
        limit: usize,
    },
    /// Read a commit's complete message.
    CommitMessage {
        /// Validated full object ID used as the revision.
        commit: ObjectId,
    },
    /// List paths changed by a commit relative to an explicit baseline.
    ChangedFiles {
        /// Commit shown on the newer side.
        commit: ObjectId,
        /// Empty tree or first parent shown on the older side.
        baseline: CommitBaseline,
    },
    /// Read a patch for one path in a commit comparison.
    CommitDiff {
        /// Commit shown on the newer side.
        commit: ObjectId,
        /// Empty tree or first parent shown on the older side.
        baseline: CommitBaseline,
        /// Repository-relative pathspec.
        path: RepoPath,
    },
    /// List the direct entries of one commit tree object.
    TreeEntries {
        /// Validated tree or commit object ID.
        treeish: ObjectId,
    },
}

impl GitCommand {
    /// Returns a stable operation name used in diagnostics and errors.
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
            Self::RepositoryFiles => "list repository files",
            Self::Grep { .. } => "search repository content",
            Self::FileHistory { .. } => "read file history",
            Self::CommitMessage { .. } => "read commit message",
            Self::ChangedFiles { .. } => "read changed files",
            Self::CommitDiff { .. } => "read commit diff",
            Self::TreeEntries { .. } => "read tree entries",
        }
    }
}
