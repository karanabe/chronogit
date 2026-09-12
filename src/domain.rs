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
pub use commit::{
    CommitBaseline, CommitMessage, CommitPage, CommitSummary, ObjectId, ObjectIdError,
};
pub use diff::{
    DiffDocument, DiffLine, DiffLineKind, DiffTarget, LineNumber, LineNumberError, WorktreeDiffKind,
};
pub use path::{RepoPath, RepoPathError, RepositoryRoot, RepositoryRootError};
pub use search::{FileDocument, SearchHit, TextFileDocument};
pub use source::{
    DocumentSymbol, DocumentSymbolKind, FileRevision, NavigationTarget, RepositoryLocation,
    SemanticNavigationKind, SourcePosition, SourceRange,
};
pub use tree::{GitTreeMode, GitTreeModeError, TreeEntry, TreeKind};
