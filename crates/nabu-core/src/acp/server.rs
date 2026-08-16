//! # ACP Server — Protocol Coordinator
//!
//! [`AcpServer`] is the central coordinator for the ACP protocol layer. It
//! owns the session state machine, validates protocol-level preconditions, and
//! delegates actual session operations to an [`AcpHandler`] implementation.
//!
//! The server integrates with the existing JSON-RPC [`Router`] by registering
//! one [`RpcHandler`] per ACP method (`initialize`, `session/new`,
//! `session/load`, `session/resume`, `session/prompt`, `session/close`). Each
//! handler:
//!
//! 1. Deserialises the JSON-RPC `params` into a strongly-typed ACP request.
//! 2. Validates protocol and session state.
//! 3. Delegates to the [`AcpHandler`].
//! 4. Serialises the typed response back into a JSON-RPC `result`.
//!
//! ## Architecture
//!
//! ```text
//! JSON-RPC Client
//!      │  (Request: { method, params })
//!      ▼
//! Router::dispatch
//!      │  (looks up method → RpcHandler)
//!      ▼
//! Per-method RpcHandler  (deserialize params → AcpRequest)
//!      │
//!      ▼
//! AcpServer::do_*  (validate state → delegate to AcpHandler → update state)
//!      │
//!      ▼
//! AcpHandler  (agent runtime — Phase 3d+)
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::rpc::{JsonRpcError, Router, RpcHandler};

use crate::acp::error::AcpError;
use crate::acp::AcpHandler;
use crate::acp::state::{ProtocolState, ServerEntry, SessionStatus};
use crate::acp::types::{
    decode_params, AgentCapabilities, CloseSessionRequest, CloseSessionResponse,
    InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, ResumeSessionRequest,
    ResumeSessionResponse, SessionId,
};

// ---------------------------------------------------------------------------
// Method name constants — match the ACP v1 specification exactly
// ---------------------------------------------------------------------------

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_NEW_SESSION: &str = "session/new";
pub const METHOD_LOAD_SESSION: &str = "session/load";
pub const METHOD_RESUME_SESSION: &str = "session/resume";
pub const METHOD_PROMPT: &str = "session/prompt";
pub const METHOD_CLOSE_SESSION: &str = "session/close";

// ---------------------------------------------------------------------------
// AcpServer
// ---------------------------------------------------------------------------

/// The ACP protocol coordinator.
///
/// Holds the connection-level protocol state, the in-memory session registry,
/// and a reference to the [`AcpHandler`] that performs the actual work.
///
/// ## State machine
///
/// ```text
/// Protocol:  Uninitialized ──initialize──▶ Initialized
///
/// Session:   Created ──prompt──▶ Active
///            │                      │
///            └──close──▶ Closed ◀───┘
/// ```
pub struct AcpServer {
    protocol_state: RwLock<ProtocolState>,
    sessions: RwLock<HashMap<SessionId, ServerEntry>>,
    handler: Arc<dyn AcpHandler>,
    agent_capabilities: RwLock<AgentCapabilities>,
}

impl AcpServer {
    /// Create a new ACP server with the given handler.
    pub fn new(handler: Arc<dyn AcpHandler>) -> Self {
        Self {
            protocol_state: RwLock::new(ProtocolState::Uninitialized),
            sessions: RwLock::new(HashMap::new()),
            handler,
            agent_capabilities: RwLock::new(AgentCapabilities::default()),
        }
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Returns the current protocol state.
    pub async fn protocol_state(&self) -> ProtocolState {
        *self.protocol_state.read().await
    }

    /// Returns `true` if `initialize` has completed.
    pub async fn is_initialized(&self) -> bool {
        self.protocol_state.read().await.is_initialized()
    }

    /// Returns the number of sessions currently tracked.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Returns the status of a tracked session, if it exists.
    pub async fn session_status(&self, id: &SessionId) -> Option<SessionStatus> {
        self.sessions.read().await.get(id).map(|e| e.status)
    }

    /// Returns a snapshot of session IDs matching the given status.
    pub async fn sessions_by_status(&self, status: SessionStatus) -> Vec<SessionId> {
        let sessions = self.sessions.read().await;
        sessions
            .iter()
            .filter(|(_, e)| e.status == status)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Returns the negotiated agent capabilities (empty before initialize).
    pub async fn agent_capabilities(&self) -> AgentCapabilities {
        self.agent_capabilities.read().await.clone()
    }

    // -----------------------------------------------------------------------
    // Handler registration
    // -----------------------------------------------------------------------

    /// Registers all ACP method handlers on the given router.
    ///
    /// Each method (`initialize`, `session/new`, `session/load`,
    /// `session/resume`, `session/prompt`, `session/close`) is registered as
    /// a separate [`RpcHandler`] that delegates to the corresponding `do_*`
    /// method on this server.
    ///
    /// The `Arc<AcpServer>` is cloned into each handler so they can share
    /// ownership of the server's state.
    pub async fn register_handlers(self: &Arc<Self>, router: &Router) {
        macro_rules! register {
            ($method_name:expr, $handler_ty:ty) => {
                router
                    .register(
                        $method_name,
                        Arc::new(<$handler_ty>::new(self.clone())),
                    )
                    .await;
            };
        }

        register!(METHOD_INITIALIZE, InitializeHandler);
        register!(METHOD_NEW_SESSION, NewSessionHandler);
        register!(METHOD_LOAD_SESSION, LoadSessionHandler);
        register!(METHOD_RESUME_SESSION, ResumeSessionHandler);
        register!(METHOD_PROMPT, PromptHandler);
        register!(METHOD_CLOSE_SESSION, CloseSessionHandler);

        tracing::info!("ACP protocol handlers registered on JSON-RPC router");
    }

    // -----------------------------------------------------------------------
    // Operation: initialize
    // -----------------------------------------------------------------------

    /// Handle the `initialize` method.
    ///
    /// Delegates to the handler for capability negotiation, stores negotiated
    /// capabilities, and transitions the protocol state to `Initialized`.
    async fn do_initialize(&self, req: InitializeRequest) -> Result<InitializeResponse, AcpError> {
        {
            let state = self.protocol_state.read().await;
            if !matches!(*state, ProtocolState::Uninitialized) {
                return Err(AcpError::invalid_state(
                    "ACP protocol is already initialized; cannot re-initialize",
                ));
            }
        }

        let resp = self.handler.initialize(req).await?;
        *self.agent_capabilities.write().await =
            resp.agent_capabilities.clone().unwrap_or_default();

        {
            let mut state = self.protocol_state.write().await;
            *state = state.initialize()?;
        }

        tracing::debug!(protocol_version = resp.protocol_version, "ACP protocol initialized");
        Ok(resp)
    }

    // -----------------------------------------------------------------------
    // Operation: session/new
    // -----------------------------------------------------------------------

    async fn do_new_session(&self, req: NewSessionRequest) -> Result<NewSessionResponse, AcpError> {
        self.ensure_initialized().await?;

        let cwd = req.cwd.clone();
        let resp = self.handler.new_session(req).await?;

        let entry = ServerEntry::new(cwd);
        self.sessions.write().await.insert(resp.session_id.clone(), entry);

        tracing::debug!(session_id = %resp.session_id, "ACP session created");
        Ok(resp)
    }

    // -----------------------------------------------------------------------
    // Operation: session/load
    // -----------------------------------------------------------------------

    async fn do_load_session(
        &self,
        req: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, AcpError> {
        self.ensure_initialized().await?;

        {
            let caps = self.agent_capabilities.read().await;
            if !caps.load_session {
                return Err(AcpError::unsupported(
                    "agent does not advertise the 'loadSession' capability; \
                     cannot call session/load",
                ));
            }
        }

        let session_id = req.session_id.clone();
        let cwd = req.cwd.clone();

        {
            let sessions = self.sessions.read().await;
            if let Some(entry) = sessions.get(&session_id) {
                if entry.status == SessionStatus::Active {
                    return Err(AcpError::invalid_state_session(
                        &session_id,
                        "session is already active; close it before loading",
                    ));
                }
            }
        }

        let resp = self.handler.load_session(req).await?;

        let entry = ServerEntry::new(cwd);
        self.sessions.write().await.insert(session_id.clone(), entry);

        tracing::debug!(session_id = %session_id, "ACP session loaded");
        Ok(resp)
    }

    // -----------------------------------------------------------------------
    // Operation: session/resume
    // -----------------------------------------------------------------------

    async fn do_resume_session(
        &self,
        req: ResumeSessionRequest,
    ) -> Result<ResumeSessionResponse, AcpError> {
        self.ensure_initialized().await?;

        {
            let caps = self.agent_capabilities.read().await;
            if caps
                .session_capabilities
                .as_ref()
                .and_then(|sc| sc.resume.as_ref())
                .is_none()
            {
                return Err(AcpError::unsupported(
                    "agent does not advertise the 'sessionCapabilities.resume' capability; \
                     cannot call session/resume",
                ));
            }
        }

        let session_id = req.session_id.clone();
        let cwd = req.cwd.clone();

        {
            let sessions = self.sessions.read().await;
            if let Some(entry) = sessions.get(&session_id) {
                if entry.status == SessionStatus::Active {
                    return Err(AcpError::invalid_state_session(
                        &session_id,
                        "session is already active; close it before resuming",
                    ));
                }
            }
        }

        let resp = self.handler.resume_session(req).await?;

        let entry = ServerEntry::new(cwd);
        self.sessions.write().await.insert(session_id.clone(), entry);

        tracing::debug!(session_id = %session_id, "ACP session resumed");
        Ok(resp)
    }

    // -----------------------------------------------------------------------
    // Operation: session/prompt
    // -----------------------------------------------------------------------

    async fn do_prompt(&self, req: PromptRequest) -> Result<PromptResponse, AcpError> {
        self.ensure_initialized().await?;

        {
            let sessions = self.sessions.read().await;
            let entry = sessions.get(&req.session_id).ok_or_else(|| {
                AcpError::unknown_session(&req.session_id)
            })?;
            if !entry.status.accepts_prompt() {
                return Err(AcpError::invalid_transition(
                    entry.status,
                    "prompt",
                    format!(
                        "session is in '{}' state; prompts are not accepted",
                        entry.status
                    ),
                ));
            }
        }

        let session_id = req.session_id.clone();
        let resp = self.handler.prompt(req).await?;

        {
            let mut sessions = self.sessions.write().await;
            if let Some(entry) = sessions.get_mut(&session_id) {
                entry.status = entry.status.after_prompt()?;
                entry.touch();
            }
        }

        Ok(resp)
    }

    // -----------------------------------------------------------------------
    // Operation: session/close
    // -----------------------------------------------------------------------

    async fn do_close_session(
        &self,
        req: CloseSessionRequest,
    ) -> Result<CloseSessionResponse, AcpError> {
        self.ensure_initialized().await?;

        {
            let caps = self.agent_capabilities.read().await;
            if caps
                .session_capabilities
                .as_ref()
                .and_then(|sc| sc.close.as_ref())
                .is_none()
            {
                return Err(AcpError::unsupported(
                    "agent does not advertise the 'sessionCapabilities.close' capability; \
                     cannot call session/close",
                ));
            }
        }

        let session_id = req.session_id.clone();

        {
            let sessions = self.sessions.read().await;
            let entry = sessions.get(&session_id).ok_or_else(|| {
                AcpError::unknown_session(&session_id)
            })?;
            if !entry.status.can_end() {
                return Err(AcpError::invalid_transition(
                    entry.status,
                    "close",
                    format!(
                        "session is in '{}' state; cannot close",
                        entry.status
                    ),
                ));
            }
        }

        let resp = self.handler.close_session(req).await?;

        {
            let mut sessions = self.sessions.write().await;
            if let Some(entry) = sessions.get_mut(&session_id) {
                entry.status = entry.status.after_end()?;
                entry.touch();
            }
        }

        tracing::debug!(session_id = %session_id, "ACP session closed");
        Ok(resp)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Returns `InvalidState` if the protocol has not been initialized.
    async fn ensure_initialized(&self) -> Result<(), AcpError> {
        let state = self.protocol_state.read().await;
        if !state.is_initialized() {
            return Err(AcpError::invalid_state(
                "ACP protocol must be initialized before performing session operations",
            ));
        }
        Ok(())
    }
}

// ===========================================================================
// Per-operation handler structs
// ===========================================================================
//
// Each ACP method gets its own non-generic handler struct holding an
// `Arc<AcpServer>`. The `acp_handler!` macro generates the `RpcHandler`
// implementation, which deserializes JSON-RPC `params` → typed request,
// delegates to `AcpServer::do_*` → serializes response → converts `AcpError`
// into `JsonRpcError`.
//
// This replaces the old generic `AcpRpcHandler<Req, Resp>` + `AcpOperation`
// enum dispatch that was compiler-unsound.

/// Generate a per-operation `RpcHandler` implementation for an ACP method.
macro_rules! acp_handler {
    (
        name = $name:ident,
        do_method = $do_method:ident,
        req_ty = $req_ty:ty,
        resp_ty = $resp_ty:ty,
    ) => {
        pub struct $name {
            pub server: Arc<AcpServer>,
        }

        impl $name {
            pub fn new(server: Arc<AcpServer>) -> Self {
                Self { server }
            }
        }

        #[async_trait]
        impl RpcHandler for $name {
            async fn handle(
                &self,
                params: Option<Value>,
            ) -> Result<Value, JsonRpcError> {
                let req: $req_ty =
                    decode_params(params).map_err(AcpError::into_jsonrpc_error)?;
                let resp: $resp_ty = self
                    .server
                    .$do_method(req)
                    .await
                    .map_err(AcpError::into_jsonrpc_error)?;
                serde_json::to_value(resp).map_err(|e| {
                    JsonRpcError::internal(format!(
                        "failed to serialize ACP response: {}",
                        e
                    ))
                })
            }
        }
    };
}

acp_handler! {
    name = InitializeHandler,
    do_method = do_initialize,
    req_ty = InitializeRequest,
    resp_ty = InitializeResponse,
}

acp_handler! {
    name = NewSessionHandler,
    do_method = do_new_session,
    req_ty = NewSessionRequest,
    resp_ty = NewSessionResponse,
}

acp_handler! {
    name = LoadSessionHandler,
    do_method = do_load_session,
    req_ty = LoadSessionRequest,
    resp_ty = LoadSessionResponse,
}

acp_handler! {
    name = ResumeSessionHandler,
    do_method = do_resume_session,
    req_ty = ResumeSessionRequest,
    resp_ty = ResumeSessionResponse,
}

acp_handler! {
    name = PromptHandler,
    do_method = do_prompt,
    req_ty = PromptRequest,
    resp_ty = PromptResponse,
}

acp_handler! {
    name = CloseSessionHandler,
    do_method = do_close_session,
    req_ty = CloseSessionRequest,
    resp_ty = CloseSessionResponse,
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::handler::NoopHandler;
    use crate::acp::types::{ContentBlock, Implementation, StopReason, TextContent};
    use crate::rpc::Request;

    async fn make_server() -> Arc<AcpServer> {
        Arc::new(AcpServer::new(Arc::new(NoopHandler)))
    }

    async fn init(server: &Arc<AcpServer>) -> InitializeResponse {
        let req = InitializeRequest {
            protocol_version: 1,
            client_capabilities: Some(Default::default()),
            client_info: Some(Implementation {
                name: "test-client".into(),
                title: None,
                version: "0.0.1".into(),
                _meta: None,
            }),
            _meta: None,
        };
        server.do_initialize(req).await.unwrap()
    }

    #[tokio::test]
    async fn initialize_transitions_protocol_state() {
        let server = make_server().await;
        assert_eq!(server.protocol_state().await, ProtocolState::Uninitialized);

        let resp = init(&server).await;
        assert_eq!(resp.protocol_version, 1);
        assert_eq!(server.protocol_state().await, ProtocolState::Initialized);
    }

    #[tokio::test]
    async fn double_initialize_is_rejected() {
        let server = make_server().await;
        init(&server).await;

        let req = InitializeRequest {
            protocol_version: 1,
            ..Default::default()
        };
        let err = server.do_initialize(req).await.unwrap_err();
        assert!(matches!(err, AcpError::InvalidState { .. }));
    }

    #[tokio::test]
    async fn session_new_creates_tracked_session() {
        let server = make_server().await;
        init(&server).await;

        let req = NewSessionRequest {
            cwd: "/tmp/test".into(),
            mcp_servers: vec![],
            ..Default::default()
        };
        let resp = server.do_new_session(req).await.unwrap();
        assert!(!resp.session_id.is_empty());

        let status = server.session_status(&resp.session_id).await;
        assert_eq!(status, Some(SessionStatus::Created));
        assert_eq!(server.session_count().await, 1);
    }

    #[tokio::test]
    async fn session_new_before_init_fails() {
        let server = make_server().await;
        let req = NewSessionRequest {
            cwd: "/tmp".into(),
            mcp_servers: vec![],
            ..Default::default()
        };
        let err = server.do_new_session(req).await.unwrap_err();
        assert!(matches!(err, AcpError::InvalidState { .. }));
    }

    #[tokio::test]
    async fn session_load_requires_load_session_capability() {
        let server = make_server().await;
        init(&server).await;

        let req = LoadSessionRequest {
            session_id: "sess_test".into(),
            cwd: "/tmp".into(),
            mcp_servers: vec![],
            ..Default::default()
        };
        let err = server.do_load_session(req).await.unwrap_err();
        assert!(matches!(err, AcpError::UnsupportedOperation { .. }));
    }

    #[tokio::test]
    async fn session_resume_requires_resume_capability() {
        let server = make_server().await;
        init(&server).await;

        let req = ResumeSessionRequest {
            session_id: "sess_test".into(),
            cwd: "/tmp".into(),
            mcp_servers: vec![],
            ..Default::default()
        };
        let err = server.do_resume_session(req).await.unwrap_err();
        assert!(matches!(err, AcpError::UnsupportedOperation { .. }));
    }

    #[tokio::test]
    async fn session_prompt_transitions_created_to_active() {
        let server = make_server().await;
        init(&server).await;

        let new_resp = server
            .do_new_session(NewSessionRequest {
                cwd: "/tmp".into(),
                mcp_servers: vec![],
                ..Default::default()
            })
            .await
            .unwrap();

        let prompt_resp = server
            .do_prompt(PromptRequest {
                session_id: new_resp.session_id.clone(),
                prompt: vec![ContentBlock::Text(TextContent {
                    text: "hello".into(),
                    annotations: None,
                    _meta: None,
                })],
                _meta: None,
            })
            .await
            .unwrap();
        assert_eq!(prompt_resp.stop_reason, StopReason::EndTurn);

        assert_eq!(
            server.session_status(&new_resp.session_id).await,
            Some(SessionStatus::Active)
        );
    }

    #[tokio::test]
    async fn session_prompt_unknown_session_fails() {
        let server = make_server().await;
        init(&server).await;

        let err = server
            .do_prompt(PromptRequest {
                session_id: "nonexistent".into(),
                prompt: vec![],
                _meta: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AcpError::UnknownSession { .. }));
    }

    #[tokio::test]
    async fn session_close_transitions_active_to_closed() {
        let server = make_server().await;
        init(&server).await;

        {
            let mut caps = server.agent_capabilities.write().await;
            caps.session_capabilities = Some(crate::acp::types::SessionCapabilities {
                close: Some(crate::acp::types::SessionCloseCapabilities::default()),
                ..Default::default()
            });
        }

        let new_resp = server
            .do_new_session(NewSessionRequest {
                cwd: "/tmp".into(),
                mcp_servers: vec![],
                ..Default::default()
            })
            .await
            .unwrap();

        server
            .do_prompt(PromptRequest {
                session_id: new_resp.session_id.clone(),
                prompt: vec![],
                _meta: None,
            })
            .await
            .unwrap();

        server
            .do_close_session(CloseSessionRequest {
                session_id: new_resp.session_id.clone(),
                _meta: None,
            })
            .await
            .unwrap();

        assert_eq!(
            server.session_status(&new_resp.session_id).await,
            Some(SessionStatus::Closed)
        );
    }

    #[tokio::test]
    async fn session_close_rejects_unknown_session() {
        let server = make_server().await;
        init(&server).await;

        let err = server
            .do_close_session(CloseSessionRequest {
                session_id: "nonexistent".into(),
                _meta: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AcpError::UnknownSession { .. }));
    }

    #[tokio::test]
    async fn session_close_requires_close_capability() {
        let server = make_server().await;
        init(&server).await;

        let new_resp = server
            .do_new_session(NewSessionRequest {
                cwd: "/tmp".into(),
                mcp_servers: vec![],
                ..Default::default()
            })
            .await
            .unwrap();

        let err = server
            .do_close_session(CloseSessionRequest {
                session_id: new_resp.session_id.clone(),
                _meta: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AcpError::UnsupportedOperation { .. }));
    }

    #[tokio::test]
    async fn session_close_closed_session_fails() {
        let server = make_server().await;
        init(&server).await;

        {
            let mut caps = server.agent_capabilities.write().await;
            caps.session_capabilities = Some(crate::acp::types::SessionCapabilities {
                close: Some(crate::acp::types::SessionCloseCapabilities::default()),
                ..Default::default()
            });
        }

        let new_resp = server
            .do_new_session(NewSessionRequest {
                cwd: "/tmp".into(),
                mcp_servers: vec![],
                ..Default::default()
            })
            .await
            .unwrap();
        let sid = new_resp.session_id.clone();

        server
            .do_close_session(CloseSessionRequest {
                session_id: sid.clone(),
                _meta: None,
            })
            .await
            .unwrap();

        let err = server
            .do_close_session(CloseSessionRequest {
                session_id: sid.clone(),
                _meta: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AcpError::InvalidTransition { .. }));
    }

    #[tokio::test]
    async fn handler_deserialises_prompts_correctly() {
        let server = make_server().await;
        init(&server).await;

        let req = NewSessionRequest {
            cwd: "/tmp".into(),
            mcp_servers: vec![],
            ..Default::default()
        };
        let new_resp = server.do_new_session(req).await.unwrap();

        let params = serde_json::json!({
            "sessionId": new_resp.session_id,
            "prompt": [
                {
                    "type": "text",
                    "text": "Hi!"
                }
            ]
        });

        let handler = PromptHandler::new(server.clone());
        let result = handler.handle(Some(params)).await.unwrap();
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn handler_deserialises_close_correctly() {
        let server = make_server().await;
        init(&server).await;

        {
            let mut caps = server.agent_capabilities.write().await;
            caps.session_capabilities = Some(crate::acp::types::SessionCapabilities {
                close: Some(crate::acp::types::SessionCloseCapabilities::default()),
                ..Default::default()
            });
        }

        let req = NewSessionRequest {
            cwd: "/tmp".into(),
            mcp_servers: vec![],
            ..Default::default()
        };
        let new_resp = server.do_new_session(req).await.unwrap();

        let handler = CloseSessionHandler::new(server.clone());
        let params = serde_json::json!({ "sessionId": new_resp.session_id });
        let result = handler.handle(Some(params)).await.unwrap();
        assert_eq!(result, serde_json::json!({}));
    }

    #[tokio::test]
    async fn handler_returns_jsonrpc_error_on_bad_params() {
        let server = make_server().await;
        init(&server).await;

        let handler = PromptHandler::new(server.clone());
        let params = serde_json::json!({ "missing_fields": true });
        let err = handler.handle(Some(params)).await.unwrap_err();
        assert_eq!(err.code, crate::acp::error::code::ACP_MALFORMED_REQUEST);
    }

    #[tokio::test]
    async fn handler_returns_jsonrpc_error_on_unknown_session() {
        let server = make_server().await;
        init(&server).await;

        let handler = PromptHandler::new(server.clone());
        let params = serde_json::json!({
            "sessionId": "nonexistent",
            "prompt": []
        });
        let err = handler.handle(Some(params)).await.unwrap_err();
        assert_eq!(err.code, crate::acp::error::code::ACP_UNKNOWN_SESSION);
    }

    #[tokio::test]
    async fn register_handlers_registers_all_methods() {
        let server = make_server().await;
        init(&server).await;

        let router = Router::new();
        server.register_handlers(&router).await;
        assert_eq!(router.method_count().await, 6);
        assert!(router.has_method(METHOD_INITIALIZE).await);
        assert!(router.has_method(METHOD_NEW_SESSION).await);
        assert!(router.has_method(METHOD_LOAD_SESSION).await);
        assert!(router.has_method(METHOD_RESUME_SESSION).await);
        assert!(router.has_method(METHOD_PROMPT).await);
        assert!(router.has_method(METHOD_CLOSE_SESSION).await);
    }

    #[tokio::test]
    async fn router_dispatch_routes_initialize() {
        let server = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let req = InitializeRequest {
            protocol_version: 1,
            client_capabilities: Some(Default::default()),
            client_info: Some(Implementation {
                name: "test".into(),
                title: None,
                version: "1.0".into(),
                _meta: None,
            }),
            _meta: None,
        };

        let params: Value = serde_json::to_value(req).unwrap();
        let request = Request::new(99, METHOD_INITIALIZE, Some(params));
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: InitializeResponse = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.protocol_version, 1);
    }

    #[tokio::test]
    async fn router_dispatch_creates_session() {
        let server = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let init_req = InitializeRequest {
            protocol_version: 1,
            ..Default::default()
        };
        let params: Value = serde_json::to_value(init_req).unwrap();
        let request = Request::new(1, METHOD_INITIALIZE, Some(params));
        router.dispatch(request).await;

        let new_req = NewSessionRequest {
            cwd: "/tmp".into(),
            mcp_servers: vec![],
            ..Default::default()
        };
        let params: Value = serde_json::to_value(new_req).unwrap();
        let request = Request::new(2, METHOD_NEW_SESSION, Some(params));
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: NewSessionResponse =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert!(!result.session_id.is_empty());
        assert_eq!(server.session_count().await, 1);
    }

    #[tokio::test]
    async fn router_dispatch_close_before_init_fails() {
        let server = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let req = CloseSessionRequest {
            session_id: "sess_123".into(),
            _meta: None,
        };
        let params: Value = serde_json::to_value(req).unwrap();
        let request = Request::new(3, METHOD_CLOSE_SESSION, Some(params));
        let response = router.dispatch(request).await;

        assert!(!response.is_success());
        let err = response.into_error().unwrap();
        assert_eq!(err.code, crate::acp::error::code::ACP_INVALID_STATE);
    }
}
