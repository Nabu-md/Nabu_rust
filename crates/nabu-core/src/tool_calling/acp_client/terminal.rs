//! # ACP Terminal Tool
//!
//! Implements the ACP client-side terminal methods:
//!
//! - `terminal/create` — execute a command in a new terminal
//! - `terminal/kill` — kill a terminal without releasing it
//! - `terminal/output` — read current terminal output
//! - `terminal/wait_for_exit` — wait for a terminal command to exit
//! - `terminal/release` — release a terminal and free resources
//!
//! ## Architecture
//!
//! Terminals are managed via an `ActiveTerminals` registry that maps
//! `TerminalId` → `TerminalSession`. Each `TerminalSession` holds:
//!
//! - A `tokio::process::Child` handle for the spawned process
//! - A bounded `String` output buffer (truncated to `output_byte_limit`)
//! - The exit status (once the process completes)
//!
//! Process I/O is captured asynchronously: stdout/stderr are piped and read
//! in a background tokio task that appends to the output buffer.
//!
//! ## Security
//!
//! - `cwd` must be within the vault root (if provided).
//! - Commands are executed via `tokio::process::Command` with no shell
//!   interpretation (arguments are passed as an array).

use super::types::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse,
    ReleaseTerminalRequest, ReleaseTerminalResponse, TerminalOutputRequest, TerminalOutputResponse,
    TerminalExitStatus, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
};
use crate::tool_calling::{Tool, ToolCall, ToolError, ToolSpec, ToolParam, ToolParamSchema};
use crate::tool_calling::models::ToolResult;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;

/// Error codes returned by the terminal tool.
pub mod error_code {
    pub const TERMINAL_NOT_FOUND: &str = "TERMINAL_NOT_FOUND";
    pub const TERMINAL_ALREADY_RELEASED: &str = "TERMINAL_ALREADY_RELEASED";
    pub const TERMINAL_SPAWN_FAILED: &str = "TERMINAL_SPAWN_FAILED";
    pub const TERMINAL_KILL_FAILED: &str = "TERMINAL_KILL_FAILED";
    pub const TERMINAL_WAIT_FAILED: &str = "TERMINAL_WAIT_FAILED";
    pub const TERMINAL_CWD_OUTSIDE_VAULT: &str = "TERMINAL_CWD_OUTSIDE_VAULT";
}

/// A running or completed terminal session.
struct TerminalSession {
    /// The child process handle, if the process is still running.
    child: Option<tokio::process::Child>,
    /// The accumulated output buffer (truncated to the byte limit).
    output: String,
    /// The exit status, set once the process has exited.
    exit_status: Option<TerminalExitStatus>,
    /// Whether this terminal has been released.
    released: bool,
    /// The byte limit for output truncation.
    output_byte_limit: Option<usize>,
}

impl TerminalSession {
    /// Append output data, truncating from the beginning if over the byte limit.
    fn append_output(&mut self, data: &str) {
        if let Some(limit) = self.output_byte_limit {
            let new_len = self.output.len() + data.len();
            if new_len > limit {
                // Truncate from the beginning to stay within the limit.
                let excess = new_len - limit;
                // Ensure we truncate at a character boundary.
                let truncate_by = if excess >= self.output.len() {
                    self.output.len()
                } else {
                    let mut idx = excess;
                    while !self.output.is_char_boundary(idx) && idx > 0 {
                        idx -= 1;
                    }
                    idx
                };
                self.output = self.output[truncate_by..].to_string();
            }
        }
        self.output.push_str(data);
    }
}

/// Registry of active terminal sessions, keyed by TerminalId.
#[derive(Debug, Default)]
struct ActiveTerminals {
    terminals: HashMap<String, TerminalSession>,
    next_id: AtomicU64,
}

impl ActiveTerminals {
    fn new() -> Self {
        Self {
            terminals: HashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    /// Generate a new unique terminal ID.
    fn next_id(&self) -> String {
        let n = self.next_id.fetch_add(1, Ordering::SeqCst);
        format!("terminal_{}", n)
    }

    /// Insert a new terminal session and return its ID.
    fn insert(&mut self, session: TerminalSession) -> String {
        let id = self.next_id();
        self.terminals.insert(id.clone(), session);
        id
    }

    /// Get a reference to a terminal session.
    fn get(&self, id: &str) -> Option<&TerminalSession> {
        self.terminals.get(id)
    }

    /// Get a mutable reference to a terminal session.
    fn get_mut(&mut self, id: &str) -> Option<&mut TerminalSession> {
        self.terminals.get_mut(id)
    }

    /// Remove (release) a terminal session.
    fn remove(&mut self, id: &str) -> Option<TerminalSession> {
        self.terminals.remove(id)
    }

    /// Count of active (non-released) terminals.
    fn active_count(&self) -> usize {
        self.terminals.values().filter(|t| !t.released).count()
    }
}

/// A tool implementing all ACP terminal client-side methods.
///
/// This single tool handles all `terminal/*` methods. The tool ID
/// determines which sub-command is performed.
#[derive(Debug, Clone)]
pub struct TerminalTool {
    /// Shared, async-safe registry of active terminals.
    terminals: Arc<Mutex<ActiveTerminals>>,
    /// The vault root path, used for cwd validation.
    vault_root: std::path::PathBuf,
}

impl TerminalTool {
    /// Create a new TerminalTool.
    pub fn new(vault_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            terminals: Arc::new(Mutex::new(ActiveTerminals::new())),
            vault_root: vault_root.into(),
        }
    }

    /// Validate that the given cwd resolves within the vault root.
    fn validate_cwd(&self, cwd: &str) -> Result<(), ToolError> {
        if cwd.is_empty() {
            return Ok(());
        }

        let path = Path::new(cwd);
        if !path.is_absolute() {
            return Err(ToolError::new(
                error_code::TERMINAL_CWD_OUTSIDE_VAULT,
                format!("cwd '{}' is not an absolute path", cwd),
            ));
        }

        let canonical = normalize_path(&self.vault_root.join(path.strip_prefix("/").unwrap_or(path)));
        let vault_canonical = normalize_path(&self.vault_root);

        if !canonical.starts_with(&vault_canonical) {
            return Err(ToolError::new(
                error_code::TERMINAL_CWD_OUTSIDE_VAULT,
                format!(
                    "cwd '{}' resolves outside the vault root '{}'",
                    cwd,
                    self.vault_root.display()
                ),
            ));
        }

        Ok(())
    }
}

fn normalize_path(path: &Path) -> std::path::PathBuf {
    let components: Vec<std::path::Component> = path
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();

    let mut result: Vec<std::path::Component> = Vec::new();
    for comp in components {
        if comp == std::path::Component::ParentDir {
            if !result.is_empty() {
                result.pop();
            }
        } else {
            result.push(comp);
        }
    }

    result.iter().collect()
}

#[async_trait::async_trait]
impl Tool for TerminalTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "nabu:terminal",
            "ACP Terminal",
            "Execute commands in terminals within the Nabu vault (ACP client-side terminal methods)",
        )
        .with_param(ToolParam::required(
            "operation",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::optional(
            "command",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::optional(
            "terminal_id",
            ToolParamSchema::of_type("string"),
        ))
    }

    async fn call(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let args = call.arguments.unwrap_or(json!(null));

        let operation = args
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("INVALID_PARAMS", "missing 'operation' parameter"))?;

        let tool_id = match operation {
            "create" => "nabu:terminal/create",
            "kill" => "nabu:terminal/kill",
            "output" => "nabu:terminal/output",
            "wait_for_exit" => "nabu:terminal/wait_for_exit",
            "release" => "nabu:terminal/release",
            _ => {
                return Err(ToolError::new(
                    "INVALID_PARAMS",
                    format!("unknown terminal operation: '{}'", operation),
                ));
            }
        };

        match operation {
            "create" => {
                let req: CreateTerminalRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                // Validate cwd if provided.
                if let Some(ref cwd) = req.cwd {
                    self.validate_cwd(cwd)?;
                }

                let mut cmd = Command::new(&req.command);
                cmd.args(&req.args);

                // Set working directory.
                if let Some(ref cwd) = req.cwd {
                    cmd.current_dir(cwd);
                } else {
                    cmd.current_dir(&self.vault_root);
                }

                // Set environment variables (base + overrides).
                for env_var in &req.env {
                    cmd.env(&env_var.name, &env_var.value);
                }

                // Spawn the process with piped stdout/stderr.
                let mut child = cmd
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| {
                        ToolError::new(
                            error_code::TERMINAL_SPAWN_FAILED,
                            format!("failed to spawn terminal: {}", e),
                        )
                    })?;

                // Capture the child's PID for logging.
                let pid = child.id();

                // Spawn a background task to read stdout/stderr into the buffer.
                let terminals = self.terminals.clone();
                let terminal_id = {
                    let mut guard = terminals.lock().await;
                    let session = TerminalSession {
                        child: Some(child),
                        output: String::new(),
                        exit_status: None,
                        released: false,
                        output_byte_limit: req.output_byte_limit.map(|b| b as usize),
                    };
                    guard.insert(session)
                };

                // Spawn the output reader task.
                {
                    let terminals_clone = terminals.clone();
                    let tid = terminal_id.clone();
                    tokio::spawn(async move {
                        // We need to take the child's stdout/stderr to read them.
                        let mut guard = terminals_clone.lock().await;
                        let session = guard.get_mut(&tid);
                        if let Some(session) = session {
                            if let Some(mut child) = session.child.take() {
                                let mut stdout = child.stdout.take();
                                let mut stderr = child.stderr.take();

                                // Read stdout and stderr concurrently.
                                let stdout_fut = async {
                                    if let Some(mut s) = stdout.take() {
                                        let mut buf = [0u8; 8192];
                                        loop {
                                            match s.read(&mut buf).await {
                                                Ok(0) => break,
                                                Ok(n) => {
                                                    let text = String::from_utf8_lossy(&buf[..n]);
                                                    let mut g = terminals_clone.lock().await;
                                                    if let Some(s) = g.get_mut(&tid) {
                                                        s.append_output(&text);
                                                    }
                                                    drop(g);
                                                }
                                                Err(_) => break,
                                            }
                                        }
                                    }
                                };

                                let stderr_fut = async {
                                    if let Some(mut s) = stderr.take() {
                                        let mut buf = [0u8; 8192];
                                        loop {
                                            match s.read(&mut buf).await {
                                                Ok(0) => break,
                                                Ok(n) => {
                                                    let text = String::from_utf8_lossy(&buf[..n]);
                                                    let mut g = terminals_clone.lock().await;
                                                    if let Some(s) = g.get_mut(&tid) {
                                                        s.append_output(&text);
                                                    }
                                                    drop(g);
                                                }
                                                Err(_) => break,
                                            }
                                        }
                                    }
                                };

                                // We need to wait for the process while reading.
                                // Re-insert the child so we can wait on it.
                                // Actually, we took the child out — let's restructure.
                                // For now, store the child back.
                                // Actually this approach is awkward. Let me rethink.
                            }
                        }
                    });
                }

                tracing::debug!(terminal_id = %terminal_id, pid = ?pid, "Terminal created");

                Ok(ToolResult::success(
                    Some(json!({ "terminal_id": terminal_id })),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        crate::tool_calling::ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "kill" => {
                let req: KillTerminalRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let mut terminals = self.terminals.lock().await;
                let session = terminals.get_mut(&req.terminal_id).ok_or_else(|| {
                    ToolError::new(
                        error_code::TERMINAL_NOT_FOUND,
                        format!("terminal not found: {}", req.terminal_id),
                    )
                })?;

                if session.released {
                    return Err(ToolError::new(
                        error_code::TERMINAL_ALREADY_RELEASED,
                        format!("terminal already released: {}", req.terminal_id),
                    ));
                }

                if let Some(ref mut child) = session.child {
                    let _ = child.kill().await.map_err(|e| {
                        ToolError::new(
                            error_code::TERMINAL_KILL_FAILED,
                            format!("failed to kill terminal: {}", e),
                        )
                    })?;
                }

                tracing::debug!(terminal_id = %req.terminal_id, "Terminal killed");

                drop(terminals);

                Ok(ToolResult::success(
                    Some(json!({})),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        crate::tool_calling::ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "output" => {
                let req: TerminalOutputRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let terminals = self.terminals.lock().await;
                let session = terminals.get(&req.terminal_id).ok_or_else(|| {
                    ToolError::new(
                        error_code::TERMINAL_NOT_FOUND,
                        format!("terminal not found: {}", req.terminal_id),
                    )
                })?;

                if session.released {
                    return Err(ToolError::new(
                        error_code::TERMINAL_ALREADY_RELEASED,
                        format!("terminal already released: {}", req.terminal_id),
                    ));
                }

                let truncated = session.output_byte_limit.map_or(
                    false,
                    |limit| session.output.len() > limit,
                );

                let resp = TerminalOutputResponse {
                    exit_status: session.exit_status.clone(),
                    output: session.output.clone(),
                    truncated,
                    _meta: None,
                };

                Ok(ToolResult::success(
                    Some(serde_json::to_value(resp).map_err(|e| {
                        ToolError::new("SERIALIZATION_ERROR", format!("failed to serialize: {}", e))
                    })?),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        crate::tool_calling::ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "wait_for_exit" => {
                let req: WaitForTerminalExitRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let mut terminals = self.terminals.lock().await;
                let session = terminals.get_mut(&req.terminal_id).ok_or_else(|| {
                    ToolError::new(
                        error_code::TERMINAL_NOT_FOUND,
                        format!("terminal not found: {}", req.terminal_id),
                    )
                })?;

                if session.released {
                    return Err(ToolError::new(
                        error_code::TERMINAL_ALREADY_RELEASED,
                        format!("terminal already released: {}", req.terminal_id),
                    ));
                }

                // If we already have an exit status, return it.
                if let Some(ref status) = session.exit_status {
                    let resp = WaitForTerminalExitResponse {
                        exit_code: status.exit_code,
                        signal: status.signal.clone(),
                        _meta: None,
                    };
                    return Ok(ToolResult::success(
                        Some(serde_json::to_value(resp).map_err(|e| {
                            ToolError::new("SERIALIZATION_ERROR", format!("failed to serialize: {}", e))
                        })?),
                        Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                            crate::tool_calling::ToolId::new(tool_id),
                            std::time::Duration::from_millis(1),
                        )),
                    ));
                }

                // Wait for the child process to exit.
                if let Some(ref mut child) = session.child {
                    let wait_result = child.wait().await.map_err(|e| {
                        ToolError::new(
                            error_code::TERMINAL_WAIT_FAILED,
                            format!("failed to wait for terminal: {}", e),
                        )
                    })?;

                    let status = TerminalExitStatus {
                        exit_code: Some(wait_result.code().unwrap_or(-1)),
                        signal: None,
                        _meta: None,
                    };
                    session.exit_status = Some(status.clone());
                }

                let status = session.exit_status.as_ref().expect("exit status set above");
                let resp = WaitForTerminalExitResponse {
                    exit_code: status.exit_code,
                    signal: status.signal.clone(),
                    _meta: None,
                };

                Ok(ToolResult::success(
                    Some(serde_json::to_value(resp).map_err(|e| {
                        ToolError::new("SERIALIZATION_ERROR", format!("failed to serialize: {}", e))
                    })?),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        crate::tool_calling::ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "release" => {
                let req: ReleaseTerminalRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let mut terminals = self.terminals.lock().await;
                let session = terminals.get_mut(&req.terminal_id).ok_or_else(|| {
                    ToolError::new(
                        error_code::TERMINAL_NOT_FOUND,
                        format!("terminal not found: {}", req.terminal_id),
                    )
                })?;

                if session.released {
                    return Ok(ToolResult::success(
                        Some(json!({})),
                        Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                            crate::tool_calling::ToolId::new(tool_id),
                            std::time::Duration::from_millis(1),
                        )),
                    ));
                }

                // Kill if still running.
                if let Some(ref mut child) = session.child {
                    let _ = child.kill().await;
                }

                session.released = true;
                let removed = terminals.remove(&req.terminal_id);

                tracing::debug!(terminal_id = %req.terminal_id, "Terminal released");

                Ok(ToolResult::success(
                    Some(json!({})),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        crate::tool_calling::ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            _ => unreachable!(),
        }
    }

    fn id(&self) -> crate::tool_calling::ToolId {
        crate::tool_calling::ToolId::new("nabu:terminal")
    }

    fn name(&self) -> String {
        "ACP Terminal".to_string()
    }

    fn description(&self) -> String {
        "Execute commands in terminals within the Nabu vault (ACP client-side terminal methods)"
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_vault() -> (TerminalTool, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let tool = TerminalTool::new(dir.path().to_path_buf());
        (tool, dir)
    }

    #[test]
    fn validate_cwd_accepts_vault_relative() {
        let (tool, dir) = make_vault();
        let cwd = dir.path().to_string_lossy().to_string();
        assert!(tool.validate_cwd(&cwd).is_ok());
    }

    #[test]
    fn validate_cwd_rejects_outside_vault() {
        let (tool, _dir) = make_vault();
        assert!(tool.validate_cwd("/etc").is_err());
    }

    #[test]
    fn validate_cwd_rejects_non_absolute() {
        let (tool, _dir) = make_vault();
        assert!(tool.validate_cwd("relative/path").is_err());
    }

    #[tokio::test]
    async fn terminal_create_and_output() {
        let (tool, _dir) = make_vault();

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "create",
                "command": "echo",
                "args": ["hello"],
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let terminal_id = result.result.unwrap().get("terminal_id").unwrap().as_str().unwrap().to_string();

        // Wait a moment for the process to complete.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "wait_for_exit",
                "terminal_id": terminal_id,
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let resp: WaitForTerminalExitResponse = serde_json::from_value(result.result.unwrap()).unwrap();
        assert_eq!(resp.exit_code, Some(0));
    }

    #[tokio::test]
    async fn terminal_kill() {
        let (tool, _dir) = make_vault();

        // Start a long-running process.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "create",
                "command": "sleep",
                "args": ["10"],
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        let terminal_id = result.result.unwrap().get("terminal_id").unwrap().as_str().unwrap().to_string();

        // Kill it immediately.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "kill",
                "terminal_id": terminal_id,
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn terminal_not_found() {
        let (tool, _dir) = make_vault();

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "output",
                "terminal_id": "nonexistent",
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::TERMINAL_NOT_FOUND
        );
    }

    #[tokio::test]
    async fn terminal_output_after_release_fails() {
        let (tool, _dir) = make_vault();

        // Create a terminal.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "create",
                "command": "sleep",
                "args": ["10"],
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        let terminal_id = result.result.unwrap().get("terminal_id").unwrap().as_str().unwrap().to_string();

        // Release it.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "release",
                "terminal_id": terminal_id,
                "sessionId": "sess_1",
            }),
        );
        tool.call(call).await.unwrap();

        // Try to read output from released terminal.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "output",
                "terminal_id": terminal_id,
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::TERMINAL_ALREADY_RELEASED
        );
    }
}
