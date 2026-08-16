//! # ACP Permission Tool
//!
//! Implements the ACP client-side `session/request_permission` method.
//!
//! This method allows an ACP agent to request user authorization before
//! executing a potentially sensitive tool call. The agent provides permission
//! options (e.g. "Allow Once", "Allow Always", "Reject") and the client
//! presents them to the user, returning the selected option.
//!
//! ## User Interaction
//!
//! User interaction (rendering a permission dialog) cannot be performed by
//! the core library alone. The [`PermissionHandler`] trait defines the
//! delegation boundary: the tool calls `handler.request_permission()`, and
//! the integration layer provides a concrete implementation that bridges to
//! the UI.
//!
//! ## Security
//!
//! The default handler (`AllowPermissionHandler`) allows operations by
//! selecting the first "allow" option. This is appropriate for a desktop
//! application where the user has already launched Nabu and granted it
//! access. For more granular control, a custom handler should be provided
//! that surfaces specific permission dialogs for sensitive operations.

use super::callbacks::PermissionHandler;
use super::types::{
    PermissionOption, RequestPermissionOutcome, RequestPermissionResponse,
};
use crate::tool_calling::{Tool, ToolCall, ToolError, ToolId, ToolParam, ToolParamSchema, ToolSpec};
use crate::tool_calling::models::ToolResult;
use serde_json::json;
use std::sync::Arc;

/// Error codes returned by the permission tool.
pub mod error_code {
    pub const PERMISSION_CANCELLED: &str = "PERMISSION_CANCELLED";
    pub const PERMISSION_DENIED: &str = "PERMISSION_DENIED";
}

/// Tool ID for the permission tool.
const TOOL_ID: &str = "nabu:session/request_permission";

/// A tool implementing the ACP `session/request_permission` client-side method.
pub struct PermissionTool {
    handler: Arc<dyn PermissionHandler>,
}

impl PermissionTool {
    /// Create a new PermissionTool with the given handler.
    pub fn new(handler: Arc<dyn PermissionHandler>) -> Self {
        Self { handler }
    }

    /// Create a new PermissionTool with the default handler.
    ///
    /// The default handler selects the first "allow" option, falling back
    /// to cancellation if no allow option is available.
    pub fn with_default_handler() -> Self {
        Self::new(Arc::new(
            super::callbacks::AllowPermissionHandler,
        ))
    }
}

#[async_trait::async_trait]
impl Tool for PermissionTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            TOOL_ID,
            "ACP Permission",
            "Request permission from the user for a tool call operation (ACP client-side session/request_permission)",
        )
        .with_param(ToolParam::required(
            "options",
            ToolParamSchema::of_type("array"),
        ))
        .with_param(ToolParam::required(
            "session_id",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::required(
            "tool_call",
            ToolParamSchema::of_type("object"),
        ))
    }

    async fn call(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let args = call.arguments.unwrap_or(json!(null));

        let options: Vec<PermissionOption> = args
            .get("options")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let session_id = args
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tool_call_id = args
            .get("toolCall")
            .and_then(|v| v.get("toolCallId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let outcome = self
            .handler
            .request_permission(&session_id, &tool_call_id, &options)
            .await;

        match outcome {
            RequestPermissionOutcome::Selected { .. } => {
                let resp = RequestPermissionResponse {
                    outcome,
                    _meta: None,
                };
                Ok(ToolResult::success(
                    Some(serde_json::to_value(resp).map_err(|e| {
                        ToolError::new("SERIALIZATION_ERROR", format!("failed to serialize: {}", e))
                    })?),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        ToolId::new(TOOL_ID),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            RequestPermissionOutcome::Cancelled { .. } => {
                Err(ToolError::new(
                    error_code::PERMISSION_CANCELLED,
                    "user cancelled the permission request",
                ))
            }
        }
    }

    fn id(&self) -> ToolId {
        ToolId::new(TOOL_ID)
    }

    fn name(&self) -> String {
        "ACP Permission".to_string()
    }

    fn description(&self) -> String {
        "Request permission from the user for a tool call operation (ACP client-side session/request_permission)"
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_calling::acp_client::callbacks::PermissionHandler;
    use crate::tool_calling::acp_client::types::{PermissionOptionKind, ToolCallStatus};
    use async_trait::async_trait;

    struct MockPermissionHandler {
        outcome: RequestPermissionOutcome,
    }

    #[async_trait]
    impl PermissionHandler for MockPermissionHandler {
        async fn request_permission(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _options: &[PermissionOption],
        ) -> RequestPermissionOutcome {
            self.outcome.clone()
        }
    }

    fn make_options() -> Vec<PermissionOption> {
        vec![
            PermissionOption {
                kind: PermissionOptionKind::AllowOnce,
                name: "Allow".to_string(),
                option_id: "allow_once".to_string(),
                _meta: None,
            },
            PermissionOption {
                kind: PermissionOptionKind::RejectOnce,
                name: "Reject".to_string(),
                option_id: "reject_once".to_string(),
                _meta: None,
            },
        ]
    }

    #[tokio::test]
    async fn permission_selected_returns_response() {
        let handler = Arc::new(MockPermissionHandler {
            outcome: RequestPermissionOutcome::Selected {
                option_id: "allow_once".to_string(),
                _meta: None,
            },
        });
        let tool = PermissionTool::new(handler);

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "options": make_options(),
                "sessionId": "sess_1",
                "toolCall": {
                    "toolCallId": "tc_1",
                    "status": "in_progress"
                }
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let resp: RequestPermissionResponse =
            serde_json::from_value(result.result.unwrap()).unwrap();
        match resp.outcome {
            RequestPermissionOutcome::Selected { option_id, .. } => {
                assert_eq!(option_id, "allow_once");
            }
            _ => panic!("expected Selected"),
        }
    }

    #[tokio::test]
    async fn permission_cancelled_returns_error() {
        let handler = Arc::new(MockPermissionHandler {
            outcome: RequestPermissionOutcome::Cancelled { _meta: None },
        });
        let tool = PermissionTool::new(handler);

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "options": make_options(),
                "sessionId": "sess_1",
                "toolCall": {
                    "toolCallId": "tc_1"
                }
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::PERMISSION_CANCELLED
        );
    }

    #[tokio::test]
    async fn permission_default_handler_allows() {
        let tool = PermissionTool::with_default_handler();

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "options": make_options(),
                "sessionId": "sess_1",
                "toolCall": {
                    "toolCallId": "tc_1"
                }
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn permission_default_handler_denies_when_no_allow() {
        let tool = PermissionTool::with_default_handler();

        let options = vec![PermissionOption {
            kind: PermissionOptionKind::RejectOnce,
            name: "Reject".to_string(),
            option_id: "reject".to_string(),
            _meta: None,
        }];

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "options": options,
                "sessionId": "sess_1",
                "toolCall": {
                    "toolCallId": "tc_1"
                }
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::PERMISSION_CANCELLED
        );
    }
}
