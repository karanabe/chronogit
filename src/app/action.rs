//! User intent accepted by the reducer and completion events returned by effects.

use crate::app::{CommitLoadMode, DocumentRevision, RequestId, SearchDirection, VisibleTreeEntry};
use crate::domain::{
    ChangedFile, CommitMessage, CommitPage, CommitSummary, DiffDocument, FileDocument, LocalBranch,
    ObjectId, RepoPath, SearchHit, SemanticNavigationKind, SourcePosition, TreeEntry,
    WorktreeChange,
};
use crate::git::GitError;
use crate::lsp::LspError;
use vim_navigation::Motion as VimMotion;

/// How a Vim mark determines the destination column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkJumpTarget {
    /// Move to the first non-blank byte of the marked line.
    Line,
    /// Preserve the exact marked byte column.
    Exact,
}

impl MarkJumpTarget {
    pub(crate) const fn is_linewise(self) -> bool {
        matches!(self, Self::Line)
    }
}

/// Whether a completed mark jump is added to the shared jump history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JumpHistory {
    /// Add the location left by the jump to history.
    Record,
    /// Move without changing jump history.
    Preserve,
}

impl JumpHistory {
    pub(crate) const fn records_jump(self) -> bool {
        matches!(self, Self::Record)
    }
}

/// A semantic input handled by [`crate::app::AppState`].
///
/// Key bindings map terminal-specific input to these values so state updates do
/// not depend on crossterm events or a particular key layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    /// Leave the application event loop.
    Quit,
    /// Switch to unstaged worktree changes.
    ShowChanges,
    /// Switch to the paged commit-history view.
    ShowHistory,
    /// Switch to the commit graph.
    ShowGraph,
    /// Switch to the working-tree code viewer.
    ShowCode,
    /// Open the local-branch switcher from any view or document overlay.
    OpenBranches,
    /// Move focus to the preceding pane or search input.
    FocusLeft,
    /// Move focus to the following pane or search results.
    FocusRight,
    /// Move the current selection or scroll position up.
    MoveUp,
    /// Move the current selection or scroll position down.
    MoveDown,
    /// Move to the first item or line.
    MoveTop,
    /// Move to the last item or line.
    MoveBottom,
    /// Scroll the active document up by half a page.
    HalfPageUp,
    /// Scroll the active document down by half a page.
    HalfPageDown,
    /// Scroll a diff horizontally to the left.
    ScrollLeft,
    /// Scroll a diff horizontally to the right.
    ScrollRight,
    /// Move the focused Code cursor left, or move to the preceding pane.
    MoveCursorLeft,
    /// Move the focused Code cursor right, or move to the following pane.
    MoveCursorRight,
    /// Apply a count-aware Vim normal-mode movement.
    VimMotion(VimMotion),
    /// Set a Vim mark at the active Code cursor.
    SetVimMark(char),
    /// Jump to a Vim mark, either at its exact column or first non-blank.
    JumpToVimMark {
        /// Mark name supplied after backtick or apostrophe.
        mark: char,
        /// Whether to select the marked line or the exact byte column.
        target: MarkJumpTarget,
        /// Whether the jump updates the shared jump list.
        history: JumpHistory,
    },
    /// Open or close language-server hover information at the Code cursor.
    ToggleLspHover,
    /// Open the LSP document-symbol/context chooser for the active source file.
    OpenSymbolContext,
    /// Open the active diff or file as a complete source document.
    OpenFullFile,
    /// Toggle a complete file between changed-line annotations and plain new state.
    ToggleFullFileMode,
    /// Request one standard semantic target from the enabled language server.
    GoToSemanticTarget(SemanticNavigationKind),
    /// Return to the source location preceding the latest semantic jump.
    GoBackFromSemanticTarget,
    /// Revisit the semantic location most recently left by a backward jump.
    GoForwardFromSemanticTarget,
    /// Move backward through the shared Vim/LSP jump list.
    JumpListBack(usize),
    /// Move forward through the shared Vim/LSP jump list.
    JumpListForward(usize),
    /// Reload data owned by the current view.
    Refresh,
    /// Open or close the selected commit's complete message.
    ToggleMessage,
    /// Toggle between history summary and commit-detail layouts.
    ToggleDetails,
    /// Toggle the middle history pane between changed files and the commit tree.
    ToggleTree,
    /// Open repository-wide file-name search.
    OpenFileSearch,
    /// Open repository-wide fixed-text content search.
    OpenContentSearch,
    /// Activate the selected item; open text documents interpret Enter as `+`.
    Activate,
    /// Begin in-document search in the requested direction.
    StartSearch(SearchDirection),
    /// Append one character to an active search prompt.
    InsertSearch(char),
    /// Delete the last character from an active search prompt.
    /// An already empty in-document prompt is cancelled; repository search stays open.
    DeleteSearch,
    /// Accept an active search prompt or move into repository search results.
    ConfirmSearch,
    /// Cancel the active prompt without closing the surrounding document.
    CancelSearch,
    /// Select the next in-document search match.
    NextMatch,
    /// Select the previous in-document search match.
    PreviousMatch,
    /// Open or close the key help overlay.
    ToggleHelp,
    /// Close the topmost overlay or return one navigation level.
    CloseOverlay,
    /// Dismiss active Diff/Code search highlights, otherwise close/back (default Esc).
    DismissSearchOrClose,
    /// Advance timers used for pending key sequences and deferred work.
    Tick,
}

/// The completion of an asynchronous [`crate::app::GitEffect`].
///
/// Every variant carries the originating [`RequestId`]. The reducer ignores a
/// completion that no longer matches current state, preventing slow work from
/// replacing a newer selection or query.
#[derive(Debug)]
pub enum Event {
    /// Completed local branch enumeration.
    BranchesLoaded {
        /// Identifier allocated when the picker opened or refreshed.
        request_id: RequestId,
        /// Local branches or a recoverable boundary error.
        result: Result<Vec<LocalBranch>, GitError>,
    },
    /// Completed an explicitly requested branch switch.
    BranchSwitched {
        /// Identifier allocated when the selected branch was activated.
        request_id: RequestId,
        /// Git's result; failures never trigger a forced retry.
        result: Result<(), GitError>,
    },
    /// Completed an unstaged-worktree status request.
    ChangesLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Parsed changes or the Git boundary error.
        result: Result<Vec<WorktreeChange>, GitError>,
    },
    /// Completed one page of commit history.
    CommitsLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Whether the page replaces or extends the current history.
        mode: CommitLoadMode,
        /// Requested page, used to detect the end of history.
        page: CommitPage,
        /// Parsed commit summaries or the Git boundary error.
        result: Result<Vec<CommitSummary>, GitError>,
    },
    /// Completed the changed-file list for a selected commit.
    FilesLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Commit that was selected when loading began.
        commit: ObjectId,
        /// Parsed changed files or the Git boundary error.
        result: Result<Vec<ChangedFile>, GitError>,
    },
    /// Completed a worktree or commit diff request.
    DiffLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Bounded diff document or the Git boundary error.
        result: Result<DiffDocument, GitError>,
    },
    /// Completed a full commit-message request.
    MessageLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Commit that was selected when loading began.
        commit: ObjectId,
        /// Complete message or the Git boundary error.
        result: Result<CommitMessage, GitError>,
    },
    /// Completed one lazy commit-tree directory request.
    TreeLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Commit whose tree is being expanded.
        commit: ObjectId,
        /// Visible directory receiving the children, or `None` for the root.
        parent: Option<VisibleTreeEntry>,
        /// Direct tree children or the Git boundary error.
        result: Result<Vec<TreeEntry>, GitError>,
    },
    /// Completed the latest repository file or content search.
    RepositorySearchLoaded {
        /// Identifier allocated when the query changed.
        request_id: RequestId,
        /// Bounded search results or the Git boundary error.
        result: Result<Vec<SearchHit>, GitError>,
    },
    /// Completed history loading for one repository path.
    FileHistoryLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Path that was selected when loading began.
        path: RepoPath,
        /// Commit summaries touching the file or the Git boundary error.
        result: Result<Vec<CommitSummary>, GitError>,
    },
    /// Completed a bounded current-file read.
    FileContentLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Path that was selected when loading began.
        path: RepoPath,
        /// Typed file document or the filesystem/Git boundary error.
        result: Result<FileDocument, GitError>,
    },
    /// Completed the working-tree file list used by the code viewer.
    CodeTreeLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Repository-relative file paths or the Git boundary error.
        result: Result<Vec<RepoPath>, GitError>,
    },
    /// Completed a bounded code-viewer file read.
    CodeFileLoaded {
        /// Identifier allocated when the request began.
        request_id: RequestId,
        /// Path that was selected when loading began.
        path: RepoPath,
        /// Typed file document or the filesystem/Git boundary error.
        result: Result<FileDocument, GitError>,
    },
    /// Completed a full working-tree or historical source-file read.
    FullFileLoaded {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Snapshot selected when loading began.
        revision: crate::domain::FileRevision,
        /// Repository path selected when loading began.
        path: RepoPath,
        /// Typed source document or Git/filesystem boundary error.
        result: Result<FileDocument, GitError>,
    },
    /// Completed the latest document-symbol request.
    DocumentSymbolsCompleted {
        /// Identifier allocated for the symbol-list intent.
        request_id: RequestId,
        /// Document selected when the request was sent.
        path: RepoPath,
        /// Full-file document generation used to reject refresh races.
        document_revision: DocumentRevision,
        /// Flattened symbols using UTF-8 byte source positions.
        result: Result<Vec<crate::domain::DocumentSymbol>, LspError>,
    },
    /// Completed the latest semantic navigation request.
    SemanticNavigationCompleted {
        /// Identifier allocated for the navigation intent.
        request_id: RequestId,
        /// Document selected when the request was sent.
        path: RepoPath,
        /// Cursor selected when the request was sent.
        position: SourcePosition,
        /// Code document generation selected when the request was sent.
        document_revision: DocumentRevision,
        /// Requested standard navigation operation.
        kind: SemanticNavigationKind,
        /// Normalized repository or explicitly unsupported targets.
        result: Result<Vec<crate::domain::NavigationTarget>, LspError>,
    },
    /// Completed the latest language-server hover request.
    LspHoverCompleted {
        /// Identifier allocated for the hover intent.
        request_id: RequestId,
        /// Document selected when the request was sent.
        path: RepoPath,
        /// Cursor selected when the request was sent.
        position: SourcePosition,
        /// Code document generation selected when the request was sent.
        document_revision: DocumentRevision,
        /// Plain or Markdown-formatted hover text, when the server has any.
        result: Result<Option<String>, LspError>,
    },
    /// Bounded status text from the server handling the current LSP request.
    LspStatus {
        /// LSP request for which the status is relevant.
        request_id: RequestId,
        /// Sanitized server progress or log text.
        message: String,
    },
}
