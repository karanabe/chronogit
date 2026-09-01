use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{Semaphore, mpsc, watch};

use crate::app::{Event, RequestId, VisibleTreeEntry};
use crate::domain::{CommitBaseline, DiffTarget, ObjectId, RepoPath};
use crate::git::{GitError, GitRunner, GitService};

const DIFF_DEBOUNCE: Duration = Duration::from_millis(75);
const REPOSITORY_SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitEffect {
    LoadChanges {
        request_id: RequestId,
    },
    LoadCommits {
        request_id: RequestId,
        skip: usize,
        limit: usize,
        append: bool,
    },
    LoadFiles {
        request_id: RequestId,
        commit: ObjectId,
        baseline: CommitBaseline,
    },
    LoadDiff {
        request_id: RequestId,
        target: DiffTarget,
    },
    LoadMessage {
        request_id: RequestId,
        commit: ObjectId,
    },
    LoadTree {
        request_id: RequestId,
        commit: ObjectId,
        treeish: ObjectId,
        parent: Option<VisibleTreeEntry>,
    },
    SearchFiles {
        request_id: RequestId,
        query: String,
    },
    SearchContent {
        request_id: RequestId,
        query: String,
    },
    LoadFileHistory {
        request_id: RequestId,
        path: RepoPath,
        limit: usize,
    },
    LoadFileContent {
        request_id: RequestId,
        path: RepoPath,
    },
}

#[derive(Debug)]
pub struct EffectExecutor<R> {
    service: Arc<GitService<R>>,
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
}

impl<R> Clone for EffectExecutor<R> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            permits: Arc::clone(&self.permits),
            latest: Arc::clone(&self.latest),
            request_version: self.request_version.clone(),
        }
    }
}

impl<R: GitRunner> EffectExecutor<R> {
    #[must_use]
    pub fn new(service: Arc<GitService<R>>) -> Self {
        Self {
            service,
            // Two Git reads keep navigation responsive without flooding large repositories.
            permits: Arc::new(Semaphore::new(2)),
            latest: Arc::new(LatestRequests::default()),
            request_version: watch::channel(0).0,
        }
    }

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
            | Self::LoadFileContent { request_id, .. } => *request_id,
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

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::mpsc;

    use super::{EffectExecutor, GitEffect};
    use crate::app::{Action, AppState, AppView, Event};
    use crate::domain::{DiffTarget, RepoPath};
    use crate::git::{CommandOutput, GitCommand, GitError, GitRunner, GitService};

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

    fn worktree_target(path: &[u8]) -> DiffTarget {
        DiffTarget::Worktree {
            path: RepoPath::from_bytes(path.to_vec())
                .unwrap_or_else(|error| panic!("invalid fake path: {error}")),
            untracked: false,
        }
    }
}
