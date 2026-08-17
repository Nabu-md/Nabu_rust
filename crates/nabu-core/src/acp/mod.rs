//! # ACP Client Module — Nabu as an ACP Client
//!
//! Nabu is the **ACP client** (editor). The external process is the ACP
//! **agent** (coding agent).
//!
//! ## Architecture
//!
//! ```text
//! Nabu (ACP client)
//!   │
//!   │ JSON-RPC 2.0 over stdio
//!   │
//!   ▼
//! ACP Agent
//!   │
//!   ├── responses to client requests
//!   ├── session/update notifications
//!   └── agent → client requests (fs/read_text_file, terminal/*, etc.)
//! ```
//!
//! ## Module layout
//!
//! - [`types`] — ACP v1 protocol types (requests, responses, notifications,
//!   capabilities, content blocks)
//! - [`error`] — Structured client-side error types
//! - [`state`] — Client connection and session state machine
//! - [`events`] — Inbound message classification and agent→client request
//!   dispatch enum
//! - [`transport`] — Transport trait and concrete implementations (stdio,
//!   mock channels)
//! - [`handler`] — `AcpClientHandler` trait for dispatching agent→client
//!   requests to application logic
//! - [`client`] — The `AcpClient` struct: sends requests, runs the message
//!   loop, dispatches responses/notifications/requests
//!
//! ## Usage
//!
//! ```no_run
//! use nabu_core::acp::{AcpClient, NoopClientHandler, MockTransport};
//! use std::sync::Arc;
//! use nabu_core::acp::types::{
//!     Implementation, ClientCapabilities, FileSystemCapabilities,
//! };
//!
//! let (transport, _peer) = MockTransport::pair();
//! let handler = Arc::new(NoopClientHandler);
//! let mut client = AcpClient::new(transport, handler);
//!
//! // Initialize the connection
//! let _init = client.initialize(
//!     Some(Implementation {
//!         name: "nabu".to_string(),
//!         title: Some("Nabu".to_string()),
//!         version: "0.1.0".to_string(),
//!         _meta: None,
//!     }),
//!     Some(ClientCapabilities {
//!         fs: Some(FileSystemCapabilities {
//!             read_text_file: true,
//!             write_text_file: true,
//!             _meta: None,
//!         }),
//!         terminal: true,
//!         session: None,
//!         _meta: None,
//!     }),
//! ).await;
//! ```

pub mod client;
pub mod error;
pub mod events;
pub mod handler;
pub mod state;
pub mod transport;
pub mod types;

// Re-exports
pub use client::AcpClient;
pub use error::{AcpError, ErrorKind};
pub use handler::{dispatch_agent_request, AcpClientHandler, NoopClientHandler};
pub use state::{ClientState, ConnectionState, NegotiatedCapabilities, SessionEntry, SessionState};
#[cfg(not(target_arch = "wasm32"))]
pub use transport::StdioTransport;
pub use transport::{MockPeer, MockTransport, Transport};
pub use types::*;

// Re-export key RPC types used by the ACP client
pub use crate::rpc::{JsonRpcError, RequestId};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn method_constants_are_correct() {
        assert_eq!(METHOD_INITIALIZE, "initialize");
        assert_eq!(METHOD_NEW_SESSION, "session/new");
        assert_eq!(METHOD_PROMPT, "session/prompt");
        assert_eq!(METHOD_CANCEL, "session/cancel");
        assert_eq!(METHOD_CLOSE_SESSION, "session/close");
        assert_eq!(METHOD_DELETE_SESSION, "session/delete");
        assert_eq!(METHOD_LIST_SESSIONS, "session/list");
        assert_eq!(METHOD_LOAD_SESSION, "session/load");
        assert_eq!(METHOD_RESUME_SESSION, "session/resume");
        assert_eq!(METHOD_READ_TEXT_FILE, "fs/read_text_file");
        assert_eq!(METHOD_WRITE_TEXT_FILE, "fs/write_text_file");
        assert_eq!(METHOD_REQUEST_PERMISSION, "session/request_permission");
        assert_eq!(CANCEL_REQUEST_METHOD, "$/cancel_request");
        assert_eq!(SUPPORTED_PROTOCOL_VERSION, 1);
    }

    #[tokio::test]
    async fn noop_handler_implements_trait() {
        let handler: Arc<dyn AcpClientHandler> = Arc::new(NoopClientHandler);
        let _ = handler
            .read_text_file(&ReadTextFileRequest {
                path: "/test".to_string(),
                session_id: "s1".to_string(),
                limit: None,
                line: None,
                _meta: None,
            })
            .await;
    }
}
