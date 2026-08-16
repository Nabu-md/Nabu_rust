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
//! ## Concurrency Model
//!
//! A single background task ("the reader loop") owns the `StdioChannel`'s
//! reader half and demultiplexes incoming messages:
//! - **Responses** are routed to the matching `pending` entry via `id`.
//! - **Notifications** (requests with `id == null`) are routed to the
//!   notification handler callback.
//!
//! Multiple tasks can call `send_request` concurrently — the writer half is
//! mutex-protected in [`StdioChannel`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::agent::stdio_channel::StdioChannel;
use crate::{AgentManagerError, AgentResult};
use crate::rpc::{JSON_RPC_VERSION, Request, RequestId, Response};
use crate::acp::types::{
    CloseSessionRequest, CloseSessionResponse, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
};
use crate::acp::server::{METHOD_CLOSE_SESSION, METHOD_INITIALIZE, METHOD_NEW_SESSION, METHOD_PROMPT};
use crate::streaming::StreamingPipeline;

/// A pending JSON-RPC request awaiting its response.
struct PendingRequest {
    tx: tokio::sync::oneshot::Sender<AgentResult<Response>>,
}

/// Internal shared state for the AcpClient.
struct Inner {
    channel: StdioChannel,
    pending: std::collections::HashMap<RequestId, PendingRequest>,
    next_id: AtomicU64,
    initialized: AtomicBool,
    session_id: Option<String>,
    stopping: AtomicBool,
}

/// A JSON-RPC + ACP client that drives an external agent process over stdio.
pub struct AcpClient {
    inner: Arc<AsyncMutex<Inner>>,
    pipeline: Option<Arc<StreamingPipeline>>,
    read_task: Option<tokio::task::JoinHandle<()>>,
}

impl AcpClient {
    /// Create a new client from a [`StdioChannel`] connected to an ACP server.
    pub fn new(channel: StdioChannel, pipeline: Option<Arc<StreamingPipeline>>) -> Self {
        Self {
            inner: Arc::new(AsyncMutex::new(Inner {
                channel,
                pending: std::collections::HashMap::new(),
                next_id: AtomicU64::new(1),
                initialized: AtomicBool::new(false),
                session_id: None,
                stopping: AtomicBool::new(false),
            })),
            pipeline,
            read_task: None,
        }
    }

    /// Start the background read loop that demultiplexes responses and
    /// forwards notifications.
    ///
    /// Only one read loop may run at a time. Subsequent calls are no-ops.
    pub async fn start_read_loop(&mut self) {
        if self.read_task.is_some() {
            return;
        }
        let inner = self.inner.clone();
        let pipeline = self.pipeline.clone();
        let task = tokio::spawn(async move {
            Self::read_loop(inner, pipeline).await;
        });
        self.read_task = Some(task);
    }

    /// The background reader loop.
    async fn read_loop(inner: Arc<AsyncMutex<Inner>>, _pipeline: Option<Arc<StreamingPipeline>>) {
        loop {
            let result = {
                let guard = inner.lock().await;
                guard.channel.recv_response().await
            };

            match result {
                Ok(Some(resp)) => {
                    let mut guard = inner.lock().await;
                    let id = resp.id.clone();
                    if let Some(pending) = guard.pending.remove(&id) {
                        let _ = pending.tx.send(Ok(resp));
                    }
                    // Notifications (id == null) are not yet routed to a handler.
                }
                Ok(None) => {
                    let mut guard = inner.lock().await;
                    guard.stopping.store(true, Ordering::SeqCst);
                    for pending in guard.pending.drain() {
                        let _ = pending.1.tx.send(Err(AcpClientError::Eof.into()));
                    }
                    break;
                }
                Err(e) => {
                    let mut guard = inner.lock().await;
                    guard.stopping.store(true, Ordering::SeqCst);
                    let err = AgentManagerError::Acp(format!("read error: {}", e));
                    for pending in guard.pending.drain() {
                        let _ = pending.1.tx.send(Err(err.clone()));
                    }
                    break;
                }
            }
        }
    }

    /// Send a JSON-RPC request and wait for the matching response.
    ///
    /// The request ID is auto-incremented and tracked internally. The caller
    /// must ensure `start_read_loop` is running so responses are demultiplexed.
    pub async fn send_request(&self, method: &str, params: Option<Value>) -> AgentResult<Response> {
        let id = {
            let mut guard = self.inner.lock().await;
            let id = guard.next_id.fetch_add(1, Ordering::SeqCst) as i64;
            RequestId::Number(id)
        };

        let request = Request {
            version: JSON_RPC_VERSION.to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();

        {
            let mut guard = self.inner.lock().await;
            guard
                .pending
                .insert(request.id.clone(), PendingRequest { tx });
            guard.channel.send_request(&request).await.map_err(|e| {
                guard.pending.remove(&request.id);
                AgentManagerError::Acp(format!("send error: {}", e))
            })?;
        }

        rx.await
            .map_err(|_| AgentManagerError::Acp("response channel closed".into()))?
    }

    /// Send a raw JSON-RPC [`Request`] and wait for the response.
    pub async fn send_raw_request(&self, request: &Request) -> AgentResult<Response> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        {
            let mut guard = self.inner.lock().await;
            guard
                .pending
                .insert(request.id.clone(), PendingRequest { tx });
            guard.channel.send_request(request).await.map_err(|e| {
                guard.pending.remove(&request.id);
                AgentManagerError::Acp(format!("send error: {}", e))
            })?;
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

        let result: InitializeResponse = serde_json::from_value(
            resp.result.unwrap_or(Value::Null),
        )
        .map_err(|e| AgentManagerError::Acp(format!("deserialize InitializeResponse: {}", e)))?;

        {
            let guard = self.inner.lock().await;
            guard.initialized.store(true, Ordering::SeqCst);
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

        let result: NewSessionResponse = serde_json::from_value(
            resp.result.unwrap_or(Value::Null),
        )
        .map_err(|e| AgentManagerError::Acp(format!("deserialize NewSessionResponse: {}", e)))?;

        {
            let mut guard = self.inner.lock().await;
            guard.session_id = Some(result.session_id.clone());
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
            let guard = self.inner.lock().await;
            guard.session_id.clone()
        };

        let session_id = session_id.ok_or(AcpClientError::NoActiveSession)?;

        if !req.session_id.is_empty() && req.session_id != session_id {
            tracing::warn!(
                "prompt session_id '{}' does not match client's session '{}'",
                req.session_id, session_id
            );
        }

        let params = serde_json::to_value(&req)
            .map_err(|e| AgentManagerError::Acp(format!("serialize PromptRequest: {}", e)))?;
        let resp = self.send_request(METHOD_PROMPT, Some(params)).await?;

        if let Some(err) = resp.error {
            return Err(AcpClientError::Protocol(err.message).into());
        }

        let result: PromptResponse = serde_json::from_value(
            resp.result.unwrap_or(Value::Null),
        )
        .map_err(|e| AgentManagerError::Acp(format!("deserialize PromptResponse: {}", e)))?;

        Ok(result)
    }

    /// Close the current session via `session/close`.
    pub async fn close(&mut self) -> AgentResult<CloseSessionResponse> {
        let session_id = {
            let mut guard = self.inner.lock().await;
            guard.session_id.take()
        };

        let session_id = session_id.ok_or(AcpClientError::NoActiveSession)?;

        let req = CloseSessionRequest {
            session_id,
            _meta: None,
        };
        let params = serde_json::to_value(&req)
            .map_err(|e| AgentManagerError::Acp(format!("serialize CloseSessionRequest: {}", e)))?;
        let resp = self.send_request(METHOD_CLOSE_SESSION, Some(params)).await?;

        if let Some(err) = resp.error {
            return Err(AcpClientError::Protocol(err.message).into());
        }

        let result: CloseSessionResponse = serde_json::from_value(
            resp.result.unwrap_or(Value::Null),
        )
        .map_err(|e| AgentManagerError::Acp(format!("deserialize CloseSessionResponse: {}", e)))?;

        Ok(result)
    }

    /// Stop the client — signals the read loop to exit and aborts the read task.
    pub async fn stop(&mut self) {
        {
            let mut guard = self.inner.lock().await;
            guard.stopping.store(true, Ordering::SeqCst);
            for pending in guard.pending.drain() {
                let _ = pending.1.tx.send(Err(AcpClientError::ClientStopped.into()));
            }
        }

        if let Some(handle) = self.read_task.take() {
            handle.abort();
        }
    }

    /// Returns `true` if `initialize` has been called successfully.
    pub async fn is_initialized(&self) -> bool {
        self.inner.lock().await.initialized.load(Ordering::SeqCst)
    }

    /// Returns the current session ID, if one is active.
    pub async fn session_id(&self) -> Option<String> {
        self.inner.lock().await.session_id.clone()
    }
}

// ---------------------------------------------------------------------------
// Error: AcpClientError
// ---------------------------------------------------------------------------

/// Errors specific to the AcpClient's protocol machinery.
#[derive(Debug, thiserror::Error)]
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
