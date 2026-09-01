//! # Callback traits for user-interaction operations
//!
//! The ACP client-side methods `elicitation/create` and `session/request_permission`
//! require user interaction — they cannot be fulfilled by the core library alone
//! because Nabu's UI lives in a separate process (`ui-react`).
//!
//! These traits define the delegation boundary: the [`ElicitationTool`] and
//! [`PermissionTool`] call into a provided handler implementation, which is
//! responsible for presenting the question to the user and returning their
//! response. The default implementations use simple synchronous approval
//! (denying all requests), which is the safe fallback.

use super::types::{ElicitationId, ElicitationOutcome, RequestPermissionOutcome};
use async_trait::async_trait;

/// Handles elicitation requests — presents a form or URL to the user and
/// returns their structured response.
///
/// Implementors are responsible for bridging the elicitation request to the
/// UI layer (e.g. by sending a message over the Tauri IPC channel or by
/// queueing it for the Dioxus frontend to render as a modal dialog).
#[async_trait]
pub trait ElicitationHandler: Send + Sync {
    /// Present an elicitation form to the user and wait for their response.
    ///
    /// The `elicitation_id` can be used to correlate the response with the
    /// original request. The `message` describes what information is needed.
    /// The `requested_schema` describes the form fields to render.
    ///
    /// Returns the user's structured response as a JSON value, or `None` if
    /// the user cancelled the elicitation.
    async fn request_elicitation(
        &self,
        elicitation_id: &ElicitationId,
        message: &str,
        requested_schema: &super::types::ElicitationSchema,
    ) -> ElicitationOutcome;
}

/// A default elicitation handler that always denies requests.
///
/// This is the safe fallback — it never exposes user data or performs
/// any I/O. Production code should provide a real handler that delegates
/// to the UI layer.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyElicitationHandler;

#[async_trait]
impl ElicitationHandler for DenyElicitationHandler {
    async fn request_elicitation(
        &self,
        _elicitation_id: &ElicitationId,
        _message: &str,
        _requested_schema: &super::types::ElicitationSchema,
    ) -> ElicitationOutcome {
        ElicitationOutcome::Cancelled
    }
}

/// Handles permission requests — presents options to the user and returns
/// their decision.
///
/// Implementors are responsible for bridging the permission request to the
/// UI layer (e.g. by showing a dialog with the provided options).
#[async_trait]
pub trait PermissionHandler: Send + Sync {
    /// Present permission options to the user and wait for their decision.
    ///
    /// The `session_id` identifies the session context. The `tool_call_id`
    /// identifies the specific tool call requiring permission. The `options`
    /// are the choices presented to the user.
    ///
    /// Returns the user's selected option ID, or `None` if the user cancelled.
    async fn request_permission(
        &self,
        session_id: &str,
        tool_call_id: &str,
        options: &[super::types::PermissionOption],
    ) -> RequestPermissionOutcome;
}

/// A default permission handler that always allows operations.
///
/// **Security note:** This is the safe default for a desktop application —
/// the user has already launched Nabu and granted it access. Production code
/// may want to provide a more granular handler that surfaces specific
/// permission dialogs for sensitive operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowPermissionHandler;

#[async_trait]
impl PermissionHandler for AllowPermissionHandler {
    async fn request_permission(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        options: &[super::types::PermissionOption],
    ) -> RequestPermissionOutcome {
        // Default: allow once by selecting the first "allow" option if present.
        for opt in options {
            if matches!(
                opt.kind,
                super::types::PermissionOptionKind::AllowOnce
                    | super::types::PermissionOptionKind::AllowAlways
            ) {
                return RequestPermissionOutcome::Selected {
                    option_id: opt.option_id.clone(),
                    _meta: None,
                };
            }
        }
        // No allow option available — treat as cancelled.
        RequestPermissionOutcome::Cancelled { _meta: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deny_handler_always_cancels() {
        let handler = DenyElicitationHandler;
        let schema = super::super::types::ElicitationSchema::default();
        let outcome = handler
            .request_elicitation(&"elic_1".to_string(), "Test", &schema)
            .await;
        assert!(matches!(outcome, ElicitationOutcome::Cancelled));
    }

    #[tokio::test]
    async fn allow_handler_selects_first_allow_option() {
        let handler = AllowPermissionHandler;
        let options = vec![
            super::super::types::PermissionOption {
                kind: super::super::types::PermissionOptionKind::RejectOnce,
                name: "Reject".to_string(),
                option_id: "reject".to_string(),
                _meta: None,
            },
            super::super::types::PermissionOption {
                kind: super::super::types::PermissionOptionKind::AllowOnce,
                name: "Allow".to_string(),
                option_id: "allow".to_string(),
                _meta: None,
            },
        ];
        let outcome = handler.request_permission("sess_1", "tc_1", &options).await;
        match outcome {
            RequestPermissionOutcome::Selected { option_id, .. } => {
                assert_eq!(option_id, "allow");
            }
            _ => panic!("expected Selected"),
        }
    }

    #[tokio::test]
    async fn allow_handler_cancels_when_no_allow_option() {
        let handler = AllowPermissionHandler;
        let options = vec![super::super::types::PermissionOption {
            kind: super::super::types::PermissionOptionKind::RejectOnce,
            name: "Reject".to_string(),
            option_id: "reject".to_string(),
            _meta: None,
        }];
        let outcome = handler.request_permission("sess_1", "tc_1", &options).await;
        assert!(matches!(
            outcome,
            RequestPermissionOutcome::Cancelled { .. }
        ));
    }
}
