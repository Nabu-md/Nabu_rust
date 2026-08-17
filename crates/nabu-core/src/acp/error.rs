//! # ACP Client Error Types
//!
//! Structured errors for the ACP client, covering protocol failures,
//! transport errors, state-machine violations, and agent→client
//! request dispatch errors.
//!
//! All `AcpError` values are convertible into a JSON-RPC error response
//! via [`AcpError::into_jsonrpc`] so that the client can communicate
//! failures back to the agent when it is acting as a handler for
//! agent-originated requests.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A structured ACP client error.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpError {
    /// The error category, suitable for programmatic matching.
    pub kind: ErrorKind,
    /// Human-readable description of the error.
    pub message: String,
    /// Optional additional structured data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Categories of ACP client errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The transport (stdio pipe) was closed or produced an I/O error.
    TransportClosed,
    /// The agent sent a message that could not be parsed as JSON-RPC.
    JsonRpcParseError,
    /// The client sent a request in an invalid internal state.
    InvalidState,
    /// The protocol version negotiated does not match what the client supports.
    ProtocolVersionMismatch,
    /// The agent returned an error response to a request.
    AgentError,
    /// The agent's response was missing required fields or had wrong types.
    MalformedResponse,
    /// An incoming request/notification had invalid parameters.
    MalformedRequest,
    /// The client received an agent→client request for a method it does not
    /// support.
    UnsupportedOperation,
    /// The client could not find a session with the given ID.
    UnknownSession,
    /// A pending request was cancelled (either by the client or the agent).
    RequestCancelled,
    /// A timeout occurred while waiting for a response.
    Timeout,
    /// The agent returned the `method not found` error.
    MethodNotFound,
    /// Invalid parameters were sent to an ACP method.
    InvalidParams,
    /// An internal error occurred in the ACP client logic.
    Internal,
}

impl AcpError {
    /// Create a new `AcpError` with the given category and message.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            data: None,
        }
    }

    /// Attach additional structured data to the error.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Convenience: create an `InvalidState` error.
    pub fn invalid_state(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidState, msg)
    }

    /// Convenience: create a `ProtocolVersionMismatch` error.
    pub fn protocol_version_mismatch(
        expected: crate::acp::types::ProtocolVersion,
        actual: crate::acp::types::ProtocolVersion,
    ) -> Self {
        Self::new(
            ErrorKind::ProtocolVersionMismatch,
            format!(
                "protocol version mismatch: client supports v{}, agent offers v{}",
                expected, actual
            ),
        )
        .with_data(serde_json::json!({
            "expected": expected,
            "actual": actual
        }))
    }

    /// Convenience: create an `InvalidState` error for a specific session.
    pub fn invalid_state_session(session_id: &str, msg: impl Into<String>) -> Self {
        Self::new(
            ErrorKind::InvalidState,
            format!("session '{}': {}", session_id, msg.into()),
        )
        .with_data(serde_json::json!({ "sessionId": session_id }))
    }

    /// Convenience: create an `InvalidState` error for an invalid transition.
    pub fn invalid_transition(
        current: impl std::fmt::Debug,
        target: &str,
        msg: impl Into<String>,
    ) -> Self {
        Self::new(
            ErrorKind::InvalidState,
            format!(
                "invalid transition from {:?} to '{}': {}",
                current,
                target,
                msg.into()
            ),
        )
    }

    /// Convenience: create an `UnsupportedOperation` error.
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::unsupported_operation(msg)
    }

    /// Convenience: create a `MalformedRequest` error.
    pub fn malformed_request(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::MalformedRequest, msg)
    }

    /// Convenience: create a `MalformedResponse` error.
    pub fn malformed_response(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::MalformedResponse, msg)
    }

    /// Convenience: create an `UnknownSession` error.
    pub fn unknown_session(session_id: impl Into<String>) -> Self {
        let sid = session_id.into();
        Self::new(
            ErrorKind::UnknownSession,
            format!("unknown session: {}", sid),
        )
        .with_data(serde_json::json!({ "sessionId": sid }))
    }

    /// Convenience: create an `UnsupportedOperation` error.
    pub fn unsupported_operation(method: impl Into<String>) -> Self {
        let m = method.into();
        Self::new(
            ErrorKind::UnsupportedOperation,
            format!("unsupported method: {}", m),
        )
        .with_data(serde_json::json!({ "method": m }))
    }

    /// Convenience: create a `RequestCancelled` error.
    pub fn request_cancelled(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::RequestCancelled, msg)
    }

    /// Convenience: create a `TransportClosed` error.
    pub fn transport_closed(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::TransportClosed, msg)
    }

    /// Convenience: create a `Timeout` error.
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Timeout, msg)
    }

    /// Convert this ACP error into a JSON-RPC error object, suitable for
    /// responding to an agent-originated request with an error.
    ///
    /// The JSON-RPC error code is derived from the `ErrorKind`:
    /// - `MethodNotFound` → `-32601`
    /// - `InvalidParams` / `MalformedRequest` → `-32602`
    /// - `Timeout` / `TransportClosed` → `-32000` (server error)
    /// - Everything else → `-32000` (server error)
    pub fn into_jsonrpc(self) -> crate::rpc::JsonRpcError {
        self.into_jsonrpc_error()
    }

    /// Convert this ACP error into a JSON-RPC error object.
    ///
    /// Alias for [`into_jsonrpc`](Self::into_jsonrpc) for backwards
    /// compatibility with the server-side handler code.
    pub fn into_jsonrpc_error(self) -> crate::rpc::JsonRpcError {
        let code = match self.kind {
            ErrorKind::MethodNotFound => -32601,
            ErrorKind::InvalidParams | ErrorKind::MalformedRequest => -32602,
            _ => -32000,
        };

        crate::rpc::JsonRpcError {
            code,
            message: self.message,
            data: self.data,
        }
    }
}

impl fmt::Display for AcpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)?;
        if let Some(data) = &self.data {
            write!(f, " ({})", data)?;
        }
        Ok(())
    }
}

impl std::error::Error for AcpError {}

impl From<crate::rpc::JsonRpcError> for AcpError {
    fn from(e: crate::rpc::JsonRpcError) -> Self {
        let kind = match e.code {
            -32601 => ErrorKind::MethodNotFound,
            -32602 => ErrorKind::InvalidParams,
            -32603 | -32000 => ErrorKind::Internal,
            _ => ErrorKind::AgentError,
        };
        Self::new(kind, e.message.clone())
            .with_data(e.data.clone().unwrap_or(serde_json::Value::Null))
    }
}

impl From<std::io::Error> for AcpError {
    fn from(e: std::io::Error) -> Self {
        // Distinguish a broken pipe (transport closed) from other I/O errors.
        let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof
            || e.kind() == std::io::ErrorKind::BrokenPipe
        {
            ErrorKind::TransportClosed
        } else {
            ErrorKind::Internal
        };
        Self::new(kind, e.to_string())
    }
}

impl From<serde_json::Error> for AcpError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ErrorKind::JsonRpcParseError, e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_jsonrpc_maps_error_codes() {
        let err = AcpError::new(ErrorKind::MethodNotFound, "method not found");
        let rpc = err.into_jsonrpc();
        assert_eq!(rpc.code, -32601);
        assert_eq!(rpc.message, "method not found");

        let err = AcpError::new(ErrorKind::InvalidParams, "bad params");
        let rpc = err.into_jsonrpc();
        assert_eq!(rpc.code, -32602);

        let err = AcpError::new(ErrorKind::Timeout, "timed out");
        let rpc = err.into_jsonrpc();
        assert_eq!(rpc.code, -32000);
    }

    #[test]
    fn protocol_version_mismatch_message() {
        let err = AcpError::protocol_version_mismatch(1, 2);
        assert!(err.message.contains("v1"));
        assert!(err.message.contains("v2"));
        assert_eq!(err.kind, ErrorKind::ProtocolVersionMismatch);
    }

    #[test]
    fn unknown_session_message() {
        let err = AcpError::unknown_session("sess_abc");
        assert!(err.message.contains("sess_abc"));
        assert_eq!(err.kind, ErrorKind::UnknownSession);
    }

    #[test]
    fn unsupported_operation_message() {
        let err = AcpError::unsupported_operation("fs/read_binary_file");
        assert!(err.message.contains("fs/read_binary_file"));
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
    }

    #[test]
    fn display_includes_kind_and_message() {
        let err = AcpError::new(ErrorKind::Timeout, "operation too slow");
        let s = format!("{}", err);
        assert!(s.contains("Timeout"));
        assert!(s.contains("operation too slow"));
    }

    #[test]
    fn from_io_error_transport_closed_on_eof() {
        let io_err = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "pipe closed");
        let err: AcpError = io_err.into();
        assert_eq!(err.kind, ErrorKind::TransportClosed);
    }

    #[test]
    fn from_io_error_internal_on_other() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err: AcpError = io_err.into();
        assert_eq!(err.kind, ErrorKind::Internal);
    }

    #[test]
    fn from_jsonrpc_error_method_not_found() {
        let rpc = crate::rpc::JsonRpcError {
            code: -32601,
            message: "method not found".to_string(),
            data: None,
        };
        let err: AcpError = rpc.into();
        assert_eq!(err.kind, ErrorKind::MethodNotFound);
    }

    #[test]
    fn error_is_serializable() {
        let err = AcpError::new(ErrorKind::Timeout, "timed out")
            .with_data(serde_json::json!({"ms": 5000}));
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("timeout"));
        assert!(json.contains("5000"));
    }
}
