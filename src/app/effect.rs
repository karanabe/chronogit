//! Bounded asynchronous routing of typed Git reads and optional LSP requests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{Semaphore, mpsc, watch};

use crate::app::{Event, RequestId, VisibleTreeEntry};
use crate::domain::{
    CommitBaseline, DiffTarget, NavigationTarget, ObjectId, RepoPath, RepositoryLocation,
    SemanticNavigationKind, SourcePosition, SourceRange,
};
use crate::git::{GitError, GitRunner, GitService};
use crate::lsp::{LspError, LspManager, WireNavigationTarget, from_lsp_character};

const DIFF_DEBOUNCE: Duration = Duration::from_millis(75);
const REPOSITORY_SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);

/// Any asynchronous work requested by the complete application reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEffect {
    /// A bounded one-shot repository read.
    Git(GitEffect),
    /// Optional persistent language-server work.
    Lsp(LspEffect),
}

impl From<GitEffect> for AppEffect {
    fn from(value: GitEffect) -> Self {
        Self::Git(value)
    }
}

/// A typed operation at the Language Server Protocol boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LspEffect {
    /// Synchronize the current document and request semantic locations.
    Navigate {
        /// Identifier used by cancellation and stale-response checks.
        request_id: RequestId,
        /// Standard semantic operation requested by the user.
        kind: SemanticNavigationKind,
        /// Repository-relative current document.
        path: RepoPath,
        /// Exact complete text currently displayed.
        text: String,
        /// ChronoGit's zero-based UTF-8 byte position.
        position: SourcePosition,
        /// Code document generation used to reject refresh races.
        document_revision: u64,
    },
    /// Synchronize the current document and request hover information.
    Hover {
        /// Identifier used by cancellation and stale-response checks.
        request_id: RequestId,
        /// Repository-relative current document.
        path: RepoPath,
        /// Exact complete text currently displayed.
        text: String,
        /// ChronoGit's zero-based UTF-8 byte position.
        position: SourcePosition,
        /// Code document generation used to reject refresh races.
        document_revision: u64,
    },
}

struct LspPositionRequest {
    request_id: RequestId,
    path: RepoPath,
    text: String,
    position: SourcePosition,
    document_revision: u64,
}

/// A repository operation requested by an application-state transition.
///
/// Effects contain validated domain values rather than arbitrary Git arguments.
/// Their request IDs let both the executor and reducer discard obsolete work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitEffect {
    /// Load unstaged worktree changes.
    LoadChanges {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
    },
    /// Load one page of commit summaries.
    LoadCommits {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Number of leading commits to omit.
        skip: usize,
        /// Maximum number of commits requested.
        limit: usize,
        /// Whether the returned page extends an existing history list.
        append: bool,
    },
    /// Load paths changed by one commit relative to its baseline.
    LoadFiles {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Commit shown on the newer side.
        commit: ObjectId,
        /// Empty tree or first parent shown on the older side.
        baseline: CommitBaseline,
    },
    /// Load a bounded diff document.
    LoadDiff {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Worktree or commit comparison to execute.
        target: DiffTarget,
    },
    /// Load the complete message for one commit.
    LoadMessage {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Commit whose message is requested.
        commit: ObjectId,
    },
    /// Load one directory from a commit tree.
    LoadTree {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Commit whose cached tree state owns the result.
        commit: ObjectId,
        /// Tree object whose direct children are requested.
        treeish: ObjectId,
        /// Visible parent to expand, or `None` for the root tree.
        parent: Option<VisibleTreeEntry>,
    },
    /// Search tracked and untracked repository file names.
    SearchFiles {
        /// Identifier allocated for this query text.
        request_id: RequestId,
        /// Fixed text matched against repository-relative paths.
        query: String,
    },
    /// Search non-binary working-tree contents for fixed text.
    SearchContent {
        /// Identifier allocated for this query text.
        request_id: RequestId,
        /// Fixed text matched against file contents.
        query: String,
    },
    /// Load commits that touched one repository path.
    LoadFileHistory {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Repository-relative path whose history is requested.
        path: RepoPath,
        /// Maximum number of commits requested.
        limit: usize,
    },
    /// Read bounded content from one current working-tree path.
    LoadFileContent {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Repository-relative path to read.
        path: RepoPath,
    },
    /// Load the tracked and untracked paths used by the code-viewer tree.
    LoadCodeTree {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
    },
    /// Read bounded current content for the selected code-viewer file.
    LoadCodeFile {
        /// Identifier used to reject an obsolete response.
        request_id: RequestId,
        /// Repository-relative path to read.
        path: RepoPath,
    },
}

/// Runs [`AppEffect`] values without blocking the terminal event loop.
///
/// Clones share the same two-permit concurrency bound and latest-request slots.
/// Diff and live-search work is briefly debounced; superseded work is discarded
/// before it acquires a permit whenever possible.
#[derive(Debug)]
pub struct EffectExecutor<R> {
    service: Arc<GitService<R>>,
    lsp: Option<LspManager>,
    permits: Arc<Semaphore>,
    latest: Arc<LatestRequests>,
    request_version: watch::Sender<u64>,
}

#[derive(Debug, Default)]
struct LatestRequests {
    changes: AtomicU64,
    commits: AtomicU64,
    files: AtomicU64,
    diff: AtomicU64,
    message: AtomicU64,
    tree: AtomicU64,
    repository_search: AtomicU64,
    file_history: AtomicU64,
    file_content: AtomicU64,
    code_tree: AtomicU64,
    code_file: AtomicU64,
    lsp_request: AtomicU64,
}

impl<R> Clone for EffectExecutor<R> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            lsp: self.lsp.clone(),
            permits: Arc::clone(&self.permits),
            latest: Arc::clone(&self.latest),
            request_version: self.request_version.clone(),
        }
    }
}

impl<R: GitRunner> EffectExecutor<R> {
    /// Creates an executor backed by a discovered repository service.
    #[must_use]
    pub fn new(service: Arc<GitService<R>>) -> Self {
        Self {
            service,
            lsp: None,
            // Two Git reads keep navigation responsive without flooding large repositories.
            permits: Arc::new(Semaphore::new(2)),
            latest: Arc::new(LatestRequests::default()),
            request_version: watch::channel(0).0,
        }
    }

    /// Creates an executor with an explicitly enabled lazy LSP manager.
    #[must_use]
    pub fn with_lsp(service: Arc<GitService<R>>, lsp: LspManager) -> Self {
        service.enable_exact_source();
        let mut executor = Self::new(service);
        executor.lsp = Some(lsp);
        executor
    }

    /// Routes an application effect to the Git worker or LSP manager.
    pub fn dispatch_app(&self, effect: AppEffect, sender: mpsc::Sender<Event>) {
        match effect {
            AppEffect::Git(effect) => self.dispatch(effect, sender),
            AppEffect::Lsp(effect) => self.dispatch_lsp(effect, sender),
        }
    }

    /// Shuts down all optional resident language-server children.
    pub async fn shutdown(&self) {
        if let Some(manager) = &self.lsp {
            manager.shutdown().await;
        }
    }

    fn dispatch_lsp(&self, effect: LspEffect, sender: mpsc::Sender<Event>) {
        match effect {
            LspEffect::Navigate {
                request_id,
                kind,
                path,
                text,
                position,
                document_revision,
            } => self.dispatch_navigation(
                LspPositionRequest {
                    request_id,
                    path,
                    text,
                    position,
                    document_revision,
                },
                kind,
                sender,
            ),
            LspEffect::Hover {
                request_id,
                path,
                text,
                position,
                document_revision,
            } => self.dispatch_hover(
                LspPositionRequest {
                    request_id,
                    path,
                    text,
                    position,
                    document_revision,
                },
                sender,
            ),
        }
    }

    fn dispatch_navigation(
        &self,
        request: LspPositionRequest,
        kind: SemanticNavigationKind,
        sender: mpsc::Sender<Event>,
    ) {
        let LspPositionRequest {
            request_id,
            path,
            text,
            position,
            document_revision,
        } = request;
        self.latest
            .lsp_request
            .store(request_id.value(), Ordering::Release);
        let manager = self.lsp.clone();
        let service = Arc::clone(&self.service);
        let latest = Arc::clone(&self.latest);
        tokio::spawn(async move {
            let result = if let Some(manager) = manager {
                let mut status = manager.subscribe_status();
                let navigation = async {
                    manager.cancel_obsolete(request_id.value()).await;
                    match manager
                        .navigate(request_id.value(), kind, &path, &text, position)
                        .await
                    {
                        Ok(targets) => normalize_targets_async(service, targets).await,
                        Err(error) => Err(error),
                    }
                };
                tokio::pin!(navigation);
                loop {
                    tokio::select! {
                        result = &mut navigation => break result,
                        changed = status.changed() => {
                            if changed.is_err() {
                                continue;
                            }
                            let message = status.borrow_and_update().clone();
                            if latest.lsp_request.load(Ordering::Acquire) == request_id.value()
                                && let Some(message) = message
                            {
                                let _ignored = sender.try_send(Event::LspStatus {
                                    request_id,
                                    message,
                                });
                            }
                        }
                    }
                }
            } else {
                Err(LspError::Disabled(
                    "LSP is disabled; restart with --lsp PROFILE for a trusted repository"
                        .to_owned(),
                ))
            };
            if latest.lsp_request.load(Ordering::Acquire) != request_id.value() {
                return;
            }
            let _ignored = sender
                .send(Event::SemanticNavigationCompleted {
                    request_id,
                    path,
                    position,
                    document_revision,
                    kind,
                    result,
                })
                .await;
        });
    }

    fn dispatch_hover(&self, request: LspPositionRequest, sender: mpsc::Sender<Event>) {
        let LspPositionRequest {
            request_id,
            path,
            text,
            position,
            document_revision,
        } = request;
        self.latest
            .lsp_request
            .store(request_id.value(), Ordering::Release);
        let manager = self.lsp.clone();
        let latest = Arc::clone(&self.latest);
        tokio::spawn(async move {
            let result = if let Some(manager) = manager {
                let mut status = manager.subscribe_status();
                let hover = async {
                    manager.cancel_obsolete(request_id.value()).await;
                    manager
                        .hover(request_id.value(), &path, &text, position)
                        .await
                };
                tokio::pin!(hover);
                loop {
                    tokio::select! {
                        result = &mut hover => break result,
                        changed = status.changed() => {
                            if changed.is_err() {
                                continue;
                            }
                            let message = status.borrow_and_update().clone();
                            if latest.lsp_request.load(Ordering::Acquire) == request_id.value()
                                && let Some(message) = message
                            {
                                let _ignored = sender.try_send(Event::LspStatus {
                                    request_id,
                                    message,
                                });
                            }
                        }
                    }
                }
            } else {
                Err(LspError::Disabled(
                    "LSP is disabled; restart with --lsp PROFILE for a trusted repository"
                        .to_owned(),
                ))
            };
            if latest.lsp_request.load(Ordering::Acquire) != request_id.value() {
                return;
            }
            let _ignored = sender
                .send(Event::LspHoverCompleted {
                    request_id,
                    path,
                    position,
                    document_revision,
                    result,
                })
                .await;
        });
    }

    /// Schedules an effect and sends its typed completion to `sender`.
    ///
    /// Delivery is best-effort when the receiving event loop has already
    /// stopped. Newer effects of the same resource class supersede older ones.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime because execution is spawned
    /// onto the current runtime.
    pub fn dispatch(&self, effect: GitEffect, sender: mpsc::Sender<Event>) {
        effect
            .latest_slot(&self.latest)
            .store(effect.request_id().value(), Ordering::Release);
        self.request_version
            .send_modify(|version| *version = version.saturating_add(1));
        let mut request_changes = self.request_version.subscribe();
        let service = Arc::clone(&self.service);
        let permits = Arc::clone(&self.permits);
        let latest = Arc::clone(&self.latest);
        tokio::spawn(async move {
            let debounce = match &effect {
                GitEffect::LoadDiff { .. } => Some(DIFF_DEBOUNCE),
                GitEffect::SearchFiles { .. } | GitEffect::SearchContent { .. } => {
                    Some(REPOSITORY_SEARCH_DEBOUNCE)
                }
                _ => None,
            };
            if let Some(debounce) = debounce {
                tokio::time::sleep(debounce).await;
            }
            if !effect.is_current(&latest) {
                return;
            }
            let _permit = loop {
                tokio::select! {
                    permit = Arc::clone(&permits).acquire_owned() => {
                        let Ok(permit) = permit else {
                            return;
                        };
                        break permit;
                    }
                    changed = request_changes.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        if !effect.is_current(&latest) {
                            return;
                        }
                    }
                }
            };
            if !effect.is_current(&latest) {
                return;
            }
            let event = execute(service, effect).await;
            let _ignored = sender.send(event).await;
        });
    }
}

impl GitEffect {
    fn request_id(&self) -> RequestId {
        match self {
            Self::LoadChanges { request_id }
            | Self::LoadCommits { request_id, .. }
            | Self::LoadFiles { request_id, .. }
            | Self::LoadDiff { request_id, .. }
            | Self::LoadMessage { request_id, .. }
            | Self::LoadTree { request_id, .. }
            | Self::SearchFiles { request_id, .. }
            | Self::SearchContent { request_id, .. }
            | Self::LoadFileHistory { request_id, .. }
            | Self::LoadFileContent { request_id, .. }
            | Self::LoadCodeTree { request_id }
            | Self::LoadCodeFile { request_id, .. } => *request_id,
        }
    }

    fn latest_slot<'a>(&self, latest: &'a LatestRequests) -> &'a AtomicU64 {
        match self {
            Self::LoadChanges { .. } => &latest.changes,
            Self::LoadCommits { .. } => &latest.commits,
            Self::LoadFiles { .. } => &latest.files,
            Self::LoadDiff { .. } => &latest.diff,
            Self::LoadMessage { .. } => &latest.message,
            Self::LoadTree { .. } => &latest.tree,
            Self::SearchFiles { .. } | Self::SearchContent { .. } => &latest.repository_search,
            Self::LoadFileHistory { .. } => &latest.file_history,
            Self::LoadFileContent { .. } => &latest.file_content,
            Self::LoadCodeTree { .. } => &latest.code_tree,
            Self::LoadCodeFile { .. } => &latest.code_file,
        }
    }

    fn is_current(&self, latest: &LatestRequests) -> bool {
        self.latest_slot(latest).load(Ordering::Acquire) == self.request_id().value()
    }
}

async fn execute<R: GitRunner>(service: Arc<GitService<R>>, effect: GitEffect) -> Event {
    match effect {
        GitEffect::LoadChanges { request_id } => {
            let result = run_blocking(move || service.changes()).await;
            Event::ChangesLoaded { request_id, result }
        }
        GitEffect::LoadCommits {
            request_id,
            skip,
            limit,
            append,
        } => {
            let result = run_blocking(move || service.commits(skip, limit)).await;
            Event::CommitsLoaded {
                request_id,
                append,
                limit,
                result,
            }
        }
        GitEffect::LoadFiles {
            request_id,
            commit,
            baseline,
        } => {
            let event_commit = commit.clone();
            let result = run_blocking(move || service.changed_files(&commit, &baseline)).await;
            Event::FilesLoaded {
                request_id,
                commit: event_commit,
                result,
            }
        }
        GitEffect::LoadDiff { request_id, target } => {
            let result = run_blocking(move || service.diff(&target)).await;
            Event::DiffLoaded { request_id, result }
        }
        GitEffect::LoadMessage { request_id, commit } => {
            let event_commit = commit.clone();
            let result = run_blocking(move || service.commit_message(&commit)).await;
            Event::MessageLoaded {
                request_id,
                commit: event_commit,
                result,
            }
        }
        GitEffect::LoadTree {
            request_id,
            commit,
            treeish,
            parent,
        } => {
            let event_commit = commit.clone();
            let result = run_blocking(move || service.tree_entries(&treeish)).await;
            Event::TreeLoaded {
                request_id,
                commit: event_commit,
                parent,
                result,
            }
        }
        GitEffect::SearchFiles { request_id, query } => {
            let result = run_blocking(move || service.search_files(&query)).await;
            Event::RepositorySearchLoaded { request_id, result }
        }
        GitEffect::SearchContent { request_id, query } => {
            let result = run_blocking(move || service.search_content(&query)).await;
            Event::RepositorySearchLoaded { request_id, result }
        }
        GitEffect::LoadFileHistory {
            request_id,
            path,
            limit,
        } => {
            let event_path = path.clone();
            let result = run_blocking(move || service.file_history(&path, limit)).await;
            Event::FileHistoryLoaded {
                request_id,
                path: event_path,
                result,
            }
        }
        GitEffect::LoadFileContent { request_id, path } => {
            let event_path = path.clone();
            let result = run_blocking(move || service.file_content(&path)).await;
            Event::FileContentLoaded {
                request_id,
                path: event_path,
                result,
            }
        }
        GitEffect::LoadCodeTree { request_id } => {
            let result = run_blocking(move || service.repository_files()).await;
            Event::CodeTreeLoaded { request_id, result }
        }
        GitEffect::LoadCodeFile { request_id, path } => {
            let event_path = path.clone();
            let result = run_blocking(move || service.file_content(&path)).await;
            Event::CodeFileLoaded {
                request_id,
                path: event_path,
                result,
            }
        }
    }
}

async fn run_blocking<T, F>(operation: F) -> Result<T, GitError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, GitError> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| GitError::Unsupported(format!("Git worker failed: {error}")))?
}

async fn normalize_targets_async<R: GitRunner>(
    service: Arc<GitService<R>>,
    targets: Vec<WireNavigationTarget>,
) -> Result<Vec<NavigationTarget>, LspError> {
    tokio::task::spawn_blocking(move || normalize_targets(&service, targets))
        .await
        .map_err(|error| LspError::Process(format!("LSP location worker failed: {error}")))?
}

fn normalize_targets<R: GitRunner>(
    service: &GitService<R>,
    targets: Vec<WireNavigationTarget>,
) -> Result<Vec<NavigationTarget>, LspError> {
    let mut normalized = Vec::new();
    for target in targets {
        let target = match target {
            WireNavigationTarget::External { display_uri } => {
                NavigationTarget::External { display_uri }
            }
            WireNavigationTarget::Repository(location) => {
                let document = service.file_content(&location.path).map_err(|error| {
                    LspError::InvalidDocument(format!(
                        "could not safely read navigation target {}: {error}",
                        location.path.display()
                    ))
                })?;
                let source = document.source().ok_or_else(|| {
                    LspError::InvalidDocument(format!(
                        "navigation target {} is not complete UTF-8 text",
                        location.path.display()
                    ))
                })?;
                let start_line = source_line(source, location.selection.start.line)?;
                let end_line = source_line(source, location.selection.end.line)?;
                let start = SourcePosition::new(
                    location.selection.start.line,
                    from_lsp_character(
                        start_line,
                        location.selection.start.character,
                        location.encoding,
                    )?,
                );
                let end = SourcePosition::new(
                    location.selection.end.line,
                    from_lsp_character(
                        end_line,
                        location.selection.end.character,
                        location.encoding,
                    )?,
                );
                NavigationTarget::Repository(RepositoryLocation::new(
                    location.path,
                    SourceRange::new(start, end),
                ))
            }
        };
        if !normalized.contains(&target) {
            normalized.push(target);
        }
    }
    Ok(normalized)
}

fn source_line(source: &str, line: u32) -> Result<&str, LspError> {
    source
        .split('\n')
        .nth(usize::try_from(line).unwrap_or(usize::MAX))
        .map(|value| value.strip_suffix('\r').unwrap_or(value))
        .ok_or_else(|| {
            LspError::Protocol("language server returned a line outside the target file".to_owned())
        })
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::mpsc;

    use super::{AppEffect, EffectExecutor, GitEffect, LspEffect};
    use crate::app::{Action, AppState, AppView, Event};
    use crate::domain::{DiffTarget, RepoPath, SemanticNavigationKind, SourcePosition};
    use crate::git::{CommandOutput, GitCommand, GitError, GitRunner, GitService};
    use crate::lsp::{LspConfig, LspManager};

    #[derive(Clone, Debug)]
    struct CountingRunner {
        root: PathBuf,
        diff_calls: Arc<AtomicUsize>,
        repository_file_calls: Arc<AtomicUsize>,
    }

    impl GitRunner for CountingRunner {
        fn run(
            &self,
            _root: Option<&crate::domain::RepositoryRoot>,
            command: &GitCommand,
        ) -> Result<CommandOutput, GitError> {
            match command {
                GitCommand::Discover { .. } => {
                    let mut output = self.root.as_os_str().as_bytes().to_vec();
                    output.push(b'\n');
                    Ok(CommandOutput::for_test(true, Some(0), output, Vec::new()))
                }
                GitCommand::IsBare => Ok(CommandOutput::for_test(
                    true,
                    Some(0),
                    b"false\n".to_vec(),
                    Vec::new(),
                )),
                GitCommand::WorktreeDiff { .. } => {
                    self.diff_calls.fetch_add(1, Ordering::AcqRel);
                    Ok(CommandOutput::for_test(
                        true,
                        Some(0),
                        Vec::new(),
                        Vec::new(),
                    ))
                }
                GitCommand::RepositoryFiles => {
                    self.repository_file_calls.fetch_add(1, Ordering::AcqRel);
                    Ok(CommandOutput::for_test(
                        true,
                        Some(0),
                        Vec::new(),
                        Vec::new(),
                    ))
                }
                other => Err(GitError::Unsupported(format!(
                    "unexpected fake command: {}",
                    other.kind()
                ))),
            }
        }
    }

    #[tokio::test]
    async fn rapid_diff_requests_execute_only_the_latest_target() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create fake repository: {error}"));
        let root = directory
            .path()
            .canonicalize()
            .unwrap_or_else(|error| panic!("could not resolve fake repository: {error}"));
        let diff_calls = Arc::new(AtomicUsize::new(0));
        let repository_file_calls = Arc::new(AtomicUsize::new(0));
        let runner = CountingRunner {
            root,
            diff_calls: Arc::clone(&diff_calls),
            repository_file_calls,
        };
        let service = Arc::new(
            GitService::discover(runner, Path::new(directory.path()))
                .unwrap_or_else(|error| panic!("could not create fake Git service: {error}")),
        );
        let mut state = AppState::new(service.root().clone(), AppView::Changes);
        let first = state
            .request_diff(worktree_target(b"first"))
            .pop()
            .unwrap_or_else(|| panic!("expected first diff effect"));
        let second = state
            .request_diff(worktree_target(b"second"))
            .pop()
            .unwrap_or_else(|| panic!("expected second diff effect"));
        let second_id = match &second {
            GitEffect::LoadDiff { request_id, .. } => *request_id,
            _ => panic!("expected diff effect"),
        };
        let executor = EffectExecutor::new(service);
        let (sender, mut receiver) = mpsc::channel(8);

        executor.dispatch(first, sender.clone());
        executor.dispatch(second, sender);

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
            .await
            .unwrap_or_else(|_| panic!("latest diff timed out"))
            .unwrap_or_else(|| panic!("effect channel closed"));
        assert!(matches!(
            event,
            Event::DiffLoaded { request_id, .. } if request_id == second_id
        ));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(diff_calls.load(Ordering::Acquire), 1);
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn rapid_live_search_executes_only_the_latest_query() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create fake repository: {error}"));
        let root = directory
            .path()
            .canonicalize()
            .unwrap_or_else(|error| panic!("could not resolve fake repository: {error}"));
        let repository_file_calls = Arc::new(AtomicUsize::new(0));
        let runner = CountingRunner {
            root,
            diff_calls: Arc::new(AtomicUsize::new(0)),
            repository_file_calls: Arc::clone(&repository_file_calls),
        };
        let service = Arc::new(
            GitService::discover(runner, Path::new(directory.path()))
                .unwrap_or_else(|error| panic!("could not create fake Git service: {error}")),
        );
        let mut state = AppState::new(service.root().clone(), AppView::Changes);
        let _none = state.handle_action(Action::OpenFileSearch);
        let first = state
            .handle_action(Action::InsertSearch('a'))
            .pop()
            .unwrap_or_else(|| panic!("expected first live-search effect"));
        let second = state
            .handle_action(Action::InsertSearch('b'))
            .pop()
            .unwrap_or_else(|| panic!("expected second live-search effect"));
        let second_id = match &second {
            GitEffect::SearchFiles { request_id, .. } => *request_id,
            _ => panic!("expected file-search effect"),
        };
        let executor = EffectExecutor::new(service);
        let (sender, mut receiver) = mpsc::channel(8);

        executor.dispatch(first, sender.clone());
        executor.dispatch(second, sender);

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
            .await
            .unwrap_or_else(|_| panic!("latest live search timed out"))
            .unwrap_or_else(|| panic!("effect channel closed"));
        assert!(matches!(
            event,
            Event::RepositorySearchLoaded { request_id, .. } if request_id == second_id
        ));
        tokio::time::sleep(std::time::Duration::from_millis(125)).await;
        assert_eq!(repository_file_calls.load(Ordering::Acquire), 1);
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn disabled_executor_returns_a_recoverable_lsp_event_without_starting_a_server() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create fake repository: {error}"));
        let root = directory
            .path()
            .canonicalize()
            .unwrap_or_else(|error| panic!("could not resolve fake repository: {error}"));
        let runner = CountingRunner {
            root,
            diff_calls: Arc::new(AtomicUsize::new(0)),
            repository_file_calls: Arc::new(AtomicUsize::new(0)),
        };
        let service = Arc::new(
            GitService::discover(runner, Path::new(directory.path()))
                .unwrap_or_else(|error| panic!("could not create fake Git service: {error}")),
        );
        let mut state = AppState::new(service.root().clone(), AppView::Code);
        let request_id = state.request_id();
        let path = RepoPath::from_bytes(b"src/main.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        let executor = EffectExecutor::new(service);
        let (sender, mut receiver) = mpsc::channel(1);
        executor.dispatch_app(
            AppEffect::Lsp(LspEffect::Navigate {
                request_id,
                kind: SemanticNavigationKind::Definition,
                path: path.clone(),
                text: "fn main() {}\n".to_owned(),
                position: SourcePosition::new(0, 3),
                document_revision: 1,
            }),
            sender.clone(),
        );
        let event = receiver
            .recv()
            .await
            .unwrap_or_else(|| panic!("effect channel closed"));
        assert!(matches!(
            event,
            Event::SemanticNavigationCompleted {
                result: Err(crate::lsp::LspError::Disabled(_)),
                ..
            }
        ));

        let hover_request_id = state.request_id();
        executor.dispatch_app(
            AppEffect::Lsp(LspEffect::Hover {
                request_id: hover_request_id,
                path,
                text: "fn main() {}\n".to_owned(),
                position: SourcePosition::new(0, 3),
                document_revision: 1,
            }),
            sender,
        );
        let event = receiver
            .recv()
            .await
            .unwrap_or_else(|| panic!("effect channel closed"));
        assert!(matches!(
            event,
            Event::LspHoverCompleted {
                result: Err(crate::lsp::LspError::Disabled(_)),
                ..
            }
        ));
    }

    #[test]
    fn exact_source_text_is_retained_only_when_lsp_is_configured() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create fake repository: {error}"));
        std::fs::write(directory.path().join("main.rs"), "fn main() {}\n")
            .unwrap_or_else(|error| panic!("could not write source: {error}"));
        let root = directory
            .path()
            .canonicalize()
            .unwrap_or_else(|error| panic!("could not resolve fake repository: {error}"));
        let runner = CountingRunner {
            root,
            diff_calls: Arc::new(AtomicUsize::new(0)),
            repository_file_calls: Arc::new(AtomicUsize::new(0)),
        };
        let service = Arc::new(
            GitService::discover(runner, Path::new(directory.path()))
                .unwrap_or_else(|error| panic!("could not create fake Git service: {error}")),
        );
        let path = RepoPath::from_bytes(b"main.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        assert!(
            service
                .file_content(&path)
                .unwrap_or_else(|error| panic!("content: {error}"))
                .source()
                .is_none()
        );

        let manager = LspManager::new(service.root().clone(), LspConfig::disabled());
        let _executor = EffectExecutor::with_lsp(Arc::clone(&service), manager);
        assert_eq!(
            service
                .file_content(&path)
                .unwrap_or_else(|error| panic!("LSP content: {error}"))
                .source(),
            Some("fn main() {}\n")
        );
    }

    fn worktree_target(path: &[u8]) -> DiffTarget {
        DiffTarget::Worktree {
            path: RepoPath::from_bytes(path.to_vec())
                .unwrap_or_else(|error| panic!("invalid fake path: {error}")),
            untracked: false,
        }
    }
}
