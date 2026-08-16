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
//! loop via channels.

use crate::acp::error::{AcpError, ErrorKind};
use crate::acp::events::{
    classify_message, classify_notification, InboundMessage, NotificationKind,
};
use crate::acp::handler::AcpClientHandler;
use crate::acp::state::{ClientState, NegotiatedCapabilities};
use crate::acp::transport::{MockTransport, Transport, TransportRead, TransportWrite};
use crate::acp::types::*;
use crate::rpc::{JsonRpcError, RequestId};

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// A pending request waiting for a response from the agent.
struct PendingEntry {
    tx: oneshot::Sender<Result<Value, JsonRpcError>>,
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
/// The client owns a background message loop task that reads JSON-RPC messages
/// from the transport and dispatches them. All public methods are `async`.
pub struct AcpClient<T: Transport> {
    /// Outbound message channel — client → message loop → transport.
    outbound_tx: mpsc::UnboundedSender<(Value, oneshot::Sender<Result<(), AcpError>>)>,
    /// Inbound notification channel — message loop → client.
    /// Used for session/update notifications and other pushed events.
    inbound_notify: mpsc::UnboundedReceiver<InboundEvent>,
    inbound_notify_tx: mpsc::UnboundedSender<InboundEvent>,
    /// Pending request responses.
    pending: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<String, oneshot::Sender<Result<Value, JsonRpcError>>>,
        >,
    >,
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
    /// Phantom marker for the transport type.
    _transport: std::marker::PhantomData<T>,
}

/// Internal events the message loop pushes to the client.
#[derive(Debug, Clone)]
pub enum InboundEvent {
    /// A session/update notification.
    SessionUpdate {
        session_id: String,
        update: SessionUpdate,
    },
    /// The transport was closed (EOF).
    TransportClosed,
}

impl<T: Transport + 'static> AcpClient<T> {
    /// Create a new ACP client.
    ///
    /// The client is in the `Disconnected` state. Call [`initialize`] to
    /// complete the handshake.
    ///
    /// [`initialize`]: AcpClient::initialize
    pub fn new(transport: T, handler: Arc<dyn AcpClientHandler>) -> Self {
        let (outbound_tx, _) = mpsc::unbounded_channel();
        let (inbound_notify_tx, inbound_notify) = mpsc::unbounded_channel();

        Self {
            outbound_tx,
            inbound_notify,
            inbound_notify_tx,
            pending: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            next_id: Arc::new(tokio::sync::atomic::AtomicI64::new(0)),
            state: Arc::new(tokio::sync::Mutex::new(ClientState::new())),
            handler,
            update_callback: None,
            loop_handle: None,
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            _transport: std::marker::PhantomData,
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

    /// Generate the next request ID (monotonically increasing integer).
    fn next_request_id(&self) -> String {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{}", id)
    }

    /// Start the background message loop that reads incoming JSON-RPC
    /// messages from the transport and dispatches them.
    ///
    /// The loop takes ownership of the transport and runs in a spawned task.
    /// The client communicates with the loop via channels.
    async fn start_message_loop(
        transport: T,
        state: Arc<tokio::sync::Mutex<ClientState>>,
        pending: Arc<
            tokio::sync::Mutex<
                std::collections::HashMap<String, oneshot::Sender<Result<Value, JsonRpcError>>>,
            >,
        >,
        handler: Arc<dyn AcpClientHandler>,
        outbound_rx: mpsc::UnboundedReceiver<(Value, oneshot::Sender<Result<(), AcpError>>)>,
        notify_tx: mpsc::UnboundedSender<InboundEvent>,
        update_callback: Option<UpdateCallback>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut transport = transport;
            let mut outbound_rx = outbound_rx;
            let notify_tx = notify_tx;
            let pending = pending;
            let state = state;
            let handler = handler;
            let update_callback = update_callback;

            loop {
                tokio::select! {
                    // Outbound messages (client → agent)
                    msg = outbound_rx.recv() => {
                        match msg {
                            Some((json_msg, resp_tx)) => {
                                let result = transport.send_json(&json_msg).await;
                                let _ = resp_tx.send(result);
                            }
                            None => {
                                // Outbound channel closed
                                break;
                            }
                        }
                    }

                    // Inbound messages (agent → client)
                    line_result = transport.read_line() => {
                        match line_result {
                            Ok(Some(line)) => {
                                Self::process_line(
                                    &mut transport,
                                    &state,
                                    &pending,
                                    &handler,
                                    &update_callback,
                                    &notify_tx,
                                    &line,
                                ).await;
                            }
                            Ok(None) => {
                                // EOF — transport closed
                                let _ = notify_tx.send(InboundEvent::TransportClosed);
                                break;
                            }
                            Err(e) => {
                                tracing::error!("ACP transport read error: {}", e);
                                let _ = notify_tx.send(InboundEvent::TransportClosed);
                                break;
                            }
                        }
                    }
                }
            }
        })
    }

    /// Process a single line received from the transport.
    async fn process_line(
        transport: &mut T,
        state: &Arc<tokio::sync::Mutex<ClientState>>,
        pending: &Arc<
            tokio::sync::Mutex<
                std::collections::HashMap<String, oneshot::Sender<Result<Value, JsonRpcError>>>,
            >,
        >,
        handler: &Arc<dyn AcpClientHandler>,
        update_callback: &Option<UpdateCallback>,
        notify_tx: &mpsc::UnboundedSender<InboundEvent>,
        line: &str,
    ) {
        // Parse and classify the incoming message
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
                    tracing::warn!("ACP: received response for unknown request id: {}", id_key);
                }
            }

            // A notification from the agent (no response expected)
            InboundMessage::Notification { method, params } => {
                let kind = classify_notification(&method);
                match kind {
                    NotificationKind::SessionUpdate => {
                        if let Some(update_params) = &params {
                            if let Ok(notif_params) =
                                serde_json::from_value::<SessionNotificationParams>(
                                    update_params.clone(),
                                )
                            {
                                if let Some(cb) = update_callback {
                                    let sid = notif_params.session_id.clone();
                                    let update = notif_params.update.clone();
                                    cb(&sid, update).await;
                                }
                            }
                        }
                    }
                    NotificationKind::CancelRequest => {
                        tracing::debug!(
                            "ACP: received $/cancel_request notification: params={:?}",
                            params
                        );
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
                        tracing::debug!("ACP: received unknown notification: {}", method);
                    }
                }
            }

            // A request from the agent to the client
            InboundMessage::AgentRequest {
                id,
                method,
                params,
                typed,
            } => {
                let id_val = id_to_value(&id);
                let method_str = method.clone();
                let params_clone = params.clone();
                let handler_clone = handler.clone();

                // Dispatch the agent→client request
                let dispatch_result =
                    dispatch_async(handler_clone, &method_str, &params_clone).await;

                // Build and send the JSON-RPC response
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

                let _ = transport.send_json(&response).await;

                // If the typed request was parseable, log it
                if let Some(typed_req) = &typed {
                    let _ = typed_req; // suppress unused warning
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Public API — protocol methods (client → agent)
    // -----------------------------------------------------------------------

    /// Send the `initialize` request and wait for the response.
    ///
    /// This negotiates the protocol version and capabilities with the agent.
    /// After a successful initialization, the client transitions to the
    /// `Connected` state and the message loop starts.
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

        // Create the outbound channel for the message loop
        let (outbound_tx, outbound_rx) =
            mpsc::unbounded_channel::<(Value, oneshot::Sender<Result<(), AcpError>>)>();
        self.outbound_tx = outbound_tx;

        // The transport needs to be moved into the message loop. But the
        // client doesn't own it directly — it was passed to `new`. We need
        // to restructure. Let's store the transport in the client and
        // pass it to the loop.

        // Actually, looking at the AcpClient struct, the transport is stored
        // as a PhantomData — we lost it. We need to store the actual transport.

        // Let me reconsider the design: the client should own the transport.
        // But Transport requires Send + Unpin, and we need to move it into
        // the background task. We can use Option<T> and take() it.

        todo!("restructure to own the transport")
    }

    // -----------------------------------------------------------------------
    // More protocol methods — these need the message loop running
    // -----------------------------------------------------------------------

    /// Create a new session by sending `session/new`.
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
        // The message loop will break when outbound_rx returns None

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
    /// response.
    async fn send_request(&self, method: &str, params: &Value) -> Result<Option<Value>, AcpError> {
        let id = self.next_request_id();
        let id_clone = id.clone();

        let (tx, rx) = oneshot::channel::<Result<Value, JsonRpcError>>();
        {
            let mut map = self.pending.lock().await;
            map.insert(id.clone(), PendingEntry { tx });
        }

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id_clone,
            "method": method,
            "params": params
        });

        let (send_tx, send_rx) = oneshot::channel::<Result<(), AcpError>>();
        if self.outbound_tx.send((msg, send_tx)).is_err() {
            return Err(AcpError::transport_closed("outbound channel closed"));
        }

        // Wait for the message to be sent
        send_rx
            .await
            .map_err(|_| AcpError::transport_closed("message loop task unavailable"))?;
        send_rx.await.ok();

        // Wait for the response (with a generous timeout)
        let timeout = std::time::Duration::from_secs(120);
        let result = tokio::time::timeout(timeout, rx).await.map_err(|_| {
            AcpError::timeout(format!(
                "request '{}' timed out after {:?}",
                method, timeout
            ))
        })?;

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

        let (send_tx, send_rx) = oneshot::channel::<Result<(), AcpError>>();
        if self.outbound_tx.send((msg, send_tx)).is_err() {
            return Err(AcpError::transport_closed("outbound channel closed"));
        }

        send_rx
            .await
            .map_err(|_| AcpError::transport_closed("message loop task unavailable"))?;
        send_rx.await.ok();

        Ok(())
    }
}

/// Async dispatch helper - calls the handler's dispatch logic.
async fn dispatch_async(
    handler: Arc<dyn AcpClientHandler>,
    method: &str,
    params: &Option<serde_json::Value>,
) -> Result<Value, AcpError> {
    crate::acp::handler::dispatch_agent_request(handler.as_ref(), method, params).await
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
