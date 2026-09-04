//! Validated, I/O-free values shared by the application and Git adapter.
//!
//! Domain types keep repository paths as bytes where the platform permits,
//! distinguish commit baselines explicitly, and represent diff/file outcomes
//! as enums so callers cannot confuse text, binary, empty, and truncated data.

mod change;
mod commit;
mod diff;
mod path;
mod search;
mod source;
mod tree;

pub use change::{ChangeKind, ChangedFile, WorktreeChange};
pub use commit::{CommitBaseline, CommitMessage, CommitSummary, ObjectId};
pub use diff::{DiffDocument, DiffLine, DiffLineKind, DiffTarget, LineNumber};
pub use path::{RepoPath, RepositoryRoot};
pub use search::{FileDocument, SearchHit};
pub use source::{
    NavigationTarget, RepositoryLocation, SemanticNavigationKind, SourcePosition, SourceRange,
};
pub use tree::{TreeEntry, TreeKind};
