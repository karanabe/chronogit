//! The authoritative application model and its bounded view-specific state.

use std::collections::{HashMap, VecDeque};

use crate::app::{Action, Event, GitEffect, SearchState};
use crate::domain::{
    ChangedFile, CommitMessage, CommitSummary, DiffDocument, DiffTarget, FileDocument, ObjectId,
    RepoPath, RepositoryRoot, SearchHit, TreeEntry, WorktreeChange,
};

const MAX_DIFF_CACHE_ENTRIES: usize = 16;
const MAX_DIFF_CACHE_BYTES: usize = 16 * 1024 * 1024;

/// The main screen currently rendered by the TUI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppView {
    /// Unstaged worktree paths and an index-to-worktree diff.
    Changes,
    /// Commit list, changed files or tree, and selected diff.
    History,
    /// Commit list, full message body, and changed files.
    CommitDetails,
    /// Commit list rendered with parent lanes.
    Graph,
    /// Graph plus changed-file and diff details.
    GraphDetails,
    /// History and current content for one repository path.
    FileHistory,
}

/// The pane receiving navigation actions in the current view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusedPane {
    /// The first list or search-input pane.
    Primary,
    /// The second list, body, or search-results pane.
    Secondary,
    /// The diff or document pane.
    Diff,
}

/// Content displayed in the middle section of the history view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryPanel {
    /// Paths changed relative to the commit baseline.
    ChangedFiles,
    /// Lazily expandable entries from the selected commit tree.
    Tree,
}

/// A modal surface drawn above a main [`AppView`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Overlay {
    /// No modal surface is active.
    None,
    /// Built-in key and navigation help.
    Help,
    /// The selected commit's complete message.
    CommitMessage,
    /// A full-screen diff document.
    Diff,
    /// Repository-wide file-name or content search.
    RepositorySearch,
    /// A full-screen current working-tree file.
    FileContent,
}

/// Repository-wide search mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositorySearchKind {
    /// Match fixed text against tracked and untracked path names.
    Files,
    /// Match fixed text within non-binary working-tree files.
    Content,
}

impl RepositorySearchKind {
    /// Returns the lowercase label shown in the search prompt.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Content => "content",
        }
    }
}

/// A monotonically increasing identifier attached to asynchronous work.
///
/// The value is scoped to one [`AppState`]. Reducers compare it with the current
/// [`LoadState::Loading`] request to discard stale completions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestId(u64);

impl RequestId {
    /// Returns the numeric identifier for executor-side atomic comparison.
    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A sanitized, display-ready recoverable error message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorNotice(String);

impl ErrorNotice {
    /// Creates a notice from boundary-provided text.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// Returns the text shown in the affected pane or footer.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.0
    }
}

/// The lifecycle of one asynchronously loaded resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadState<T> {
    /// No request has been issued for the current resource.
    Idle,
    /// A request is in flight.
    Loading {
        /// Identifier required for a completion to be accepted.
        request_id: RequestId,
    },
    /// Data for the current resource is available.
    Ready(T),
    /// The most recent request failed and may be retried by user action.
    Failed(ErrorNotice),
}

impl<T> LoadState<T> {
    /// Returns the current request identifier only while loading.
    #[must_use]
    pub fn loading_request(&self) -> Option<RequestId> {
        match self {
            Self::Loading { request_id } => Some(*request_id),
            Self::Idle | Self::Ready(_) | Self::Failed(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Selection {
    index: Option<usize>,
}

impl Selection {
    pub(crate) fn new() -> Self {
        Self { index: None }
    }

    pub(crate) fn index(&self) -> Option<usize> {
        self.index
    }

    pub(crate) fn reset(&mut self, len: usize) {
        self.index = (len > 0).then_some(0);
    }

    pub(crate) fn reset_to(&mut self, len: usize, preferred: Option<usize>) {
        self.index = if len == 0 {
            None
        } else {
            Some(preferred.unwrap_or(0).min(len - 1))
        };
    }

    pub(crate) fn clamp(&mut self, len: usize) {
        self.index = match (self.index, len) {
            (_, 0) => None,
            (Some(index), _) => Some(index.min(len - 1)),
            (None, _) => Some(0),
        };
    }

    pub(crate) fn move_by(&mut self, delta: isize, len: usize) -> bool {
        let previous = self.index;
        if len == 0 {
            self.index = None;
            return previous != self.index;
        }
        let current = self.index.unwrap_or(0);
        let next = current.saturating_add_signed(delta).min(len - 1);
        self.index = Some(next);
        previous != self.index
    }

    pub(crate) fn top(&mut self, len: usize) -> bool {
        let previous = self.index;
        self.index = (len > 0).then_some(0);
        previous != self.index
    }

    pub(crate) fn bottom(&mut self, len: usize) -> bool {
        let previous = self.index;
        self.index = len.checked_sub(1);
        previous != self.index
    }
}

/// A flattened commit-tree entry ready for list rendering.
///
/// The underlying [`TreeEntry`] name is relative to its direct parent, while
/// [`VisibleTreeEntry::path`] stores the complete repository-relative path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleTreeEntry {
    entry: TreeEntry,
    path: RepoPath,
    depth: usize,
    expanded: bool,
}

impl VisibleTreeEntry {
    /// Creates a collapsed visible entry at the supplied tree depth.
    #[must_use]
    pub fn new(entry: TreeEntry, path: RepoPath, depth: usize) -> Self {
        Self {
            entry,
            path,
            depth,
            expanded: false,
        }
    }

    /// Returns the direct Git tree entry.
    #[must_use]
    pub fn entry(&self) -> &TreeEntry {
        &self.entry
    }

    /// Returns the complete repository-relative path.
    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// Returns the zero-based nesting depth used for indentation.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Reports whether a directory's loaded children are currently visible.
    #[must_use]
    pub fn expanded(&self) -> bool {
        self.expanded
    }

    pub(crate) fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DiffViewState {
    pub(crate) target: Option<DiffTarget>,
    pub(crate) content: LoadState<DiffDocument>,
    pub(crate) vertical: usize,
    pub(crate) horizontal: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct MessageState {
    pub(crate) commit: Option<ObjectId>,
    pub(crate) content: LoadState<CommitMessage>,
    pub(crate) scroll: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct HistoryPageState {
    pub(crate) has_more: bool,
    pub(crate) loading_more: Option<RequestId>,
}

#[derive(Clone, Debug)]
pub(crate) struct TreeState {
    pub(crate) commit: Option<ObjectId>,
    pub(crate) visible: LoadState<Vec<VisibleTreeEntry>>,
    pub(crate) selection: Selection,
    pub(crate) children: HashMap<ObjectId, Vec<TreeEntry>>,
    pub(crate) pending: Option<RequestId>,
}

#[derive(Debug)]
pub(crate) struct RepositorySearchState {
    pub(crate) kind: RepositorySearchKind,
    pub(crate) prompt: Option<String>,
    pub(crate) query: String,
    pub(crate) results: LoadState<Vec<SearchHit>>,
    pub(crate) selection: Selection,
    pub(crate) return_view: AppView,
}

impl RepositorySearchState {
    fn new(view: AppView) -> Self {
        Self {
            kind: RepositorySearchKind::Files,
            prompt: None,
            query: String::new(),
            results: LoadState::Idle,
            selection: Selection::new(),
            return_view: view,
        }
    }
}

#[derive(Debug)]
pub(crate) struct FileViewState {
    pub(crate) path: Option<RepoPath>,
    pub(crate) commits: LoadState<Vec<CommitSummary>>,
    pub(crate) selection: Selection,
    pub(crate) content: LoadState<FileDocument>,
    pub(crate) showing_history_diff: bool,
    pub(crate) vertical: usize,
    pub(crate) horizontal: usize,
    pub(crate) return_view: AppView,
}

impl FileViewState {
    fn new(view: AppView) -> Self {
        Self {
            path: None,
            commits: LoadState::Idle,
            selection: Selection::new(),
            content: LoadState::Idle,
            showing_history_diff: false,
            vertical: 0,
            horizontal: 0,
            return_view: view,
        }
    }
}

#[derive(Debug)]
struct DiffCache {
    entries: VecDeque<(DiffTarget, DiffDocument, usize)>,
    bytes: usize,
}

impl DiffCache {
    fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
        }
    }

    fn get(&mut self, target: &DiffTarget) -> Option<DiffDocument> {
        let index = self.entries.iter().position(|(key, _, _)| key == target)?;
        let entry = self.entries.remove(index)?;
        let result = entry.1.clone();
        self.entries.push_front(entry);
        Some(result)
    }

    fn insert(&mut self, target: DiffTarget, document: DiffDocument) {
        let size = document.approximate_bytes();
        if size > MAX_DIFF_CACHE_BYTES {
            return;
        }
        if let Some(index) = self.entries.iter().position(|(key, _, _)| key == &target)
            && let Some((_, _, old_size)) = self.entries.remove(index)
        {
            self.bytes = self.bytes.saturating_sub(old_size);
        }
        self.bytes = self.bytes.saturating_add(size);
        self.entries.push_front((target, document, size));
        while self.entries.len() > MAX_DIFF_CACHE_ENTRIES || self.bytes > MAX_DIFF_CACHE_BYTES {
            if let Some((_, _, removed)) = self.entries.pop_back() {
                self.bytes = self.bytes.saturating_sub(removed);
            } else {
                break;
            }
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

/// The complete mutable state consumed by ChronoGit's reducer and renderer.
///
/// State owns selections, load lifecycles, overlay navigation, stale-request
/// tracking, and the bounded diff cache. External callers drive it exclusively
/// through [`AppState::start`], [`AppState::handle_action`], and
/// [`AppState::handle_event`]; fields remain crate-visible so the renderer can
/// borrow them without exposing mutation as a public contract.
#[derive(Debug)]
pub struct AppState {
    pub(crate) root: RepositoryRoot,
    pub(crate) view: AppView,
    pub(crate) focus: FocusedPane,
    pub(crate) history_panel: HistoryPanel,
    pub(crate) overlay: Overlay,
    pub(crate) should_quit: bool,
    pub(crate) changes: LoadState<Vec<WorktreeChange>>,
    pub(crate) change_selection: Selection,
    pub(crate) commits: LoadState<Vec<CommitSummary>>,
    pub(crate) commit_selection: Selection,
    pub(crate) history_page: HistoryPageState,
    pub(crate) files: LoadState<Vec<ChangedFile>>,
    pub(crate) file_selection: Selection,
    pub(crate) diff: DiffViewState,
    pub(crate) message: MessageState,
    pub(crate) tree: TreeState,
    pub(crate) search: SearchState,
    pub(crate) repository_search: RepositorySearchState,
    pub(crate) file_view: FileViewState,
    pub(crate) notice: Option<ErrorNotice>,
    pub(crate) preferred_change: Option<RepoPath>,
    pub(crate) preferred_commit: Option<ObjectId>,
    next_request: u64,
    diff_cache: DiffCache,
}

impl AppState {
    /// Creates an idle application rooted at a discovered repository.
    ///
    /// Call [`AppState::start`] after constructing the matching effect executor
    /// to request data for `view`.
    #[must_use]
    pub fn new(root: RepositoryRoot, view: AppView) -> Self {
        Self {
            root,
            view,
            focus: FocusedPane::Primary,
            history_panel: HistoryPanel::ChangedFiles,
            overlay: Overlay::None,
            should_quit: false,
            changes: LoadState::Idle,
            change_selection: Selection::new(),
            commits: LoadState::Idle,
            commit_selection: Selection::new(),
            history_page: HistoryPageState {
                has_more: false,
                loading_more: None,
            },
            files: LoadState::Idle,
            file_selection: Selection::new(),
            diff: DiffViewState {
                target: None,
                content: LoadState::Idle,
                vertical: 0,
                horizontal: 0,
            },
            message: MessageState {
                commit: None,
                content: LoadState::Idle,
                scroll: 0,
            },
            tree: TreeState {
                commit: None,
                visible: LoadState::Idle,
                selection: Selection::new(),
                children: HashMap::new(),
                pending: None,
            },
            search: SearchState::new(),
            repository_search: RepositorySearchState::new(view),
            file_view: FileViewState::new(view),
            notice: None,
            preferred_change: None,
            preferred_commit: None,
            next_request: 1,
            diff_cache: DiffCache::new(),
        }
    }

    /// Starts initial loading for the configured main view.
    ///
    /// Calling this again issues a fresh request; callers should normally invoke
    /// it once when entering the terminal loop.
    pub fn start(&mut self) -> Vec<GitEffect> {
        match self.view {
            AppView::Changes => self.request_changes(),
            AppView::History | AppView::CommitDetails | AppView::Graph | AppView::GraphDetails => {
                self.request_commits(false)
            }
            AppView::FileHistory => Vec::new(),
        }
    }

    /// Applies semantic user input and returns any resulting repository work.
    pub fn handle_action(&mut self, action: Action) -> Vec<GitEffect> {
        crate::app::update::apply_action(self, action)
    }

    /// Applies an asynchronous completion and returns follow-up repository work.
    ///
    /// Obsolete events are ignored according to their request ID and resource
    /// identity.
    pub fn handle_event(&mut self, event: Event) -> Vec<GitEffect> {
        crate::app::update::apply_event(self, event)
    }

    /// Reports whether the event loop should exit without dispatching new work.
    #[must_use]
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Returns the current main view.
    #[must_use]
    pub fn view(&self) -> AppView {
        self.view
    }

    /// Returns the pane receiving navigation actions.
    #[must_use]
    pub fn focus(&self) -> FocusedPane {
        self.focus
    }

    /// Returns the active modal overlay.
    #[must_use]
    pub fn overlay(&self) -> Overlay {
        self.overlay
    }

    /// Reports whether ordinary character keys should edit a search prompt.
    #[must_use]
    pub fn is_search_input_active(&self) -> bool {
        self.search.is_input_active()
            || (self.overlay == Overlay::RepositorySearch
                && self.repository_search.prompt.is_some())
    }

    pub(crate) fn request_id(&mut self) -> RequestId {
        let id = RequestId(self.next_request);
        self.next_request = self.next_request.saturating_add(1);
        id
    }

    pub(crate) fn request_changes(&mut self) -> Vec<GitEffect> {
        let request_id = self.request_id();
        self.changes = LoadState::Loading { request_id };
        self.notice = None;
        vec![GitEffect::LoadChanges { request_id }]
    }

    pub(crate) fn request_commits(&mut self, append: bool) -> Vec<GitEffect> {
        const PAGE_SIZE: usize = 200;
        let request_id = self.request_id();
        let skip = if append {
            match &self.commits {
                LoadState::Ready(commits) => commits.len(),
                _ => 0,
            }
        } else {
            self.commits = LoadState::Loading { request_id };
            self.history_page.loading_more = None;
            0
        };
        if append {
            self.history_page.loading_more = Some(request_id);
        }
        self.notice = None;
        vec![GitEffect::LoadCommits {
            request_id,
            skip,
            limit: PAGE_SIZE,
            append,
        }]
    }

    pub(crate) fn request_diff(&mut self, target: DiffTarget) -> Vec<GitEffect> {
        self.diff.vertical = 0;
        self.diff.horizontal = 0;
        self.search.clear();
        if let Some(cached) = self.diff_cache.get(&target) {
            self.diff.target = Some(target);
            self.diff.content = LoadState::Ready(cached);
            return Vec::new();
        }
        let request_id = self.request_id();
        self.diff.target = Some(target.clone());
        self.diff.content = LoadState::Loading { request_id };
        vec![GitEffect::LoadDiff { request_id, target }]
    }

    pub(crate) fn cache_diff(&mut self, target: DiffTarget, document: DiffDocument) {
        self.diff_cache.insert(target, document);
    }

    pub(crate) fn clear_cache(&mut self) {
        self.diff_cache.clear();
    }
}
