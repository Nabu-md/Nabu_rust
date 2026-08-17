//! # ACP Test Agent — a minimal ACP v1 agent for integration testing
//!
//! This binary implements a small but spec-compliant ACP agent that speaks
//! JSON-RPC 2.0 over stdin/stdout.  It is designed to be spawned as a
//! subprocess by integration tests to verify the ACP client runtime
//! (`crate::acp::client::AcpClient` and `crate::agent::acp_client::AcpClient`)
//! against a *real* stdio process — not just an in-memory mock transport.
//!
//! ## Behaviour
//!
//! On `initialize` → responds with `protocolVersion: 1`, `agentInfo`,
//!   `agentCapabilities`, and `authMethods`.
//! On `session/new` → responds with `sessionId: "test-session-123"`.
//! On `session/prompt` → emits three `session/update` notifications
//!   (`agent_message_chunk`) before returning a `stopReason: end_turn`
//!   `PromptResponse`.
//! On `session/close` → responds with an empty object.
//!
//! ## Failure modes (command-line flags)
//!
//! * `--exit-after-init` — terminate with code 0 immediately after responding
//!   to `initialize`.
//! * `--exit-during-prompt` — terminate with code 0 before sending the prompt
//!   response (but after sending one notification).
//! * `--malformed-response` — emit malformed JSON instead of a valid response
//!   to `initialize`.
//! * `--stderr-noise` — write diagnostic text to stderr before responding.
//!
//! In all cases the agent reads NDJSON from stdin, one JSON-RPC request per
//! line, and writes one JSON-RPC message (response or notification) per line
//! on stdout.

use std::io::{self, BufRead, Write};

use nabu_core::acp::types::{
    AgentCapabilities, Implementation, InitializeResponse, NewSessionResponse, PromptResponse,
    SessionNotificationParams, SessionUpdate, TextContent,
    ContentBlock, ContentChunk, SUPPORTED_PROTOCOL_VERSION, StopReason,
};
use nabu_core::rpc::{JSON_RPC_VERSION, Request, RequestId};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let exit_after_init = args.iter().any(|a| a == "--exit-after-init");
    let exit_during_prompt = args.iter().any(|a| a == "--exit-during-prompt");
    let malformed_response = args.iter().any(|a| a == "--malformed-response");
    let stderr_noise = args.iter().any(|a| a == "--stderr-noise");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let mut session_id = String::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(_) => {
                let resp = serde_json::json!({
                    "jsonrpc": JSON_RPC_VERSION,
                    "id": null,
                    "error": {"code": -32700, "message": "Parse error"}
                });
                let _ = writeln!(out, "{}", resp);
                let _ = out.flush();
                continue;
            }
        };

        if stderr_noise {
            let _ = writeln!(
                io::stderr(),
                "acp_test_agent: received method {}",
                req.method
            );
        }

        match req.method.as_str() {
            "initialize" => {
                if malformed_response {
                    let _ = writeln!(out, "this is not valid json {{{{");
                    let _ = out.flush();
                    break;
                }
                let resp = InitializeResponse {
                    protocol_version: SUPPORTED_PROTOCOL_VERSION,
                    agent_info: Some(Implementation {
                        name: "acp-test-agent".to_string(),
                        title: Some("ACP Test Agent".to_string()),
                        version: "1.0.0".to_string(),
                        _meta: None,
                    }),
                    agent_capabilities: Some(AgentCapabilities::default()),
                    auth_methods: vec![],
                    _meta: None,
                };
                send_response(&mut out, &req.id, &resp);
                if exit_after_init {
                    break;
                }
            }

            "session/new" => {
                let resp = NewSessionResponse {
                    session_id: "test-session-123".to_string(),
                    config_options: None,
                    modes: None,
                    _meta: None,
                };
                send_response(&mut out, &req.id, &resp);
                session_id = "test-session-123".to_string();
            }

            "session/prompt" => {
                let chunks: Vec<(&str, Option<&str>)> = vec![
                    ("Hello ", Some("msg-1")),
                    ("world!", Some("msg-1")),
                    ("\n", Some("msg-1")),
                ];

                for (text, msg_id) in chunks {
                    let content = ContentChunk {
                        content: ContentBlock::Text(TextContent {
                            text: text.to_string(),
                            annotations: None,
                            _meta: None,
                        }),
                        message_id: msg_id.map(String::from),
                        _meta: None,
                    };
                    let update = SessionUpdate::AgentMessageChunk(content);
                    let notif = SessionNotificationParams {
                        session_id: session_id.clone(),
                        update,
                        _meta: None,
                    };
                    let notif_value = serde_json::to_value(&notif).unwrap();
                    let msg = serde_json::json!({
                        "jsonrpc": JSON_RPC_VERSION,
                        "method": "session/update",
                        "params": notif_value
                    });
                    let _ = writeln!(out, "{}", msg);
                    let _ = out.flush();

                    if exit_during_prompt {
                        std::process::exit(0);
                    }
                }

                let resp = PromptResponse {
                    stop_reason: StopReason::EndTurn,
                    _meta: None,
                };
                send_response(&mut out, &req.id, &resp);
            }

            "session/close" => {
                send_response(&mut out, &req.id, &serde_json::Value::Object(serde_json::Map::new()));
            }

            "session/cancel" => {
                let msg = serde_json::json!({
                    "jsonrpc": JSON_RPC_VERSION,
                    "method": "session/cancel",
                    "params": {"sessionId": session_id.clone()}
                });
                let _ = writeln!(out, "{}", msg);
                let _ = out.flush();
            }

            _ => {
                let resp = serde_json::json!({
                    "jsonrpc": JSON_RPC_VERSION,
                    "id": id_to_value(&req.id),
                    "error": {"code": -32601, "message": "Method not found"}
                });
                let _ = writeln!(out, "{}", resp);
                let _ = out.flush();
            }
        }
    }
}

fn send_response<T: serde::Serialize + std::fmt::Debug>(
    out: &mut impl Write,
    id: &RequestId,
    result: &T,
) {
    let result_val = match serde_json::to_value(result) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(out, "{}", serde_json::json!({
                "jsonrpc": JSON_RPC_VERSION,
                "id": id_to_value(id),
                "error": {"code": -32603, "message": format!("Internal error: {}", e)}
            }));
            let _ = out.flush();
            return;
        }
    };
    let _ = writeln!(out, "{}", serde_json::json!({
        "jsonrpc": JSON_RPC_VERSION,
        "id": id_to_value(id),
        "result": result_val
    }));
    let _ = out.flush();
}

fn id_to_value(id: &RequestId) -> serde_json::Value {
    match id {
        RequestId::Number(n) => serde_json::Value::Number((*n).into()),
        RequestId::String(s) => serde_json::Value::String(s.clone()),
        RequestId::Null => serde_json::Value::Null,
    }
}
