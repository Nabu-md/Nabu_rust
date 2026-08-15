//! Native messaging protocol implementation for browser extension communication.
//!
//! This module provides:
//! - Message types for browser-to-app communication
//! - Validation of incoming messages
//! - Reading/writing length-prefixed JSON messages (standard native messaging format)
//!
//! ## Wire protocol
//!
//! The canonical wire format uses **camelCase** field names, matching the
//! JavaScript convention used by the browser extension:
//!
//! ```json
//! {
//!   "requestId": 1,
//!   "command": "capture",
//!   "captureType": "bookmark",
//!   "payload": { ... }
//! }
//! ```
//!
//! The `#[serde(rename_all = "camelCase")]` attribute on `Message` ensures
//! Rust's snake_case fields (`request_id`, `capture_type`) serialize and
//! deserialize to/from the camelCase keys the browser actually sends.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use thiserror::Error;

/// Maximum payload size (1MB) to prevent memory exhaustion attacks
const MAX_PAYLOAD_SIZE: usize = 1024 * 1024;

/// Allowed capture commands
const ALLOWED_COMMANDS: &[&str] = &["capture"];

/// Errors that can occur during native messaging operations
#[derive(Debug, Error)]
pub enum NativeMessagingError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Socket error: {0}")]
    SocketError(String),

    #[error("Unknown command: {0}")]
    UnknownCommand(String),

    #[error("Payload too large: {0} bytes (max: {1})")]
    PayloadTooLarge(usize, usize),
}

/// A message sent between the browser extension and the native host.
///
/// Field names use camelCase on the wire (matching the JavaScript extension)
/// via `#[serde(rename_all = "camelCase")]`.  This is the canonical wire
/// representation — the browser extension emits `captureType` and `requestId`,
/// and the Rust host deserializes them into `capture_type` / `request_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Unique request identifier for matching requests with responses
    pub request_id: Option<u64>,

    /// The command to execute (e.g., "capture")
    pub command: String,

    /// The capture type (e.g., "bookmark", "note", "document")
    ///
    /// On the wire this is `captureType` (camelCase).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_type: Option<String>,

    /// The message payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,

    /// Success flag for responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,

    /// Error message for failed responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Result data for successful responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

/// Native messaging host that reads from stdin and writes to stdout.
///
/// This is the thin bridge that sits between the browser extension (which
/// speaks the native messaging protocol over stdio) and the Nabu Tauri
/// application (which speaks a Unix-domain-socket protocol).  It reads
/// length-prefixed JSON messages from stdin, validates them, forwards them
/// over the Unix socket, reads the response, and writes a length-prefixed
/// JSON reply to stdout.
pub struct NativeMessagingHost {
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
}

impl NativeMessagingHost {
    /// Creates a new native messaging host
    pub fn new() -> Self {
        Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
        }
    }

    /// Reads a length-prefixed JSON message from stdin
    pub fn read_message(&mut self) -> Result<Message, NativeMessagingError> {
        // Read 4-byte length prefix (big-endian)
        let mut length_bytes = [0u8; 4];
        self.stdin.read_exact(&mut length_bytes)?;
        let length = u32::from_be_bytes(length_bytes) as usize;

        // Validate length
        if length > MAX_PAYLOAD_SIZE {
            return Err(NativeMessagingError::PayloadTooLarge(
                length,
                MAX_PAYLOAD_SIZE,
            ));
        }

        // Read message body
        let mut buffer = vec![0u8; length];
        self.stdin.read_exact(&mut buffer)?;

        // Deserialize JSON
        let message: Message = serde_json::from_slice(&buffer)
            .map_err(|e| NativeMessagingError::DeserializationError(e.to_string()))?;

        Ok(message)
    }

    /// Writes a length-prefixed JSON message to stdout
    pub fn write_message(&mut self, message: &Message) -> Result<(), NativeMessagingError> {
        let json = serde_json::to_vec(message)
            .map_err(|e| NativeMessagingError::SerializationError(e.to_string()))?;

        let length = json.len() as u32;
        let length_bytes = length.to_be_bytes();

        self.stdout.write_all(&length_bytes)?;
        self.stdout.write_all(&json)?;
        self.stdout.flush()?;

        Ok(())
    }

    /// Validates an incoming message
    pub fn validate_message(&self, message: &Message) -> Result<Message, NativeMessagingError> {
        // Check command is present
        if message.command.is_empty() {
            return Err(NativeMessagingError::ValidationError(
                "Command is required".to_string(),
            ));
        }

        // Check command is allowed
        if !ALLOWED_COMMANDS.contains(&message.command.as_str()) {
            return Err(NativeMessagingError::UnknownCommand(
                message.command.clone(),
            ));
        }

        // For capture commands, validate payload
        if message.command == "capture" {
            if let Some(ref payload) = message.payload {
                // Validate payload size
                let payload_str = serde_json::to_string(payload)
                    .map_err(|e| NativeMessagingError::SerializationError(e.to_string()))?;
                if payload_str.len() > MAX_PAYLOAD_SIZE {
                    return Err(NativeMessagingError::PayloadTooLarge(
                        payload_str.len(),
                        MAX_PAYLOAD_SIZE,
                    ));
                }

                // Validate capture type — must match the set accepted by the
                // socket server and the CaptureEngine handler names.
                if let Some(ref capture_type) = message.capture_type {
                    let valid_capture_types = [
                        "bookmark",
                        "note",
                        "document",
                        "reader_mode",
                        "safari_reader",
                        "clipboard",
                        "screenshot",
                        "screen_capture",
                        "file_drop",
                        "watch_folder",
                        "youtube",
                        "github",
                        "email",
                        "article",
                        "browser",
                    ];
                    if !valid_capture_types.contains(&capture_type.as_str()) {
                        return Err(NativeMessagingError::ValidationError(format!(
                            "Invalid capture type: {}",
                            capture_type
                        )));
                    }
                } else {
                    return Err(NativeMessagingError::ValidationError(
                        "Capture type is required for capture command".to_string(),
                    ));
                }
            } else {
                return Err(NativeMessagingError::ValidationError(
                    "Payload is required for capture command".to_string(),
                ));
            }
        }

        Ok(message.clone())
    }
}

impl Default for NativeMessagingHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: serialize a `Message` to its JSON wire representation.
    fn to_json(message: &Message) -> String {
        serde_json::to_string(message).expect("serialization should succeed")
    }

    /// Helper: deserialize a JSON string into a `Message`.
    fn from_json(json: &str) -> Result<Message, serde_json::Error> {
        serde_json::from_str(json)
    }

    // ---- Test 1: Canonical payload (camelCase wire format) ----

    /// Verify that a browser-style payload using camelCase field names
    /// (`captureType`, `requestId`) deserializes correctly into the Rust
    /// `Message`.
    #[test]
    fn test_canonical_payload_deserializes_with_camel_case() {
        // This is the exact payload shape the browser extension sends.
        let json = r#"{
            "requestId": 42,
            "command": "capture",
            "captureType": "bookmark",
            "payload": {
                "url": "https://example.com",
                "title": "Example Domain"
            }
        }"#;

        let message = from_json(json).expect("camelCase payload must deserialize");
        assert_eq!(message.request_id, Some(42));
        assert_eq!(message.command, "capture");
        assert_eq!(message.capture_type.as_deref(), Some("bookmark"));
        assert!(message.payload.is_some());
    }

    /// Verify that serialization produces camelCase JSON, not snake_case.
    #[test]
    fn test_serialize_uses_camel_case() {
        let message = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: Some("bookmark".to_string()),
            payload: Some(serde_json::json!({
                "url": "https://example.com",
                "title": "Example"
            })),
            success: None,
            error: None,
            result: None,
        };

        let json = to_json(&message);
        // Verify the wire format uses camelCase, not snake_case
        assert!(json.contains("\"requestId\""));
        assert!(json.contains("\"captureType\""));
        assert!(!json.contains("capture_type"));
        assert!(!json.contains("request_id"));
    }

    /// Verify that a full round-trip (serialize → deserialize) preserves
    /// the camelCase wire format and all fields.
    #[test]
    fn test_round_trip_preserves_capture_type() {
        let original = Message {
            request_id: Some(7),
            command: "capture".to_string(),
            capture_type: Some("note".to_string()),
            payload: Some(serde_json::json!({ "text": "hello" })),
            success: None,
            error: None,
            result: None,
        };

        let json = to_json(&original);
        let decoded = from_json(&json).expect("round-trip should succeed");
        assert_eq!(decoded.capture_type, original.capture_type);
        assert_eq!(decoded.request_id, original.request_id);
    }

    // ---- Test 2: Invalid payloads are rejected ----

    /// Verify that snake_case field names on the wire are NOT silently
    /// accepted — `capture_type` does not map to the `captureType` wire
    /// key, so the field becomes `None` and validation catches it.
    #[test]
    fn test_snake_case_capture_type_is_rejected() {
        let json = r#"{
            "command": "capture",
            "capture_type": "bookmark",
            "payload": {}
        }"#;

        let message = from_json(json).expect("JSON is syntactically valid");
        assert!(
            message.capture_type.is_none(),
            "snake_case field should not map to capture_type"
        );
        let host = NativeMessagingHost::new();
        let err = host.validate_message(&message).unwrap_err();
        assert!(
            err.to_string().contains("Capture type is required"),
            "expected validation error about missing capture type, got: {}", err
        );
    }

    /// Verify that a message missing `captureType` entirely is rejected.
    #[test]
    fn test_reject_missing_capture_type_field() {
        let json = r#"{
            "command": "capture",
            "payload": {}
        }"#;

        let message = from_json(json).expect("JSON is syntactically valid");
        assert!(message.capture_type.is_none(),
            "captureType field should be absent");
        let host = NativeMessagingHost::new();
        let err = host.validate_message(&message).unwrap_err();
        assert!(err.to_string().contains("Capture type is required"));
    }

    /// Verify that malformed JSON fails to deserialize.
    #[test]
    fn test_reject_malformed_json() {
        let bad_json = r#"{"command":"capture","captureType":"bookmark""#;
        let result = from_json(bad_json);
        assert!(result.is_err(), "malformed JSON must fail to deserialize");
    }

    /// Verify that an invalid capture type value is rejected.
    #[test]
    fn test_reject_invalid_capture_type_value() {
        let json = r#"{
            "command": "capture",
            "captureType": "bogus",
            "payload": {}
        }"#;

        let message = from_json(json).expect("JSON is syntactically valid");
        let host = NativeMessagingHost::new();
        let err = host.validate_message(&message).unwrap_err();
        assert!(err.to_string().contains("Invalid capture type"));
    }

    // ---- Existing validation tests (unchanged behavior) ----

    #[test]
    fn test_validate_capture_message() {
        let host = NativeMessagingHost::new();

        let valid_message = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: Some("bookmark".to_string()),
            payload: Some(serde_json::json!({
                "url": "https://example.com",
                "title": "Example"
            })),
            success: None,
            error: None,
            result: None,
        };

        assert!(host.validate_message(&valid_message).is_ok());
    }

    #[test]
    fn test_reject_unknown_command() {
        let host = NativeMessagingHost::new();

        let invalid_message = Message {
            request_id: Some(1),
            command: "delete".to_string(),
            capture_type: None,
            payload: None,
            success: None,
            error: None,
            result: None,
        };

        assert!(host.validate_message(&invalid_message).is_err());
    }

    #[test]
    fn test_reject_missing_capture_type() {
        let host = NativeMessagingHost::new();

        let invalid_message = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: None,
            payload: Some(serde_json::json!({})),
            success: None,
            error: None,
            result: None,
        };

        assert!(host.validate_message(&invalid_message).is_err());
    }

    #[test]
    fn test_reject_invalid_capture_type() {
        let host = NativeMessagingHost::new();

        let invalid_message = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: Some("invalid".to_string()),
            payload: Some(serde_json::json!({})),
            success: None,
            error: None,
            result: None,
        };

        assert!(host.validate_message(&invalid_message).is_err());
    }

    #[test]
    fn test_reject_empty_command() {
        let host = NativeMessagingHost::new();

        let invalid_message = Message {
            request_id: Some(1),
            command: String::new(),
            capture_type: None,
            payload: None,
            success: None,
            error: None,
            result: None,
        };

        assert!(host.validate_message(&invalid_message).is_err());
    }

    #[test]
    fn test_reject_missing_payload() {
        let host = NativeMessagingHost::new();

        let invalid_message = Message {
            request_id: Some(1),
            command: "capture".to_string(),
            capture_type: Some("bookmark".to_string()),
            payload: None,
            success: None,
            error: None,
            result: None,
        };

        assert!(host.validate_message(&invalid_message).is_err());
    }
}
