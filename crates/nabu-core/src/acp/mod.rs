//! # ACP — Agent Communication Protocol
//!
//! This module implements the ACP v1 protocol layer for the Nabu Capability
//! Platform. It provides:
//!
//! - [`AcpServer`]: The protocol coordinator — owns session state, validates
//!   lifecycle transitions, and delegates to an [`AcpHandler`].
//! - [`AcpHandler`]: The delegation boundary between the protocol layer and
//!   the agent runtime.
//! - [`NoopHandler`]: A minimal reference implementation for testing.
//! - [`AcpError`]: Structured protocol-level errors with JSON-RPC mappings.
//! - [`decode_params`]: Convenience deserialiser for ACP request parameters.
//!
//! ## Integration
//!
//! After creating an [`AcpServer`] with a concrete [`AcpHandler`]
//! implementation, call [`AcpServer::register_handlers`] to register all ACP
//! method handlers on a [`crate::rpc::Router`]. The router then dispatches
//! incoming JSON-RPC requests through the per-operation handler structs
//! (`InitializeHandler`, `NewSessionHandler`, etc.).

pub mod error;
pub mod handler;
pub mod server;
pub mod state;
pub mod types;

pub use error::AcpError;
pub use handler::{AcpClientHandler, NoopClientHandler};

// Backwards-compatible alias for the server-side handler trait.
// This trait is separate from AcpClientHandler to maintain dyn-compatibility
// (the client handler uses RPIT impl Future which is not object-safe).
#[async_trait::async_trait]
pub trait AcpHandler: Send + Sync {
    /// Handle the `initialize` method.
    async fn initialize(
        &self,
        request: crate::acp::types::InitializeRequest,
    ) -> Result<crate::acp::types::InitializeResponse, AcpError>;

    /// Handle the `session/new` method.
    async fn new_session(
        &self,
        request: crate::acp::types::NewSessionRequest,
    ) -> Result<crate::acp::types::NewSessionResponse, AcpError>;

    /// Handle the `session/load` method.
    async fn load_session(
        &self,
        request: crate::acp::types::LoadSessionRequest,
    ) -> Result<crate::acp::types::LoadSessionResponse, AcpError>;

    /// Handle the `session/resume` method.
    async fn resume_session(
        &self,
        request: crate::acp::types::ResumeSessionRequest,
    ) -> Result<crate::acp::types::ResumeSessionResponse, AcpError>;

    /// Handle the `session/prompt` method.
    async fn prompt(
        &self,
        request: crate::acp::types::PromptRequest,
    ) -> Result<crate::acp::types::PromptResponse, AcpError>;

    /// Handle the `session/close` method.
    async fn close_session(
        &self,
        request: crate::acp::types::CloseSessionRequest,
    ) -> Result<crate::acp::types::CloseSessionResponse, AcpError>;
}

pub type NoopHandler = NoopClientHandler;
pub use server::AcpServer;
pub use state::{
    ClientState, ConnectionState, NegotiatedCapabilities, ProtocolState, ServerEntry,
    SessionEntry, SessionState, SessionStatus,
};

// Re-export method constants.
pub use server::{
    METHOD_CLOSE_SESSION, METHOD_INITIALIZE, METHOD_LOAD_SESSION, METHOD_NEW_SESSION,
    METHOD_PROMPT, METHOD_RESUME_SESSION,
};

pub use types::decode_params;

// Re-export key protocol types.
pub use types::{
    AgentCapabilities, ClientCapabilities, ClientSessionCapabilities,
    CloseSessionRequest, CloseSessionResponse, ContentBlock, Cost,
    EnvVariable, FileSystemCapabilities, HttpHeader, Implementation,
    InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    LogoutCapabilities, McpCapabilities, McpServer, McpServerHttp, McpServerSse,
    McpServerStdio, NewSessionRequest, NewSessionResponse, PromptCapabilities,
    PromptRequest, PromptResponse, ResumeSessionRequest, ResumeSessionResponse,
    Role, SessionId, SessionListCapabilities, SessionResumeCapabilities,
    StopReason, SUPPORTED_PROTOCOL_VERSION, UsageUpdate,
};

// Blanket impl: any type that implements AcpClientHandler can be used as
// an AcpHandler. The NoopClientHandler provides no-op implementations.
#[async_trait::async_trait]
impl AcpHandler for NoopClientHandler {
    async fn initialize(
        &self,
        _request: crate::acp::types::InitializeRequest,
    ) -> Result<crate::acp::types::InitializeResponse, AcpError> {
        Ok(crate::acp::types::InitializeResponse {
            protocol_version: crate::acp::types::SUPPORTED_PROTOCOL_VERSION,
            agent_capabilities: Some(crate::acp::types::AgentCapabilities {
                load_session: false,
                ..Default::default()
            }),
            agent_info: None,
            auth_methods: vec![],
            _meta: None,
        })
    }

    async fn new_session(
        &self,
        _request: crate::acp::types::NewSessionRequest,
    ) -> Result<crate::acp::types::NewSessionResponse, AcpError> {
        Ok(crate::acp::types::NewSessionResponse {
            session_id: uuid::Uuid::new_v4().to_string(),
            config_options: None,
            modes: None,
            _meta: None,
        })
    }

    async fn load_session(
        &self,
        _request: crate::acp::types::LoadSessionRequest,
    ) -> Result<crate::acp::types::LoadSessionResponse, AcpError> {
        Ok(crate::acp::types::LoadSessionResponse::default())
    }

    async fn resume_session(
        &self,
        _request: crate::acp::types::ResumeSessionRequest,
    ) -> Result<crate::acp::types::ResumeSessionResponse, AcpError> {
        Ok(crate::acp::types::ResumeSessionResponse::default())
    }

    async fn prompt(
        &self,
        _request: crate::acp::types::PromptRequest,
    ) -> Result<crate::acp::types::PromptResponse, AcpError> {
        Ok(crate::acp::types::PromptResponse {
            stop_reason: crate::acp::types::StopReason::EndTurn,
            _meta: None,
        })
    }

    async fn close_session(
        &self,
        _request: crate::acp::types::CloseSessionRequest,
    ) -> Result<crate::acp::types::CloseSessionResponse, AcpError> {
        Ok(crate::acp::types::CloseSessionResponse::default())
    }
}
