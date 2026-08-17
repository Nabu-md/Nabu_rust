//! # ACP Elicitation Tool
//!
//! Implements the ACP client-side `elicitation/create` method.
//!
//! This method allows an ACP agent to request structured user input from
//! the client (Nabu). The agent sends a message and a JSON Schema describing
//! the form fields to render, and the client is responsible for presenting
//! this to the user and returning their response.
//!
//! ## User Interaction
//!
//! User interaction (rendering a form dialog, opening a URL, etc.) cannot
//! be performed by the core library alone — the UI lives in `nabu-ui`.
//! The [`ElicitationHandler`] trait defines the delegation boundary: the
//! tool calls `handler.request_elicitation()`, and the integration layer
//! provides a concrete implementation that bridges to the UI.
//!
//! ## Security
//!
//! The default handler (`DenyElicitationHandler`) always returns `Cancelled`.
//! Production deployments must provide a handler that explicitly prompts the
//! user — permissions are never auto-approved.

use super::callbacks::ElicitationHandler;
use super::types::{ElicitationOutcome, ElicitationSchema};
use crate::tool_calling::models::ToolResult;
use crate::tool_calling::{
    Tool, ToolCall, ToolError, ToolId, ToolParam, ToolParamSchema, ToolSpec,
};
use serde_json::json;

/// Error codes returned by the elicitation tool.
pub mod error_code {
    pub const ELICITATION_FAILED: &str = "ELICITATION_FAILED";
    pub const ELICITATION_CANCELLED: &str = "ELICITATION_CANCELLED";
}

/// Tool ID prefix for elicitation.
const TOOL_ID: &str = "nabu:elicitation/create";

/// A tool implementing the ACP `elicitation/create` client-side method.
///
/// When invoked, this tool delegates to an [`ElicitationHandler`] to present
/// the elicitation to the user and collect their response. The default handler
/// denies all requests.
pub struct ElicitationTool {
    handler: Arc<dyn ElicitationHandler>,
}

impl ElicitationTool {
    /// Create a new ElicitationTool with the given handler.
    pub fn new(handler: Arc<dyn ElicitationHandler>) -> Self {
        Self { handler }
    }

    /// Create a new ElicitationTool with the default denying handler.
    pub fn with_default_handler() -> Self {
        Self::new(Arc::new(super::callbacks::DenyElicitationHandler))
    }
}

#[async_trait::async_trait]
impl Tool for ElicitationTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            TOOL_ID,
            "ACP Elicitation",
            "Request structured user input via a form or URL (ACP client-side elicitation/create)",
        )
        .with_param(ToolParam::required(
            "message",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::required(
            "mode",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::optional(
            "requested_schema",
            ToolParamSchema::of_type("object"),
        ))
        .with_param(ToolParam::optional(
            "url",
            ToolParamSchema::of_type("string"),
        ))
    }

    async fn call(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let args = call.arguments.unwrap_or(json!(null));

        let tool_id = TOOL_ID;

        let error_result = |err: ToolError| {
            ToolResult::error(
                err,
                Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                    crate::tool_calling::ToolId::new(tool_id),
                    std::time::Duration::from_millis(1),
                )),
            )
        };

        // Extract the elicitation message.
        let message = match args.get("message").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => {
                return Ok(error_result(ToolError::new(
                    "INVALID_PARAMS",
                    "missing 'message' parameter",
                )))
            }
        };

        // Determine the mode (form or url).
        let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("form");

        match mode {
            "form" => {
                let schema: ElicitationSchema = args
                    .get("requested_schema")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();

                let elicitation_id = args
                    .get("elicitation_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                let outcome = self
                    .handler
                    .request_elicitation(&elicitation_id, message, &schema)
                    .await;

                match outcome {
                    ElicitationOutcome::Provided { response } => Ok(ToolResult::success(
                        Some(json!({ "response": response })),
                        Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                            crate::tool_calling::ToolId::new(tool_id),
                            std::time::Duration::from_millis(1),
                        )),
                    )),
                    ElicitationOutcome::Cancelled => Ok(error_result(ToolError::new(
                        error_code::ELICITATION_CANCELLED,
                        "user cancelled the elicitation",
                    ))),
                }
            }
            "url" => {
                let url = match args.get("url").and_then(|v| v.as_str()) {
                    Some(u) => u,
                    None => {
                        return Ok(error_result(ToolError::new(
                            "INVALID_PARAMS",
                            "missing 'url' parameter for url elicitation",
                        )))
                    }
                };

                // For URL elicitations, the client opens the URL and the
                // agent receives the result externally. In this core
                // implementation, we return an error indicating that URL
                // elicitations are not yet supported in the default handler.
                // A production handler should open the URL in the system
                // browser and coordinate the response back.
                let _ = url;
                Ok(error_result(ToolError::new(
                    error_code::ELICITATION_FAILED,
                    "URL elicitations are handled externally; no response channel available",
                )))
            }
            other => {
                // Forward-compat: unknown modes are passed through to the handler.
                Ok(error_result(ToolError::new(
                    "INVALID_PARAMS",
                    format!("unknown elicitation mode: '{}'", other),
                )))
            }
        }
    }

    fn id(&self) -> ToolId {
        ToolId::new(TOOL_ID)
    }

    fn name(&self) -> String {
        "ACP Elicitation".to_string()
    }

    fn description(&self) -> String {
        "Request structured user input via a form or URL (ACP client-side elicitation/create)"
            .to_string()
    }
}

use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_calling::acp_client::callbacks::ElicitationHandler;
    use crate::tool_calling::acp_client::types::ElicitationId;
    use async_trait::async_trait;

    struct MockElicitationHandler {
        response: serde_json::Value,
    }

    #[async_trait]
    impl ElicitationHandler for MockElicitationHandler {
        async fn request_elicitation(
            &self,
            _elicitation_id: &ElicitationId,
            _message: &str,
            _schema: &ElicitationSchema,
        ) -> ElicitationOutcome {
            ElicitationOutcome::Provided {
                response: self.response.clone(),
            }
        }
    }

    struct CancelHandler;

    #[async_trait]
    impl ElicitationHandler for CancelHandler {
        async fn request_elicitation(
            &self,
            _elicitation_id: &ElicitationId,
            _message: &str,
            _schema: &ElicitationSchema,
        ) -> ElicitationOutcome {
            ElicitationOutcome::Cancelled
        }
    }

    #[tokio::test]
    async fn elicitation_form_returns_response() {
        let handler = Arc::new(MockElicitationHandler {
            response: json!({ "name": "test" }),
        });
        let tool = ElicitationTool::new(handler);

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "message": "Please enter your name",
                "mode": "form",
                "requested_schema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    }
                }
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let resp = result.result.unwrap();
        assert_eq!(resp["response"]["name"], "test");
    }

    #[tokio::test]
    async fn elicitation_cancelled_returns_error() {
        let handler = Arc::new(CancelHandler);
        let tool = ElicitationTool::new(handler);

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "message": "Please enter your name",
                "mode": "form",
                "requested_schema": {}
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::ELICITATION_CANCELLED
        );
    }

    #[tokio::test]
    async fn elicitation_url_mode_returns_error() {
        let handler = Arc::new(MockElicitationHandler {
            response: json!({}),
        });
        let tool = ElicitationTool::new(handler);

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "message": "Authenticate",
                "mode": "url",
                "url": "https://example.com/auth"
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(result.error.unwrap().code, error_code::ELICITATION_FAILED);
    }

    #[tokio::test]
    async fn elicitation_default_handler_denies() {
        let tool = ElicitationTool::with_default_handler();

        let call = ToolCall::with_args(
            TOOL_ID,
            json!({
                "message": "Enter data",
                "mode": "form",
                "requested_schema": {}
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::ELICITATION_CANCELLED
        );
    }

    #[tokio::test]
    async fn elicitation_missing_message_fails() {
        let tool = ElicitationTool::with_default_handler();

        let call = ToolCall::with_args(TOOL_ID, json!({ "mode": "form" }));

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
    }
}
