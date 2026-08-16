//! # ACP Client Handler — Agent→Client Request Dispatch
//!
//! When the ACP agent sends a request to Nabu (the client) for a capability
//! Nabu advertised, the client must dispatch it through a handler trait that
//! is implemented by the application layer (Phase 3c / Phase 3d).
//!
//! The handler is the **boundary** between the ACP protocol layer and
//! Nabu's application logic. The protocol layer knows *that* a request
//! arrived; the handler knows *what to do* about it.
//!
//! ## Agent→client methods
//!
//! | Method                      | Handler trait method        |
//! |-----------------------------|-----------------------------|
//! | `fs/read_text_file`         | `read_text_file`            |
//! | `fs/write_text_file`        | `write_text_file`           |
//! | `session/request_permission`| `request_permission`        |
//! | `terminal/create`           | `terminal_create` (future) |
//! | `terminal/output`           | `terminal_output` (future) |
//! | `terminal/release`          | `terminal_release` (future)|
//! | `terminal/wait_for_exit`    | `terminal_wait_for_exit` (future)|
//! | `terminal/kill`             | `terminal_kill` (future)    |
//! | `elicitation/create`        | `elicitation_create` (future)|
//!
//! Terminal and elicitation methods are included as default trait
//! methods that return an `UnsupportedOperation` error, so the trait is
//! forward-compatible. Individual method support is determined by what Nabu
//! advertises in its `ClientCapabilities` during `initialize`.

use crate::acp::error::{AcpError, ErrorKind};
use crate::acp::types::{
    PermissionOutcome, ReadTextFileRequest, ReadTextFileResponse, RequestPermissionRequest,
    WriteTextFileRequest, WriteTextFileResponse,
};

/// Trait for handling agent→client requests dispatched by the ACP client.
///
/// The implementor decides what action to take for each request. The ACP
/// client calls these methods when it receives an agent-originated request
/// and automatically wraps the result (or error) back into a JSON-RPC
/// response sent to the agent.
///
/// All methods except the three core ones (`read_text_file`,
/// `write_text_file`, `request_permission`) have default implementations
/// that return `UnsupportedOperation`, so the trait is forward-compatible
/// with future ACP methods.
///
/// This trait uses `#[async_trait]` to ensure `dyn`-compatibility, so it
/// can be stored as `Arc<dyn AcpClientHandler>`.
#[async_trait::async_trait]
pub trait AcpClientHandler: Send + Sync {
    /// Handle `fs/read_text_file` — return the contents of the requested file.
    async fn read_text_file(
        &self,
        request: &ReadTextFileRequest,
    ) -> Result<ReadTextFileResponse, AcpError>;

    /// Handle `fs/write_text_file` — write content to the requested path.
    async fn write_text_file(
        &self,
        request: &WriteTextFileRequest,
    ) -> Result<WriteTextFileResponse, AcpError>;

    /// Handle `session/request_permission` — present options to the user and
    /// return the selected outcome (or cancellation).
    async fn request_permission(
        &self,
        request: &RequestPermissionRequest,
    ) -> Result<PermissionOutcome, AcpError>;

    // -----------------------------------------------------------------------
    // Terminal methods — default implementations return UnsupportedOperation.
    // Available when the client advertises `terminal: true`.
    // -----------------------------------------------------------------------

    /// Handle `terminal/create` — create a new terminal session.
    ///
    /// Default: returns an error. Override when terminal capability is
    /// supported.
    #[allow(clippy::type_complexity)]
    async fn terminal_create(
        &self,
        _session_id: &str,
        _terminal_id: &str,
        _cwd: Option<&str>,
    ) -> Result<Option<serde_json::Value>, AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "terminal/create is not supported by this client",
        ))
    }

    /// Handle `terminal/output` — receive terminal output.
    async fn terminal_output(
        &self,
        _session_id: &str,
        _terminal_id: &str,
        _output: &[u8],
    ) -> Result<(), AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "terminal/output is not supported by this client",
        ))
    }

    /// Handle `terminal/release` — release a terminal.
    async fn terminal_release(
        &self,
        _session_id: &str,
        _terminal_id: &str,
    ) -> Result<(), AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "terminal/release is not supported by this client",
        ))
    }

    /// Handle `terminal/wait_for_exit` — wait for a terminal to exit.
    async fn terminal_wait_for_exit(
        &self,
        _session_id: &str,
        _terminal_id: &str,
    ) -> Result<Option<i32>, AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "terminal/wait_for_exit is not supported by this client",
        ))
    }

    /// Handle `terminal/kill` — kill a terminal.
    async fn terminal_kill(&self, _session_id: &str, _terminal_id: &str) -> Result<(), AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "terminal/kill is not supported by this client",
        ))
    }

    // -----------------------------------------------------------------------
    // Elicitation methods — default implementations return UnsupportedOperation.
    // Available when the client advertises `elicitation` capability.
    // -----------------------------------------------------------------------

    /// Handle `elicitation/create` — prompt the user for input.
    ///
    /// Returns a map of field-id → value.
    #[allow(clippy::type_complexity)]
    async fn elicitation_create(
        &self,
        _session_id: &str,
        _request_id: Option<&str>,
        _title: &str,
        _message: Option<&str>,
        _fields: &[serde_json::Value],
    ) -> Result<Option<serde_json::Value>, AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "elicitation/create is not supported by this client",
        ))
    }
}

/// A no-op handler that returns errors for all agent→client requests.
///
/// Useful as a default when the client has not implemented any capability
/// handlers, or for testing.
pub struct NoopClientHandler;

#[async_trait::async_trait]
impl AcpClientHandler for NoopClientHandler {
    async fn read_text_file(
        &self,
        _request: &ReadTextFileRequest,
    ) -> Result<ReadTextFileResponse, AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "noop handler does not support read_text_file",
        ))
    }

    async fn write_text_file(
        &self,
        _request: &WriteTextFileRequest,
    ) -> Result<WriteTextFileResponse, AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "noop handler does not support write_text_file",
        ))
    }

    async fn request_permission(
        &self,
        _request: &RequestPermissionRequest,
    ) -> Result<PermissionOutcome, AcpError> {
        Err(AcpError::new(
            ErrorKind::UnsupportedOperation,
            "noop handler does not support request_permission",
        ))
    }
}

/// Dispatch an agent→client request to the appropriate handler method.
///
/// Returns the result as a `serde_json::Value` ready to be wrapped in a
/// JSON-RPC success response, or an `AcpError` to be wrapped as an
/// error response.
pub async fn dispatch_agent_request<H: AcpClientHandler + ?Sized>(
    handler: &H,
    method: &str,
    params: &Option<serde_json::Value>,
) -> Result<serde_json::Value, AcpError> {
    use crate::acp::types::decode_params;

    match method {
        crate::acp::types::METHOD_READ_TEXT_FILE => {
            let req: ReadTextFileRequest = decode_params::<ReadTextFileRequest>(params.clone())?;
            let resp = handler.read_text_file(&req).await?;
            Ok(serde_json::to_value(resp)?)
        }
        crate::acp::types::METHOD_WRITE_TEXT_FILE => {
            let req: WriteTextFileRequest = decode_params::<WriteTextFileRequest>(params.clone())?;
            let resp = handler.write_text_file(&req).await?;
            Ok(serde_json::to_value(resp)?)
        }
        crate::acp::types::METHOD_REQUEST_PERMISSION => {
            let req: RequestPermissionRequest =
                decode_params::<RequestPermissionRequest>(params.clone())?;
            let resp = handler.request_permission(&req).await?;
            Ok(serde_json::to_value(resp)?)
        }
        // Terminal methods
        "terminal/create" => {
            let req_val = params.clone().unwrap_or(serde_json::Value::Null);
            let session_id = req_val
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let terminal_id = req_val
                .get("terminalId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cwd = req_val.get("cwd").and_then(|v| v.as_str());
            let result = handler
                .terminal_create(session_id, terminal_id, cwd)
                .await?;
            Ok(result.unwrap_or(serde_json::Value::Null))
        }
        "terminal/output" => {
            let req_val = params.clone().unwrap_or(serde_json::Value::Null);
            let session_id = req_val
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let terminal_id = req_val
                .get("terminalId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let output = req_val.get("output").and_then(|v| v.as_str()).unwrap_or("");
            handler
                .terminal_output(session_id, terminal_id, output.as_bytes())
                .await?;
            Ok(serde_json::Value::Null)
        }
        "terminal/release" => {
            let req_val = params.clone().unwrap_or(serde_json::Value::Null);
            let session_id = req_val
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let terminal_id = req_val
                .get("terminalId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            handler.terminal_release(session_id, terminal_id).await?;
            Ok(serde_json::Value::Null)
        }
        "terminal/wait_for_exit" => {
            let req_val = params.clone().unwrap_or(serde_json::Value::Null);
            let session_id = req_val
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let terminal_id = req_val
                .get("terminalId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let result = handler
                .terminal_wait_for_exit(session_id, terminal_id)
                .await?;
            match result {
                Some(code) => Ok(serde_json::json!({ "exitCode": code })),
                None => Ok(serde_json::Value::Null),
            }
        }
        "terminal/kill" => {
            let req_val = params.clone().unwrap_or(serde_json::Value::Null);
            let session_id = req_val
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let terminal_id = req_val
                .get("terminalId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            handler.terminal_kill(session_id, terminal_id).await?;
            Ok(serde_json::Value::Null)
        }
        // Elicitation
        "elicitation/create" => {
            let req_val = params.clone().unwrap_or(serde_json::Value::Null);
            let session_id = req_val
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let request_id = req_val.get("requestId").and_then(|v| v.as_str());
            let title = req_val.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let message = req_val.get("message").and_then(|v| v.as_str());
            let fields = req_val
                .get("fields")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let result = handler
                .elicitation_create(session_id, request_id, title, message, &fields)
                .await?;
            Ok(result.unwrap_or(serde_json::Value::Null))
        }
        _ => Err(AcpError::unsupported_operation(method)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::types::{
        METHOD_READ_TEXT_FILE, METHOD_REQUEST_PERMISSION, METHOD_WRITE_TEXT_FILE,
    };
    use std::sync::Arc;
    /// A handler that records calls for inspection in tests.
    struct RecordingHandler {
        read_file_calls: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    impl RecordingHandler {
        fn new() -> Self {
            Self {
                read_file_calls: Arc::new(tokio::sync::Mutex::new(vec![])),
            }
        }
    }

    #[async_trait::async_trait]
    impl AcpClientHandler for RecordingHandler {
        async fn read_text_file(
            &self,
            request: &ReadTextFileRequest,
        ) -> Result<ReadTextFileResponse, AcpError> {
            let calls = self.read_file_calls.clone();
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

    #[tokio::test]
    async fn dispatch_read_text_file() {
        let handler = RecordingHandler::new();
        let params = serde_json::json!({"path": "/test/file.txt", "sessionId": "s1"});

        let result = dispatch_agent_request(&handler, METHOD_READ_TEXT_FILE, &Some(params))
            .await
            .unwrap();

        assert_eq!(result["content"], "contents of /test/file.txt");

        let calls = handler.read_file_calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "/test/file.txt");
    }

    #[tokio::test]
    async fn dispatch_write_text_file() {
        let handler = RecordingHandler::new();
        let params = serde_json::json!({"path": "/out.txt", "content": "hello", "sessionId": "s1"});

        let result = dispatch_agent_request(&handler, METHOD_WRITE_TEXT_FILE, &Some(params))
            .await
            .unwrap();

        assert!(result.is_null() || result.is_object());
    }

    #[tokio::test]
    async fn dispatch_request_permission() {
        let handler = RecordingHandler::new();
        let params = serde_json::json!({
            "sessionId": "s1",
            "toolCall": {"toolCallId": "tc1", "title": "test"},
            "options": [{"optionId": "allow_once", "name": "Allow once", "kind": "allow_once"}]
        });

        let result = dispatch_agent_request(&handler, METHOD_REQUEST_PERMISSION, &Some(params))
            .await
            .unwrap();

        assert!(result.get("outcome").is_some());
    }

    #[tokio::test]
    async fn dispatch_unknown_method_errors() {
        let handler = RecordingHandler::new();
        let result = dispatch_agent_request(&handler, "unknown/method", &None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::UnsupportedOperation);
    }

    #[tokio::test]
    async fn dispatch_terminal_create_not_supported() {
        let handler = RecordingHandler::new();
        let params = serde_json::json!({"sessionId": "s1", "terminalId": "t1", "cwd": "/tmp"});

        let result = dispatch_agent_request(&handler, "terminal/create", &Some(params)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::UnsupportedOperation);
    }

    #[tokio::test]
    async fn noop_handler_always_errors() {
        let handler = NoopClientHandler;
        let params = serde_json::json!({"path": "/test", "sessionId": "s1"});

        let result = dispatch_agent_request(&handler, METHOD_READ_TEXT_FILE, &Some(params)).await;
        assert!(result.is_err());
    }
}
