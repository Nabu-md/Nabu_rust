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

// Backwards-compatible aliases for the server-side handler names.
pub trait AcpHandler: AcpClientHandler {}
impl<T: AcpClientHandler + ?Sized> AcpHandler for T {}
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
