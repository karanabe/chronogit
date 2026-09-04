//! Bounded profile/workspace session ownership and URI containment.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::future::join_all;
use tokio::sync::{Mutex, watch};
use url::Url;

use crate::domain::{RepoPath, RepositoryRoot, SemanticNavigationKind, SourcePosition};
use crate::lsp::config::LspConfig;
use crate::lsp::session::{RawLocation, Session, WireRange};
use crate::lsp::{LspError, PositionEncoding};

const MAX_SESSIONS: usize = 4;

/// A repository-contained wire location awaiting safe document conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WireRepositoryLocation {
    pub(crate) path: RepoPath,
    pub(crate) selection: WireRange,
    pub(crate) encoding: PositionEncoding,
}

/// A normalized result that never treats a non-repository URI as a path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WireNavigationTarget {
    Repository(WireRepositoryLocation),
    External { display_uri: String },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SessionKey {
    profile: String,
    workspace_root: PathBuf,
}

struct SessionEntry {
    session: Arc<Session>,
    last_used: u64,
}

#[derive(Default)]
struct ManagerState {
    sessions: HashMap<SessionKey, SessionEntry>,
    clock: u64,
    shutting_down: bool,
}

/// Lazily starts and independently owns bounded language-server sessions.
///
/// Sessions are keyed by profile and detected workspace root. At most four are
/// retained; starting a fifth cleanly evicts the least recently used session.
#[derive(Clone)]
pub struct LspManager {
    repository: RepositoryRoot,
    config: LspConfig,
    cache_dir: PathBuf,
    state: Arc<Mutex<ManagerState>>,
    startup: Arc<Mutex<()>>,
    latest_request: Arc<AtomicU64>,
    status: watch::Sender<Option<String>>,
}

impl std::fmt::Debug for LspManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LspManager")
            .field("repository", &self.repository)
            .field("profiles", &self.config.profiles().len())
            .finish_non_exhaustive()
    }
}

impl LspManager {
    /// Creates an idle manager. No external process is started by construction.
    #[must_use]
    pub fn new(repository: RepositoryRoot, config: LspConfig) -> Self {
        let (status, _status_receiver) = watch::channel(None);
        Self {
            repository,
            config,
            cache_dir: cache_dir(),
            state: Arc::new(Mutex::new(ManagerState::default())),
            startup: Arc::new(Mutex::new(())),
            latest_request: Arc::new(AtomicU64::new(0)),
            status,
        }
    }

    /// Reports whether the user enabled no profiles.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.config.is_disabled()
    }

    pub(crate) fn subscribe_status(&self) -> watch::Receiver<Option<String>> {
        self.status.subscribe()
    }

    pub(crate) async fn navigate(
        &self,
        request_id: u64,
        kind: SemanticNavigationKind,
        path: &RepoPath,
        text: &str,
        position: SourcePosition,
    ) -> Result<Vec<WireNavigationTarget>, LspError> {
        let (key, session) = self.session_for(request_id, path).await?;
        // Several input tasks can wait behind one expensive initialization, but
        // only the newest intent may send a request after that wait finishes.
        if self.latest_request.load(Ordering::Acquire) != request_id {
            return Ok(Vec::new());
        }
        let document_path = self.repository.as_path().join(path.to_os_string());
        let response = session
            .navigate(request_id, kind, &document_path, text, position)
            .await;
        if matches!(
            response,
            Err(LspError::Process(_) | LspError::Protocol(_) | LspError::Timeout(_))
        ) {
            self.remove_if_same(&key, &session).await;
        }
        let locations = response?;
        Ok(locations
            .into_iter()
            .map(|location| self.contain(location, session.encoding()))
            .collect())
    }

    pub(crate) async fn hover(
        &self,
        request_id: u64,
        path: &RepoPath,
        text: &str,
        position: SourcePosition,
    ) -> Result<Option<String>, LspError> {
        let (key, session) = self.session_for(request_id, path).await?;
        if self.latest_request.load(Ordering::Acquire) != request_id {
            return Ok(None);
        }
        let document_path = self.repository.as_path().join(path.to_os_string());
        let response = session
            .hover(request_id, &document_path, text, position)
            .await;
        if matches!(
            response,
            Err(LspError::Process(_) | LspError::Protocol(_) | LspError::Timeout(_))
        ) {
            self.remove_if_same(&key, &session).await;
        }
        response
    }

    pub(crate) async fn cancel_obsolete(&self, request_id: u64) {
        let sessions = self
            .state
            .lock()
            .await
            .sessions
            .values()
            .map(|entry| Arc::clone(&entry.session))
            .collect::<Vec<_>>();
        join_all(
            sessions
                .iter()
                .map(|session| session.cancel_obsolete(request_id)),
        )
        .await;
    }

    /// Cleanly stops every resident server, killing children after a deadline.
    pub async fn shutdown(&self) {
        {
            let mut state = self.state.lock().await;
            state.shutting_down = true;
        }
        // Wait for a process currently between spawn and insertion. Queued
        // startups observe `shutting_down` and return without spawning.
        let _startup = self.startup.lock().await;
        let sessions = {
            let mut state = self.state.lock().await;
            state
                .sessions
                .drain()
                .map(|(_, entry)| entry.session)
                .collect::<Vec<_>>()
        };
        join_all(sessions.iter().map(|session| session.shutdown())).await;
    }

    async fn ensure_running(&self) -> Result<(), LspError> {
        if self.state.lock().await.shutting_down {
            Err(LspError::Process(
                "language-server manager is shutting down".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    async fn session_for(
        &self,
        request_id: u64,
        path: &RepoPath,
    ) -> Result<(SessionKey, Arc<Session>), LspError> {
        self.latest_request.fetch_max(request_id, Ordering::Release);
        self.ensure_running().await?;
        let profile = self.config.profile_for_path(path)?;
        let workspace_root = profile.workspace_root(&self.repository, path);
        let key = SessionKey {
            profile: profile.id().to_owned(),
            workspace_root: workspace_root.clone(),
        };
        if let Some(session) = self.existing_session(&key).await {
            return Ok((key, session));
        }
        // Startup is serialized so rapid intents cannot temporarily create an
        // unbounded number of heavyweight external processes.
        let _startup = self.startup.lock().await;
        self.ensure_running().await?;
        if let Some(session) = self.existing_session(&key).await {
            return Ok((key, session));
        }
        self.make_room().await;
        self.status
            .send_replace(Some(format!("starting {}", profile.id())));
        let started = Session::start(
            profile,
            workspace_root,
            &self.cache_dir,
            self.status.clone(),
        )
        .await?;
        let session = self.insert_session(key.clone(), started).await;
        Ok((key, session))
    }

    async fn existing_session(&self, key: &SessionKey) -> Option<Arc<Session>> {
        let mut state = self.state.lock().await;
        state.clock = state.clock.saturating_add(1);
        let tick = state.clock;
        let entry = state.sessions.get_mut(key)?;
        entry.last_used = tick;
        Some(Arc::clone(&entry.session))
    }

    async fn insert_session(&self, key: SessionKey, started: Arc<Session>) -> Arc<Session> {
        let evicted;
        let result;
        {
            let mut state = self.state.lock().await;
            state.clock = state.clock.saturating_add(1);
            let tick = state.clock;
            if let Some(existing) = state.sessions.get_mut(&key) {
                existing.last_used = tick;
                result = Arc::clone(&existing.session);
                evicted = Some(started);
            } else {
                evicted = if state.sessions.len() >= MAX_SESSIONS {
                    let oldest = state
                        .sessions
                        .iter()
                        .min_by_key(|(_, entry)| entry.last_used)
                        .map(|(key, _)| key.clone());
                    oldest.and_then(|key| state.sessions.remove(&key).map(|entry| entry.session))
                } else {
                    None
                };
                state.sessions.insert(
                    key,
                    SessionEntry {
                        session: Arc::clone(&started),
                        last_used: tick,
                    },
                );
                result = started;
            }
        }
        if let Some(session) = evicted {
            tokio::spawn(async move { session.shutdown().await });
        }
        result
    }

    async fn make_room(&self) {
        let removed = {
            let mut state = self.state.lock().await;
            if state.sessions.len() < MAX_SESSIONS {
                None
            } else {
                let oldest = state
                    .sessions
                    .iter()
                    .min_by_key(|(_, entry)| entry.last_used)
                    .map(|(key, _)| key.clone());
                oldest.and_then(|key| state.sessions.remove(&key).map(|entry| entry.session))
            }
        };
        if let Some(session) = removed {
            session.shutdown().await;
        }
    }

    async fn remove_if_same(&self, key: &SessionKey, failed: &Arc<Session>) {
        let removed = {
            let mut state = self.state.lock().await;
            if state
                .sessions
                .get(key)
                .is_some_and(|entry| Arc::ptr_eq(&entry.session, failed))
            {
                state.sessions.remove(key).map(|entry| entry.session)
            } else {
                None
            }
        };
        if let Some(session) = removed {
            tokio::spawn(async move { session.shutdown().await });
        }
    }

    fn contain(&self, location: RawLocation, encoding: PositionEncoding) -> WireNavigationTarget {
        let external = || WireNavigationTarget::External {
            display_uri: sanitize_uri(&location.uri),
        };
        let Ok(uri) = Url::parse(&location.uri) else {
            return external();
        };
        if uri.scheme() != "file" {
            return external();
        }
        let Ok(path) = uri.to_file_path() else {
            return external();
        };
        let Ok(relative) = path.strip_prefix(self.repository.as_path()) else {
            return external();
        };
        let Some(path) = repo_path(relative) else {
            return external();
        };
        WireNavigationTarget::Repository(WireRepositoryLocation {
            path,
            selection: location.selection,
            encoding,
        })
    }
}

#[cfg(unix)]
fn repo_path(path: &Path) -> Option<RepoPath> {
    use std::os::unix::ffi::OsStrExt;

    RepoPath::from_bytes(path.as_os_str().as_bytes().to_vec()).ok()
}

#[cfg(not(unix))]
fn repo_path(path: &Path) -> Option<RepoPath> {
    let value = path.to_str()?.replace('\\', "/");
    RepoPath::from_bytes(value.into_bytes()).ok()
}

fn sanitize_uri(uri: &str) -> String {
    uri.chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect()
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|home| home.join(".cache"))
        })
        .unwrap_or_else(std::env::temp_dir)
        .join("chronogit/lsp")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::LspManager;
    use crate::domain::{RepoPath, RepositoryRoot, SemanticNavigationKind, SourcePosition};
    use crate::lsp::LspConfig;
    use crate::lsp::session::{RawLocation, WirePosition, WireRange};

    fn manager() -> LspManager {
        LspManager::new(
            RepositoryRoot::new(PathBuf::from("/repo"))
                .unwrap_or_else(|error| panic!("root: {error}")),
            LspConfig::disabled(),
        )
    }

    fn location(uri: &str) -> RawLocation {
        RawLocation {
            uri: uri.to_owned(),
            selection: WireRange {
                start: WirePosition {
                    line: 1,
                    character: 2,
                },
                end: WirePosition {
                    line: 1,
                    character: 3,
                },
            },
        }
    }

    #[test]
    fn contains_only_repository_file_uris() {
        let manager = manager();
        assert!(matches!(
            manager.contain(
                location("file:///repo/src/main.rs"),
                crate::lsp::PositionEncoding::Utf16
            ),
            super::WireNavigationTarget::Repository(_)
        ));
        for uri in [
            "file:///outside/main.rs",
            "jdt://contents/String.class",
            "https://example.test/x",
        ] {
            assert!(matches!(
                manager.contain(location(uri), crate::lsp::PositionEncoding::Utf16),
                super::WireNavigationTarget::External { .. }
            ));
        }
    }

    #[tokio::test]
    async fn shutdown_rejects_late_navigation_without_starting_a_server() {
        let manager = manager();
        manager.shutdown().await;
        let path = RepoPath::from_bytes(b"src/main.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        assert!(matches!(
            manager
                .navigate(
                    1,
                    SemanticNavigationKind::Definition,
                    &path,
                    "fn main() {}\n",
                    SourcePosition::new(0, 3),
                )
                .await,
            Err(crate::lsp::LspError::Process(_))
        ));
    }

    #[tokio::test]
    async fn missing_server_is_a_recoverable_request_error() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        let config_path = directory.path().join("lsp.toml");
        fs::write(
            &config_path,
            "[servers.missing]\nlanguage_id='missing'\nextensions=['missing']\ncommand=['/definitely/missing/chronogit-language-server']\n",
        )
        .unwrap_or_else(|error| panic!("config: {error}"));
        let config = LspConfig::load(&["missing".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"));
        let root =
            fs::canonicalize(directory.path()).unwrap_or_else(|error| panic!("root: {error}"));
        let mut manager = LspManager::new(
            RepositoryRoot::new(root).unwrap_or_else(|error| panic!("root value: {error}")),
            config,
        );
        manager.cache_dir = directory.path().join("cache");
        let path = RepoPath::from_bytes(b"main.missing".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        let error = match manager
            .navigate(
                1,
                SemanticNavigationKind::Definition,
                &path,
                "symbol\n",
                SourcePosition::new(0, 0),
            )
            .await
        {
            Err(error) => error,
            Ok(targets) => panic!("missing executable unexpectedly returned {targets:?}"),
        };
        assert!(matches!(error, crate::lsp::LspError::Process(_)));
    }

    #[tokio::test]
    #[ignore = "requires rust-analyzer on PATH and is an optional interoperability smoke test"]
    async fn rust_analyzer_definition_smoke_test() {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        let config_path = config_dir.path().join("lsp.toml");
        fs::write(&config_path, "[servers]\n").unwrap_or_else(|error| panic!("config: {error}"));
        let config = LspConfig::load(&["rust-analyzer".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"));
        let mut manager = LspManager::new(
            RepositoryRoot::new(repository.clone()).unwrap_or_else(|error| panic!("root: {error}")),
            config,
        );
        manager.cache_dir = config_dir.path().join("cache");
        let path = RepoPath::from_bytes(b"src/main.rs".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        let source = fs::read_to_string(repository.join("src/main.rs"))
            .unwrap_or_else(|error| panic!("source: {error}"));
        let (line, byte_column) = source
            .lines()
            .enumerate()
            .find_map(|(line, source_line)| {
                source_line
                    .find("LspConfig::load")
                    .map(|column| (line, column))
            })
            .unwrap_or_else(|| panic!("could not find smoke-test symbol"));
        let mut targets = Vec::new();
        for request_id in 1..=20 {
            match manager
                .navigate(
                    request_id,
                    SemanticNavigationKind::Definition,
                    &path,
                    &source,
                    SourcePosition::new(u32::try_from(line).unwrap_or(u32::MAX), byte_column),
                )
                .await
            {
                Ok(found) => targets = found,
                Err(error) => panic!("definition: {error}"),
            }
            if !targets.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        assert!(
            targets
                .iter()
                .any(|target| matches!(target, super::WireNavigationTarget::Repository(_))),
            "unexpected targets: {targets:?}"
        );
        let hover = manager
            .hover(
                21,
                &path,
                &source,
                SourcePosition::new(u32::try_from(line).unwrap_or(u32::MAX), byte_column),
            )
            .await
            .unwrap_or_else(|error| panic!("hover: {error}"));
        assert!(
            hover.is_some(),
            "rust-analyzer returned no hover information"
        );
        manager.shutdown().await;
    }

    #[tokio::test]
    #[ignore = "requires pyright-langserver on PATH and is an optional interoperability smoke test"]
    async fn pyright_definition_smoke_test() {
        let repository = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        fs::write(
            repository.path().join("pyrightconfig.json"),
            "{\"include\":[\".\"]}\n",
        )
        .unwrap_or_else(|error| panic!("pyright config: {error}"));
        fs::write(
            repository.path().join("helper.py"),
            "def greeting(name: str) -> str:\n    return f\"Hello, {name}\"\n",
        )
        .unwrap_or_else(|error| panic!("helper: {error}"));
        let source = "from helper import greeting\n\nprint(greeting(\"ChronoGit\"))\n";
        fs::write(repository.path().join("main.py"), source)
            .unwrap_or_else(|error| panic!("main: {error}"));
        let config_path = repository.path().join("lsp.toml");
        fs::write(&config_path, "[servers]\n").unwrap_or_else(|error| panic!("config: {error}"));
        let config = LspConfig::load(&["pyright".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"));
        let root =
            fs::canonicalize(repository.path()).unwrap_or_else(|error| panic!("root: {error}"));
        let mut manager = LspManager::new(
            RepositoryRoot::new(root).unwrap_or_else(|error| panic!("root value: {error}")),
            config,
        );
        manager.cache_dir = repository.path().join("cache");
        let path = RepoPath::from_bytes(b"main.py".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        let byte_column = source
            .lines()
            .nth(2)
            .and_then(|line| line.find("greeting"))
            .unwrap_or_else(|| panic!("could not find smoke-test symbol"));
        let mut targets = Vec::new();
        for request_id in 1..=20 {
            targets = manager
                .navigate(
                    request_id,
                    SemanticNavigationKind::Definition,
                    &path,
                    source,
                    SourcePosition::new(2, byte_column),
                )
                .await
                .unwrap_or_else(|error| panic!("definition: {error}"));
            if targets.iter().any(|target| {
                matches!(
                    target,
                    super::WireNavigationTarget::Repository(location)
                        if location.path.as_bytes() == b"helper.py"
                )
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        assert!(
            targets.iter().any(|target| {
                matches!(
                    target,
                    super::WireNavigationTarget::Repository(location)
                        if location.path.as_bytes() == b"helper.py"
                )
            }),
            "unexpected targets: {targets:?}"
        );
        let hover = manager
            .hover(21, &path, source, SourcePosition::new(2, byte_column))
            .await
            .unwrap_or_else(|error| panic!("hover: {error}"));
        assert!(hover.is_some(), "Pyright returned no hover information");
        manager.shutdown().await;
    }

    #[tokio::test]
    #[ignore = "requires jdtls on PATH and is an optional interoperability smoke test"]
    async fn jdtls_definition_smoke_test() {
        let repository = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        fs::write(
            repository.path().join("Helper.java"),
            "final class Helper { static String greeting() { return \"hello\"; } }\n",
        )
        .unwrap_or_else(|error| panic!("helper: {error}"));
        let source = "final class Main { String value = Helper.greeting(); }\n";
        fs::write(repository.path().join("Main.java"), source)
            .unwrap_or_else(|error| panic!("main: {error}"));
        let config_path = repository.path().join("lsp.toml");
        fs::write(&config_path, "[servers]\n").unwrap_or_else(|error| panic!("config: {error}"));
        let config = LspConfig::load(&["jdtls".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"));
        let root =
            fs::canonicalize(repository.path()).unwrap_or_else(|error| panic!("root: {error}"));
        let mut manager = LspManager::new(
            RepositoryRoot::new(root).unwrap_or_else(|error| panic!("root value: {error}")),
            config,
        );
        manager.cache_dir = repository.path().join("cache");
        let path = RepoPath::from_bytes(b"Main.java".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        let byte_column = source
            .lines()
            .next()
            .and_then(|line| line.find("Helper"))
            .unwrap_or_else(|| panic!("could not find smoke-test symbol"));
        let mut targets = Vec::new();
        for request_id in 1..=30 {
            targets = manager
                .navigate(
                    request_id,
                    SemanticNavigationKind::Definition,
                    &path,
                    source,
                    SourcePosition::new(0, byte_column),
                )
                .await
                .unwrap_or_else(|error| panic!("definition: {error}"));
            if targets.iter().any(|target| {
                matches!(
                    target,
                    super::WireNavigationTarget::Repository(location)
                        if location.path.as_bytes() == b"Helper.java"
                )
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        assert!(
            targets.iter().any(|target| {
                matches!(
                    target,
                    super::WireNavigationTarget::Repository(location)
                        if location.path.as_bytes() == b"Helper.java"
                )
            }),
            "unexpected targets: {targets:?}"
        );
        let hover = manager
            .hover(31, &path, source, SourcePosition::new(0, byte_column))
            .await
            .unwrap_or_else(|error| panic!("hover: {error}"));
        assert!(hover.is_some(), "JDT LS returned no hover information");
        manager.shutdown().await;
    }
}
