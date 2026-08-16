//! # ACP Client
use crate::acp::error::AcpError;
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

struct PendingEntry {
    tx: oneshot::Sender<Result<Value, JsonRpcError>>,
}

struct OutboundMessage {
    msg: Value,
    ack: Option<oneshot::Sender<Result<(), AcpError>>>,
}

type UpdateCallback = Arc<
    dyn Fn(&str, SessionUpdate) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

pub struct AcpClient<T: Transport> {
    outbound_tx: Option<mpsc::UnboundedSender<OutboundMessage>>,
    pending: Arc<tokio::sync::Mutex<std::collections::HashMap<String, PendingEntry>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
    state: Arc<tokio::sync::Mutex<ClientState>>,
    handler: Arc<dyn AcpClientHandler>,
    update_callback: Option<UpdateCallback>,
    loop_handle: Option<tokio::task::JoinHandle<()>>,
    protocol_version: ProtocolVersion,
    transport: Option<T>,
}

impl<T: Transport + 'static> AcpClient<T> {
    pub fn new(transport: T, handler: Arc<dyn AcpClientHandler>) -> Self {
        Self {
            outbound_tx: None,
            pending: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            state: Arc::new(tokio::sync::Mutex::new(ClientState::new())),
            handler,
            update_callback: None,
            loop_handle: None,
            protocol_version: SUPPORTED_PROTOCOL_VERSION,
            transport: Some(transport),
        }
    }

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

    pub async fn initialize(
        &mut self,
        client_info: Option<Implementation>,
        client_capabilities: Option<ClientCapabilities>,
    ) -> Result<InitializeResponse, AcpError> {
        {
            let mut state = self.state.lock().await;
            state
                .transition_to_initializing()
                .map_err(|e| AcpError::invalid_state(e.to_string()))?;
        }

        let transport = self
            .transport
            .take()
            .ok_or_else(|| AcpError::invalid_state("transport already in use"))?;

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

        if init_resp.protocol_version != self.protocol_version {
            return Err(AcpError::protocol_version_mismatch(
                self.protocol_version,
                init_resp.protocol_version,
            ));
        }

        let caps = NegotiatedCapabilities {
            protocol_version: init_resp.protocol_version,
            agent_capabilities: init_resp.agent_capabilities.clone(),
            client_capabilities: None,
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

    pub async fn shutdown(mut self) -> Result<(), AcpError> {
        self.outbound_tx = None;
        if let Some(handle) = self.loop_handle.take() {
            let _ = handle.await;
        }
        let mut state = self.state.lock().await;
        state.close();
        Ok(())
    }

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
        let timeout_duration = std::time::Duration::from_secs(120);
        let result = tokio::time::timeout(timeout_duration, rx).await;

        match result {
            // Timed out
            Err(_) => {
                let mut map = self.pending.lock().await;
                map.remove(&id_str);
                Err(AcpError::timeout(format!(
                    "request '{}' timed out after {:?}",
                    method, timeout_duration
                )))
            }
            // Cancelled (oneshot sender was dropped or channel closed)
            Ok(Err(_)) => Err(AcpError::request_cancelled(format!(
                "request '{}' was cancelled",
                method
            ))),
            // Got a success result
            Ok(Ok(Ok(value))) => Ok(Some(value)),
            // Got an RPC error response
            Ok(Ok(Err(rpc_err))) => Err(AcpError::from(rpc_err)),
        }
    }

    async fn send_notification(&self, method: &str, params: &Value) -> Result<(), AcpError> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.send_outbound(msg).await?;
        Ok(())
    }

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
        ack_rx
            .await
            .map_err(|_| AcpError::transport_closed("message loop task unavailable"))??;
        Ok(())
    }

    fn next_request_id(&self) -> String {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        id.to_string()
    }
}

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
            outbound_msg = outbound_rx.recv() => {
                match outbound_msg {
                    Some(OutboundMessage { msg, ack }) => {
                        let result = transport.send_json(&msg).await;
                        if let Some(ack_tx) = ack {
                            let _ = ack_tx.send(result.clone());
                        }
                        if result.is_err() {
                            tracing::error!("ACP: transport write error, stopping message loop");
                            break;
                        }
                    }
                    None => break,
                }
            }
            line_result = transport.read_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        process_line(&mut transport, &state, &pending, &handler, &update_callback, &line).await;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("ACP: transport read error: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

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
                    "result": result,
                }),
                Err(err) => {
                    let rpc_err = err.into_jsonrpc();
                    json!({
                        "jsonrpc": "2.0",
                        "id": id_val,
                        "error": rpc_err,
                    })
                }
            };

            if let Err(e) = transport.send_json(&response).await {
                tracing::warn!("ACP: failed to send response to agent: {}", e);
            }
        }
    }
}

fn id_to_key(id: &RequestId) -> String {
    match id {
        RequestId::Number(n) => format!("n{}", n),
        RequestId::String(s) => format!("s{}", s),
        RequestId::Null => "null".to_string(),
    }
}

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
    use crate::acp::types::{ContentBlock, StopReason};
    use serde_json::json;
    use std::sync::Arc;

    struct EchoHandler {
        read_calls: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    impl EchoHandler {
        fn new() -> Self {
            Self {
                read_calls: Arc::new(tokio::sync::Mutex::new(vec![])),
            }
        }
    }

    #[async_trait::async_trait]
    impl AcpClientHandler for EchoHandler {
        async fn read_text_file(
            &self,
            request: &ReadTextFileRequest,
        ) -> Result<ReadTextFileResponse, AcpError> {
            let calls = self.read_calls.clone();
            let path = request.path.clone();
            calls.lock().await.push(path.clone());
            Ok(ReadTextFileResponse {
                content: format!("contents of {}", path),
                _meta: None,
            })
        }

        async fn write_text_file(
            &self,
            _request: &WriteTextFileRequest,
        ) -> Result<WriteTextFileResponse, AcpError> {
            Ok(WriteTextFileResponse { _meta: None })
        }

        async fn request_permission(
            &self,
            _request: &RequestPermissionRequest,
        ) -> Result<PermissionOutcome, AcpError> {
            Ok(PermissionOutcome::Cancelled { _meta: None })
        }
    }

    struct NoopHandler;

    #[async_trait::async_trait]
    impl AcpClientHandler for NoopHandler {
        async fn read_text_file(
            &self,
            _request: &ReadTextFileRequest,
        ) -> Result<ReadTextFileResponse, AcpError> {
            Err(AcpError::new(
                ErrorKind::UnsupportedOperation,
                "not supported",
            ))
        }
        async fn write_text_file(
            &self,
            _request: &WriteTextFileRequest,
        ) -> Result<WriteTextFileResponse, AcpError> {
            Err(AcpError::new(
                ErrorKind::UnsupportedOperation,
                "not supported",
            ))
        }
        async fn request_permission(
            &self,
            _request: &RequestPermissionRequest,
        ) -> Result<PermissionOutcome, AcpError> {
            Err(AcpError::new(
                ErrorKind::UnsupportedOperation,
                "not supported",
            ))
        }
    }

    #[tokio::test]
    async fn initialize_success() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            let req_line = peer.recv_client_msg().await.unwrap();
            assert!(req_line.contains("initialize"));
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"mock","version":"1.0.0"},"authMethods":[]}}"#,
            )
            .unwrap();
        });

        let init_resp = client.initialize(None, None).await.unwrap();
        assert_eq!(init_resp.protocol_version, 1);
        assert_eq!(init_resp.agent_info.as_ref().unwrap().name, "mock");

        agent_task.await.unwrap();
        let state = client.state.lock().await;
        assert!(state.connection_state.is_connected());
    }

    #[tokio::test]
    async fn initialize_protocol_version_mismatch() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            let _ = peer.recv_client_msg().await;
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":2,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
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

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            let new_req = peer.recv_client_msg().await.unwrap();
            assert!(new_req.contains("\"id\":\"1\""));
            assert!(new_req.contains("session/new"));
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"sess_abc123","configOptions":null,"modes":null}}"#,
            )
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
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            let prompt_req = peer.recv_client_msg().await.unwrap();
            assert!(prompt_req.contains("session/prompt"));
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
    async fn session_cancel_sends_notification() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            let cancel = peer.recv_client_msg().await.unwrap();
            assert!(cancel.contains("session/cancel"));
            assert!(!cancel.contains("\"id\""));
            assert!(cancel.contains("s1"));
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        client.session_cancel(&sid).await.unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn session_close() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            let close_req = peer.recv_client_msg().await.unwrap();
            assert!(close_req.contains("session/close"));
            assert!(close_req.contains("\"id\":\"2\""));
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{}}"#)
                .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        client.close_session(&sid).await.unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;

        let state = client.state.lock().await;
        assert_eq!(
            state.sessions.get("s1").unwrap().state,
            crate::acp::state::SessionState::Closed
        );
    }

    #[tokio::test]
    async fn session_delete() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{}}"#)
                .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        client.delete_session(&sid).await.unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;

        let state = client.state.lock().await;
        assert!(!state.sessions.contains_key("s1"));
    }

    #[tokio::test]
    async fn session_list() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessions":[{"sessionId":"s1","cwd":"/home","additionalDirectories":[],"title":"Session 1","updatedAt":"2025-10-29T12:00:00Z"}],"nextCursor":null}}"#,
            )
            .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sessions = client.list_sessions(Some("/home")).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
        assert_eq!(sessions[0].cwd, "/home");

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn session_load() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            let req = peer.recv_client_msg().await.unwrap();
            assert!(req.contains("session/load"));
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{}}"#)
                .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        client
            .load_session("sess_x", "/workspace", vec![], vec![])
            .await
            .unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn session_resume() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            let req = peer.recv_client_msg().await.unwrap();
            assert!(req.contains("session/resume"));
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"1","result":{}}"#)
                .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        client
            .resume_session("sess_y", "/workspace", vec![], vec![])
            .await
            .unwrap();

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn request_ids_are_sequential() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            let req0 = peer.recv_client_msg().await.unwrap();
            assert!(req0.contains("\"id\":\"0\""));
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            let req1 = peer.recv_client_msg().await.unwrap();
            assert!(req1.contains("\"id\":\"1\""));
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            let req2 = peer.recv_client_msg().await.unwrap();
            assert!(req2.contains("\"id\":\"2\""));
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{}}"#)
                .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
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

    #[tokio::test]
    async fn agent_request_dispatched_to_handler() {
        let handler = Arc::new(EchoHandler {
            read_calls: Arc::new(tokio::sync::Mutex::new(vec![])),
        });
        let read_calls = handler.read_calls.clone();

        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{"stopReason":"end_turn"}}"#)
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"99","method":"fs/read_text_file","params":{"path":"/etc/hostname","sessionId":"s1"}}"#,
            )
            .unwrap();

            let resp = peer.recv_client_msg().await.unwrap();
            assert!(resp.contains("\"result\""));
            assert!(resp.contains("contents of /etc/hostname"));
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        let _ = client.session_prompt(&sid, vec![]).await.unwrap();

        agent_task.await.unwrap();

        let calls = read_calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "/etc/hostname");

        let _ = client.shutdown().await;
    }

    #[tokio::test]
    async fn agent_request_error_response() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{"stopReason":"end_turn"}}"#)
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"99","method":"fs/read_text_file","params":{"path":"/test","sessionId":"s1"}}"#,
            )
            .unwrap();

            let resp = peer.recv_client_msg().await.unwrap();
            assert!(resp.contains("\"error\""));
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
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();

            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(r#"{"jsonrpc":"2.0","id":"2","result":{"stopReason":"end_turn"}}"#)
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":"Hello from agent"}}}}"#,
            )
            .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let sid = client.new_session("/ws", vec![], vec![]).await.unwrap();
        let _ = client.session_prompt(&sid, vec![]).await.unwrap();

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
    async fn new_session_with_mcp_servers() {
        let handler = Arc::new(NoopHandler);
        let (client_transport, mut peer) = MockTransport::pair();
        let mut client = AcpClient::new(client_transport, handler);

        let agent_task = tokio::spawn(async move {
            peer.recv_client_msg().await.unwrap();
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"0","result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"m","version":"1"},"authMethods":[]}}"#,
            )
            .unwrap();

            let req = peer.recv_client_msg().await.unwrap();
            assert!(req.contains("mcpServers"));
            peer.feed_raw(
                r#"{"jsonrpc":"2.0","id":"1","result":{"sessionId":"s1","configOptions":null,"modes":null}}"#,
            )
            .unwrap();
        });

        client.initialize(None, None).await.unwrap();
        let servers = vec![McpServer::Stdio(McpServerStdio {
            name: "test".to_string(),
            command: "/bin/test".to_string(),
            args: vec![],
            env: vec![],
            _meta: None,
        })];
        let sid = client.new_session("/ws", servers, vec![]).await.unwrap();
        assert_eq!(sid, "s1");

        agent_task.await.unwrap();
        let _ = client.shutdown().await;
    }
}
