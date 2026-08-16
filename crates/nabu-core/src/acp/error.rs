//! # ACP Error Types
//!
//! Structured error types for the ACP protocol layer. These errors capture
//! protocol-level failures (invalid state, unknown sessions, malformed
//! requests, unsupported operations, invalid transitions) and translate them
//! into JSON-RPC error objects so they can be serialized through the existing
//! [`crate::rpc`] infrastructure.
//!
//! The ACP layer never uses `unwrap()` or `expect()` as control flow — every
//! failure is a typed `AcpError` that is converted to a [`JsonRpcError`] when
//! surfaced over the wire.

use serde::Serialize;
use std::fmt::{self, Display};

use crate::rpc::{ErrorCode, JsonRpcError};

use super::state::SessionStatus;
use super::types::SessionId;

/// ACP-specific JSON-RPC error codes.
///
/// These occupy the server-error range reserved by JSON-RPC 2.0
/// (`-32099` to `-32000`) so that ACP protocol failures are distinguishable
/// on the wire from generic JSON-RPC errors.
pub mod code {
    /// Operation is invalid given the current protocol/session state.
    pub const ACP_INVALID_STATE: i64 = -32000;
    /// No session with the given ID is known to the server.
    pub const ACP_UNKNOWN_SESSION: i64 = -32001;
    /// A session-state transition was rejected by the state machine.
    pub const ACP_INVALID_TRANSITION: i64 = -32002;
    /// The requested operation is not advertised/supported by the agent.
    pub const ACP_UNSUPPORTED_OPERATION: i64 = -32003;
    /// Request parameters were malformed or could not be deserialized.
    pub const ACP_MALFORMED_REQUEST: i64 = -32004;
    /// The `AcpHandler` implementation returned an error.
    pub const ACP_HANDLER_ERROR: i64 = -32005;
}

/// Errors produced by the ACP protocol layer.
///
/// Every variant carries enough context to produce a meaningful JSON-RPC error
/// response. The [`AcpError::into_jsonrpc_error`] method performs the
/// conversion, preserving the ACP-specific code and embedding useful context
/// in the `data` field.
#[derive(Debug, Clone)]
pub enum AcpError {
    /// The operation is invalid given the current protocol or session state.
    ///
    /// Examples: calling `session/prompt` before `initialize`, or calling
    /// `session/end` on an already-closed session.
    InvalidState {
        message: String,
        /// The session ID involved, if any.
        session_id: Option<SessionId>,
    },

    /// No session with the given ID is known.
    UnknownSession { session_id: SessionId },

    /// Request parameters were malformed, missing required fields, or could
    /// not be deserialized into the typed request.
    MalformedRequest { message: String },

    /// The requested operation is not supported by the agent.
    ///
    /// Examples: `session/load` when the agent did not advertise
    /// `loadSession`, or `session/end` when the agent did not advertise
    /// `sessionCapabilities.close`.
    UnsupportedOperation { message: String },

    /// A state-machine transition was explicitly rejected.
    ///
    /// This is distinct from [`AcpError::InvalidState`]: this error carries
    /// the current and attempted target state so callers and logs can report
    /// exactly what transition was disallowed.
    InvalidTransition {
        current: SessionStatus,
        target: &'static str,
        message: String,
    },

    /// An underlying RPC, serialization, or I/O failure occurred.
    RpcError { message: String },

    /// The [`AcpHandler`](super::AcpHandler) returned an error.
    HandlerError { message: String },
}

impl AcpError {
    /// Returns the ACP-specific JSON-RPC error code for this error.
    pub fn code(&self) -> i64 {
        match self {
            Self::InvalidState { .. } => code::ACP_INVALID_STATE,
            Self::UnknownSession { .. } => code::ACP_UNKNOWN_SESSION,
            Self::InvalidTransition { .. } => code::ACP_INVALID_TRANSITION,
            Self::UnsupportedOperation { .. } => code::ACP_UNSUPPORTED_OPERATION,
            Self::MalformedRequest { .. } => code::ACP_MALFORMED_REQUEST,
            Self::RpcError { .. } => ErrorCode::InternalError.code(),
            Self::HandlerError { .. } => code::ACP_HANDLER_ERROR,
        }
    }

    /// Returns the human-readable message for this error.
    pub fn message(&self) -> String {
        match self {
            Self::InvalidState { message, .. } => message.clone(),
            Self::UnknownSession { session_id } => {
                format!("Unknown session: {}", session_id)
            }
            Self::MalformedRequest { message } => message.clone(),
            Self::UnsupportedOperation { message } => message.clone(),
            Self::InvalidTransition { message, .. } => message.clone(),
            Self::RpcError { message } => message.clone(),
            Self::HandlerError { message } => message.clone(),
        }
    }

    /// Returns structured context data for the JSON-RPC `data` field.
    fn data(&self) -> Option<serde_json::Value> {
        #[derive(Serialize)]
        struct ErrorData {
            kind: &'static str,
            session_id: Option<SessionId>,
            current: Option<String>,
            target: Option<&'static str>,
        }

        let data = match self {
            Self::InvalidState { session_id, .. } => ErrorData {
                kind: "invalid_state",
                session_id: session_id.clone(),
                current: None,
                target: None,
            },
            Self::UnknownSession { session_id } => ErrorData {
                kind: "unknown_session",
                session_id: Some(session_id.clone()),
                current: None,
                target: None,
            },
            Self::MalformedRequest { .. } => ErrorData {
                kind: "malformed_request",
                session_id: None,
                current: None,
                target: None,
            },
            Self::UnsupportedOperation { .. } => ErrorData {
                kind: "unsupported_operation",
                session_id: None,
                current: None,
                target: None,
            },
            Self::InvalidTransition { current, target, .. } => ErrorData {
                kind: "invalid_transition",
                session_id: None,
                current: Some(current.to_string()),
                target: Some(*target),
            },
            Self::RpcError { .. } => ErrorData {
                kind: "rpc_error",
                session_id: None,
                current: None,
                target: None,
            },
            Self::HandlerError { .. } => ErrorData {
                kind: "handler_error",
                session_id: None,
                current: None,
                target: None,
            },
        };

        serde_json::to_value(data).ok()
    }

    /// Converts this ACP error into a [`JsonRpcError`] suitable for returning
    /// from an [`crate::rpc::RpcHandler`] implementation.
    pub fn into_jsonrpc_error(self) -> JsonRpcError {
        let code = self.code();
        let message = self.message();
        let data = self.data();

        JsonRpcError {
            code,
            message,
            data,
        }
    }
}

impl Display for AcpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for AcpError {}

/// Convenience constructors mirroring the patterns used throughout the
/// `rpc` module (e.g. [`JsonRpcError::invalid_params`]).
impl AcpError {
    /// Create an `InvalidState` error with no associated session.
    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::InvalidState {
            message: message.into(),
            session_id: None,
        }
    }

    /// Create an `InvalidState` error associated with a session.
    pub fn invalid_state_session(id: &str, message: impl Into<String>) -> Self {
        Self::InvalidState {
            message: message.into(),
            session_id: Some(id.to_string()),
        }
    }

    /// Create an `InvalidTransition` error.
    pub fn invalid_transition(
        current: SessionStatus,
        target: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::InvalidTransition {
            current,
            target,
            message: message.into(),
        }
    }

    /// Create an `UnknownSession` error.
    pub fn unknown_session(id: &str) -> Self {
        Self::UnknownSession {
            session_id: id.to_string(),
        }
    }

    /// Create a `MalformedRequest` error.
    pub fn malformed_request(message: impl Into<String>) -> Self {
        Self::MalformedRequest {
            message: message.into(),
        }
    }

    /// Create an `UnsupportedOperation` error.
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::UnsupportedOperation {
            message: message.into(),
        }
    }

    /// Create a `HandlerError`.
    pub fn handler_error(message: impl Into<String>) -> Self {
        Self::HandlerError {
            message: message.into(),
        }
    }

    /// Create an `RpcError` from any serde_json error.
    pub fn from_serde_json(err: serde_json::Error) -> Self {
        Self::RpcError {
            message: err.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// From/Into conversions
// ---------------------------------------------------------------------------

impl From<AcpError> for JsonRpcError {
    fn from(err: AcpError) -> Self {
        err.into_jsonrpc_error()
    }
}

impl From<serde_json::Error> for AcpError {
    fn from(err: serde_json::Error) -> Self {
        Self::from_serde_json(err)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_state_maps_to_acp_code() {
        let err = AcpError::invalid_state("not initialized");
        let rpc = err.into_jsonrpc_error();
        assert_eq!(rpc.code, code::ACP_INVALID_STATE);
        assert!(rpc.message.contains("not initialized"));
    }

    #[test]
    fn unknown_session_includes_id_in_data() {
        let err = AcpError::unknown_session("sess_123");
        let rpc = err.into_jsonrpc_error();
        assert_eq!(rpc.code, code::ACP_UNKNOWN_SESSION);
        let data = rpc.data.unwrap();
        assert_eq!(data["kind"], "unknown_session");
        assert_eq!(data["session_id"], "sess_123");
    }

    #[test]
    fn malformed_request_uses_invalid_params_code() {
        let err = AcpError::malformed_request("missing field `protocolVersion`");
        let rpc = err.into_jsonrpc_error();
        assert_eq!(rpc.code, code::ACP_MALFORMED_REQUEST);
        assert!(rpc.message.contains("missing field"));
    }

    #[test]
    fn unsupported_operation_has_correct_code() {
        let err = AcpError::unsupported("loadSession not advertised");
        let rpc = err.into_jsonrpc_error();
        assert_eq!(rpc.code, code::ACP_UNSUPPORTED_OPERATION);
    }

    #[test]
    fn invalid_transition_carries_states_in_data() {
        let err = AcpError::invalid_transition(
            SessionStatus::Closed,
            "prompt",
            "cannot prompt a closed session",
        );
        let rpc = err.into_jsonrpc_error();
        assert_eq!(rpc.code, code::ACP_INVALID_TRANSITION);
        let data = rpc.data.unwrap();
        assert_eq!(data["kind"], "invalid_transition");
        assert_eq!(data["current"], "Closed");
        assert_eq!(data["target"], "prompt");
    }

    #[test]
    fn handler_error_carries_message() {
        let err = AcpError::handler_error("agent crashed");
        let rpc = err.into_jsonrpc_error();
        assert_eq!(rpc.code, code::ACP_HANDLER_ERROR);
        assert_eq!(rpc.message, "agent crashed");
    }

    #[test]
    fn serde_error_converts_to_rpc_error() {
        let json = "{ invalid }";
        let result: Result<serde_json::Value, _> = serde_json::from_str(json);
        let err = AcpError::from_serde_json(result.unwrap_err());
        let rpc = err.into_jsonrpc_error();
        assert_eq!(rpc.code, ErrorCode::InternalError.code());
    }

    #[test]
    fn display_includes_code_and_message() {
        let err = AcpError::invalid_state("test message");
        let s = format!("{}", err);
        assert!(s.contains("-32000"));
        assert!(s.contains("test message"));
    }
}
