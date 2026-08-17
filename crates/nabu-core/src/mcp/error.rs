//! # MCP Error Types
//!
//! Structured error types for the Model Context Protocol (MCP) server layer.
//! These errors carry MCP-specific error codes and messages, and convert to
//! JSON-RPC errors via [`JsonRpcError`](crate::rpc::JsonRpcError) so that MCP
//! methods dispatched through the [`Router`](crate::rpc::Router) produce
//! standard protocol-level error responses.
//!
//! ## Error Categories
//!
//! | Code    | Meaning |
//! |---------|---------|
//! | `-32600` | Invalid request (protocol state, method misuse) |
//! | `-32602` | Invalid params (missing/malformed arguments) |
//! | `-32603` | Internal error (Nabu-side failure) |
//!
//! MCP does not define reserved error codes in the `-32900` range outside of
//! the notification/session layer; server-side failures use the standard
//! JSON-RPC server-error range.

use serde_json::Value;
use std::fmt;

use crate::rpc::{ErrorCode, JsonRpcError};

/// The category of an MCP error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpErrorKind {
    /// The MCP server has not been initialized (no `initialize` exchange).
    NotInitialized,
    /// The method is not recognized by the MCP server.
    MethodNotFound,
    /// Parameters were missing, malformed, or of the wrong type.
    InvalidParams,
    /// The named tool was not found.
    ToolNotFound,
    /// A resource URI was not found or is not readable.
    ResourceNotFound,
    /// An internal error occurred in the MCP server or Nabu backend.
    Internal,
}

impl McpErrorKind {
    pub fn code(self) -> i64 {
        match self {
            McpErrorKind::NotInitialized => -32600,
            McpErrorKind::MethodNotFound => -32601,
            McpErrorKind::InvalidParams => -32602,
            McpErrorKind::ToolNotFound => -32603,
            McpErrorKind::ResourceNotFound => -32603,
            McpErrorKind::Internal => -32603,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            McpErrorKind::NotInitialized => "MCP server is not initialized",
            McpErrorKind::MethodNotFound => "Method not found",
            McpErrorKind::InvalidParams => "Invalid params",
            McpErrorKind::ToolNotFound => "Tool not found",
            McpErrorKind::ResourceNotFound => "Resource not found",
            McpErrorKind::Internal => "Internal error",
        }
    }
}

/// A structured MCP error.
#[derive(Debug, Clone)]
pub struct McpError {
    pub kind: McpErrorKind,
    pub message: String,
    pub data: Option<Value>,
}

impl McpError {
    pub fn new(kind: McpErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(kind: McpErrorKind, message: impl Into<String>, data: Value) -> Self {
        Self {
            kind,
            message: message.into(),
            data: Some(data),
        }
    }

    pub fn not_initialized() -> Self {
        Self::new(
            McpErrorKind::NotInitialized,
            "MCP server must be initialized first",
        )
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(McpErrorKind::InvalidParams, message)
    }

    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::new(
            McpErrorKind::ToolNotFound,
            format!("Tool not found: {}", name.into()),
        )
    }

    pub fn resource_not_found(uri: impl Into<String>) -> Self {
        Self::new(
            McpErrorKind::ResourceNotFound,
            format!("Resource not found: {}", uri.into()),
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(McpErrorKind::Internal, message)
    }

    /// Convert into a JSON-RPC error for transport-level response encoding.
    pub fn into_jsonrpc_error(self) -> JsonRpcError {
        JsonRpcError::from(self)
    }
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.kind.code(), self.message)?;
        if let Some(data) = &self.data {
            write!(f, " ({})", data)?;
        }
        Ok(())
    }
}

impl std::error::Error for McpError {}

impl From<McpError> for JsonRpcError {
    fn from(err: McpError) -> Self {
        let code_val = err.kind.code();
        let error_code = ErrorCode::from(code_val);
        let mut rpc = JsonRpcError::new(error_code, err.message);
        if let Some(data) = err.data {
            rpc = rpc.with_data_mut(data);
        }
        rpc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_standard_jsonrpc() {
        assert_eq!(McpErrorKind::NotInitialized.code(), -32600);
        assert_eq!(McpErrorKind::InvalidParams.code(), -32602);
        assert_eq!(McpErrorKind::Internal.code(), -32603);
    }

    #[test]
    fn mcp_error_converts_to_jsonrpc_error() {
        let err = McpError::invalid_params("missing 'path'");
        let rpc: JsonRpcError = err.into();
        assert_eq!(rpc.code, -32602);
        assert!(rpc.message.contains("missing 'path'"));
    }

    #[test]
    fn mcp_error_with_data_preserves_data() {
        let err = McpError::with_data(
            McpErrorKind::Internal,
            "boom",
            serde_json::json!({ "path": "/etc/shadow" }),
        );
        let rpc: JsonRpcError = err.into();
        assert_eq!(rpc.code, -32603);
        assert!(rpc.data.is_some());
    }

    #[test]
    fn tool_not_found_error() {
        let err = McpError::tool_not_found("search_note");
        let rpc: JsonRpcError = err.into();
        assert_eq!(rpc.code, -32603);
        assert!(rpc.message.contains("search_note"));
    }

    #[test]
    fn display_includes_code_and_message() {
        let err = McpError::invalid_params("bad arg");
        let s = format!("{}", err);
        assert!(s.contains("-32602"));
        assert!(s.contains("bad arg"));
    }
}
