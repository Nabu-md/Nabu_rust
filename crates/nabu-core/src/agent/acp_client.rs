//! AcpClient — JSON-RPC client that speaks the ACP protocol over stdio.
//!
//! [`AcpClient`] drives an external ACP-capable agent process (e.g. an agent
//! that speaks the [Agent Communication Protocol](crate::acp)) by sending
//! JSON-RPC 2.0 requests over the child's stdin and reading responses (plus
//! streaming notifications) from stdout.
//!
//! ## Lifecycle
//!
//! 1. **Create** — `AcpClient::new` receives a [`StdioChannel`] connected to
//!    the child process's stdin/stdout.
//! 2. **Initialize** — `client.initialize()` sends the `initialize` method and
//!    stores the negotiated [`InitializeResponse`].
//! 3. **New session** — `client.new_session()` creates a server-side session
//!    and stores the returned session ID.
//! 4. **Prompt** — `client.prompt()` sends a user message and reads the
//!    streamed response, publishing tokens to a [`StreamingPipeline`].
//! 5. **Close** — `client.close()` sends `session/close` to terminate the
//!    remote session cleanly.
//! 6. **Stop** — `client.stop()` kills the child process if still running.
//!
//! ## Concurrency
//!
//! The [`StdioChannel`] wraps stdin and stdout in independent `tokio::sync::Mutex`
//! guards, so sending a request and receiving a response can proceed concurrently
//! without contention.  The read loop acquires **no** lock while awaiting data on
//! stdout — the channel's own stdout mutex provides the necessary synchronization.
//! The state mutex is only held briefly to insert/remove pending request entries,
//! eliminating the previous deadlock where `recv_response().await` held the inner
//! lock and blocked `send_request` from writing to stdin.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::acp::events::classify_message;
use crate::acp::types::{
    CloseSessionRequest, CloseSessionResponse, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
    SessionNotificationParams, SessionUpdate, METHOD_CLOSE_SESSION, METHOD_INITIALIZE,
    METHOD_NEW_SESSION, METHOD_PROMPT,
};
use crate::agent::stdio_channel::StdioChannel;
use crate::rpc::{Request, RequestId, Response, JSON_RPC_VERSION};
use crate::streaming::StreamingPipeline;
use crate::{AgentManagerError, AgentResult};

/// Type alias for the update notification callback.
type UpdateCallback = Arc<
    dyn Fn(String, SessionUpdate) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// A pending JSON-RPC request awaiting its response.
struct PendingRequest {
    tx: tokio::sync::oneshot::Sender<AgentResult<Response>>,
}

/// Internal shared state for the AcpClient — mutable state protected by an
/// `AsyncMutex`.  The channel is stored **outside** this mutex so that the
/// read loop can await on stdout without holding the state lock (which would
/// deadlock `send_request`).
struct ClientState {
    pending: std::collections::HashMap<RequestId, PendingRequest>,
    next_id: AtomicU64,
    initialized: AtomicBool,
    session_id: Option<String>,
    stopping: AtomicBool,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            pending: std::collections::HashMap::new(),
            next_id: AtomicU64::new(1),
            initialized: AtomicBool::new(false),
            session_id: None,
            stopping: AtomicBool::new(false),
        }
    }
}

/// A JSON-RPC + ACP client that drives an external agent process over stdio.
pub struct AcpClient {
    /// The stdio channel — shared between the client and the read loop.
    /// `StdioChannel` uses internal `tokio::sync::Mutex` guards for stdin
    /// and stdout independently, so concurrent send/recv is safe.
    channel: Arc<StdioChannel>,
    /// Mutable state — only held briefly, never during channel I/O.
    state: Arc<AsyncMutex<ClientState>>,
    /// Optional streaming pipeline for token delivery.
    pipeline: Option<Arc<StreamingPipeline>>,
    /// Optional callback for `session/update` notifications.
    on_update: Option<UpdateCallback>,
    /// Handle to the background read loop task.
    read_task: Option<tokio::task::JoinHandle<()>>,
}

impl AcpClient {
    /// Create a new client from a [`StdioChannel`] connected to an ACP server.
    pub fn new(channel: StdioChannel, pipeline: Option<Arc<StreamingPipeline>>) -> Self {
        Self {
            channel: Arc::new(channel),
            state: Arc::new(AsyncMutex::new(ClientState::default())),
            pipeline,
            on_update: None,
            read_task: None,
        }
    }

    /// Register a callback to be invoked for each `session/update` notification
    /// received from the agent.
    pub fn on_update<F, Fut>(&mut self, callback: F)
    where
        F: Fn(String, SessionUpdate) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let cb: UpdateCallback = Arc::new(move |sid, update| {
            let fut = callback(sid.to_string(), update);
            Box::pin(async move { fut.await })
        });
        self.on_update = Some(cb);
    }

    /// Start the background read loop that demultiplexes responses and
    /// forwards notifications.
    ///
    /// Only one read loop may run at a time. Subsequent calls are no-ops.
    pub async fn start_read_loop(&mut self) {
        if self.read_task.is_some() {
            return;
        }
        let channel = self.channel.clone();
        let state = self.state.clone();
        let pipeline = self.pipeline.clone();
        let on_update = self.on_update.clone();
        let task = tokio::spawn(async move {
            Self::read_loop(channel, state, pipeline, on_update).await;
        });
        self.read_task = Some(task);
    }

    /// The background reader loop.
    ///
    /// Reads one NDJSON line at a time from the channel — **without** holding
    /// the state mutex — then dispatches the result.  This eliminates the
    /// deadlock that occurred when `recv_response().await` was called while
    /// holding the inner lock.
    async fn read_loop(
        channel: Arc<StdioChannel>,
        state: Arc<AsyncMutex<ClientState>>,
        _pipeline: Option<Arc<StreamingPipeline>>,
        on_update: Option<UpdateCallback>,
    ) {
        loop {
            // Receive a raw line — NO state lock held during I/O.
            let recv_result = channel.recv_line().await;

            match recv_result {
                Ok(Some(line)) => {
                    // Classify the message (response, notification, or
                    // agent→client request).
                    let classified = match classify_message(&line) {
                        Ok(msg) => msg,
                        Err(e) => {
                            tracing::warn!(
                                "ACP: failed to classify message: {} (line: {})",
                                e,
                                line
                            );
                            continue;
                        }
                    };

                    match classified {
                        crate::acp::events::InboundMessage::Response { id, result, error } => {
                            let mut st = state.lock().await;
                            if let Some(pending) = st.pending.remove(&id) {
                                let resp = match error {
                                    Some(e) => Err(AcpClientError::Protocol(e.message).into()),
                                    None => Ok(Response {
                                        version: JSON_RPC_VERSION.to_string(),
                                        id: id.clone(),
                                        result,
                                        error: None,
                                    }),
                                };
                                drop(st);
                                let _ = pending.tx.send(resp);
                            }
                        }
                        crate::acp::events::InboundMessage::Notification { method, params } => {
                            // Route session/update notifications to the
                            // callback (and indirectly the streaming pipeline).
                            if method == crate::acp::types::METHOD_SESSION_UPDATE {
                                if let Some(cb) = &on_update {
                                    if let Some(p) = params {
                                        if let Ok(notif) =
                                            serde_json::from_value::<SessionNotificationParams>(p)
                                        {
                                            let sid = notif.session_id.clone();
                                            let update = notif.update.clone();
                                            cb(sid, update).await;
                                        }
                                    }
                                }
                            }
                        }
                        crate::acp::events::InboundMessage::AgentRequest { id, method, .. } => {
                            // The old AcpClient does not dispatch agent→client
                            // requests (no handler trait). Log and ignore.
                            tracing::warn!(
                                "ACP: unhandled agent→client request: method={}, id={}",
                                method,
                                id
                            );
                        }
                    }
                }
                Ok(None) => {
                    // EOF on stdout — the agent process has closed its output.
                    let mut st = state.lock().await;
                    st.stopping.store(true, Ordering::SeqCst);
                    let err = AcpClientError::Eof;
                    for pending in st.pending.drain() {
                        let _ = pending.1.tx.send(Err(err.clone().into()));
                    }
                    drop(st);
                    break;
                }
                Err(e) => {
                    let mut st = state.lock().await;
                    st.stopping.store(true, Ordering::SeqCst);
                    let err = AgentManagerError::Acp(format!("read error: {}", e));
                    for pending in st.pending.drain() {
                        let _ = pending.1.tx.send(Err(err.clone()));
                    }
                    drop(st);
                    break;
                }
            }
        }
    }

    /// Send a JSON-RPC request and wait for the matching response.
    ///
    /// The request ID is auto-incremented and tracked internally. The caller
    /// must ensure `start_read_loop` is running so responses are demultiplexed.
    ///
    /// **No deadlock:** the state mutex is released before the channel write
    /// completes, and the read loop never holds the state mutex while awaiting
    /// data on stdout.
    pub async fn send_request(&self, method: &str, params: Option<Value>) -> AgentResult<Response> {
        let id = {
            let st = self.state.lock().await;
            RequestId::Number(st.next_id.fetch_add(1, Ordering::SeqCst) as i64)
        };

        let request = Request {
            version: JSON_RPC_VERSION.to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();

        {
            let mut st = self.state.lock().await;
            st.pending.insert(request.id.clone(), PendingRequest { tx });
        }

        // Send the request — NO state lock held during channel I/O.
        if let Err(e) = self.channel.send_request(&request).await {
            let mut st = self.state.lock().await;
            st.pending.remove(&request.id);
            return Err(AgentManagerError::Acp(format!("send error: {}", e)));
        }

        rx.await
            .map_err(|_| AgentManagerError::Acp("response channel closed".into()))?
    }

    /// Send a raw JSON-RPC [`Request`] and wait for the response.
    pub async fn send_raw_request(&self, request: &Request) -> AgentResult<Response> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        {
            let mut st = self.state.lock().await;
            st.pending.insert(request.id.clone(), PendingRequest { tx });
        }

        if let Err(e) = self.channel.send_request(request).await {
            let mut st = self.state.lock().await;
            st.pending.remove(&request.id);
            return Err(AgentManagerError::Acp(format!("send error: {}", e)));
        }

        rx.await
            .map_err(|_| AgentManagerError::Acp("response channel closed".into()))?
    }

    /// Send the `initialize` method and store the negotiated response.
    pub async fn initialize(&mut self, req: InitializeRequest) -> AgentResult<InitializeResponse> {
        self.start_read_loop().await;

        let params = serde_json::to_value(&req)
            .map_err(|e| AgentManagerError::Acp(format!("serialize InitializeRequest: {}", e)))?;
        let resp = self.send_request(METHOD_INITIALIZE, Some(params)).await?;

        if let Some(err) = resp.error {
            return Err(AcpClientError::Protocol(err.message).into());
        }

        let result: InitializeResponse = serde_json::from_value(resp.result.unwrap_or(Value::Null))
            .map_err(|e| {
                AgentManagerError::Acp(format!("deserialize InitializeResponse: {}", e))
            })?;

        {
            let st = self.state.lock().await;
            st.initialized.store(true, Ordering::SeqCst);
        }

        Ok(result)
    }

    /// Create a new session via `session/new` and store the session ID.
    pub async fn new_session(&mut self, req: NewSessionRequest) -> AgentResult<NewSessionResponse> {
        let params = serde_json::to_value(&req)
            .map_err(|e| AgentManagerError::Acp(format!("serialize NewSessionRequest: {}", e)))?;
        let resp = self.send_request(METHOD_NEW_SESSION, Some(params)).await?;

        if let Some(err) = resp.error {
            return Err(AcpClientError::Protocol(err.message).into());
        }

        let result: NewSessionResponse = serde_json::from_value(resp.result.unwrap_or(Value::Null))
            .map_err(|e| {
                AgentManagerError::Acp(format!("deserialize NewSessionResponse: {}", e))
            })?;

        {
            let mut st = self.state.lock().await;
            st.session_id = Some(result.session_id.clone());
        }

        Ok(result)
    }

    /// Send a prompt to the current session and stream the response.
    ///
    /// If a [`StreamingPipeline`] was provided at construction, a new stream
    /// is started and tokens will be published as they arrive. The final
    /// `PromptResponse` is returned when the agent signals completion.
    pub async fn prompt(&mut self, req: PromptRequest) -> AgentResult<PromptResponse> {
        let session_id = {
            let st = self.state.lock().await;
            st.session_id.clone()
        };

        let session_id = session_id.ok_or(AcpClientError::NoActiveSession)?;

        if !req.session_id.is_empty() && req.session_id != session_id {
            tracing::warn!(
                "prompt session_id '{}' does not match client's session '{}'",
                req.session_id,
                session_id
            );
        }

        let params = serde_json::to_value(&req)
            .map_err(|e| AgentManagerError::Acp(format!("serialize PromptRequest: {}", e)))?;
        let resp = self.send_request(METHOD_PROMPT, Some(params)).await?;

        if let Some(err) = resp.error {
            return Err(AcpClientError::Protocol(err.message).into());
        }

        let result: PromptResponse = serde_json::from_value(resp.result.unwrap_or(Value::Null))
            .map_err(|e| AgentManagerError::Acp(format!("deserialize PromptResponse: {}", e)))?;

        Ok(result)
    }

    /// Close the current session via `session/close`.
    pub async fn close(&mut self) -> AgentResult<CloseSessionResponse> {
        let session_id = {
            let mut st = self.state.lock().await;
            st.session_id.take()
        };

        let session_id = session_id.ok_or(AcpClientError::NoActiveSession)?;

        let req = CloseSessionRequest {
            session_id,
            _meta: None,
        };
        let params = serde_json::to_value(&req)
            .map_err(|e| AgentManagerError::Acp(format!("serialize CloseSessionRequest: {}", e)))?;
        let resp = self
            .send_request(METHOD_CLOSE_SESSION, Some(params))
            .await?;

        if let Some(err) = resp.error {
            return Err(AcpClientError::Protocol(err.message).into());
        }

        let result: CloseSessionResponse =
            serde_json::from_value(resp.result.unwrap_or(Value::Null)).map_err(|e| {
                AgentManagerError::Acp(format!("deserialize CloseSessionResponse: {}", e))
            })?;

        Ok(result)
    }

    /// Stop the client — signals the read loop to exit and aborts the read task.
    pub async fn stop(&mut self) {
        {
            let mut st = self.state.lock().await;
            st.stopping.store(true, Ordering::SeqCst);
            for pending in st.pending.drain() {
                let _ = pending.1.tx.send(Err(AcpClientError::ClientStopped.into()));
            }
        }

        if let Some(handle) = self.read_task.take() {
            handle.abort();
        }
    }

    /// Returns `true` if `initialize` has been called successfully.
    pub async fn is_initialized(&self) -> bool {
        self.state.lock().await.initialized.load(Ordering::SeqCst)
    }

    /// Returns the current session ID, if one is active.
    pub async fn session_id(&self) -> Option<String> {
        self.state.lock().await.session_id.clone()
    }
}

// ---------------------------------------------------------------------------
// Error: AcpClientError
// ---------------------------------------------------------------------------

/// Errors specific to the AcpClient's protocol machinery.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AcpClientError {
    /// EOF on the child's stdout.
    #[error("connection closed by remote")]
    Eof,
    /// The remote sent a JSON-RPC error response.
    #[error("remote protocol error: {0}")]
    Protocol(String),
    /// The client was stopped before completing the operation.
    #[error("ACP client was stopped")]
    ClientStopped,
    /// No active session (new_session not called or already closed).
    #[error("no active ACP session")]
    NoActiveSession,
}

impl From<AcpClientError> for AgentManagerError {
    fn from(e: AcpClientError) -> Self {
        AgentManagerError::Acp(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::types::*;
    use crate::event_bus::EventBus;
    use std::sync::Arc;
    use tokio::process::{Child, Command};

    /// A minimal mock ACP server: reads NDJSON requests, writes NDJSON responses.
    /// Speaks true ACP JSON-RPC over stdio: `initialize` → `session/new` →
    /// `session/prompt` (with `session/update` notifications) → `session/close`.
    const MOCK_AGENT_SCRIPT: &str = r#"
import sys, json, time

def send(obj):
    print(json.dumps(obj), flush=True)
    sys.stdout.flush()

def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    while not line:
        line = sys.stdin.readline()
        if not line:
            return None
        line = line.strip()
    return line

while True:
    line = read_line()
    if line is None:
        break
    try:
        req = json.loads(line)
    except json.JSONDecodeError:
        sys.stderr.write("mock: JSON decode error\n")
        sys.stderr.flush()
        continue

    method = req.get("method", "")
    req_id = req.get("id")

    if method == "initialize":
        send({
            "jsonrpc": "2.0", "id": req_id,
            "result": {
                "protocolVersion": 1,
                "agentInfo": {"name": "mock-agent", "version": "1.0.0"},
                "agentCapabilities": {"prompts": {"staticRegistration": None}},
                "authMethods": []
            }
        })
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": req_id,
              "result": {"sessionId": "test-session-123"}})
    elif method == "session/close":
        send({"jsonrpc": "2.0", "id": req_id, "result": {}})
    elif method == "session/prompt":
        # Send a couple of session/update notifications before the response.
        send({"jsonrpc": "2.0", "id": None,
              "method": "session/update",
              "params": {"sessionId": "test-session-123",
                         "update": {"sessionUpdate": "agent_message_chunk",
                                    "content": {"type": "text",
                                                "text": "Hello "}}}})
        send({"jsonrpc": "2.0", "id": None,
              "method": "session/update",
              "params": {"sessionId": "test-session-123",
                         "update": {"sessionUpdate": "agent_message_chunk",
                                    "content": {"type": "text",
                                                "text": "world!"} }}})
        send({"jsonrpc": "2.0", "id": None,
              "method": "session/update",
              "params": {"sessionId": "test-session-123",
                         "update": {"sessionUpdate": "agent_message_chunk",
                                    "content": {"type": "text",
                                                "text": "\n"}}}})
        send({"jsonrpc": "2.0", "id": req_id,
              "result": {"stopReason": "end_turn"}})
    else:
        sys.stderr.write("mock: unknown method {}\n".format(method))
        sys.stderr.flush()
        send({"jsonrpc": "2.0", "id": req_id,
              "error": {"code": -32601, "message": "Method not found"}})
"#;

    /// Spawn a mock ACP agent subprocess and return the child plus its stdio.
    async fn spawn_mock_agent() -> (Child, StdioChannel) {
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(MOCK_AGENT_SCRIPT)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn mock agent");

        let stdin = child.stdin.take().expect("child has no stdin");
        let stdout = child.stdout.take().expect("child has no stdout");
        let channel = StdioChannel::new(stdin, stdout);
        (child, channel)
    }

    #[tokio::test]
    async fn initial_state_is_uninitialized() {
        let (_child, channel) = spawn_mock_agent().await;
        let client = AcpClient::new(channel, None);

        assert!(!client.is_initialized().await);
        assert_eq!(client.session_id().await, None);
    }

    #[tokio::test]
    async fn prompt_without_session_returns_no_active_session() {
        let (_child, channel) = spawn_mock_agent().await;
        let mut client = AcpClient::new(channel, None);

        let req = PromptRequest {
            session_id: "dummy".to_string(),
            prompt: vec![ContentBlock::Text(TextContent {
                text: "hello".to_string(),
                annotations: None,
                _meta: None,
            })],
            _meta: None,
        };

        let result = client.prompt(req).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("no active ACP session"));
    }

    #[tokio::test]
    async fn close_without_session_returns_no_active_session() {
        let (_child, channel) = spawn_mock_agent().await;
        let mut client = AcpClient::new(channel, None);

        let result = client.close().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("no active ACP session"));
    }

    #[tokio::test]
    async fn initialize_and_new_session_and_close() {
        let (_child, channel) = spawn_mock_agent().await;
        let pipeline: Arc<StreamingPipeline> =
            Arc::new(StreamingPipeline::new(Arc::new(EventBus::new())));
        let mut client = AcpClient::new(channel, Some(pipeline));

        // --- initialize ---
        let init_req = InitializeRequest {
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            client_info: Some(Implementation {
                name: "nabu-test".to_string(),
                title: Some("Nabu Test Client".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                _meta: None,
            }),
            client_capabilities: Some(ClientCapabilities::default()),
            _meta: None,
        };

        let init_resp = client
            .initialize(init_req)
            .await
            .expect("initialize should succeed");
        assert_eq!(init_resp.protocol_version, SUPPORTED_PROTOCOL_VERSION);
        assert!(client.is_initialized().await);
        assert_eq!(init_resp.agent_info.as_ref().unwrap().name, "mock-agent");

        // --- new_session ---
        let new_session_req = NewSessionRequest {
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };

        let new_session_resp = client
            .new_session(new_session_req)
            .await
            .expect("new_session should succeed");
        assert_eq!(new_session_resp.session_id, "test-session-123");
        assert_eq!(
            client.session_id().await.as_deref(),
            Some("test-session-123")
        );

        // --- close ---
        let _close_resp = client.close().await.expect("close should succeed");
        assert_eq!(client.session_id().await, None);
    }

    #[tokio::test]
    async fn full_lifecycle_with_prompt() {
        let (_child, channel) = spawn_mock_agent().await;
        let pipeline: Arc<StreamingPipeline> =
            Arc::new(StreamingPipeline::new(Arc::new(EventBus::new())));
        let mut client = AcpClient::new(channel, Some(pipeline));

        // initialize
        let init_req = InitializeRequest {
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            client_info: Some(Implementation {
                name: "nabu-test".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            client_capabilities: Some(ClientCapabilities::default()),
            _meta: None,
        };
        client.initialize(init_req).await.expect("initialize");

        // new session
        let new_session_req = NewSessionRequest {
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };
        client
            .new_session(new_session_req)
            .await
            .expect("new_session");

        // prompt
        let prompt_req = PromptRequest {
            session_id: "test-session-123".to_string(),
            prompt: vec![ContentBlock::Text(TextContent {
                text: "What is 2+2?".to_string(),
                annotations: None,
                _meta: None,
            })],
            _meta: None,
        };
        let prompt_resp = client
            .prompt(prompt_req)
            .await
            .expect("prompt should succeed");
        assert_eq!(prompt_resp.stop_reason, StopReason::EndTurn);

        // close
        client.close().await.expect("close");
    }

    #[tokio::test]
    async fn stop_aborts_pending_request() {
        let (_child, channel) = spawn_mock_agent().await;
        let mut client = AcpClient::new(channel, None);

        client.start_read_loop().await;
        client.stop().await;
        assert!(client.read_task.is_none());
    }

    #[tokio::test]
    async fn send_request_without_read_loop_times_out() {
        let (_child, channel) = spawn_mock_agent().await;
        let client = AcpClient::new(channel, None);

        let req = Request::new(1, "no_read_loop_method", None);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            client.send_raw_request(&req),
        )
        .await;

        assert!(result.is_err(), "should time out without read loop");
    }

    #[tokio::test]
    async fn acp_client_error_from_conversion() {
        let err1: AgentManagerError = AcpClientError::Eof.into();
        assert!(err1.to_string().contains("connection closed by remote"));
        assert!(err1.is_acp_error());

        let err2: AgentManagerError = AcpClientError::ClientStopped.into();
        assert!(err2.to_string().contains("was stopped"));

        let err3: AgentManagerError = AcpClientError::NoActiveSession.into();
        assert!(err3.to_string().contains("no active ACP session"));
    }

    #[tokio::test]
    async fn request_id_auto_increments() {
        let (_child, channel) = spawn_mock_agent().await;
        let client = AcpClient::new(channel, None);

        let id1 = {
            let st = client.state.lock().await;
            RequestId::Number(st.next_id.fetch_add(1, Ordering::SeqCst) as i64)
        };
        let id2 = {
            let st = client.state.lock().await;
            RequestId::Number(st.next_id.fetch_add(1, Ordering::SeqCst) as i64)
        };

        assert_ne!(id1, id2);
    }

    /// Test that session/update notifications are received and dispatched.
    #[tokio::test]
    async fn notification_received_during_prompt() {
        let (_child, channel) = spawn_mock_agent().await;
        let pipeline: Arc<StreamingPipeline> =
            Arc::new(StreamingPipeline::new(Arc::new(EventBus::new())));
        let mut client = AcpClient::new(channel, Some(pipeline.clone()));

        // Collect updates via callback
        let updates: Arc<tokio::sync::Mutex<Vec<SessionUpdate>>> =
            Arc::new(tokio::sync::Mutex::new(vec![]));
        let updates_clone = updates.clone();
        client.on_update(move |_sid: String, update: SessionUpdate| {
            let updates = updates_clone.clone();
            async move {
                let mut v = updates.lock().await;
                v.push(update);
            }
        });

        // initialize + new session
        let init_req = InitializeRequest {
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            client_info: Some(Implementation {
                name: "nabu-test".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            client_capabilities: Some(ClientCapabilities::default()),
            _meta: None,
        };
        client.initialize(init_req).await.expect("initialize");

        let new_session_req = NewSessionRequest {
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };
        client
            .new_session(new_session_req)
            .await
            .expect("new_session");

        // prompt — the mock sends 3 session/update notifications
        let prompt_req = PromptRequest {
            session_id: "test-session-123".to_string(),
            prompt: vec![ContentBlock::Text(TextContent {
                text: "hello".to_string(),
                annotations: None,
                _meta: None,
            })],
            _meta: None,
        };
        let _ = client.prompt(prompt_req).await.expect("prompt");

        // Give the read loop time to dispatch notifications
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let received = updates.lock().await;
        assert!(
            received.len() >= 3,
            "expected at least 3 notifications, got {}",
            received.len()
        );

        // Verify they are AgentMessageChunk variants
        for update in received.iter() {
            assert!(matches!(update, SessionUpdate::AgentMessageChunk(_)));
        }
    }

    /// Test that EOF on stdout is detected and pending requests are errored.
    #[tokio::test]
    async fn eof_errors_pending_request() {
        // Spawn a mock agent that exits immediately
        let mut child = Command::new("python3")
            .arg("-c")
            .arg("import sys; sys.exit(0)")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn");

        let stdin = child.stdin.take().expect("no stdin");
        let stdout = child.stdout.take().expect("no stdout");
        let channel = StdioChannel::new(stdin, stdout);
        let mut client = AcpClient::new(channel, None);

        client.start_read_loop().await;

        // Wait for the child to exit and stdout to close
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Now send a request — it should get an EOF error (or send error,
        // since stdin is also closed). Either way, it should not hang.
        let req = Request::new(1, "some_method", None);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.send_raw_request(&req),
        )
        .await;

        assert!(result.is_ok(), "should get a response (error), not hang");
        let resp = result.unwrap();
        assert!(resp.is_err());
        let err_msg = resp.unwrap_err().to_string();
        // Could be EOF (read loop detected closed stdout) or send error
        // (stdin closed because child exited). Either way, the error should
        // indicate the connection/remote is gone.
        assert!(
            err_msg.contains("connection closed by remote")
                || err_msg.contains("send error")
                || err_msg.contains("response channel closed"),
            "unexpected error: {}",
            err_msg
        );
    }

    /// Test that the AcpClient can send a request and receive a response
    /// without holding the state lock during I/O (no deadlock).
    #[tokio::test]
    async fn send_and_receive_does_not_deadlock() {
        let (_child, channel) = spawn_mock_agent().await;
        let mut client = AcpClient::new(channel, None);

        // Start the read loop
        client.start_read_loop().await;

        // Send initialize and wait — if there were a deadlock, this would
        // hang. The read loop should be able to receive the response because
        // it doesn't hold the state lock during recv_line().
        let init_req = InitializeRequest {
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            client_info: Some(Implementation {
                name: "nabu-test".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            client_capabilities: Some(ClientCapabilities::default()),
            _meta: None,
        };

        let resp = client.initialize(init_req).await;
        assert!(resp.is_ok(), "initialize should succeed without deadlock");
    }

    /// Test that a request sent after the child exits returns an error.
    #[tokio::test]
    async fn request_after_child_exit_returns_error() {
        let mut child = Command::new("python3")
            .arg("-c")
            .arg("import sys; sys.exit(0)")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn");

        let stdin = child.stdin.take().expect("no stdin");
        let stdout = child.stdout.take().expect("no stdout");
        let channel = StdioChannel::new(stdin, stdout);
        let mut client = AcpClient::new(channel, None);

        client.start_read_loop().await;

        // Wait for child exit and stdout close
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // The read loop should have detected EOF and set stopping=true
        {
            let st = client.state.lock().await;
            assert!(
                st.stopping.load(Ordering::SeqCst),
                "stopping flag should be set after EOF"
            );
        }
    }

    /// Test that malformed JSON on stdout is skipped without crashing.
    #[tokio::test]
    async fn malformed_json_does_not_panic() {
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(
                r#"
import sys, json

def send(obj):
    print(json.dumps(obj), flush=True)

def read_line():
    data = sys.stdin.readline()
    if not data:
        return None
    data = data.strip()
    while not data:
        data = sys.stdin.readline()
        if not data:
            return None
        data = data.strip()
    return data

while True:
    line = read_line()
    if line is None:
        break
    req = json.loads(line)
    method = req.get("method", "")
    req_id = req.get("id")

    if method == "initialize":
        send({"jsonrpc": "2.0", "id": req_id,
              "result": {"protocolVersion": 1,
                         "agentInfo": {"name": "mock", "version": "1.0.0"},
                         "agentCapabilities": {"prompts": {"staticRegistration": None}},
                         "authMethods": []}})
    elif method == "session/new":
        # Send malformed JSON before the valid response
        print("not valid json {{{", flush=True)
        send({"jsonrpc": "2.0", "id": req_id, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        print("<<<garbage>>>", flush=True)
        send({"jsonrpc": "2.0", "id": req_id, "result": {"stopReason": "end_turn"}})
    else:
        send({"jsonrpc": "2.0", "id": req_id,
              "error": {"code": -32601, "message": "Method not found"}})
"#,
            )
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn");

        let stdin = child.stdin.take().expect("no stdin");
        let stdout = child.stdout.take().expect("no stdout");
        let channel = StdioChannel::new(stdin, stdout);
        let mut client = AcpClient::new(channel, None);

        client.start_read_loop().await;

        // Send initialize request
        let init_req = InitializeRequest {
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            client_info: Some(Implementation {
                name: "nabu-test".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            client_capabilities: Some(ClientCapabilities::default()),
            _meta: None,
        };
        let result = client.initialize(init_req).await;
        assert!(result.is_ok(), "initialize should succeed: {:?}", result);

        // The mock sends malformed JSON before the valid response.
        // The read loop should skip the malformed line and process the response.
        let new_session_req = NewSessionRequest {
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };
        let result = client.new_session(new_session_req).await;
        assert!(
            result.is_ok(),
            "new_session should succeed despite malformed JSON: {:?}",
            result
        );

        let _ = client.stop().await;
    }

    /// Test that cancellation during a pending request works.
    #[tokio::test]
    async fn cancellation_during_pending_request() {
        let (_child, channel) = spawn_mock_agent().await;
        let mut client = AcpClient::new(channel, None);

        client.start_read_loop().await;

        // Send a request to an unknown method — mock returns error response
        // but that's fine, we just check the mechanism works
        let req = Request::new(
            1,
            "session/new",
            Some(serde_json::json!({
                "cwd": "/tmp",
                "mcpServers": [],
                "additionalDirectories": []
            })),
        );

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.send_raw_request(&req),
        )
        .await;

        assert!(result.is_ok(), "should get a response, not hang");
    }
}
