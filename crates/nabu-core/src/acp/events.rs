//! # ACP Event Types
//!
//! Classification and dispatch of inbound ACP messages received from the agent.
//!
//! The ACP protocol is **bidirectional**. After the client sends a request
//! (e.g. `session/prompt`), the agent can send three kinds of replies:
//!
//! 1. A **response** to a previously-sent request (matched by `id`)
//! 2. A **notification** — the agent pushes information without the client
//!    asking for it (e.g. `session/update`, `session/cancel`)
//! 3. A **request** from the agent to the client (e.g. `fs/read_text_file`,
//!    `session/request_permission`, `elicitation/create`)
//!
//! This module provides:
//! - [`InboundMessage`] — a classified inbound JSON-RPC message
//! - [`classify_message`] — parse a raw JSON line into `InboundMessage`
//! - [`AgentClientRequest`] — typed agent→client request dispatch enum

use crate::acp::types::*;
use crate::acp::{
    error::{AcpError, ErrorKind},
    types,
};
use serde::Deserialize;
use serde_json::Value;

/// A request from the ACP agent to the client.
///
/// These are JSON-RPC requests (with an `id`) that the agent sends
/// unsolicited during a prompt turn or session setup.
#[derive(Debug, Clone)]
pub enum AgentClientRequest {
    /// `fs/read_text_file` — the agent wants to read a file from the client.
    ReadTextFile {
        id: crate::rpc::RequestId,
        request: ReadTextFileRequest,
    },
    /// `fs/write_text_file` — the agent wants to write a file on the client.
    WriteTextFile {
        id: crate::rpc::RequestId,
        request: WriteTextFileRequest,
    },
    /// `session/request_permission` — the agent wants user permission for a
    /// tool call.
    RequestPermission {
        id: crate::rpc::RequestId,
        request: RequestPermissionRequest,
    },
}

impl AgentClientRequest {
    /// Returns the JSON-RPC request ID associated with this request.
    pub fn id(&self) -> &crate::rpc::RequestId {
        match self {
            AgentClientRequest::ReadTextFile { id, .. }
            | AgentClientRequest::WriteTextFile { id, .. }
            | AgentClientRequest::RequestPermission { id, .. } => id,
        }
    }

    /// Returns the ACP method name for this request.
    pub fn method(&self) -> &'static str {
        match self {
            AgentClientRequest::ReadTextFile { .. } => types::METHOD_READ_TEXT_FILE,
            AgentClientRequest::WriteTextFile { .. } => types::METHOD_WRITE_TEXT_FILE,
            AgentClientRequest::RequestPermission { .. } => types::METHOD_REQUEST_PERMISSION,
        }
    }
}

/// The result of classifying an inbound JSON-RPC message.
#[derive(Debug, Clone)]
pub enum InboundMessage {
    /// A JSON-RPC response to a request the client previously sent.
    /// Contains the raw JSON-RPC ID and the result/error payload.
    Response {
        id: crate::rpc::RequestId,
        result: Option<Value>,
        error: Option<crate::rpc::JsonRpcError>,
    },
    /// A notification from the agent (no response expected).
    /// This includes `session/update`, `session/cancel`, and protocol
    /// notifications like `$/cancel_request`.
    Notification {
        method: String,
        params: Option<Value>,
    },
    /// A request from the agent to the client.
    /// The client must respond with a JSON-RPC response.
    AgentRequest {
        id: crate::rpc::RequestId,
        method: String,
        params: Option<Value>,
        typed: Option<AgentClientRequest>,
    },
}

impl InboundMessage {
    /// Returns `true` if this message is a response to a prior client request.
    pub fn is_response(&self) -> bool {
        matches!(self, InboundMessage::Response { .. })
    }

    /// Returns `true` if this message is a notification (no ID, no response
    /// expected).
    pub fn is_notification(&self) -> bool {
        matches!(self, InboundMessage::Notification { .. })
    }

    /// Returns `true` if this message is an agent-originated request to the
    /// client.
    pub fn is_agent_request(&self) -> bool {
        matches!(self, InboundMessage::AgentRequest { .. })
    }
}

/// The raw wire format of a JSON-RPC message (before classification).
#[derive(Debug, Clone, Deserialize)]
struct RawRpcMessage {
    #[serde(rename = "jsonrpc")]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<crate::rpc::JsonRpcError>,
}

/// Classify a raw JSON-RPC message into an [`InboundMessage`].
///
/// Classification rules:
/// - If `error` is present → it's a **response** (error response)
/// - If `result` is present → it's a **response** (success response)
/// - If `method` is present and `id` is present → it's an **agent request**
///   to the client
/// - If `method` is present and `id` is absent → it's a **notification**
///
/// The JSON-RPC version MUST be `"2.0"`; otherwise a `JsonRpcParseError`
/// is returned.
pub fn classify_message(raw: &str) -> Result<InboundMessage, AcpError> {
    let msg: RawRpcMessage = serde_json::from_str(raw).map_err(|_| {
        AcpError::new(
            ErrorKind::JsonRpcParseError,
            format!("invalid JSON-RPC: {}", raw),
        )
    })?;

    if msg.jsonrpc != "2.0" {
        return Err(AcpError::new(
            ErrorKind::JsonRpcParseError,
            format!("expected jsonrpc \"2.0\", got \"{}\"", msg.jsonrpc),
        ));
    }

    // A response has no `method` field, only `result` or `error`.
    if msg.method.is_none() {
        let id = parse_request_id_opt(&msg.id)?;

        return Ok(InboundMessage::Response {
            id,
            result: msg.result,
            error: msg.error,
        });
    }

    // If there's a non-null `id`, this is a request (agent → client).
    // If there's no `id` (or id is null), this is a notification.
    let method = msg.method.unwrap();
    let params = msg.params;

    if msg.id.is_some() {
        let id = parse_request_id_opt(&msg.id)?;

        // Try to decode a typed agent→client request.
        let typed = try_parse_agent_request(&method, &params, &id);

        Ok(InboundMessage::AgentRequest {
            id,
            method,
            params,
            typed,
        })
    } else {
        // Notification
        Ok(InboundMessage::Notification { method, params })
    }
}

/// Parse a JSON-RPC ID from an `Option<&Value>`.
fn parse_request_id_opt(id: &Option<Value>) -> Result<crate::rpc::RequestId, AcpError> {
    match id {
        None | Some(Value::Null) => Ok(crate::rpc::RequestId::Null),
        Some(Value::Number(n)) if n.is_i64() => {
            Ok(crate::rpc::RequestId::Number(n.as_i64().unwrap()))
        }
        Some(Value::Number(n)) if n.is_u64() => {
            Ok(crate::rpc::RequestId::Number(n.as_u64().unwrap() as i64))
        }
        Some(Value::String(s)) => Ok(crate::rpc::RequestId::String(s.clone())),
        Some(other) => Err(AcpError::new(
            ErrorKind::JsonRpcParseError,
            format!("invalid request id type: {}", other),
        )),
    }
}

/// Try to decode a typed agent→client request from the raw method/params.
///
/// Returns `None` for unknown methods or when the params fail to deserialize
/// into the known request type.
fn try_parse_agent_request(
    method: &str,
    params: &Option<Value>,
    id: &crate::rpc::RequestId,
) -> Option<AgentClientRequest> {
    let params_val = params.clone().unwrap_or(Value::Null);

    match method {
        types::METHOD_READ_TEXT_FILE => {
            let request: ReadTextFileRequest = serde_json::from_value(params_val).ok()?;
            Some(AgentClientRequest::ReadTextFile {
                id: id.clone(),
                request,
            })
        }
        types::METHOD_WRITE_TEXT_FILE => {
            let request: WriteTextFileRequest = serde_json::from_value(params_val).ok()?;
            Some(AgentClientRequest::WriteTextFile {
                id: id.clone(),
                request,
            })
        }
        types::METHOD_REQUEST_PERMISSION => {
            let request: RequestPermissionRequest = serde_json::from_value(params_val).ok()?;
            Some(AgentClientRequest::RequestPermission {
                id: id.clone(),
                request,
            })
        }
        _ => None,
    }
}

/// Notification types that the ACP client recognizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationKind {
    /// `session/update` — a content/status update from the agent.
    SessionUpdate,
    /// `session/cancel` — the agent was canceled (notification to client).
    SessionCancel,
    /// `session/closed` — the agent closed the session.
    SessionClosed,
    /// `$/cancel_request` — protocol-level request cancellation.
    CancelRequest,
    /// An unrecognized notification method.
    Unknown,
}

/// Classify a notification method name.
pub fn classify_notification(method: &str) -> NotificationKind {
    match method {
        types::METHOD_SESSION_UPDATE => NotificationKind::SessionUpdate,
        types::METHOD_CANCEL => NotificationKind::SessionCancel,
        "session/closed" => NotificationKind::SessionClosed,
        types::CANCEL_REQUEST_METHOD => NotificationKind::CancelRequest,
        _ => NotificationKind::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::RequestId;

    #[test]
    fn classify_success_response() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::Response { id, result, error } => {
                assert_eq!(id, RequestId::Number(1));
                assert!(result.is_some());
                assert!(error.is_none());
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn classify_error_response() {
        let raw = r#"{"jsonrpc":"2.0","id":"abc","error":{"code":-32000,"message":"boom"}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::Response { id, result, error } => {
                assert_eq!(id, RequestId::String("abc".to_string()));
                assert!(result.is_none());
                assert_eq!(error.unwrap().code, -32000);
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn classify_notification_no_id() {
        let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"end_turn"}}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::Notification { method, params } => {
                assert_eq!(method, "session/update");
                assert!(params.is_some());
            }
            _ => panic!("expected Notification"),
        }
    }

    #[test]
    fn classify_agent_request_with_id() {
        let raw = r#"{"jsonrpc":"2.0","id":42,"method":"fs/read_text_file","params":{"path":"/foo","sessionId":"s1"}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::AgentRequest {
                id,
                method,
                params,
                typed,
            } => {
                assert_eq!(id, RequestId::Number(42));
                assert_eq!(method, "fs/read_text_file");
                assert!(params.is_some());
                assert!(typed.is_some());
                match typed.unwrap() {
                    AgentClientRequest::ReadTextFile { request, .. } => {
                        assert_eq!(request.path, "/foo");
                        assert_eq!(request.session_id, "s1");
                    }
                    _ => panic!("expected ReadTextFile"),
                }
            }
            _ => panic!("expected AgentRequest"),
        }
    }

    #[test]
    fn classify_request_string_id() {
        let raw = r#"{"jsonrpc":"2.0","id":"req-1","method":"fs/write_text_file","params":{"path":"/bar","content":"hi","sessionId":"s2"}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::AgentRequest { id, method, .. } => {
                assert_eq!(id, RequestId::String("req-1".to_string()));
                assert_eq!(method, "fs/write_text_file");
            }
            _ => panic!("expected AgentRequest"),
        }
    }

    #[test]
    fn classify_request_permission() {
        let raw = r#"{"jsonrpc":"2.0","id":3,"method":"session/request_permission","params":{"sessionId":"s1","toolCall":{"toolCallId":"tc1","title":"test","kind":"execute","status":"pending"},"options":[{"optionId":"allow_once","name":"Allow once","kind":"allow_once"}]}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::AgentRequest { typed, .. } => {
                assert!(typed.is_some());
                match typed.unwrap() {
                    AgentClientRequest::RequestPermission { request, .. } => {
                        assert_eq!(request.session_id, "s1");
                        assert_eq!(request.tool_call.tool_call_id, "tc1");
                        assert_eq!(request.options[0].option_id, "allow_once");
                    }
                    _ => panic!("expected RequestPermission"),
                }
            }
            _ => panic!("expected AgentRequest"),
        }
    }

    #[test]
    fn classify_rejects_non_2_0() {
        let raw = r#"{"jsonrpc":"1.0","id":1,"result":{}}"#;
        let result = classify_message(raw);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::JsonRpcParseError);
    }

    #[test]
    fn classify_rejects_bad_json() {
        let raw = "not json";
        let result = classify_message(raw);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::JsonRpcParseError);
    }

    #[test]
    fn classify_unknown_agent_method_returns_typed_none() {
        let raw = r#"{"jsonrpc":"2.0","id":5,"method":"custom/agent_method","params":{}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::AgentRequest { typed, .. } => {
                assert!(typed.is_none());
            }
            _ => panic!("expected AgentRequest"),
        }
    }

    #[test]
    fn classify_notification_null_id() {
        // A notification with explicit null ID
        let raw =
            r#"{"jsonrpc":"2.0","id":null,"method":"session/cancel","params":{"sessionId":"s1"}}"#;
        let msg = classify_message(raw).unwrap();
        // null id → treated as notification (no response expected)
        assert!(msg.is_notification());
    }

    #[test]
    fn classify_cancel_request_notification() {
        let raw = r#"{"jsonrpc":"2.0","method":"$/cancel_request","params":{"requestId":10}}"#;
        let msg = classify_message(raw).unwrap();
        assert!(msg.is_notification());
    }

    #[test]
    fn notification_kinds() {
        assert_eq!(
            classify_notification("session/update"),
            NotificationKind::SessionUpdate
        );
        assert_eq!(
            classify_notification("session/cancel"),
            NotificationKind::SessionCancel
        );
        assert_eq!(
            classify_notification("session/closed"),
            NotificationKind::SessionClosed
        );
        assert_eq!(
            classify_notification("$/cancel_request"),
            NotificationKind::CancelRequest
        );
        assert_eq!(
            classify_notification("custom/thing"),
            NotificationKind::Unknown
        );
    }

    #[test]
    fn parse_request_id_variants() {
        assert_eq!(parse_request_id_opt(&None).unwrap(), RequestId::Null);
        assert_eq!(
            parse_request_id_opt(&Some(Value::Null)).unwrap(),
            RequestId::Null
        );
        assert_eq!(
            parse_request_id_opt(&Some(Value::Number(serde_json::Number::from(42)))).unwrap(),
            RequestId::Number(42)
        );
        assert_eq!(
            parse_request_id_opt(&Some(Value::String("abc".to_string()))).unwrap(),
            RequestId::String("abc".to_string())
        );
    }

    #[test]
    fn agent_request_id_preserved_in_typed_request() {
        let raw = r#"{"jsonrpc":"2.0","id":"x99","method":"fs/read_text_file","params":{"path":"/f","sessionId":"s"}}"#;
        let msg = classify_message(raw).unwrap();
        match msg {
            InboundMessage::AgentRequest { id, typed, .. } => {
                assert_eq!(id, RequestId::String("x99".to_string()));
                let typed = typed.unwrap();
                assert_eq!(*typed.id(), &RequestId::String("x99".to_string()));
            }
            _ => panic!("expected AgentRequest"),
        }
    }
}
