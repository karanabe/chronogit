//! Read-only access to an installed Git executable.
//!
//! [`GitCommand`] is the closed command allowlist, [`GitRunner`] is the process
//! boundary, and [`GitService`] exposes domain-level repository operations.
//! Individual command output and duration are bounded; the application executor
//! separately bounds concurrent service calls.

mod command;
mod parse;
mod runner;
mod service;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputCompleteness {
    Complete,
    Truncated,
}

impl OutputCompleteness {
    pub(crate) const fn from_truncated(truncated: bool) -> Self {
        if truncated {
            Self::Truncated
        } else {
            Self::Complete
        }
    }

    pub(crate) const fn is_truncated(self) -> bool {
        matches!(self, Self::Truncated)
    }
}

pub use command::GitCommand;
pub use runner::{CommandOutput, GitError, GitRunner, SystemGitRunner};
pub use service::GitService;
