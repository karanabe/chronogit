//! One initialized language-server process and its synchronized document.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use url::Url;

use crate::domain::{
    DocumentSymbol, DocumentSymbolKind, SemanticNavigationKind, SourcePosition, SourceRange,
};
use crate::lsp::config::ServerProfile;
use crate::lsp::position::{PositionEncoding, from_lsp_character, to_lsp_character};
use crate::lsp::protocol::{read_message, write_message};
use crate::lsp::{LspError, LspOperationId};

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const WRITER_QUEUE: usize = 64;
const STDERR_LIMIT: usize = 16 * 1024;
const STATUS_LIMIT: usize = 256;
const HOVER_LIMIT: usize = 256 * 1024;
const SYMBOL_LIMIT: usize = 10_000;
const SYMBOL_DEPTH_LIMIT: usize = 64;
const FIRST_PROTOCOL_REQUEST_ID: i64 = 1;
const CONTENT_MODIFIED_ERROR_CODE: i64 = -32_801;
const CONTENT_MODIFIED_ATTEMPTS: usize = 2;
const CONTENT_MODIFIED_RETRY_DELAY: Duration = Duration::from_millis(50);
const DIAGNOSTIC_TEXT_LIMIT: usize = 512;
const SYMBOL_TEXT_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProtocolRequestId(i64);

impl ProtocolRequestId {
    const fn new(value: i64) -> Self {
        Self(value)
    }

    const fn value(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LspDocumentVersion(i32);

impl LspDocumentVersion {
    const INITIAL: Self = Self(1);

    fn advance(&mut self) {
        self.0 = self.0.saturating_add(1);
    }

    const fn value(self) -> i32 {
        self.0
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum ServerCapability {
    Definition = 1 << 0,
    Implementation = 1 << 1,
    TypeDefinition = 1 << 2,
    Declaration = 1 << 3,
    Hover = 1 << 4,
    DocumentSymbol = 1 << 5,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ServerCapabilities {
    bits: u8,
}

impl ServerCapabilities {
    #[cfg(test)]
    const fn only(capability: ServerCapability) -> Self {
        Self {
            bits: capability as u8,
        }
    }

    fn insert(&mut self, capability: ServerCapability) {
        self.bits |= capability as u8;
    }

    const fn contains(self, capability: ServerCapability) -> bool {
        self.bits & capability as u8 != 0
    }

    fn supports(self, kind: SemanticNavigationKind) -> bool {
        let capability = match kind {
            SemanticNavigationKind::Definition => ServerCapability::Definition,
            SemanticNavigationKind::Implementation => ServerCapability::Implementation,
            SemanticNavigationKind::TypeDefinition => ServerCapability::TypeDefinition,
            SemanticNavigationKind::Declaration => ServerCapability::Declaration,
        };
        self.contains(capability)
    }

    fn supports_hover(self) -> bool {
        self.contains(ServerCapability::Hover)
    }

    fn supports_document_symbols(self) -> bool {
        self.contains(ServerCapability::DocumentSymbol)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WirePosition {
    pub(crate) line: u32,
    pub(crate) character: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WireRange {
    pub(crate) start: WirePosition,
    pub(crate) end: WirePosition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RawLocation {
    pub(crate) uri: String,
    pub(crate) selection: WireRange,
}

pub(crate) struct Session {
    profile: ServerProfile,
    connection: Connection,
    child: Mutex<Option<Child>>,
    opened: Mutex<Option<OpenedDocument>>,
    active_request: Mutex<Option<(LspOperationId, ProtocolRequestId)>>,
    stderr_tail: Arc<Mutex<VecDeque<u8>>>,
    capabilities: ServerCapabilities,
    encoding: PositionEncoding,
    _workspace_data: Option<TempDir>,
}

#[derive(Debug)]
struct OpenedDocument {
    uri: String,
    version: LspDocumentVersion,
    text: String,
}

impl Session {
    pub(crate) async fn start(
        profile: ServerProfile,
        workspace_root: PathBuf,
        cache_dir: &Path,
        status: watch::Sender<Option<String>>,
    ) -> Result<Arc<Self>, LspError> {
        std::fs::create_dir_all(cache_dir).map_err(|error| {
            LspError::Process(format!("could not create the LSP cache directory: {error}"))
        })?;
        let workspace_data = create_workspace_data(&profile)?;
        let (executable, arguments) = profile.command(
            &workspace_root,
            workspace_data.as_ref().map(TempDir::path),
            cache_dir,
        )?;
        let mut command = Command::new(&executable);
        command
            .args(arguments)
            .current_dir(&workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // JDT LS recognizes these variables as alternate transports. This
            // client always owns stdio and must not inherit an unrelated port
            // or pipe selection from its parent process.
            .env_remove("CLIENT_HOST")
            .env_remove("CLIENT_PORT")
            .env_remove("STDIN_PIPE_NAME")
            .env_remove("STDOUT_PIPE_NAME")
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            LspError::Process(format!(
                "could not start LSP profile {}: {error}; {}",
                profile.id(),
                profile.install_hint()
            ))
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            LspError::Process("language server stdin was not available".to_owned())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            LspError::Process("language server stdout was not available".to_owned())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            LspError::Process("language server stderr was not available".to_owned())
        })?;
        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_LIMIT)));
        let connection = Connection::new(stdin, stdout, stderr, Arc::clone(&stderr_tail), status);
        let root_uri = file_uri(&workspace_root)?;
        let initialize = json!({
            "processId": std::process::id(),
            "clientInfo": { "name": "ChronoGit", "version": env!("CARGO_PKG_VERSION") },
            "rootUri": root_uri,
            "capabilities": {
                "general": { "positionEncodings": ["utf-8", "utf-16", "utf-32"] },
                "workspace": { "workspaceFolders": true, "configuration": true },
                "textDocument": {
                    "definition": { "linkSupport": true },
                    "implementation": { "linkSupport": true },
                    "typeDefinition": { "linkSupport": true },
                    "declaration": { "linkSupport": true },
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                }
            },
            "workspaceFolders": [{ "uri": root_uri, "name": workspace_name(&workspace_root) }],
            "initializationOptions": profile.initialization_options(),
        });
        let response = connection
            .request("initialize", initialize, INITIALIZE_TIMEOUT)
            .await
            .map_err(|error| with_stderr(error, &stderr_tail))?;
        let capabilities_value = response.get("capabilities").ok_or_else(|| {
            LspError::Protocol("initialize result omitted capabilities".to_owned())
        })?;
        let capabilities = parse_capabilities(capabilities_value);
        let encoding = PositionEncoding::from_server(
            capabilities_value
                .get("positionEncoding")
                .and_then(Value::as_str),
        );
        connection.notify("initialized", json!({})).await?;
        Ok(Arc::new(Self {
            profile,
            connection,
            child: Mutex::new(Some(child)),
            opened: Mutex::new(None),
            active_request: Mutex::new(None),
            stderr_tail,
            capabilities,
            encoding,
            _workspace_data: workspace_data,
        }))
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.encoding
    }

    pub(crate) async fn navigate(
        &self,
        app_request_id: LspOperationId,
        kind: SemanticNavigationKind,
        document_path: &Path,
        text: &str,
        position: SourcePosition,
    ) -> Result<Vec<RawLocation>, LspError> {
        if !self.capabilities.supports(kind) {
            return Err(LspError::Unsupported(format!(
                "{} does not advertise {} navigation",
                self.profile.id(),
                kind.label()
            )));
        }
        let response = self
            .request_at_position(app_request_id, method(kind), document_path, text, position)
            .await?;
        normalize_locations(response)
    }

    pub(crate) async fn hover(
        &self,
        app_request_id: LspOperationId,
        document_path: &Path,
        text: &str,
        position: SourcePosition,
    ) -> Result<Option<String>, LspError> {
        if !self.capabilities.supports_hover() {
            return Err(LspError::Unsupported(format!(
                "{} does not advertise hover information",
                self.profile.id()
            )));
        }
        let response = self
            .request_at_position(
                app_request_id,
                "textDocument/hover",
                document_path,
                text,
                position,
            )
            .await?;
        normalize_hover(&response)
    }

    pub(crate) async fn document_symbols(
        &self,
        app_request_id: LspOperationId,
        document_path: &Path,
        text: &str,
    ) -> Result<Vec<DocumentSymbol>, LspError> {
        if !self.capabilities.supports_document_symbols() {
            return Err(LspError::Unsupported(format!(
                "{} does not advertise document symbols",
                self.profile.id()
            )));
        }
        let response = self
            .request_document(
                app_request_id,
                "textDocument/documentSymbol",
                document_path,
                text,
            )
            .await?;
        normalize_document_symbols(&response, text, self.encoding)
    }

    pub(crate) async fn cancel_obsolete(&self, current_request_id: LspOperationId) {
        let active = *self.active_request.lock().await;
        if let Some((app_id, protocol_id)) = active
            && app_id != current_request_id
        {
            let _ignored = self.connection.cancel(protocol_id).await;
        }
    }

    pub(crate) async fn shutdown(&self) {
        self.shutdown_with_timeout(SHUTDOWN_TIMEOUT).await;
    }

    async fn shutdown_with_timeout(&self, timeout: Duration) {
        let request = self.connection.request("shutdown", Value::Null, timeout);
        let _ignored = request.await;
        let _ignored = self.connection.notify("exit", Value::Null).await;
        let mut child = self.child.lock().await;
        if let Some(process) = child.as_mut()
            && tokio::time::timeout(timeout, process.wait()).await.is_err()
        {
            let _ignored = process.start_kill();
            let _ignored = tokio::time::timeout(timeout, process.wait()).await;
        }
        *child = None;
        self.connection.stop().await;
    }

    async fn synchronize_document(&self, uri: &str, text: &str) -> Result<(), LspError> {
        let mut opened = self.opened.lock().await;
        match opened.as_mut() {
            Some(document) if document.uri == uri && document.text == text => return Ok(()),
            Some(document) if document.uri == uri => {
                document.version.advance();
                document.text.clear();
                document.text.push_str(text);
                self.connection
                    .notify(
                        "textDocument/didChange",
                        json!({
                            "textDocument": {
                                "uri": uri,
                                "version": document.version.value()
                            },
                            "contentChanges": [{ "text": text }]
                        }),
                    )
                    .await?;
                return Ok(());
            }
            Some(document) => {
                self.connection
                    .notify(
                        "textDocument/didClose",
                        json!({ "textDocument": { "uri": document.uri } }),
                    )
                    .await?;
            }
            None => {}
        }
        self.connection
            .notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": self.profile.language_id(),
                        "version": LspDocumentVersion::INITIAL.value(),
                        "text": text
                    }
                }),
            )
            .await?;
        *opened = Some(OpenedDocument {
            uri: uri.to_owned(),
            version: LspDocumentVersion::INITIAL,
            text: text.to_owned(),
        });
        Ok(())
    }

    async fn request_at_position(
        &self,
        app_request_id: LspOperationId,
        method: &str,
        document_path: &Path,
        text: &str,
        position: SourcePosition,
    ) -> Result<Value, LspError> {
        let uri = file_uri(document_path)?;
        self.synchronize_document(&uri, text).await?;
        let line = source_line(text, position.line()).ok_or_else(|| {
            LspError::InvalidDocument("source cursor line is outside the document".to_owned())
        })?;
        let character = to_lsp_character(line, position.byte_column(), self.encoding)?;
        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line(), "character": character }
        });
        self.request_with_content_retry(app_request_id, method, params)
            .await
    }

    async fn request_document(
        &self,
        app_request_id: LspOperationId,
        method: &str,
        document_path: &Path,
        text: &str,
    ) -> Result<Value, LspError> {
        let uri = file_uri(document_path)?;
        self.synchronize_document(&uri, text).await?;
        let params = json!({ "textDocument": { "uri": uri } });
        self.request_with_content_retry(app_request_id, method, params)
            .await
    }

    async fn request_with_content_retry(
        &self,
        app_request_id: LspOperationId,
        method: &str,
        params: Value,
    ) -> Result<Value, LspError> {
        for attempt in 0..CONTENT_MODIFIED_ATTEMPTS {
            let id = self.connection.next_id();
            *self.active_request.lock().await = Some((app_request_id, id));
            let response = self
                .connection
                .request_with_id(id, method, params.clone(), REQUEST_TIMEOUT)
                .await;
            let mut active = self.active_request.lock().await;
            if active.is_some_and(|value| value == (app_request_id, id)) {
                *active = None;
            }
            drop(active);
            if matches!(response, Err(LspError::ContentModified))
                && attempt.saturating_add(1) < CONTENT_MODIFIED_ATTEMPTS
            {
                tokio::time::sleep(CONTENT_MODIFIED_RETRY_DELAY).await;
                continue;
            }
            return response.map_err(|error| with_stderr(error, &self.stderr_tail));
        }
        Err(LspError::ContentModified)
    }
}

type PendingRequests =
    Arc<Mutex<HashMap<ProtocolRequestId, oneshot::Sender<Result<Value, LspError>>>>>;

struct Connection {
    writer: mpsc::Sender<Value>,
    pending: PendingRequests,
    closed: watch::Receiver<Option<LspError>>,
    next_request: AtomicI64,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl Connection {
    fn new(
        stdin: tokio::process::ChildStdin,
        stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr,
        stderr_tail: Arc<Mutex<VecDeque<u8>>>,
        status: watch::Sender<Option<String>>,
    ) -> Self {
        let stderr_task = tokio::spawn(async move {
            let mut stderr = stderr;
            let mut buffer = [0u8; 4096];
            while let Ok(read) = stderr.read(&mut buffer).await {
                if read == 0 {
                    break;
                }
                let mut tail = stderr_tail.lock().await;
                for byte in &buffer[..read] {
                    if tail.len() == STDERR_LIMIT {
                        tail.pop_front();
                    }
                    tail.push_back(*byte);
                }
            }
        });
        Self::from_transport_with_status(stdin, stdout, vec![stderr_task], status)
    }

    #[cfg(test)]
    fn from_transport<W, R>(stdin: W, stdout: R, extra_tasks: Vec<JoinHandle<()>>) -> Self
    where
        W: AsyncWrite + Send + Unpin + 'static,
        R: AsyncRead + Send + Unpin + 'static,
    {
        let (status, _status_receiver) = watch::channel(None);
        Self::from_transport_with_status(stdin, stdout, extra_tasks, status)
    }

    fn from_transport_with_status<W, R>(
        stdin: W,
        stdout: R,
        mut extra_tasks: Vec<JoinHandle<()>>,
        status: watch::Sender<Option<String>>,
    ) -> Self
    where
        W: AsyncWrite + Send + Unpin + 'static,
        R: AsyncRead + Send + Unpin + 'static,
    {
        let (writer, mut messages) = mpsc::channel::<Value>(WRITER_QUEUE);
        let pending = Arc::new(Mutex::new(HashMap::<
            ProtocolRequestId,
            oneshot::Sender<Result<Value, LspError>>,
        >::new()));
        let (closed_sender, closed) = watch::channel(None::<LspError>);

        let writer_closed = closed_sender.clone();
        let writer_pending = Arc::clone(&pending);
        let writer_task = tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(message) = messages.recv().await {
                if let Err(error) = write_message(&mut stdin, &message).await {
                    let _ignored = writer_closed.send(Some(error.clone()));
                    let mut pending = writer_pending.lock().await;
                    for (_, sender) in pending.drain() {
                        let _ignored = sender.send(Err(error.clone()));
                    }
                    break;
                }
            }
        });

        let reader_pending = Arc::clone(&pending);
        let reader_writer = writer.clone();
        let reader_closed = closed_sender;
        let reader_task = tokio::spawn(async move {
            let mut stdout = BufReader::new(stdout);
            loop {
                match read_message(&mut stdout).await {
                    Ok(message) => {
                        route_incoming(message, &reader_pending, &reader_writer, &status).await;
                    }
                    Err(error) => {
                        let _ignored = reader_closed.send(Some(error.clone()));
                        let mut pending = reader_pending.lock().await;
                        for (_, sender) in pending.drain() {
                            let _ignored = sender.send(Err(error.clone()));
                        }
                        break;
                    }
                }
            }
        });

        extra_tasks.extend([writer_task, reader_task]);

        Self {
            writer,
            pending,
            closed,
            next_request: AtomicI64::new(FIRST_PROTOCOL_REQUEST_ID),
            tasks: Mutex::new(extra_tasks),
        }
    }

    fn next_id(&self) -> ProtocolRequestId {
        ProtocolRequestId::new(self.next_request.fetch_add(1, Ordering::Relaxed))
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
        self.request_with_id(self.next_id(), method, params, timeout)
            .await
    }

    async fn request_with_id(
        &self,
        id: ProtocolRequestId,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
        if let Some(error) = self.closed.borrow().clone() {
            return Err(error);
        }
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id.value(),
            "method": method,
            "params": params
        });
        if self.writer.send(message).await.is_err() {
            self.pending.lock().await.remove(&id);
            return Err(LspError::Process(
                "language server request channel closed".to_owned(),
            ));
        }
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(LspError::Process(
                "language server response channel closed".to_owned(),
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                let _ignored = self.cancel(id).await;
                Err(LspError::Timeout(format!(
                    "language server did not answer {method} before the timeout"
                )))
            }
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.writer
            .send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await
            .map_err(|_| LspError::Process("language server request channel closed".to_owned()))
    }

    async fn cancel(&self, id: ProtocolRequestId) -> Result<(), LspError> {
        self.notify("$/cancelRequest", json!({ "id": id.value() }))
            .await
    }

    async fn stop(&self) {
        let mut tasks = self.tasks.lock().await;
        for task in tasks.drain(..) {
            task.abort();
        }
    }
}

async fn route_incoming(
    message: Value,
    pending: &Mutex<HashMap<ProtocolRequestId, oneshot::Sender<Result<Value, LspError>>>>,
    writer: &mpsc::Sender<Value>,
    status: &watch::Sender<Option<String>>,
) {
    let Some(object) = message.as_object() else {
        return;
    };
    if let Some(response_id) = object.get("id").cloned() {
        if object.contains_key("method") {
            let method = object.get("method").and_then(Value::as_str).unwrap_or("");
            let response = server_request_response(response_id, method, object.get("params"));
            let _ignored = writer.send(response).await;
            return;
        }
        let Some(id) = response_id.as_i64().map(ProtocolRequestId::new) else {
            return;
        };
        if let Some(sender) = pending.lock().await.remove(&id) {
            let result = if let Some(error) = object.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("language server returned an error");
                if error.get("code").and_then(Value::as_i64) == Some(CONTENT_MODIFIED_ERROR_CODE) {
                    Err(LspError::ContentModified)
                } else {
                    Err(LspError::RequestFailed(format!(
                        "language server rejected the request: {}",
                        sanitize(message, DIAGNOSTIC_TEXT_LIMIT)
                    )))
                }
            } else {
                Ok(object.get("result").cloned().unwrap_or(Value::Null))
            };
            let _ignored = sender.send(result);
        }
        return;
    }
    if let Some(method) = object.get("method").and_then(Value::as_str)
        && let Some(message) = notification_status(method, object.get("params"))
    {
        status.send_replace(Some(message));
    }
}

fn notification_status(method: &str, params: Option<&Value>) -> Option<String> {
    let params = params?;
    let message = match method {
        "window/logMessage" | "window/showMessage" | "language/status" => {
            params.get("message").and_then(Value::as_str)
        }
        "$/progress" => params
            .get("value")
            .and_then(|value| value.get("message").or_else(|| value.get("title")))
            .and_then(Value::as_str),
        _ => None,
    }?;
    let message = sanitize(message, STATUS_LIMIT);
    (!message.is_empty()).then_some(message)
}

fn server_request_response(id: Value, method: &str, params: Option<&Value>) -> Value {
    match method {
        "workspace/configuration" => {
            let count = params
                .and_then(|value| value.get("items"))
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            json!({ "jsonrpc": "2.0", "id": id, "result": vec![Value::Null; count] })
        }
        "window/workDoneProgress/create" => {
            json!({ "jsonrpc": "2.0", "id": id, "result": null })
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "method not supported by ChronoGit" }
        }),
    }
}

fn parse_capabilities(value: &Value) -> ServerCapabilities {
    let enabled = |name: &str| {
        value
            .get(name)
            .is_some_and(|provider| !provider.is_null() && provider != &Value::Bool(false))
    };
    let mut capabilities = ServerCapabilities::default();
    for (provider, capability) in [
        ("definitionProvider", ServerCapability::Definition),
        ("implementationProvider", ServerCapability::Implementation),
        ("typeDefinitionProvider", ServerCapability::TypeDefinition),
        ("declarationProvider", ServerCapability::Declaration),
        ("hoverProvider", ServerCapability::Hover),
        ("documentSymbolProvider", ServerCapability::DocumentSymbol),
    ] {
        if enabled(provider) {
            capabilities.insert(capability);
        }
    }
    capabilities
}

fn normalize_document_symbols(
    value: &Value,
    source: &str,
    encoding: PositionEncoding,
) -> Result<Vec<DocumentSymbol>, LspError> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let items = value.as_array().ok_or_else(|| {
        LspError::Protocol("language server returned invalid document symbols".to_owned())
    })?;
    let source_lines = source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect::<Vec<_>>();
    let mut symbols = Vec::new();
    append_document_symbols(items, &source_lines, encoding, 0, &mut symbols)?;
    Ok(symbols)
}

fn append_document_symbols(
    items: &[Value],
    source_lines: &[&str],
    encoding: PositionEncoding,
    depth: usize,
    symbols: &mut Vec<DocumentSymbol>,
) -> Result<(), LspError> {
    for item in items {
        if symbols.len() == SYMBOL_LIMIT {
            break;
        }
        let name = item.get("name").and_then(Value::as_str).ok_or_else(|| {
            LspError::Protocol("language server returned a symbol without a name".to_owned())
        })?;
        let range_value = item
            .get("range")
            .or_else(|| {
                item.get("location")
                    .and_then(|location| location.get("range"))
            })
            .ok_or_else(|| {
                LspError::Protocol("language server returned a symbol without a range".to_owned())
            })?;
        let wire_range = parse_range(range_value)?;
        let selection_value = item.get("selectionRange").unwrap_or(range_value);
        let wire_selection = parse_range(selection_value)?;
        let range = source_range(source_lines, wire_range, encoding)?;
        let selection = source_range(source_lines, wire_selection, encoding)?.start();
        let detail = item
            .get("detail")
            .and_then(Value::as_str)
            .or_else(|| item.get("containerName").and_then(Value::as_str))
            .map(|value| sanitize(value, SYMBOL_TEXT_LIMIT));
        let kind = item
            .get("kind")
            .and_then(Value::as_u64)
            .map_or(DocumentSymbolKind::Unspecified, DocumentSymbolKind::from);
        symbols.push(DocumentSymbol::new(
            sanitize(name, SYMBOL_TEXT_LIMIT),
            detail,
            kind,
            range,
            selection,
            depth,
        ));
        if depth < SYMBOL_DEPTH_LIMIT
            && let Some(children) = item.get("children").and_then(Value::as_array)
        {
            append_document_symbols(
                children,
                source_lines,
                encoding,
                depth.saturating_add(1),
                symbols,
            )?;
        }
    }
    Ok(())
}

fn source_range(
    source_lines: &[&str],
    range: WireRange,
    encoding: PositionEncoding,
) -> Result<SourceRange, LspError> {
    let convert = |position: WirePosition| -> Result<SourcePosition, LspError> {
        let line = source_lines
            .get(usize::try_from(position.line).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                LspError::Protocol(
                    "language server returned a symbol outside the document".to_owned(),
                )
            })?;
        Ok(SourcePosition::new(
            position.line,
            from_lsp_character(line, position.character, encoding)?,
        ))
    };
    let start = convert(range.start)?;
    let end = convert(range.end)?;
    if (end.line(), end.byte_column()) < (start.line(), start.byte_column()) {
        return Err(LspError::Protocol(
            "language server returned an inverted source range".to_owned(),
        ));
    }
    Ok(SourceRange::new(start, end))
}

fn normalize_hover(value: &Value) -> Result<Option<String>, LspError> {
    if value.is_null() {
        return Ok(None);
    }
    let contents = value.get("contents").ok_or_else(|| {
        LspError::Protocol("language server returned hover information without contents".to_owned())
    })?;
    let parts = if let Some(items) = contents.as_array() {
        items
            .iter()
            .map(hover_part)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![hover_part(contents)?]
    };
    let combined = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut bounded = combined
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(HOVER_LIMIT.saturating_add(1))
        .collect::<String>();
    if bounded.chars().count() > HOVER_LIMIT {
        bounded = bounded.chars().take(HOVER_LIMIT).collect();
        bounded.push_str("\n… hover text truncated …");
    }
    Ok((!bounded.trim().is_empty()).then_some(bounded))
}

fn hover_part(value: &Value) -> Result<String, LspError> {
    if let Some(text) = value.as_str() {
        return Ok(text.to_owned());
    }
    if let Some(text) = value.get("value").and_then(Value::as_str) {
        return Ok(text.to_owned());
    }
    Err(LspError::Protocol(
        "language server returned invalid hover contents".to_owned(),
    ))
}

fn normalize_locations(value: Value) -> Result<Vec<RawLocation>, LspError> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let items = if let Some(array) = value.as_array() {
        array.clone()
    } else {
        vec![value]
    };
    let mut locations = Vec::new();
    for item in items {
        let (uri, range) = if let Some(uri) = item.get("uri").and_then(Value::as_str) {
            (uri, item.get("range"))
        } else if let Some(uri) = item.get("targetUri").and_then(Value::as_str) {
            (
                uri,
                item.get("targetSelectionRange")
                    .or_else(|| item.get("targetRange")),
            )
        } else {
            return Err(LspError::Protocol(
                "language server returned a location without a URI".to_owned(),
            ));
        };
        let range = parse_range(range.ok_or_else(|| {
            LspError::Protocol("language server returned a location without a range".to_owned())
        })?)?;
        locations.push(RawLocation {
            uri: uri.to_owned(),
            selection: range,
        });
    }
    locations.dedup();
    Ok(locations)
}

fn parse_range(value: &Value) -> Result<WireRange, LspError> {
    let position = |name: &str| -> Result<WirePosition, LspError> {
        let value = value.get(name).ok_or_else(|| {
            LspError::Protocol("language server returned an incomplete source range".to_owned())
        })?;
        let line = value.get("line").and_then(Value::as_u64);
        let character = value.get("character").and_then(Value::as_u64);
        let (Some(line), Some(character)) = (line, character) else {
            return Err(LspError::Protocol(
                "language server returned an invalid source position".to_owned(),
            ));
        };
        Ok(WirePosition {
            line: u32::try_from(line).map_err(|_| {
                LspError::Protocol("language server returned an oversized line".to_owned())
            })?,
            character: u32::try_from(character).map_err(|_| {
                LspError::Protocol("language server returned an oversized character".to_owned())
            })?,
        })
    };
    Ok(WireRange {
        start: position("start")?,
        end: position("end")?,
    })
}

fn source_line(source: &str, line: u32) -> Option<&str> {
    source
        .split('\n')
        .nth(usize::try_from(line).ok()?)
        .map(|value| value.strip_suffix('\r').unwrap_or(value))
}

fn method(kind: SemanticNavigationKind) -> &'static str {
    match kind {
        SemanticNavigationKind::Definition => "textDocument/definition",
        SemanticNavigationKind::Implementation => "textDocument/implementation",
        SemanticNavigationKind::TypeDefinition => "textDocument/typeDefinition",
        SemanticNavigationKind::Declaration => "textDocument/declaration",
    }
}

fn file_uri(path: &Path) -> Result<String, LspError> {
    Url::from_file_path(path).map(String::from).map_err(|()| {
        LspError::InvalidDocument("could not represent file path as a URI".to_owned())
    })
}

fn workspace_name(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("workspace")
        .to_owned()
}

fn create_workspace_data(profile: &ServerProfile) -> Result<Option<TempDir>, LspError> {
    if !profile.needs_workspace_data() {
        return Ok(None);
    }
    let directory = tempfile::Builder::new()
        .prefix("chronogit-lsp-")
        .tempdir()
        .map_err(|error| {
            LspError::Process(format!(
                "could not create workspace data for {}: {error}",
                profile.id()
            ))
        })?;
    std::fs::create_dir(directory.path().join("configuration")).map_err(|error| {
        LspError::Process(format!(
            "could not create workspace configuration for {}: {error}",
            profile.id()
        ))
    })?;
    Ok(Some(directory))
}

fn sanitize(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect()
}

fn with_stderr(error: LspError, tail: &Mutex<VecDeque<u8>>) -> LspError {
    if !matches!(error, LspError::Process(_) | LspError::Protocol(_)) {
        return error;
    }
    let Ok(tail) = tail.try_lock() else {
        return error;
    };
    if tail.is_empty() {
        return error;
    }
    let bytes = tail.iter().copied().collect::<Vec<_>>();
    let detail = sanitize(&String::from_utf8_lossy(&bytes), DIAGNOSTIC_TEXT_LIMIT);
    LspError::Process(format!("{error}; server stderr: {detail}"))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{BufReader, duplex, split};
    use tokio::process::Command;
    use tokio::sync::Mutex;

    use super::{
        Connection, SYMBOL_DEPTH_LIMIT, ServerCapabilities, ServerCapability, Session,
        create_workspace_data, normalize_document_symbols, normalize_hover, normalize_locations,
        notification_status, parse_capabilities, server_request_response,
    };
    use crate::domain::{DocumentSymbolKind, SourcePosition};
    use crate::lsp::protocol::{read_message, write_message};
    use crate::lsp::{LspConfig, LspOperationId, PositionEncoding};

    #[test]
    fn normalizes_location_and_location_links() {
        let locations = normalize_locations(json!([
            {"uri":"file:///repo/a.rs","range":{"start":{"line":1,"character":2},"end":{"line":1,"character":3}}},
            {"targetUri":"file:///repo/b.rs","targetRange":{"start":{"line":3,"character":1},"end":{"line":3,"character":8}},"targetSelectionRange":{"start":{"line":3,"character":4},"end":{"line":3,"character":5}}}
        ]))
        .unwrap_or_else(|error| panic!("normalize: {error}"));
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[1].selection.start.character, 4);
    }

    #[test]
    fn normalizes_all_standard_hover_content_shapes() {
        let markdown = normalize_hover(&json!({
            "contents": {"kind":"markdown", "value":"```rust\nstruct Action;\n```"}
        }))
        .unwrap_or_else(|error| panic!("hover: {error}"));
        assert_eq!(markdown.as_deref(), Some("```rust\nstruct Action;\n```"));

        let marked = normalize_hover(&json!({
            "contents": ["Documentation", {"language":"rust", "value":"struct Action;"}]
        }))
        .unwrap_or_else(|error| panic!("hover: {error}"));
        assert_eq!(marked.as_deref(), Some("Documentation\n\nstruct Action;"));
        assert_eq!(
            normalize_hover(&json!(null)).unwrap_or_else(|error| panic!("hover: {error}")),
            None
        );
    }

    #[test]
    fn flattens_hierarchical_and_flat_document_symbols_with_utf8_positions() {
        let symbols = normalize_document_symbols(
            &json!([
                {
                    "name":"module",
                    "detail":"mod detail",
                    "kind":2,
                    "range":{"start":{"line":0,"character":0},"end":{"line":1,"character":8}},
                    "selectionRange":{"start":{"line":0,"character":0},"end":{"line":0,"character":6}},
                    "children":[{
                        "name":"界value",
                        "kind":13,
                        "range":{"start":{"line":1,"character":2},"end":{"line":1,"character":8}},
                        "selectionRange":{"start":{"line":1,"character":2},"end":{"line":1,"character":8}}
                    }]
                },
                {
                    "name":"flat",
                    "kind":12,
                    "containerName":"root",
                    "location":{"uri":"file:///tmp/example.rs","range":{"start":{"line":2,"character":0},"end":{"line":2,"character":4}}}
                }
            ]),
            "module\n  界value\nflat\n",
            PositionEncoding::Utf16,
        )
        .unwrap_or_else(|error| panic!("symbols: {error}"));
        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].kind(), DocumentSymbolKind::Module);
        assert_eq!(symbols[1].depth(), 1);
        assert_eq!(symbols[1].selection(), SourcePosition::new(1, 2));
        assert_eq!(symbols[2].detail(), Some("root"));
    }

    #[test]
    fn rejects_inverted_document_symbol_ranges() {
        let error = normalize_document_symbols(
            &json!([{
                "name":"broken",
                "kind":12,
                "range":{"start":{"line":1,"character":0},"end":{"line":0,"character":1}},
                "selectionRange":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}}
            }]),
            "a\nb\n",
            PositionEncoding::Utf8,
        )
        .err()
        .unwrap_or_else(|| panic!("an inverted untrusted range must be rejected"));
        assert!(error.to_string().contains("inverted source range"));
    }

    #[test]
    fn bounds_document_symbol_nesting() {
        let mut item = json!({
            "name":"leaf",
            "kind":12,
            "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},
            "selectionRange":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}
        });
        for index in 0..SYMBOL_DEPTH_LIMIT.saturating_add(20) {
            item = json!({
                "name":format!("parent-{index}"),
                "kind":2,
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},
                "selectionRange":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},
                "children":[item]
            });
        }
        let symbols = normalize_document_symbols(&json!([item]), "x\n", PositionEncoding::Utf8)
            .unwrap_or_else(|error| panic!("symbols: {error}"));
        assert_eq!(symbols.len(), SYMBOL_DEPTH_LIMIT.saturating_add(1));
        assert_eq!(
            symbols.last().map(|symbol| symbol.depth()),
            Some(SYMBOL_DEPTH_LIMIT)
        );
    }

    #[tokio::test]
    async fn hover_synchronizes_the_document_and_sends_the_negotiated_position() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        let config_path = directory.path().join("lsp.toml");
        std::fs::write(&config_path, "[servers]\n")
            .unwrap_or_else(|error| panic!("config: {error}"));
        let profile = LspConfig::load(&["rust-analyzer".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"))
            .profiles()[0]
            .clone();
        let (client, server) = duplex(4096);
        let (client_read, client_write) = split(client);
        let connection = Connection::from_transport(client_write, client_read, Vec::new());
        let fake_server = tokio::spawn(async move {
            let (server_read, mut server_write) = split(server);
            let mut reader = BufReader::new(server_read);
            let opened = read_message(&mut reader)
                .await
                .unwrap_or_else(|error| panic!("didOpen: {error}"));
            assert_eq!(opened["method"], "textDocument/didOpen");
            assert_eq!(opened["params"]["textDocument"]["languageId"], "rust");
            assert_eq!(opened["params"]["textDocument"]["text"], "界Action\n");
            let hover = read_message(&mut reader)
                .await
                .unwrap_or_else(|error| panic!("hover: {error}"));
            assert_eq!(hover["method"], "textDocument/hover");
            assert_eq!(hover["params"]["position"]["line"], 0);
            assert_eq!(hover["params"]["position"]["character"], 1);
            write_message(
                &mut server_write,
                &json!({
                    "jsonrpc":"2.0",
                    "id":hover["id"].clone(),
                    "result":{"contents":{"kind":"markdown","value":"`Action` docs"}}
                }),
            )
            .await
            .unwrap_or_else(|error| panic!("hover response: {error}"));
        });
        let session = Session {
            profile,
            connection,
            child: Mutex::new(None),
            opened: Mutex::new(None),
            active_request: Mutex::new(None),
            stderr_tail: Arc::new(Mutex::new(VecDeque::new())),
            capabilities: ServerCapabilities::only(ServerCapability::Hover),
            encoding: PositionEncoding::Utf16,
            _workspace_data: None,
        };

        let result = session
            .hover(
                LspOperationId::new(7),
                Path::new("/tmp/example.rs"),
                "界Action\n",
                SourcePosition::new(0, "界".len()),
            )
            .await
            .unwrap_or_else(|error| panic!("hover request: {error}"));
        assert_eq!(result.as_deref(), Some("`Action` docs"));
        fake_server
            .await
            .unwrap_or_else(|error| panic!("fake server: {error}"));
        session.connection.stop().await;
    }

    #[tokio::test]
    async fn document_symbols_synchronize_the_exact_displayed_document() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        let config_path = directory.path().join("lsp.toml");
        std::fs::write(&config_path, "[servers]\n")
            .unwrap_or_else(|error| panic!("config: {error}"));
        let profile = LspConfig::load(&["rust-analyzer".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"))
            .profiles()[0]
            .clone();
        let (client, server) = duplex(4096);
        let (client_read, client_write) = split(client);
        let connection = Connection::from_transport(client_write, client_read, Vec::new());
        let fake_server = tokio::spawn(async move {
            let (server_read, mut server_write) = split(server);
            let mut reader = BufReader::new(server_read);
            let opened = read_message(&mut reader)
                .await
                .unwrap_or_else(|error| panic!("didOpen: {error}"));
            assert_eq!(opened["method"], "textDocument/didOpen");
            assert_eq!(
                opened["params"]["textDocument"]["text"],
                "fn historical() {}\n"
            );
            let request = read_message(&mut reader)
                .await
                .unwrap_or_else(|error| panic!("document symbols: {error}"));
            assert_eq!(request["method"], "textDocument/documentSymbol");
            assert!(request["params"].get("position").is_none());
            write_message(
                &mut server_write,
                &json!({
                    "jsonrpc":"2.0",
                    "id":request["id"].clone(),
                    "result":[{
                        "name":"historical",
                        "kind":12,
                        "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":18}},
                        "selectionRange":{"start":{"line":0,"character":3},"end":{"line":0,"character":13}}
                    }]
                }),
            )
            .await
            .unwrap_or_else(|error| panic!("symbol response: {error}"));
        });
        let session = Session {
            profile,
            connection,
            child: Mutex::new(None),
            opened: Mutex::new(None),
            active_request: Mutex::new(None),
            stderr_tail: Arc::new(Mutex::new(VecDeque::new())),
            capabilities: ServerCapabilities::only(ServerCapability::DocumentSymbol),
            encoding: PositionEncoding::Utf8,
            _workspace_data: None,
        };

        let symbols = session
            .document_symbols(
                LspOperationId::new(8),
                Path::new("/tmp/example.rs"),
                "fn historical() {}\n",
            )
            .await
            .unwrap_or_else(|error| panic!("document-symbol request: {error}"));
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name(), "historical");
        assert_eq!(symbols[0].selection(), SourcePosition::new(0, 3));
        fake_server
            .await
            .unwrap_or_else(|error| panic!("fake server: {error}"));
        session.connection.stop().await;
    }

    #[test]
    fn capabilities_accept_boolean_and_registration_objects() {
        let capabilities = parse_capabilities(&json!({
            "definitionProvider": true,
            "implementationProvider": {"documentSelector": null},
            "typeDefinitionProvider": false,
            "hoverProvider": true,
            "documentSymbolProvider": {}
        }));
        assert!(capabilities.contains(ServerCapability::Definition));
        assert!(capabilities.contains(ServerCapability::Implementation));
        assert!(!capabilities.contains(ServerCapability::TypeDefinition));
        assert!(capabilities.contains(ServerCapability::Hover));
        assert!(capabilities.contains(ServerCapability::DocumentSymbol));
    }

    #[test]
    fn answers_configuration_and_rejects_unadvertised_requests() {
        let configuration = server_request_response(
            3.into(),
            "workspace/configuration",
            Some(&json!({"items":[{}, {}]})),
        );
        assert_eq!(configuration["result"].as_array().map(Vec::len), Some(2));
        let unsupported = server_request_response(4.into(), "workspace/applyEdit", None);
        assert_eq!(unsupported["error"]["code"], -32601);
    }

    #[test]
    fn extracts_only_bounded_displayable_server_status() {
        assert_eq!(
            notification_status(
                "$/progress",
                Some(&json!({"value":{"kind":"report","message":"indexing\n42%"}})),
            )
            .as_deref(),
            Some("indexing42%")
        );
        let long = "x".repeat(300);
        assert_eq!(
            notification_status("window/logMessage", Some(&json!({"message":long})))
                .map(|message| message.len()),
            Some(256)
        );
        assert!(notification_status("textDocument/publishDiagnostics", Some(&json!({}))).is_none());
    }

    #[test]
    fn jdt_workspace_data_is_unique_temporary_and_outside_the_repository() {
        let repository = tempfile::tempdir().unwrap_or_else(|error| panic!("repository: {error}"));
        let config_path = repository.path().join("lsp.toml");
        std::fs::write(&config_path, "[servers]\n")
            .unwrap_or_else(|error| panic!("config: {error}"));
        let profile = LspConfig::load(&["jdtls".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"))
            .profiles()[0]
            .clone();
        let first = create_workspace_data(&profile)
            .unwrap_or_else(|error| panic!("first workspace data: {error}"))
            .unwrap_or_else(|| panic!("jdtls should require workspace data"));
        let second = create_workspace_data(&profile)
            .unwrap_or_else(|error| panic!("second workspace data: {error}"))
            .unwrap_or_else(|| panic!("jdtls should require workspace data"));
        let first_path = first.path().to_path_buf();

        assert!(first_path.is_absolute());
        assert_ne!(first.path(), second.path());
        assert!(!first.path().starts_with(repository.path()));
        assert!(first.path().join("configuration").is_dir());
        drop(first);
        assert!(!first_path.exists());
    }

    #[tokio::test]
    async fn connection_completes_initialize_notifications_and_shutdown_with_interleaving() {
        let (client, server) = duplex(4096);
        let (client_read, client_write) = split(client);
        let connection = Connection::from_transport(client_write, client_read, Vec::new());
        let fake_server = tokio::spawn(async move {
            let (server_read, mut server_write) = split(server);
            let mut server_read = BufReader::new(server_read);
            let initialize = read_message(&mut server_read)
                .await
                .unwrap_or_else(|error| panic!("read initialize: {error}"));
            assert_eq!(initialize["method"], "initialize");
            let initialize_id = initialize["id"].clone();
            write_message(
                &mut server_write,
                &json!({
                    "jsonrpc":"2.0",
                    "id":91,
                    "method":"workspace/configuration",
                    "params":{"items":[{}]}
                }),
            )
            .await
            .unwrap_or_else(|error| panic!("write server request: {error}"));
            write_message(
                &mut server_write,
                &json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"index","value":{}}}),
            )
            .await
            .unwrap_or_else(|error| panic!("write notification: {error}"));
            write_message(
                &mut server_write,
                &json!({"jsonrpc":"2.0","id":initialize_id,"result":{"capabilities":{}}}),
            )
            .await
            .unwrap_or_else(|error| panic!("write initialize response: {error}"));

            let mut saw_configuration_response = false;
            let mut saw_initialized = false;
            while !saw_configuration_response || !saw_initialized {
                let message = read_message(&mut server_read)
                    .await
                    .unwrap_or_else(|error| panic!("read interleaved client message: {error}"));
                saw_configuration_response |=
                    message["id"] == 91 && message.get("result").is_some();
                saw_initialized |= message["method"] == "initialized";
            }
            let shutdown = read_message(&mut server_read)
                .await
                .unwrap_or_else(|error| panic!("read shutdown: {error}"));
            assert_eq!(shutdown["method"], "shutdown");
            write_message(
                &mut server_write,
                &json!({"jsonrpc":"2.0","id":shutdown["id"].clone(),"result":null}),
            )
            .await
            .unwrap_or_else(|error| panic!("write shutdown response: {error}"));
            let exit = read_message(&mut server_read)
                .await
                .unwrap_or_else(|error| panic!("read exit: {error}"));
            assert_eq!(exit["method"], "exit");
        });

        let initialized = connection
            .request("initialize", json!({}), Duration::from_secs(1))
            .await
            .unwrap_or_else(|error| panic!("initialize: {error}"));
        assert!(initialized.get("capabilities").is_some());
        connection
            .notify("initialized", json!({}))
            .await
            .unwrap_or_else(|error| panic!("initialized: {error}"));
        connection
            .request("shutdown", json!(null), Duration::from_secs(1))
            .await
            .unwrap_or_else(|error| panic!("shutdown: {error}"));
        connection
            .notify("exit", json!(null))
            .await
            .unwrap_or_else(|error| panic!("exit: {error}"));
        fake_server
            .await
            .unwrap_or_else(|error| panic!("fake server task: {error}"));
        connection.stop().await;
    }

    #[tokio::test]
    async fn request_timeout_emits_cancel() {
        let (client, server) = duplex(1024);
        let (client_read, client_write) = split(client);
        let connection = Connection::from_transport(client_write, client_read, Vec::new());
        let fake_server = tokio::spawn(async move {
            let (server_read, _server_write) = split(server);
            let mut reader = BufReader::new(server_read);
            let request = read_message(&mut reader)
                .await
                .unwrap_or_else(|error| panic!("read request: {error}"));
            let cancel = read_message(&mut reader)
                .await
                .unwrap_or_else(|error| panic!("read cancel: {error}"));
            assert_eq!(cancel["method"], "$/cancelRequest");
            assert_eq!(cancel["params"]["id"], request["id"]);
        });
        assert!(
            connection
                .request(
                    "textDocument/definition",
                    json!({}),
                    Duration::from_millis(10)
                )
                .await
                .is_err()
        );
        fake_server
            .await
            .unwrap_or_else(|error| panic!("fake server task: {error}"));
        connection.stop().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_deadline_terminates_an_unresponsive_child() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        let config_path = directory.path().join("lsp.toml");
        std::fs::write(&config_path, "[servers]\n")
            .unwrap_or_else(|error| panic!("config: {error}"));
        let profile = LspConfig::load(&["rust-analyzer".to_owned()], Some(&config_path))
            .unwrap_or_else(|error| panic!("profile: {error}"))
            .profiles()[0]
            .clone();
        let mut child = Command::new("sleep");
        child.arg("30").kill_on_drop(true);
        let child = child
            .spawn()
            .unwrap_or_else(|error| panic!("sleep child: {error}"));
        let (client, server) = duplex(1024);
        let (client_read, client_write) = split(client);
        let connection = Connection::from_transport(client_write, client_read, Vec::new());
        let fake_server = tokio::spawn(async move {
            let (server_read, _server_write) = split(server);
            let mut reader = BufReader::new(server_read);
            loop {
                let message = read_message(&mut reader)
                    .await
                    .unwrap_or_else(|error| panic!("read shutdown message: {error}"));
                if message["method"] == "exit" {
                    break;
                }
            }
        });
        let session = Session {
            profile,
            connection,
            child: Mutex::new(Some(child)),
            opened: Mutex::new(None),
            active_request: Mutex::new(None),
            stderr_tail: Arc::new(Mutex::new(VecDeque::new())),
            capabilities: ServerCapabilities::default(),
            encoding: PositionEncoding::Utf16,
            _workspace_data: None,
        };
        session
            .shutdown_with_timeout(Duration::from_millis(20))
            .await;
        assert!(session.child.lock().await.is_none());
        fake_server
            .await
            .unwrap_or_else(|error| panic!("fake server task: {error}"));
    }
}
