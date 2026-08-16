//! # ACP Client — Nabu as an ACP Client
//!
//! This module implements Nabu's role as an ACP **client** that communicates
//! with an external ACP **agent** over JSON-RPC 2.0 via stdio.
//!
//! ## Lifecycle
//!
//! ```text
//! let client = AcpClient::new(transport, handler);  → Disconnected
//!   client.initialize(info, caps).await;            → Connected
//!   client.new_session("/workspace").await;          → session created
//!   client.prompt(session_id, text).await;          → agent turn
//!   client.close_session(session_id).await;         → session closed
//!   client.shutdown().await;                        → Closed
//! ```
//!
//! ## Concurrency
//!
//! On `initialize()`, the client spawns a background **message loop** task
//! that reads JSON-RPC messages from the transport. The loop dispatches:
//! - Responses to pending requests (via per-request oneshot channels)
//! - Notifications (e.g. `session/update`) to a user-provided callback
//! - Agent→client requests (e.g. `fs/read_text_file`) to the handler trait
//!
//! The client's public methods are `async` and communicate with the message
//! loop via an outbound channel.

use crate::acp::error::{AcpError, ErrorKind};
use crate::acp::events::{
    classify_message, classify_notification, InboundMessage, NotificationKind,
};
use crate::acp::handler::AcpClientHandler;
use crate::acp::state::{ClientState, NegotiatedCapabilities};
use crate::acp::transport::Transport;
use crate::acp::types::*;
use crate::rpc::{JsonRpcError, RequestId};

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// A pending request waiting for a response from the agent.
struct PendingEntry {
    tx: oneshot::Sender<Result<Value, JsonRpcError>>,
}

/// An outbound message with an optional response channel.
struct OutboundMessage {
    msg: Value,
    /// If `Some`, the message loop sends the result of the write here.
    ack: Option<oneshot::Sender<Result<(), AcpError>>>,
}

/// The callback type for receiving `session/update` notifications from the agent.
type UpdateCallback = Arc<
    dyn Fn(&str, SessionUpdate) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// The ACP client — Nabu's connection to an external ACP agent.
///
/// Created with [`AcpClient::new`] and connected via [`AcpClient::initialize`].
///
/// The client owns a background message loop task that reads JSON-RPC
/// messages from the transport and dispatches them. All public methods are
/// `async`.
pub struct AcpClient<T: Transport> {
    /// Outbound message channel — client → message loop → transport.
    outbound_tx: Option<mpsc::UnboundedSender<OutboundMessage>>,
    /// Pending request responses, keyed by request ID string.
    pending: Arc<tokio::sync::Mutex<std::collections::HashMap<String, PendingEntry>>>,
    /// Next request ID counter.
    next_id: Arc<tokio::sync::atomic::AtomicI64>,
    /// Shared connection state.
    state: Arc<tokio::sync::Mutex<ClientState>>,
    /// The handler for agent→client requests.
    handler: Arc<dyn AcpClientHandler>,
    /// Callback for session/update notifications.
    update_callback: Option<UpdateCallback>,
    /// Message loop task handle.
    loop_handle: Option<tokio::task::JoinHandle<()>>,
    /// The negotiated protocol version.
    protocol_version: ProtocolVersion,
    /// The transport (stored until initialize() starts the loop).
    transport: Option<T>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Transport + 'static> AcpClient<T> {
    /// Create a new ACP client.
    ///
    /// The client is in the `Disconnected` state. Call [`initialize`] to
    /// complete the handshake.
    ///
    /// [`initialize`]: AcpClient::initialize
    pub fn new(transport: T, handler: Arc<dyn AcpClientHandler>) -> Self {
        Self {
            outbound_tx: None,
            pending: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            next_id: Arc::new(tokio::sync::atomic::AtomicI64::new(0)),
            state: Arc::new(tokio::sync::Mutex::new(ClientState::new())),
            handler,
            update_callback: None,
            loop_handle: None,
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            transport: Some(transport),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set a callback to receive `session/update` notifications.
    ///
    /// The callback is called from the message loop task. It should be cheap
    /// and non-blocking — use a channel or spawn a task if heavy work is
    /// needed.
    pub fn on_update<F, Fut>(&mut self, callback: F)
    where
        F: Fn(&str, SessionUpdate) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let cb: UpdateCallback = Arc::new(move |sid, update| {
            let fut = callback(sid, update);
            Box::pin(async move { fut.await })
        });
        self.update_callback = Some(cb);
    }

    // -----------------------------------------------------------------------
    // Public protocol API
    // -----------------------------------------------------------------------

    /// Send the `initialize` request and wait for the response.
    ///
    /// This negotiates the protocol version and capabilities with the agent.
    /// After a successful initialization, the client transitions to the
    /// `Connected` state and the background message loop starts.
    pub async fn initialize(
        &mut self,
        client_info: Option<Implementation>,
        client_capabilities: Option<ClientCapabilities>,
    ) -> Result<InitializeResponse, AcpError> {
        // Transition to Initializing state
        {
            let mut state = self.state.lock().await;
            state
                .transition_to_initializing()
                .map_err(|e| AcpError::invalid_state(e.to_string()))?;
        }

        // Start the message loop (takes ownership of the transport)
        let transport = self
            .transport
            .take()
            .ok_or_else(|| AcpError::invalid_state("transport already started"))?;

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<OutboundMessage>();
        self.outbound_tx = Some(outbound_tx);

        let handler = self.handler.clone();
        let pending = self.pending.clone();
        let state = self.state.clone();
        let update_cb = self.update_callback.clone();

        let loop_handle = tokio::spawn(async move {
            run_message_loop(transport, outbound_rx, pending, state, handler, update_cb).await;
        });
        self.loop_handle = Some(loop_handle);

        // Send the initialize request
        let req = InitializeRequest {
            protocol_version: self.protocol_version,
            client_capabilities,
            client_info,
            _meta: None,
        };

        let resp = self
            .send_request(METHOD_INITIALIZE, &json!(req))
            .await?
            .ok_or_else(|| AcpError::malformed_response("initialize response missing result"))?;

        let init_resp: InitializeResponse = serde_json::from_value(resp)?;

        // Verify protocol version compatibility
        if init_resp.protocol_version != self.protocol_version {
            return Err(AcpError::protocol_version_mismatch(
                self.protocol_version,
                init_resp.protocol_version,
            ));
        }

        // Transition to Connected state
        let caps = NegotiatedCapabilities {
            protocol_version: init_resp.protocol_version,
            agent_capabilities: init_resp.agent_capabilities.clone(),
            client_capabilities: self
                .state
                .lock()
                .await
                .capabilities
                .as_ref()
                .and_then(|c| c.client_capabilities.clone()),
            agent_info: init_resp.agent_info.clone(),
            client_info: None,
            _meta: None,
        };

        {
            let mut state = self.state.lock().await;
            state
                .transition_to_connected(caps)
                .map_err(|e| AcpError::invalid_state(e.to_string()))?;
        }

        Ok(init_resp)
    }

    /// Create a new session by sending `session/new`.
    ///
    /// Returns the server-assigned session ID.
    pub async fn new_session(
        &self,
        cwd: &str,
        mcp_servers: Vec<McpServer>,
        additional_directories: Vec<String>,
    ) -> Result<String, AcpError> {
        self.ensure_connected().await?;

        let req = NewSessionRequest {
            cwd: cwd.to_string(),
            mcp_servers,
            additional_directories,
            _meta: None,
        };

        let resp = self
            .send_request(METHOD_NEW_SESSION, &json!(req))
            .await?
            .ok_or_else(|| AcpError::malformed_response("session/new: no result"))?;

        let parsed: NewSessionResponse = serde_json::from_value(resp)?;

        let mut state = self.state.lock().await;
        state.register_session(parsed.session_id.clone(), cwd.to_string());

        Ok(parsed.session_id)
    }

    /// Send a prompt to a session via `session/prompt`.
    pub async fn session_prompt(
        &self,
        session_id: &str,
        prompt: Vec<ContentBlock>,
    ) -> Result<PromptResponse, AcpError> {
        self.ensure_connected().await?;

        {
            let mut state = self.state.lock().await;
            state
                .activate_session(&session_id.to_string())
                .map_err(|e| AcpError::invalid_state(e.to_string()))?;
        }

        let req = PromptRequest {
            session_id: session_id.to_string(),
            prompt,
            _meta: None,
        };

        let resp = self
            .send_request(METHOD_PROMPT, &json!(req))
            .await?
            .ok_or_else(|| AcpError::malformed_response("session/prompt: no result"))?;

        let parsed: PromptResponse = serde_json::from_value(resp)?;
        Ok(parsed)
    }

    /// Cancel an in-progress prompt turn via `session/cancel` (notification).
    pub async fn session_cancel(&self, session_id: &str) -> Result<(), AcpError> {
        self.ensure_connected().await?;

        let notif = CancelNotification {
            session_id: session_id.to_string(),
            _meta: None,
        };

        self.send_notification(METHOD_CANCEL, &json!(notif)).await?;

        let mut state = self.state.lock().await;
        if state.active_session.as_deref() == Some(session_id) {
            state.active_session = None;
        }

        Ok(())
    }

    /// Close a session via `session/close`.
    pub async fn close_session(&self, session_id: &str) -> Result<(), AcpError> {
        self.ensure_connected().await?;

        let req = CloseSessionRequest {
            session_id: session_id.to_string(),
            _meta: None,
        };

        let _ = self.send_request(METHOD_CLOSE_SESSION, &json!(req)).await?;

        let mut state = self.state.lock().await;
        state.close_session(&session_id.to_string());

        Ok(())
    }

    /// Delete a session from the session list via `session/delete`.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), AcpError> {
        self.ensure_connected().await?;

        let req = DeleteSessionRequest {
            session_id: session_id.to_string(),
            _meta: None,
        };

        let _ = self
            .send_request(METHOD_DELETE_SESSION, &json!(req))
            .await?;

        let mut state = self.state.lock().await;
        state.remove_session(&session_id.to_string());

        Ok(())
    }

    /// List sessions via `session/list`.
    pub async fn list_sessions(&self, cwd: Option<&str>) -> Result<Vec<SessionInfo>, AcpError> {
        self.ensure_connected().await?;

        let req = ListSessionsRequest {
            cwd: cwd.map(|s| s.to_string()),
            cursor: None,
            _meta: None,
        };

        let resp = self
            .send_request(METHOD_LIST_SESSIONS, &json!(req))
            .await?
            .ok_or_else(|| AcpError::malformed_response("session/list: no result"))?;

        let parsed: ListSessionsResponse = serde_json::from_value(resp)?;
        Ok(parsed.sessions)
    }

    /// Load an existing session via `session/load`.
    pub async fn load_session(
        &self,
        session_id: &str,
        cwd: &str,
        mcp_servers: Vec<McpServer>,
        additional_directories: Vec<String>,
    ) -> Result<(), AcpError> {
        self.ensure_connected().await?;

        let req = LoadSessionRequest {
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            mcp_servers,
            additional_directories,
            _meta: None,
        };

        let _ = self.send_request(METHOD_LOAD_SESSION, &json!(req)).await?;

        let mut state = self.state.lock().await;
        state.register_session(session_id.to_string(), cwd.to_string());

        Ok(())
    }

    /// Resume an existing session via `session/resume`.
    pub async fn resume_session(
        &self,
        session_id: &str,
        cwd: &str,
        mcp_servers: Vec<McpServer>,
        additional_directories: Vec<String>,
    ) -> Result<(), AcpError> {
        self.ensure_connected().await?;

        let req = ResumeSessionRequest {
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            mcp_servers,
            additional_directories,
            _meta: None,
        };

        let _ = self
            .send_request(METHOD_RESUME_SESSION, &json!(req))
            .await?;

        let mut state = self.state.lock().await;
        state.register_session(session_id.to_string(), cwd.to_string());

        Ok(())
    }

    /// Shut down the client: stop the message loop and close the transport.
    pub async fn shutdown(mut self) -> Result<(), AcpError> {
        // Close the outbound channel (signals the message loop to stop)
        self.outbound_tx = None;

        // Wait for the message loop to finish
        if let Some(handle) = self.loop_handle.take() {
            let _ = handle.await;
        }

        let mut state = self.state.lock().await;
        state.close();

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Verify the client is in the `Connected` state.
    async fn ensure_connected(&self) -> Result<(), AcpError> {
        let state = self.state.lock().await;
        if !state.connection_state.is_connected() {
            return Err(AcpError::invalid_state(format!(
                "not connected (state: {:?})",
                state.connection_state
            )));
        }
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response.
    ///
    /// Generates a unique request ID, registers a pending entry, sends the
    /// request via the message loop's outbound channel, and awaits the
    /// response (with a 120s timeout).
    async fn send_request(&self, method: &str, params: &Value) -> Result<Option<Value>, AcpError> {
        let id_str = self.next_request_id();
        let id_val = json!(id_str);

        let (tx, rx) = oneshot::channel::<Result<Value, JsonRpcError>>();
        {
            let mut map = self.pending.lock().await;
            map.insert(id_str.clone(), PendingEntry { tx });
        }

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id_val,
            "method": method,
            "params": params
        });

        self.send_outbound(msg).await?;

        // Wait for the response (with a generous timeout)
        let timeout = std::time::Duration::from_secs(120);
        let result = tokio::time::timeout(timeout, rx).await.map_err(|_| {
            // Clean up the pending entry on timeout
            let mut map = self.pending.lock().await;
            map.remove(&id_str);
            AcpError::timeout(format!(
                "request '{}' timed out after {:?}",
                method, timeout
            ))
        })?;

        // The pending entry was consumed by the one-shot sender in the loop.
        let result = result.map_err(|_| {
            AcpError::request_cancelled(format!("request '{}' was cancelled", method))
        })?;

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rpc_err) => Err(AcpError::from(rpc_err)),
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(&self, method: &str, params: &Value) -> Result<(), AcpError> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        self.send_outbound(msg).await?;
        Ok(())
    }

    /// Push a message to the message loop's outbound channel.
    async fn send_outbound(&self, msg: Value) -> Result<(), AcpError> {
        let tx = self.outbound_tx.as_ref().ok_or_else(|| {
            AcpError::invalid_state("message loop not started; call initialize() first")
        })?;

        let (ack_tx, ack_rx) = oneshot::channel::<Result<(), AcpError>>();
        if tx
            .send(OutboundMessage {
                msg,
                ack: Some(ack_tx),
            })
            .is_err()
        {
            return Err(AcpError::transport_closed("outbound channel closed"));
        }

        // Wait for the message loop to confirm the write
        ack_rx
            .await
            .map_err(|_| AcpError::transport_closed("message loop task unavailable"))??;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Message loop
// ---------------------------------------------------------------------------

/// The background message loop that reads from and writes to the transport.
///
/// Uses `tokio::select!` to concurrently:
/// - Read inbound JSON-RPC lines from the transport
/// - Send outbound messages (client requests) to the transport
async fn run_message_loop<T: Transport>(
    mut transport: T,
    mut outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    pending: Arc<tokio::sync::Mutex<std::collections::HashMap<String, PendingEntry>>>,
    state: Arc<tokio::sync::Mutex<ClientState>>,
    handler: Arc<dyn AcpClientHandler>,
    update_callback: Option<UpdateCallback>,
) {
    loop {
        tokio::select! {
            // Outbound messages (client → agent)
            outbound_msg = outbound_rx.recv() => {
                match outbound_msg {
                    Some(OutboundMessage { msg, ack }) => {
                        let result = transport.send_json(&msg).await;
                        if let Some(ack_tx) = ack {
                            let _ = ack_tx.send(result);
                        }
                        // If the write failed, stop the loop
                        if result.is_err() {
                            tracing::error!("ACP: transport write error, stopping message loop");
                            break;
                        }
                    }
                    None => {
                        // Outbound channel closed — client is shutting down
                        break;
                    }
                }
            }

            // Inbound messages (agent → client)
            line_result = transport.read_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        process_line(
                            &mut transport,
                            &state,
                            &pending,
                            &handler,
                            &update_callback,
                            &line,
                        ).await;
                    }
                    Ok(None) => {
                        // EOF — transport closed
                        tracing::warn!("ACP: transport EOF, message loop stopping");
                        break;
                    }
                    Err(e) => {
                        tracing::error!("ACP: transport read error: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

/// Process a single line received from the transport.
async fn process_line<T: Transport>(
    transport: &mut T,
    state: &Arc<tokio::sync::Mutex<ClientState>>,
    pending: &Arc<tokio::sync::Mutex<std::collections::HashMap<String, PendingEntry>>>,
    handler: &Arc<dyn AcpClientHandler>,
    update_callback: &Option<UpdateCallback>,
    line: &str,
) {
    let classified = match classify_message(line) {
        Ok(msg) => msg,
        Err(e) => {
            tracing::warn!("ACP: failed to classify message: {} (line: {})", e, line);
            return;
        }
    };

    match classified {
        // A response to a request we sent
        InboundMessage::Response { id, result, error } => {
            let id_key = id_to_key(&id);
            let mut map = pending.lock().await;
            if let Some(entry) = map.remove(&id_key) {
                let _ = entry.tx.send(match error {
                    Some(e) => Err(e),
                    None => Ok(result.unwrap_or(Value::Null)),
                });
            } else {
                tracing::warn!("ACP: response for unknown request id: {}", id_key);
            }
        }

        // A notification from the agent (no response expected)
        InboundMessage::Notification { method, params } => {
            let kind = classify_notification(&method);
            match kind {
                NotificationKind::SessionUpdate => {
                    if let Some(update_params) = &params {
                        if let Ok(notif_params) = serde_json::from_value::<SessionNotificationParams>(
                            update_params.clone(),
                        ) {
                            if let Some(cb) = update_callback {
                                let sid = notif_params.session_id.clone();
                                let update = notif_params.update.clone();
                                cb(&sid, update).await;
                            }
                        }
                    }
                }
                NotificationKind::CancelRequest => {
                    tracing::debug!("ACP: received $/cancel_request: {:?}", params);
                }
                NotificationKind::SessionCancel => {
                    tracing::debug!("ACP: received session/cancel notification");
                }
                NotificationKind::SessionClosed => {
                    tracing::debug!("ACP: received session/closed notification");
                    let sid = params
                        .as_ref()
                        .and_then(|v| v.get("sessionId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mut s = state.lock().await;
                    s.close_session(&sid);
                }
                NotificationKind::Unknown => {
                    tracing::debug!("ACP: unknown notification: {}", method);
                }
            }
        }

        // A request from the agent to the client
        InboundMessage::AgentRequest {
            id, method, params, ..
        } => {
            let id_val = id_to_value(&id);
            let method_str = method.clone();
            let params_clone = params.clone();
            let handler_clone = handler.clone();

            let dispatch_result = crate::acp::handler::dispatch_agent_request(
                handler_clone.as_ref(),
                &method_str,
                &params_clone,
            )
            .await;

            let response = match dispatch_result {
                Ok(result) => json!({
                    "jsonrpc": "2.0",
                    "id": id_val,
                    "result": result
                }),
                Err(err) => {
                    let rpc_err = err.into_jsonrpc();
                    json!({
                        "jsonrpc": "2.0",
                        "id": id_val,
                        "error": rpc_err
                    })
                }
            };

            if let Err(e) = transport.send_json(&response).await {
                tracing::warn!("ACP: failed to send response to agent: {}", e);
            }
        }
    }
}

/// Convert a JSON-RPC `RequestId` to a string key for the pending map.
fn id_to_key(id: &RequestId) -> String {
    match id {
        RequestId::Number(n) => format!("n{}", n),
        RequestId::String(s) => format!("s{}", s),
        RequestId::Null => "null".to_string(),
    }
}

/// Convert a JSON-RPC `RequestId` to a `serde_json::Value` for serialization.
fn id_to_value(id: &RequestId) -> Value {
    match id {
        RequestId::Number(n) => Value::Number((*n).into()),
        RequestId::String(s) => Value::String(s.clone()),
        RequestId::Null => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::transport::MockTransport;
    use crate::acp::types::{ContentBlock, StopReason, TextContent};
    use serde_json::json;

    /// A handler that echoes back read requests with a fixed response.
    struct EchoHandler {
        pub read_calls: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    impl AcpClientHandler for EchoHandler {
        fn read_text_file(
            &self,
            request: &ReadTextFileRequest,
        ) -> impl std::future::Future<Output = Result<ReadTextFileResponse, AcpError>> + Send
        {
            let calls = self.read_calls.clone();
            let path = request.path.clone();
            async move {
                calls.lock().await.push(path.clone());
                Ok(ReadTextFileResponse {
                    content: format!("contents of {}", path),
                    _meta: None,
                })
            }
        }

        fn write_text_file(
            &self,
            _request: &WriteTextFileRequest,
        ) -> impl std::future::Future<Output = Result<WriteTextFileResponse, AcpError>> + Send
        {
            async move { Ok(WriteTextFileResponse { _meta: None }) }
        }

        fn request_permission(
            &self,
            _request: &RequestPermissionRequest,
        ) -> impl std::future::Future<Output = Result<PermissionOutcome, AcpError>> + Send {
            async move { Ok(PermissionOutcome::Cancelled { _meta: None }) }
        }
    }

    /// A handler that returns errors for all requests (for error path tests).
    struct ErrorHandler;

    impl AcpClientHandler for ErrorHandler {
        fn read_text_file(
            &self,
            _request: &ReadTextFileRequest,
        ) -> impl std::future::Future<Output = Result<ReadTextFileResponse, AcpError>> + Send
        {
            async move { Err(AcpError::new(ErrorKind::Internal, "read failed")) }
        }

        fn write_text_file(
            &self,
            _request: &WriteTextFileRequest,
        ) -> impl std::future::Future<Output = Result<WriteTextFileResponse, AcpError>> + Send
        {
            async move { Err(AcpError::new(ErrorKind::Internal, "write failed")) }
        }

        fn request_permission(
            &self,
            _request: &RequestPermissionRequest,
        ) -> impl std::future::Future<Output = Result<PermissionOutcome, AcpError>> + Send {
            async move { Err(AcpError::new(ErrorKind::Internal, "permission denied")) }
        }
    }

    /// A handler that returns errors for all requests (for error path tests).
    struct NoopHandler;

    impl AcpClientHandler for NoopHandler {
        fn read_text_file(
            &self,
            _request: &ReadTextFileRequest,
        ) -> impl std::future::Future<Output = Result<ReadTextFileResponse, AcpError>> + Send
        {
            async move { Err(AcpError::new(ErrorKind::Internal, "not supported")) }
        }

        fn write_text_file(
            &self,
            _request: &WriteTextFileRequest,
        ) -> impl std::future::Future<Output = Result<WriteTextFileResponse, AcpError>> + Send
        {
            async move { Err(AcpError::new(ErrorKind::Internal, "not supported")) }
        }

        fn request_permission(
            &self,
            _request: &RequestPermissionRequest,
        ) -> impl std::future::Future<Output = Result<PermissionOutcome, AcpError>> + Send {
            async move { Err(AcpError::new(ErrorKind::Internal, "not supported")) }
        }
    }

    #[tokio::test]
    async fn initialize_success() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        // Spawn a mock agent task
        let agent_task = tokio::spawn(async move {
            // Read the initialize request
            let req_line = peer.recv_client_msg().await.unwrap();
            assert!(req_line.contains("initialize"));

            // Send the initialize response
            let resp = json!({
                "jsonrpc": "2.0",
                "id": "0",
                "result": {
                    "protocolVersion": 1,
                    "agentCapabilities": {"loadSession": false},
                    "agentInfo": {
                        "name": "mock-agent",
                        "version": "1.0.0"
                    },
                    "authMethods": []
                }
            });
            peer.feed_raw(&serde_json::to_string(&resp).unwrap())
                .unwrap();
        });

        let init_resp = client.initialize(None, None).await.unwrap();
        assert_eq!(init_resp.protocol_version, 1);
        assert_eq!(init_resp.agent_info.as_ref().unwrap().name, "mock-agent");

        // Wait for the agent task to finish
        agent_task.await.unwrap();

        // Verify client state
        let state = client.state.lock().await;
        assert!(state.connection_state.is_connected());
    }

    #[tokio::test]
    async fn initialize_protocol_version_mismatch() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            let _ = peer.recv_client_msg().await; // read request
            let resp = json!({
                "jsonrpc": "2.0",
                "id": "0",
                "result": {
                    "protocolVersion": 2,
                    "agentCapabilities": {},
                    "agentInfo": {"name": "mock", "version": "1.0.0"},
                    "authMethods": []
                }
            });
            peer.feed_raw(&serde_json::to_string(&resp).unwrap())
                .unwrap();
        });

        let result = client.initialize(None, None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::ProtocolVersionMismatch);

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn new_session() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        // Initialize first
        let agent_task = tokio::spawn(async move {
            let init_req = peer.recv_client_msg().await.unwrap();
            assert!(init_req.contains("initialize"));

            let resp = json!({
                "jsonrpc": "2.0",
                "id": "0",
                "result": {
                    "protocolVersion": 1,
                    "agentCapabilities": {},
                    "agentInfo": {"name": "mock", "version": "1.0.0"},
                    "authMethods": []
                }
            });
            peer.feed_raw(&serde_json::to_string(&resp).unwrap())
                .unwrap();

            // Read session/new request
            let new_req = peer.recv_client_msg().await.unwrap();
            assert!(new_req.contains("session/new"));

            let new_resp = json!({
                "jsonrpc": "2.0",
                "id": "1",
                "result": {
                    "sessionId": "sess_abc123",
                    "configOptions": null,
                    "modes": null
                }
            });
            peer.feed_raw(&serde_json::to_string(&new_resp).unwrap())
                .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let session_id = client
            .new_session("/workspace", vec![], vec![])
            .await
            .unwrap();
        assert_eq!(session_id, "sess_abc123");

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn session_prompt() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            // Read initialize
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#).unwrap();

            // Read session/new
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#).unwrap();

            // Read session/prompt
            let prompt_req = peer.recv_client_msg().await.unwrap();
            assert!(prompt_req.contains("session/prompt"));

            // Send prompt response
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{"stopReason":"end_turn"}}"#)
                .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        let resp = client
            .session_prompt(
                &sid,
                vec![ContentBlock::Text(TextContent {
                    text: "Hello".to_string(),
                    annotations: None,
                    _meta: None,
                })],
            )
            .await
            .unwrap();

        assert_eq!(resp.stop_reason, StopReason::EndTurn);

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn session_cancel_send_notification() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            // init
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#).unwrap();

            // session/new
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#).unwrap();

            // session/cancel notification (no id)
            let cancel = peer.recv_client_msg().await.unwrap();
            assert!(cancel.contains("session/cancel"));
            assert!(!cancel.contains("\"id\""));
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        client.session_cancel(&sid).await.unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn agent_request_dispatched_to_handler() {
        let handler = Arc::new(EchoHandler {
            read_calls: Arc::new(tokio::sync::Mutex::new(vec![])),
        });
        let read_calls = handler.read_calls.clone();

        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            // init
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#).unwrap();

            // session/new
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#).unwrap();

            // session/prompt
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{"stopReason":"end_turn"}}"#)
                .unwrap();

            // Wait a bit, then send agent→client request
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"99","method":"fs/read_text_file","params":{"path":"/etc/hostname","sessionId":"s1"}}"#).unwrap();

            // Wait for the client's response
            let resp = peer.recv_client_msg().await.unwrap();
            assert!(resp.contains("\"result\""));
            assert!(resp.contains("contents of /etc/hostname"));
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        let _ = client.session_prompt(&sid, vec![]).await.unwrap();

        agent_task.await.unwrap();

        // Verify the handler was called
        let calls = read_calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "/etc/hostname");

        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn agent_request_error_response() {
        let handler = Arc::new(ErrorHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            // init
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#).unwrap();

            // session/new
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#).unwrap();

            // session/prompt
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{"stopReason":"end_turn"}}"#)
                .unwrap();

            // Wait, then send agent→client request that will fail
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"99","method":"fs/read_text_file","params":{"path":"/test","sessionId":"s1"}}"#).unwrap();

            // Read the error response
            let resp = peer.recv_client_msg().await.unwrap();
            assert!(resp.contains("\"error\""));
            assert!(resp.contains("-32601") || resp.contains("-32000"));
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        let _ = client.session_prompt(&sid, vec![]).await.unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn session_update_notification_callback() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let (notify_tx, mut notify_rx) =
            tokio::sync::mpsc::unbounded_channel::<(String, SessionUpdate)>();

        client.on_update(move |sid, update| {
            let tx = notify_tx.clone();
            Box::pin(async move {
                tx.send((sid.to_string(), update)).unwrap();
            })
        });

        let agent_task = tokio::spawn(async move {
            // init
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#).unwrap();

            // session/new
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#).unwrap();

            // session/prompt
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{"stopReason":"end_turn"}}"#)
                .unwrap();

            // Send a session/update notification
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            peer.feed_raw(r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":"Hello from agent"}}}}"#).unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        let _ = client.session_prompt(&sid, vec![]).await.unwrap();

        // Wait for the notification to arrive
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let (received_sid, received_update) = notify_rx.recv().await.unwrap();
        assert_eq!(received_sid, "s1");
        match received_update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                assert_eq!(chunk.message_id.as_deref(), Some("m1"));
                match chunk.content {
                    ContentBlock::Text(t) => assert_eq!(t.text, "Hello from agent"),
                    _ => panic!("expected text"),
                }
            }
            _ => panic!("expected AgentMessageChunk"),
        }

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn request_ids_are_sequential() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            // init -- request id "0"
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#).unwrap();

            // session/new -- request id "1"
            let new_req = peer.recv_client_msg().await.unwrap();
            assert!(new_req.contains(r#"id":"1"#));
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#).unwrap();

            // session/delete -- request id "2"
            let del_req = peer.recv_client_msg().await.unwrap();
            assert!(del_req.contains(r#"id":"2"#));
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{}}"#).unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();

        // Verify delete works (uses request id "2")
        client.delete_session(&sid).await.unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn transport_closed_when_peer_drops() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);
        drop(peer);

        let result = client.initialize(None, None).await;
        assert!(result.is_err());
    }
}
