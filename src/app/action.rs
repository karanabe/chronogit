use crate::app::{RequestId, SearchDirection, VisibleTreeEntry};
use crate::domain::{
    ChangedFile, CommitMessage, CommitSummary, DiffDocument, FileDocument, ObjectId, RepoPath,
    SearchHit, TreeEntry, WorktreeChange,
};
use crate::git::GitError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Quit,
    ShowChanges,
    ShowHistory,
    ShowGraph,
    FocusLeft,
    FocusRight,
    MoveUp,
    MoveDown,
    MoveTop,
    MoveBottom,
    HalfPageUp,
    HalfPageDown,
    ScrollLeft,
    ScrollRight,
    Refresh,
    ToggleMessage,
    ToggleDetails,
    ToggleTree,
    OpenFileSearch,
    OpenContentSearch,
    Activate,
    StartSearch(SearchDirection),
    InsertSearch(char),
    DeleteSearch,
    ConfirmSearch,
    CancelSearch,
    NextMatch,
    PreviousMatch,
    ToggleHelp,
    CloseOverlay,
    Tick,
}

#[derive(Debug)]
pub enum Event {
    ChangesLoaded {
        request_id: RequestId,
        result: Result<Vec<WorktreeChange>, GitError>,
    },
    CommitsLoaded {
        request_id: RequestId,
        append: bool,
        limit: usize,
        result: Result<Vec<CommitSummary>, GitError>,
    },
    FilesLoaded {
        request_id: RequestId,
        commit: ObjectId,
        result: Result<Vec<ChangedFile>, GitError>,
    },
    DiffLoaded {
        request_id: RequestId,
        result: Result<DiffDocument, GitError>,
    },
    MessageLoaded {
        request_id: RequestId,
        commit: ObjectId,
        result: Result<CommitMessage, GitError>,
    },
    TreeLoaded {
        request_id: RequestId,
        commit: ObjectId,
        parent: Option<VisibleTreeEntry>,
        result: Result<Vec<TreeEntry>, GitError>,
    },
    RepositorySearchLoaded {
        request_id: RequestId,
        result: Result<Vec<SearchHit>, GitError>,
    },
    FileHistoryLoaded {
        request_id: RequestId,
        path: RepoPath,
        result: Result<Vec<CommitSummary>, GitError>,
    },
    FileContentLoaded {
        request_id: RequestId,
        path: RepoPath,
        result: Result<FileDocument, GitError>,
    },
}
