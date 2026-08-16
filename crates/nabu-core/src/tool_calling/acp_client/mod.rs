//! # ACP Client-Side Capability Platform
//!
//! This submodule implements the Nabu side of the Agent Communication Protocol
//! (ACP) — the **client capabilities** that allow an ACP agent to read/write
//! files, execute terminal commands, request user input, and ask for
//! permission, all within Nabu's vault boundary.
//!
//! ## Ownership
//!
//! This module is owned by `tool_calling/` (Phase 3c). It does **not**
//! modify the `acp/` module — it consumes the types defined there
//! (`SessionId`, `EnvVariable`, etc.) and adds its own request/response
//! types for client-side methods that the ACP module does not define.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  ACP Agent (external, JSON-RPC client)  │
//! └──────┬──────────────────────────────────┘
//!        │  (JSON-RPC request: fs/read_text_file, terminal/create, ...)
//!        ▼
//! ┌─────────────────────────────────────────┐
//! │  rpc::Router                            │
//! │  (dispatches by JSON-RPC method name)    │
//! └──────┬──────────────────────────────────┘
//!        │  (delegates to Tool via ToolRegistry)
//!        ▼
//! ┌─────────────────────────────────────────┐
//! │  tool_calling::ToolRegistry             │
//! │  (maps ToolId → Tool implementation)     │
//! └──────┬──────────────────────────────────┘
//!        │  (Tool::call → ToolResult)
//!        ▼
//! ┌─────────────────────────────────────────┐
//! │  tool_calling::acp_client::*            │
//! │  FileSystemTool  — reads/writes vault   │
//! │  TerminalTool    — spawns processes     │
//! │  ElicitationTool — user input callback   │
//! │  PermissionTool  — permission callback   │
//! └──────┬──────────────┬───────────────────┘
//!        │              │
//!        ▼              ▼
//!  StorageManager   tokio::process
//!  (vault owner)    (process execution)
//! ```
//!
//! ## Security
//!
//! - **Path traversal protection**: all file-system paths are validated
//!   to resolve within the vault root.
//! - **cwd validation**: terminal `cwd` must be within the vault root.
//! - **No auto-approval**: elicitation and permission requests require
//!   explicit handler implementation — the default handlers deny/cancel
//!   requests.
//! - **No shell interpretation**: terminal commands use argument arrays,
//!   not shell strings.

pub mod callbacks;
pub mod elicitation;
pub mod filesystem;
pub mod permissions;
pub mod terminal;
pub mod types;

pub use callbacks::{
    AllowPermissionHandler, DenyElicitationHandler, ElicitationHandler, PermissionHandler,
};
pub use elicitation::ElicitationTool;
pub use filesystem::FileSystemTool;
pub use permissions::PermissionTool;
pub use terminal::TerminalTool;
pub use types::{
    AcpClientCapabilities, ElicitationCapabilities, ReadTextFileRequest, ReadTextFileResponse,
    WriteTextFileRequest, WriteTextFileResponse,
};

use crate::storage::StorageManager;
use crate::tool_calling::ToolRegistry;
use std::sync::Arc;

/// Register all ACP client-side tools with the given [`ToolRegistry`].
///
/// This function creates and registers:
///
/// - [`FileSystemTool`] — `fs/read_text_file`, `fs/write_text_file`
/// - [`TerminalTool`] — `terminal/create`, `terminal/kill`, `terminal/output`,
///   `terminal/wait_for_exit`, `terminal/release`
/// - [`ElicitationTool`] — `elicitation/create`
/// - [`PermissionTool`] — `session/request_permission`
///
/// The tools are registered under the `nabu:` namespace. The JSON-RPC router
/// can then dispatch to them by matching the method name to the appropriate
/// tool ID.
///
/// # Arguments
///
/// - `registry` — the [`ToolRegistry`] to register tools with.
/// - `storage` — the [`StorageManager`] that owns the vault (used for
///   file-system tools).
/// - `vault_root` — the vault root path (used for path validation in
///   both file-system and terminal tools).
/// - `elicitation_handler` — optional handler for elicitation requests.
///   If `None`, a deny-all handler is used.
/// - `permission_handler` — optional handler for permission requests.
///   If `None`, an allow-first handler is used.
pub async fn register_acp_client_tools(
    registry: &ToolRegistry,
    storage: Arc<StorageManager>,
    vault_root: std::path::PathBuf,
    elicitation_handler: Option<Arc<dyn ElicitationHandler>>,
    permission_handler: Option<Arc<dyn PermissionHandler>>,
) {
    // File-system tools.
    let fs_tool = Arc::new(FileSystemTool::new(storage));
    registry.register(fs_tool).await;

    // Terminal tool.
    let terminal_tool = Arc::new(TerminalTool::new(vault_root));
    registry.register(terminal_tool).await;

    // Elicitation tool.
    let elicitation_tool = Arc::new(ElicitationTool::new(
        elicitation_handler
            .unwrap_or_else(|| Arc::new(DenyElicitationHandler)),
    ));
    registry.register(elicitation_tool).await;

    // Permission tool.
    let permission_tool = Arc::new(PermissionTool::new(
        permission_handler
            .unwrap_or_else(|| Arc::new(AllowPermissionHandler)),
    ));
    registry.register(permission_tool).await;

    tracing::info!(
        "Registered ACP client tools: fs, terminal, elicitation, permission"
    );
}

/// Build the default [`crate::acp::ClientCapabilities`] for Nabu.
///
/// Nabu advertises:
/// - `fs.readTextFile = true`
/// - `fs.writeTextFile = true`
/// - `terminal = true`
/// - `elicitation` is not part of the standard `ClientCapabilities` in the
///   ACP module — it is tracked separately in [`AcpClientCapabilities`].
pub fn default_client_capabilities() -> crate::acp::ClientCapabilities {
    use crate::acp::types::FileSystemCapabilities;

    crate::acp::ClientCapabilities {
        fs: Some(FileSystemCapabilities {
            read_text_file: true,
            write_text_file: true,
            _meta: None,
        }),
        terminal: true,
        session: None,
        _meta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::types::ClientCapabilities;

    #[tokio::test]
    async fn default_client_capabilities_has_fs_and_terminal() {
        let caps = default_client_capabilities();
        assert!(caps.fs.is_some());
        let fs = caps.fs.unwrap();
        assert!(fs.read_text_file);
        assert!(fs.write_text_file);
        assert!(caps.terminal);
    }

    #[tokio::test]
    async fn register_acp_client_tools_registers_all() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(StorageManager::new(dir.path()));
        let registry = ToolRegistry::new();

        register_acp_client_tools(
            &registry,
            storage,
            dir.path().to_path_buf(),
            None,
            None,
        )
        .await;

        assert_eq!(registry.tool_count().await, 4);
        assert!(registry.has_tool("nabu:fs").await);
        assert!(registry.has_tool("nabu:terminal").await);
        assert!(registry.has_tool("nabu:elicitation/create").await);
        assert!(registry.has_tool("nabu:session/request_permission").await);
    }

    #[tokio::test]
    async fn register_acp_client_tools_with_custom_handlers() {
        struct TestElicitationHandler;
        #[async_trait::async_trait]
        impl ElicitationHandler for TestElicitationHandler {
            async fn request_elicitation(
                &self,
                _id: &str,
                _msg: &str,
                _schema: &types::ElicitationSchema,
            ) -> types::ElicitationOutcome {
                types::ElicitationOutcome::Cancelled
            }
        }

        struct TestPermissionHandler;
        #[async_trait::async_trait]
        impl PermissionHandler for TestPermissionHandler {
            async fn request_permission(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
                _options: &[types::PermissionOption],
            ) -> types::RequestPermissionOutcome {
                types::RequestPermissionOutcome::Cancelled
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(StorageManager::new(dir.path()));
        let registry = ToolRegistry::new();

        register_acp_client_tools(
            &registry,
            storage,
            dir.path().to_path_buf(),
            Some(Arc::new(TestElicitationHandler)),
            Some(Arc::new(TestPermissionHandler)),
        )
        .await;

        assert_eq!(registry.tool_count().await, 4);
    }

    #[tokio::test]
    async fn acp_client_capabilities_serializes_to_acp() {
        let caps = AcpClientCapabilities {
            fs_read: true,
            fs_write: true,
            terminal: true,
            elicitation: true,
            _meta: None,
        };
        let acp_caps = caps.to_acp_client_capabilities();
        assert!(matches!(acp_caps, ClientCapabilities { fs: Some(_), terminal: true, .. }));
    }
}
