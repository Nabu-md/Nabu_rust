//! Unix socket server for native messaging host communication.
//!
//! This module provides a socket server that listens for connections from
//! the native messaging host binary. It receives capture messages, validates
//! them, converts them to canonical [`CaptureRequest`] values, and dispatches
//! them through the canonical [`CaptureEngine::ingest`] flow.
//!
//! The wire protocol matches the shared `native_messaging::Message` type used
//! by the browser extension host — the JSON uses camelCase field names
//! (`captureType`, `requestId`) as defined by `#[serde(rename_all = "camelCase")]`.
//! Nothing custom, no side paths.
//!
//! ## Lifecycle & Security
//!
//! The socket server is built on [`SocketManager`] from `nabu-core`, which
//! provides:
//!
//! - Secure permissions (`0600`) on socket files.
//! - Stale socket cleanup before binding.
//! - Graceful shutdown via [`SocketServerHandle::shutdown`].
//! - Lifecycle integration (`Created → Initialized → Running → Shutdown`).
//!
//! See the [`SocketManager`](nabu_core::ipc_socket::SocketManager) documentation
//! for full lifecycle and permission model details.

use std::path::Path;
use std::sync::Arc;

use nabu_core::capture::{CaptureData, CaptureEngine, CaptureRequest};
use nabu_core::ipc_socket::{SocketConfig, SocketManager};
use nabu_core::registry::lifecycle::Lifecycle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::native_messaging::Message;

/// The filesystem path for the native messaging socket.
///
/// This constant is shared between the socket server (this module) and the
/// native messaging host binary (`bin/native_messaging_host.rs`) to ensure
/// both agree on the IPC endpoint location.
///
/// The path lives under the OS temp directory and is independent of the
/// repository checkout or Cargo target directories, so it resolves correctly
/// for both development builds and installed bundles.
pub const SOCKET_PATH: &str = "/tmp/nabu-native-messaging.sock";

/// Maximum message size: 10 MB.
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Socket Server State
// ---------------------------------------------------------------------------

/// Shared state for the socket server.
pub struct SocketServerState {
    pub engine: Arc<CaptureEngine>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during socket operations.
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Capture error: {0}")]
    CaptureError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Socket lifecycle error: {0}")]
    LifecycleError(String),
}

/// Result type for socket operations.
pub type SocketResult<T> = Result<T, SocketError>;

// ---------------------------------------------------------------------------
// Message validation & conversion
// ---------------------------------------------------------------------------

/// Validates a native messaging capture message.
///
/// Ensures the message is a `capture` command with a known capture type and a
/// non-empty payload within size limits.
///
/// This is the **second** validation point in the pipeline.  The native
/// messaging host (`native_messaging_host.rs`) validates on the stdin side
/// before forwarding over the socket; this function re-validates on the socket
/// server side so that the Tauri application never trusts data that came from
/// an unauthenticated subprocess.
fn validate_capture_message(message: &Message) -> SocketResult<()> {
    if message.command != "capture" {
        return Err(SocketError::ValidationError(format!(
            "Unknown command: {}",
            message.command
        )));
    }

    let capture_type = message
        .capture_type
        .as_deref()
        .ok_or_else(|| SocketError::ValidationError("Capture type is required".to_string()))?;

    let valid_sources = [
        "clipboard",
        "screenshot",
        "screen_capture",
        "browser",
        "watch_folder",
        "reader_mode",
        "safari_reader",
        "youtube",
        "github",
        "email",
        "bookmark",
        "note",
        "document",
        "article",
    ];
    if !valid_sources.contains(&capture_type) {
        return Err(SocketError::ValidationError(format!(
            "Invalid capture type: {}. Valid sources: {:?}",
            capture_type, valid_sources
        )));
    }

    let payload = message
        .payload
        .as_ref()
        .ok_or_else(|| SocketError::ValidationError("Payload is required".to_string()))?;

    let payload_size = serde_json::to_vec(payload).map_err(SocketError::SerializationError)?;
    if payload_size.len() > MAX_MESSAGE_SIZE {
        return Err(SocketError::ValidationError(format!(
            "Payload size {} exceeds maximum {} bytes",
            payload_size.len(),
            MAX_MESSAGE_SIZE
        )));
    }

    Ok(())
}

/// Converts a validated capture `Message` into a canonical [`CaptureRequest`].
///
/// The payload is a free-form JSON object; this extracts the conventional
/// fields (`title`, `url`, `text`/`content`) into the canonical model.
fn message_to_capture_request(message: &Message) -> CaptureRequest {
    let payload = message
        .payload
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));

    let title = payload
        .get("title")
        .and_then(|v| v.as_str())
        .map(String::from);
    let url = payload
        .get("url")
        .and_then(|v| v.as_str())
        .map(String::from);
    let text = payload
        .get("text")
        .or_else(|| payload.get("content"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let html = payload
        .get("html")
        .and_then(|v| v.as_str())
        .map(String::from);

    let capture_type = message.capture_type.as_deref().unwrap_or("note");

    let data = if capture_type == "screen_capture" {
        let selection = payload
            .get("selection")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                if arr.len() == 4 {
                    Some((
                        arr[0].as_i64()? as i32,
                        arr[1].as_i64()? as i32,
                        arr[2].as_i64()? as u32,
                        arr[3].as_i64()? as u32,
                    ))
                } else {
                    None
                }
            });
        CaptureData::ScreenCapture { selection }
    } else if capture_type == "reader_mode" || capture_type == "safari_reader" {
        if let Some(html) = html {
            CaptureData::Text(html)
        } else if let Some(text) = text {
            CaptureData::Text(text)
        } else if let Some(url) = &url {
            CaptureData::Uri(url.clone())
        } else {
            CaptureData::Text(payload.to_string())
        }
    } else if let Some(url) = url.clone() {
        if matches!(
            capture_type,
            "bookmark" | "browser" | "youtube" | "github" | "email"
        ) {
            CaptureData::Uri(url)
        } else {
            match text {
                Some(text) => CaptureData::Text(text),
                None => CaptureData::Uri(url),
            }
        }
    } else if let Some(text) = text {
        CaptureData::Text(text)
    } else if let Some(html) = html {
        CaptureData::Text(html)
    } else {
        CaptureData::Text(payload.to_string())
    };

    let mut request = CaptureRequest::new(data);
    if let Some(title) = title {
        request = request.with_title(title);
    }
    if let Some(url) = url {
        request = request.with_url(url);
    }
    request
}

// ---------------------------------------------------------------------------
// Connection handling
// ---------------------------------------------------------------------------

/// Handles a single client connection.
///
/// Reads length-prefixed JSON messages, validates them, converts to
/// `CaptureRequest`, dispatches through `CaptureEngine::ingest`, and writes
/// length-prefixed JSON responses.
async fn handle_connection(mut stream: UnixStream, engine: Arc<CaptureEngine>) {
    loop {
        let mut length_bytes = [0u8; 4];
        match stream.read_exact(&mut length_bytes).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                tracing::error!("Read length error: {}", e);
                break;
            }
        }
        let length = u32::from_be_bytes(length_bytes) as usize;

        if length > MAX_MESSAGE_SIZE {
            tracing::warn!("Message size {} exceeds maximum allowed size", length);
            break;
        }

        let mut buffer = vec![0u8; length];
        if let Err(e) = stream.read_exact(&mut buffer).await {
            tracing::error!("Read body error: {}", e);
            break;
        }

        let message: Message = match serde_json::from_slice(&buffer) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::error!("Deserialization error: {}", e);
                break;
            }
        };

        if let Err(e) = validate_capture_message(&message) {
            tracing::warn!("Invalid capture request: {}", e);
            let error_response = Message {
                request_id: message.request_id,
                command: "capture".to_string(),
                capture_type: message.capture_type,
                payload: None,
                success: Some(false),
                error: Some(e.to_string()),
                result: None,
            };
            if let Err(e) = write_message(&mut stream, &error_response).await {
                tracing::error!("Failed to write error response: {}", e);
            }
            continue;
        }

        let request = message_to_capture_request(&message);

        let response = match engine.ingest(request).await {
            Ok(Some(object_id)) => Message {
                request_id: message.request_id,
                command: "capture".to_string(),
                capture_type: message.capture_type,
                payload: None,
                success: Some(true),
                error: None,
                result: Some(serde_json::json!({ "object_id": object_id })),
            },
            Ok(None) => Message {
                request_id: message.request_id,
                command: "capture".to_string(),
                capture_type: message.capture_type,
                payload: None,
                success: Some(false),
                error: Some("No capture handler accepted the request".to_string()),
                result: None,
            },
            Err(e) => Message {
                request_id: message.request_id,
                command: "capture".to_string(),
                capture_type: message.capture_type,
                payload: None,
                success: Some(false),
                error: Some(e.to_string()),
                result: None,
            },
        };

        if let Err(e) = write_message(&mut stream, &response).await {
            tracing::error!("Failed to write response: {}", e);
            break;
        }
    }
}

/// Writes a length-prefixed `Message` to the stream.
async fn write_message(stream: &mut UnixStream, message: &Message) -> Result<(), SocketError> {
    let json = serde_json::to_vec(message).map_err(SocketError::SerializationError)?;
    let length = json.len() as u32;
    let length_bytes = length.to_be_bytes();
    stream.write_all(&length_bytes).await?;
    stream.write_all(&json).await?;
    stream.flush().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

/// Starts the Unix socket server for native messaging communication.
///
/// Uses [`SocketManager`] from `nabu-core` for lifecycle management, secure
/// permissions (`0600`), and stale socket cleanup. The server listens for
/// connections from the native messaging host binary and processes capture
/// requests through the canonical `CaptureEngine::ingest` flow.
///
/// Returns a [`SocketServerHandle`] that can be used to signal shutdown.
/// The handle should be stored as Tauri managed state and shut down on
/// `Exit` to ensure graceful shutdown.
pub fn start_socket_server(state: Arc<SocketServerState>) -> Result<SocketServerHandle, SocketError> {
    let engine = state.engine.clone();

    let config = SocketConfig::new(SOCKET_PATH.to_string())
        .with_handler(move |stream| {
            let engine = engine.clone();
            async move {
                handle_connection(stream, engine).await;
                true
            }
        });

    let manager = SocketManager::new(config);

    manager
        .initialize()
        .map_err(|e| SocketError::LifecycleError(e.to_string()))?;

    manager
        .start()
        .map_err(|e| SocketError::LifecycleError(e.to_string()))?;

    Ok(SocketServerHandle {
        manager: Arc::new(manager),
    })
}

// ---------------------------------------------------------------------------
// Socket Server Handle
// ---------------------------------------------------------------------------

/// Handle for controlling the socket server.
///
/// Wraps an [`Arc<SocketManager>`] so that the manager stays alive for the
/// lifetime of the handle. This ensures the accept loop task is not orphaned
/// and can be gracefully shut down on application exit.
pub struct SocketServerHandle {
    manager: Arc<SocketManager>,
}

impl SocketServerHandle {
    /// Signals the socket server to shut down gracefully.
    ///
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn shutdown(&self) {
        let _ = self.manager.shutdown();
    }

    /// Returns the path to the socket file.
    pub fn socket_path(&self) -> &Path {
        self.manager.socket_path()
    }

    /// Returns `true` if the socket is currently running.
    pub fn is_running(&self) -> bool {
        self.manager.is_running()
    }

    /// Returns `true` if the socket has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.manager.is_shutdown()
    }
}

impl Clone for SocketServerHandle {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
        }
    }
}

/// State wrapper for the socket handle, stored as Tauri managed state.
///
/// Uses `Option` so that the setup closure can store `None` if the socket
/// server failed to start. The Exit handler checks for `Some` before calling
/// `shutdown()`.
pub struct SocketServerHandleState(pub Option<SocketServerHandle>);

impl SocketServerHandleState {
    /// Returns the socket handle if the server started successfully.
    pub fn handle(&self) -> Option<&SocketServerHandle> {
        self.0.as_ref()
    }

    /// Shuts down the socket server if it is running.
    /// Safe to call when the server never started (no-op).
    pub fn shutdown(&self) {
        if let Some(handle) = &self.0 {
            handle.shutdown();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path_constant() {
        assert_eq!(SOCKET_PATH, "/tmp/nabu-native-messaging.sock");
    }

    // ---- Test 1: Canonical payload (camelCase wire format) ----

    /// Verify that a browser-style payload using camelCase field names
    /// (`captureType`, `requestId`) deserializes correctly into the Rust
    /// `Message` and then routes through validation + CaptureRequest conversion.
    #[test]
    fn test_camel_case_payload_routes_to_capture_request() {
        let json = r#"{
            "requestId": 42,
            "command": "capture",
            "captureType": "bookmark",
            "payload": {
                "url": "https://example.com",
                "title": "Example Domain"
            }
        }"#;

        let message: Message = serde_json::from_str(json)
            .expect("camelCase payload must deserialize");
        assert_eq!(message.request_id, Some(42));
        assert_eq!(message.capture_type.as_deref(), Some("bookmark"));

        // Validate through the socket layer
        validate_capture_message(&message)
            .expect("valid bookmark capture should pass validation");

        // Convert to CaptureRequest
        let request = message_to_capture_request(&message);
        assert!(
            matches!(request.data, CaptureData::Uri(ref u) if u == "https://example.com"),
            "expected Uri capture data, got {:?}",
            request.data
        );
        assert_eq!(request.title.as_deref(), Some("Example Domain"));
        assert_eq!(request.source_url.as_deref(), Some("https://example.com"));
    }

    /// Same flow but for a `note` capture type — verifies the text path.
    #[test]
    fn test_note_capture_routes_to_text_capture_request() {
        let json = r#"{
            "requestId": 1,
            "command": "capture",
            "captureType": "note",
            "payload": {
                "text": "Selected text from the page",
                "title": "My Note",
                "url": "https://example.com/article"
            }
        }"#;

        let message: Message = serde_json::from_str(json)
            .expect("camelCase note payload must deserialize");

        validate_capture_message(&message)
            .expect("valid note capture should pass validation");

        let request = message_to_capture_request(&message);
        assert!(
            matches!(request.data, CaptureData::Text(ref t) if t == "Selected text from the page"),
            "expected Text capture data, got {:?}",
            request.data
        );
    }

    /// Verify that snake_case wire format (the old, mismatched format) is
    /// NOT silently accepted — the field becomes `None` and validation
    /// rejects with an explicit error.
    #[test]
    fn test_snake_case_capture_type_is_rejected() {
        let json = r#"{
            "command": "capture",
            "capture_type": "bookmark",
            "payload": {}
        }"#;

        let message: Message = serde_json::from_str(json)
            .expect("JSON is syntactically valid");
        assert!(
            message.capture_type.is_none(),
            "snake_case field should not map to capture_type"
        );

        let err = validate_capture_message(&message).unwrap_err();
        assert!(
            err.to_string().contains("Capture type is required"),
            "expected explicit validation error, got: {}", err
        );
    }

    // ---- Test 2: Invalid payloads are rejected ----

    #[test]
    fn test_reject_missing_capture_type_field() {
        let json = r#"{
            "command": "capture",
            "payload": {}
        }"#;

        let message: Message = serde_json::from_str(json)
            .expect("JSON is syntactically valid");
        let err = validate_capture_message(&message).unwrap_err();
        assert!(err.to_string().contains("Capture type is required"));
    }

    #[test]
    fn test_reject_invalid_capture_type_value() {
        let json = r#"{
            "command": "capture",
            "captureType": "bogus",
            "payload": {}
        }"#;

        let message: Message = serde_json::from_str(json)
            .expect("JSON is syntactically valid");
        let err = validate_capture_message(&message).unwrap_err();
        assert!(err.to_string().contains("Invalid capture type"));
    }

    #[test]
    fn test_reject_unknown_command() {
        let json = r#"{
            "command": "delete",
            "captureType": "bookmark",
            "payload": {}
        }"#;

        let message: Message = serde_json::from_str(json)
            .expect("JSON is syntactically valid");
        let err = validate_capture_message(&message).unwrap_err();
        assert!(err.to_string().contains("Unknown command"));
    }

    #[test]
    fn test_reject_missing_payload() {
        let json = r#"{
            "command": "capture",
            "captureType": "bookmark"
        }"#;

        let message: Message = serde_json::from_str(json)
            .expect("JSON is syntactically valid");
        let err = validate_capture_message(&message).unwrap_err();
        assert!(err.to_string().contains("Payload is required"));
    }

    #[test]
    fn test_reject_malformed_json() {
        let bad_json = r#"{"command":"capture","captureType":"bookmark""#;
        let result: Result<Message, _> = serde_json::from_str(bad_json);
        assert!(result.is_err(), "malformed JSON must fail to deserialize");
    }

    // ---- Test 3: Native host routing (message → CaptureRequest) ----

    #[test]
    fn test_message_to_capture_request_bookmark() {
        let bookmark = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: Some("bookmark".to_string()),
            payload: Some(serde_json::json!({ "url": "https://example.com", "title": "Example" })),
            success: None,
            error: None,
            result: None,
        };
        let request = message_to_capture_request(&bookmark);
        assert!(matches!(request.data, CaptureData::Uri(ref u) if u == "https://example.com"));
        assert_eq!(request.title.as_deref(), Some("Example"));
    }

    #[test]
    fn test_message_to_capture_request_note() {
        let note = Message {
            request_id: Some(2),
            command: "capture".to_string(),
            capture_type: Some("note".to_string()),
            payload: Some(serde_json::json!({ "text": "Hello world", "title": "Note" })),
            success: None,
            error: None,
            result: None,
        };
        let request = message_to_capture_request(&note);
        assert!(matches!(request.data, CaptureData::Text(ref t) if t == "Hello world"));
    }

    /// Verify that a reader_mode capture with HTML falls through to
    /// `message_to_capture_request` as `CaptureData::Text`.
    #[test]
    fn test_message_to_capture_request_reader_mode() {
        let reader = Message {
            request_id: Some(3),
            command: "capture".to_string(),
            capture_type: Some("reader_mode".to_string()),
            payload: Some(serde_json::json!({
                "html": "<article>Readable content</article>",
                "title": "Article Title",
                "url": "https://example.com/article"
            })),
            success: None,
            error: None,
            result: None,
        };
        let request = message_to_capture_request(&reader);
        assert!(
            matches!(request.data, CaptureData::Text(ref t) if t == "<article>Readable content</article>"),
            "expected Text capture data for reader_mode, got {:?}",
            request.data
        );
        assert_eq!(request.title.as_deref(), Some("Article Title"));
    }

    #[test]
    fn test_message_to_capture_request_falls_back_to_note() {
        // When no url/text/html in payload, falls back to full payload string.
        let msg = Message {
            request_id: Some(4),
            command: "capture".to_string(),
            capture_type: Some("note".to_string()),
            payload: Some(serde_json::json!({ "unexpected": "data" })),
            success: None,
            error: None,
            result: None,
        };
        let request = message_to_capture_request(&msg);
        assert!(
            matches!(request.data, CaptureData::Text(ref t) if t.contains("unexpected")),
            "expected fallback Text capture data, got {:?}",
            request.data
        );
    }

    // ---- Test 4: Existing socket/framing behavior (regression) ----

    #[test]
    fn test_validate_capture_message() {
        let valid = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: Some("bookmark".to_string()),
            payload: Some(serde_json::json!({ "url": "https://example.com", "title": "Example" })),
            success: None,
            error: None,
            result: None,
        };
        assert!(validate_capture_message(&valid).is_ok());

        let bad_command = Message {
            request_id: Some(1),
            command: "delete".to_string(),
            capture_type: Some("bookmark".to_string()),
            payload: Some(serde_json::json!({})),
            success: None,
            error: None,
            result: None,
        };
        assert!(validate_capture_message(&bad_command).is_err());

        let no_payload = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: Some("bookmark".to_string()),
            payload: None,
            success: None,
            error: None,
            result: None,
        };
        assert!(validate_capture_message(&no_payload).is_err());
    }

    #[test]
    fn test_socket_error_display() {
        let err = SocketError::ValidationError("test error".to_string());
        assert_eq!(err.to_string(), "Validation error: test error");
    }

    #[test]
    fn test_socket_handle_state_shutdown_is_noop_when_none() {
        let state = SocketServerHandleState(None);
        state.shutdown();
        assert!(state.handle().is_none());
    }
}
