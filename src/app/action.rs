//! User intent accepted by the reducer and completion events returned by effects.

use crate::app::{RequestId, SearchDirection, VisibleTreeEntry};
use crate::domain::{
    ChangedFile, CommitMessage, CommitSummary, DiffDocument, FileDocument, ObjectId, RepoPath,
    SearchHit, TreeEntry, WorktreeChange,
};
use crate::git::GitError;

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
    /// Activate the selected item or close a full-screen document.
    Activate,
    /// Begin in-document search in the requested direction.
    StartSearch(SearchDirection),
    /// Append one character to an active search prompt.
    InsertSearch(char),
    /// Delete the last character from an active search prompt.
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
        /// Whether the reducer should append rather than replace the page.
        append: bool,
        /// Requested page size, used to detect the end of history.
        limit: usize,
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
}
