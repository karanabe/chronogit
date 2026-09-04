//! User intent accepted by the reducer and completion events returned by effects.

use crate::app::{RequestId, SearchDirection, VisibleTreeEntry};
use crate::domain::{
    ChangedFile, CommitMessage, CommitSummary, DiffDocument, FileDocument, ObjectId, RepoPath,
    SearchHit, SemanticNavigationKind, SourcePosition, TreeEntry, WorktreeChange,
};
use crate::git::GitError;
use crate::lsp::LspError;

/// A Vim normal-mode cursor or viewport movement.
///
/// The key mapper attaches the optional count and character argument before an
/// action reaches the reducer. Keeping those details here lets Code, diff, and
/// history documents share one motion implementation without depending on
/// terminal events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VimMotion {
    kind: VimMotionKind,
    count: usize,
    explicit_count: bool,
    target: Option<char>,
    repeated: bool,
}

impl VimMotion {
    /// Creates an uncounted motion template for a key binding.
    #[must_use]
    pub const fn new(kind: VimMotionKind) -> Self {
        Self {
            kind,
            count: 1,
            explicit_count: false,
            target: None,
            repeated: false,
        }
    }

    /// Returns the motion operation.
    #[must_use]
    pub const fn kind(self) -> VimMotionKind {
        self.kind
    }

    /// Returns the normalized count (always at least one).
    #[must_use]
    pub const fn count(self) -> usize {
        self.count
    }

    /// Reports whether the user supplied a count rather than using the default.
    #[must_use]
    pub const fn has_explicit_count(self) -> bool {
        self.explicit_count
    }

    /// Returns the character supplied to `f`, `F`, `t`, or `T`.
    #[must_use]
    pub const fn target(self) -> Option<char> {
        self.target
    }

    pub(crate) const fn counted(mut self, count: usize, explicit: bool) -> Self {
        self.count = if count == 0 { 1 } else { count };
        self.explicit_count = explicit;
        self
    }

    pub(crate) const fn targeting(mut self, target: char) -> Self {
        self.target = Some(target);
        self
    }

    pub(crate) const fn repeating(mut self) -> Self {
        self.repeated = true;
        self
    }

    pub(crate) const fn is_repeated(self) -> bool {
        self.repeated
    }
}

/// The movement vocabulary accepted by ChronoGit's read-only Vim normal mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VimMotionKind {
    /// `h` or Left.
    Left,
    /// Backspace or Ctrl-H with Vim's default `'whichwrap'` behavior.
    LeftWrap,
    /// `l` or Right.
    Right,
    /// Space with Vim's default `'whichwrap'` behavior.
    RightWrap,
    /// `k`, Up, or Ctrl-P.
    Up,
    /// `j`, Down, Ctrl-J, Ctrl-N, or newline.
    Down,
    /// `0` or Home.
    LineStart,
    /// `^`.
    FirstNonBlank,
    /// `$` or End.
    LineEnd,
    /// `g_`.
    LastNonBlank,
    /// `g0`.
    ScreenLineStart,
    /// `g^`.
    ScreenFirstNonBlank,
    /// `g$`.
    ScreenLineEnd,
    /// `g<End>`.
    ScreenLastNonBlank,
    /// `gm`.
    ScreenMiddle,
    /// `gM`.
    LineMiddle,
    /// `|`.
    Column,
    /// `go`.
    ByteOffset,
    /// `w`.
    WordForward,
    /// `W`.
    BigWordForward,
    /// `e`.
    WordEndForward,
    /// `E`.
    BigWordEndForward,
    /// `b`.
    WordBackward,
    /// `B`.
    BigWordBackward,
    /// `ge`.
    WordEndBackward,
    /// `gE`.
    BigWordEndBackward,
    /// `f{char}`.
    FindForward,
    /// `F{char}`.
    FindBackward,
    /// `t{char}`.
    TillForward,
    /// `T{char}`.
    TillBackward,
    /// `;` (resolved by the key mapper to the latest character search).
    RepeatCharacterSearch,
    /// `,` (resolved by the key mapper to the reversed character search).
    ReverseCharacterSearch,
    /// `-`.
    PreviousLineFirstNonBlank,
    /// `+` or Enter in a document.
    NextLineFirstNonBlank,
    /// `_`.
    CountedLineFirstNonBlank,
    /// `gg` or Ctrl-Home.
    BufferTop,
    /// `G`.
    BufferBottom,
    /// Ctrl-End.
    BufferBottomEnd,
    /// `{count}%`.
    BufferPercentage,
    /// `H`.
    WindowTop,
    /// `M`.
    WindowMiddle,
    /// `L`.
    WindowBottom,
    /// `(`.
    SentenceBackward,
    /// `)`.
    SentenceForward,
    /// `{`.
    ParagraphBackward,
    /// `}`.
    ParagraphForward,
    /// `[[`.
    SectionStartBackward,
    /// `]]`.
    SectionStartForward,
    /// `[]`.
    SectionEndBackward,
    /// `][`.
    SectionEndForward,
    /// `%` without a count.
    MatchingPair,
    /// `g%`.
    MatchingPairBackward,
    /// `[(` or `[{` with the delimiter supplied as the target.
    UnmatchedOpenBackward,
    /// `])` or `]}` with the delimiter supplied as the target.
    UnmatchedCloseForward,
    /// `[m` / `[M`.
    MethodBackward,
    /// `]m` / `]M`.
    MethodForward,
    /// `[#`.
    PreprocessorBackward,
    /// `]#`.
    PreprocessorForward,
    /// `[*` or `[/`.
    CommentBackward,
    /// `]*` or `]/`.
    CommentForward,
    /// `[c` in a diff.
    DiffChangeBackward,
    /// `]c` in a diff.
    DiffChangeForward,
    /// `n`.
    SearchNext,
    /// `N`.
    SearchPrevious,
    /// `*`.
    SearchWordForward,
    /// `#`.
    SearchWordBackward,
    /// `g*`.
    SearchPartialWordForward,
    /// `g#`.
    SearchPartialWordBackward,
    /// `['`.
    PreviousMarkLine,
    /// `` [` ``.
    PreviousMarkExact,
    /// `]'`.
    NextMarkLine,
    /// `` ]` ``.
    NextMarkExact,
    /// Ctrl-D.
    HalfPageDown,
    /// Ctrl-U.
    HalfPageUp,
    /// Ctrl-F or PageDown.
    PageDown,
    /// Ctrl-B or PageUp.
    PageUp,
    /// Ctrl-E.
    ScrollLineDown,
    /// Ctrl-Y.
    ScrollLineUp,
    /// `zt`.
    CursorToWindowTop,
    /// `z<CR>`.
    CursorToWindowTopFirstNonBlank,
    /// `zz`.
    CursorToWindowMiddle,
    /// `z.`.
    CursorToWindowMiddleFirstNonBlank,
    /// `zb`.
    CursorToWindowBottom,
    /// `z-`.
    CursorToWindowBottomFirstNonBlank,
    /// `z+`.
    NextWindowTop,
    /// `z^`.
    PreviousWindowBottom,
    /// `zh`.
    ScrollColumnLeft,
    /// `zl`.
    ScrollColumnRight,
    /// `zH`.
    ScrollHalfScreenLeft,
    /// `zL`.
    ScrollHalfScreenRight,
    /// `zs`.
    CursorToWindowLeft,
    /// `ze`.
    CursorToWindowRight,
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
        /// Apostrophe jumps are linewise; backtick jumps retain the column.
        linewise: bool,
        /// Plain mark jumps update the jump list; `g'` / `` g` `` do not.
        record_jump: bool,
    },
    /// Open or close language-server hover information at the Code cursor.
    ToggleLspHover,
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
    /// Completed the latest semantic navigation request.
    SemanticNavigationCompleted {
        /// Identifier allocated for the navigation intent.
        request_id: RequestId,
        /// Document selected when the request was sent.
        path: RepoPath,
        /// Cursor selected when the request was sent.
        position: SourcePosition,
        /// Code document generation selected when the request was sent.
        document_revision: u64,
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
        document_revision: u64,
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
