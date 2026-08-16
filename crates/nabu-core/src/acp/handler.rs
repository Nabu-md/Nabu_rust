//! # ACP Handler Abstraction
//!
//! The [`AcpHandler`] trait is the **delegation boundary** between the ACP
//! protocol layer (this module) and the agent runtime. The protocol layer
//! owns session state, validates lifecycle transitions, and serializes
//! requests/responses over JSON-RPC. The handler owns the *actual* work:
//! spawning agent processes, connecting to MCP servers, streaming tokens, etc.
//!
//! This module defines **only** the trait. Concrete implementations live in
//! the integration layer (Phase 3d+). The protocol layer never imports
//! [`crate::agent::AgentManager`] or
//! [`crate::process_supervisor::ProcessSupervisor`].

use async_trait::async_trait;

use crate::acp::error::AcpError;
use crate::acp::types::{
    CloseSessionRequest, CloseSessionResponse, InitializeRequest, InitializeResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
    PromptRequest, PromptResponse, ResumeSessionRequest, ResumeSessionResponse,
};

/// Handler trait for delegating ACP protocol operations to the agent runtime.
///
/// Implementors of this trait are responsible for the *actual* work behind
/// each ACP method — process management, MCP connections, LLM calls, etc.
/// The [`crate::acp::AcpServer`] validates protocol state and lifecycle
/// transitions before delegating to these methods.
///
/// ## Threading
///
/// Implementations must be `Send + Sync` because the server may be shared
/// across threads (e.g. registered on the JSON-RPC [`crate::rpc::Router`]).
///
/// ## Error handling
///
/// Handlers should return [`AcpError`] values for protocol-level failures
/// (unknown session, unsupported operation) and for runtime failures. The
/// server converts these into JSON-RPC error responses. Handlers should **not**
/// panic — use `Result` instead.
#[async_trait]
pub trait AcpHandler: Send + Sync {
    /// Handle the `initialize` method.
    ///
    /// Called once at the start of a connection to negotiate the protocol
    /// version and capabilities. The handler should return its supported
    /// protocol version (which may be lower than the requested version) and
    /// the capabilities it advertises.
    async fn initialize(
        &self,
        request: InitializeRequest,
    ) -> Result<InitializeResponse, AcpError>;

    /// Handle the `session/new` method.
    ///
    /// Create a new conversation session. The handler is responsible for
    /// setting up any required state (spawning a process, connecting to MCP
    /// servers, etc.) and returning a unique session ID.
    async fn new_session(
        &self,
        request: NewSessionRequest,
    ) -> Result<NewSessionResponse, AcpError>;

    /// Handle the `session/load` method.
    ///
    /// Load a previously persisted session (identified by `session_id`) and
    /// restore its state. The handler is responsible for reconnecting to MCP
    /// servers and replaying any necessary history.
    async fn load_session(
        &self,
        request: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, AcpError>;

    /// Handle the `session/prompt` method.
    ///
    /// Process a user prompt within an existing session. The handler is
    /// responsible for invoking the agent, streaming output (via
    /// `session/update` notifications in Phase 3b), and returning the final
    /// stop reason.
    ///
    /// The server guarantees that the session exists and is in a state that
    /// accepts prompts before calling this method.
    async fn prompt(
        &self,
        request: PromptRequest,
    ) -> Result<PromptResponse, AcpError>;

    /// Handle the `session/resume` method.
    ///
    /// Resume an existing session without replaying conversation history.
    /// The handler is responsible for restoring session context and reconnecting
    /// to MCP servers. Only called if the agent advertises
    /// `sessionCapabilities.resume`.
    async fn resume_session(
        &self,
        request: ResumeSessionRequest,
    ) -> Result<ResumeSessionResponse, AcpError>;

    /// Handle the `session/close` method.
    ///
    /// Close an active session and free any associated resources. The server
    /// guarantees that the session exists and can be closed before calling this
    /// method. Only called if the agent advertises
    /// `sessionCapabilities.close`.
    async fn close_session(
        &self,
        request: CloseSessionRequest,
    ) -> Result<CloseSessionResponse, AcpError>;
}

// ---------------------------------------------------------------------------
// A no-op default handler for testing and as a reference implementation.
// ---------------------------------------------------------------------------

/// A minimal [`AcpHandler`] implementation that fulfills protocol contracts
/// without spawning any subprocesses.
///
/// This is provided primarily for unit tests and as a reference so that
/// downstream integrators can see a complete, compilable example. Production
/// code should provide its own implementation that delegates to the agent
/// runtime.
#[derive(Debug, Default)]
pub struct NoopHandler;

#[async_trait]
impl AcpHandler for NoopHandler {
    async fn initialize(
        &self,
        request: InitializeRequest,
    ) -> Result<InitializeResponse, AcpError> {
        Ok(InitializeResponse {
            protocol_version: request.protocol_version.min(crate::acp::SUPPORTED_PROTOCOL_VERSION),
            agent_capabilities: None,
            agent_info: None,
            auth_methods: Vec::new(),
            _meta: None,
        })
    }

    async fn new_session(
        &self,
        _request: NewSessionRequest,
    ) -> Result<NewSessionResponse, AcpError> {
        use uuid::Uuid;
        Ok(NewSessionResponse {
            session_id: format!("sess_{}", Uuid::new_v4()),
            config_options: None,
            modes: None,
            _meta: None,
        })
    }

    async fn load_session(
        &self,
        _request: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, AcpError> {
        Ok(LoadSessionResponse {
            config_options: None,
            modes: None,
            _meta: None,
        })
    }

    async fn resume_session(
        &self,
        _request: ResumeSessionRequest,
    ) -> Result<ResumeSessionResponse, AcpError> {
        Ok(ResumeSessionResponse::default())
    }

    async fn prompt(&self, _request: PromptRequest) -> Result<PromptResponse, AcpError> {
        Ok(PromptResponse {
            stop_reason: crate::acp::StopReason::EndTurn,
            _meta: None,
        })
    }

    async fn close_session(
        &self,
        _request: CloseSessionRequest,
    ) -> Result<CloseSessionResponse, AcpError> {
        Ok(CloseSessionResponse::default())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::types::*;

    #[tokio::test]
    async fn noop_handler_initialize_returns_min_version() {
        let handler = NoopHandler;
        let req = InitializeRequest {
            protocol_version: 5,
            client_capabilities: None,
            client_info: None,
            _meta: None,
        };
        let resp = handler.initialize(req).await.unwrap();
        assert_eq!(resp.protocol_version, crate::acp::SUPPORTED_PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn noop_handler_new_session_returns_session_id() {
        let handler = NoopHandler;
        let req = NewSessionRequest {
            cwd: "/tmp".to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };
        let resp = handler.new_session(req).await.unwrap();
        assert!(!resp.session_id.is_empty());
    }

    #[tokio::test]
    async fn noop_handler_prompt_returns_end_turn() {
        let handler = NoopHandler;
        let req = PromptRequest {
            session_id: "sess_test".to_string(),
            prompt: vec![ContentBlock::Text(TextContent {
                text: "hi".to_string(),
                annotations: None,
                _meta: None,
            })],
            _meta: None,
        };
        let resp = handler.prompt(req).await.unwrap();
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
    }

    #[tokio::test]
    async fn noop_handler_close_session_returns_empty() {
        let handler = NoopHandler;
        let req = CloseSessionRequest {
            session_id: "sess_test".to_string(),
            _meta: None,
        };
        let resp = handler.close_session(req).await.unwrap();
        assert!(resp._meta.is_none());
    }

    #[tokio::test]
    async fn noop_handler_resume_session_returns_ok() {
        let handler = NoopHandler;
        let req = ResumeSessionRequest {
            session_id: "sess_test".to_string(),
            cwd: "/tmp".to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };
        let resp = handler.resume_session(req).await.unwrap();
        assert!(resp._meta.is_none());
    }
}
