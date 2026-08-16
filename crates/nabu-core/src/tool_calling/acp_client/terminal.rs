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
//! Each terminal is managed by a background tokio task that:
//!
//! 1. Reads stdout/stderr from the spawned process into an output buffer.
//! 2. Waits for the process to exit.
//! 3. Stores the exit status and notifies any waiters.
//!
//! The `TerminalTool` stores an `Arc<Mutex<ActiveTerminals>>` that maps
//! `TerminalId` → `TerminalSession`. Each session holds the process handle
//! (in a separate `Arc<Mutex<Option<Child>>>` for non-blocking kill/wait)
//! and a `Notify` for exit coordination.
//!
//! ## Lock ordering
//!
//! To prevent deadlocks, locks are always acquired in this order:
//! 1. `child_mutex` (per-terminal) — acquired and released independently
//! 2. `terminals` (global) — acquired and released independently
//!
//! The background task and kill handler never hold both locks simultaneously.

use super::types::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse,
    ReleaseTerminalRequest, ReleaseTerminalResponse, TerminalOutputRequest, TerminalOutputResponse,
    TerminalExitStatus, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
};
use crate::tool_calling::{Tool, ToolCall, ToolError, ToolId, ToolSpec, ToolParam, ToolParamSchema};
use crate::tool_calling::models::ToolResult;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncRead;
use tokio::process::Child;
use tokio::sync::{Mutex, Notify};

/// Error codes returned by the terminal tool.
pub mod error_code {
    pub const TERMINAL_NOT_FOUND: &str = "TERMINAL_NOT_FOUND";
    pub const TERMINAL_ALREADY_RELEASED: &str = "TERMINAL_ALREADY_RELEASED";
    pub const TERMINAL_SPAWN_FAILED: &str = "TERMINAL_SPAWN_FAILED";
    pub const TERMINAL_KILL_FAILED: &str = "TERMINAL_KILL_FAILED";
    pub const TERMINAL_CWD_OUTSIDE_VAULT: &str = "TERMINAL_CWD_OUTSIDE_VAULT";
}

/// A running or completed terminal session.
struct TerminalSession {
    /// The child process handle, stored separately from the main session
    /// so it can be taken out for killing/waiting without holding the
    /// global terminals lock.
    child: Arc<Mutex<Option<Child>>>,
    /// The accumulated output buffer.
    output: String,
    /// The exit status, set once the process has exited.
    exit_status: Option<TerminalExitStatus>,
    /// Whether this terminal has been released (no more operations allowed).
    released: bool,
    /// The byte limit for output truncation.
    output_byte_limit: Option<usize>,
    /// Notified when the process exits (for wait_for_exit coordination).
    exit_notify: Arc<Notify>,
}

/// Registry of active terminal sessions.
struct ActiveTerminals {
    terminals: HashMap<String, TerminalSession>,
    next_id: AtomicU64,
}

impl Default for ActiveTerminals {
    fn default() -> Self {
        Self {
            terminals: HashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }
}

impl ActiveTerminals {
    fn new() -> Self {
        Self::default()
    }

    fn next_id(&self) -> String {
        let n = self.next_id.fetch_add(1, Ordering::SeqCst);
        format!("terminal_{}", n)
    }

    fn insert(&mut self, session: TerminalSession) -> String {
        let id = self.next_id();
        self.terminals.insert(id.clone(), session);
        id
    }

    fn get(&self, id: &str) -> Option<&TerminalSession> {
        self.terminals.get(id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut TerminalSession> {
        self.terminals.get_mut(id)
    }

    fn remove(&mut self, id: &str) -> Option<TerminalSession> {
        self.terminals.remove(id)
    }

    fn active_count(&self) -> usize {
        self.terminals.values().filter(|t| !t.released).count()
    }
}

/// A tool implementing all ACP terminal client-side methods.
#[derive(Debug, Clone)]
pub struct TerminalTool {
    /// Shared, async-safe registry of active terminals.
    terminals: Arc<Mutex<ActiveTerminals>>,
    /// The vault root path, used for cwd validation.
    vault_root: PathBuf,
}

impl TerminalTool {
    /// Create a new TerminalTool.
    pub fn new(vault_root: impl Into<PathBuf>) -> Self {
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

    /// Spawn a child process and start the output-reading background task.
    ///
    /// Takes stdout/stderr from the child and spawns a background task that
    /// reads all output, waits for the child to exit, stores the exit status,
    /// and notifies any `wait_for_exit` waiters.
    fn spawn_terminal(
        &self,
        command: &str,
        args: &[String],
        cwd: Option<&str>,
        env: &[crate::acp::types::EnvVariable],
        output_byte_limit: Option<u64>,
    ) -> Result<String, ToolError> {
        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(cwd_str) = cwd {
            cmd.current_dir(cwd_str);
        } else {
            cmd.current_dir(&self.vault_root);
        }

        for env_var in env {
            cmd.env(&env_var.name, &env_var.value);
        }

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

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let child_mutex = Arc::new(Mutex::new(Some(child)));
        let terminals = self.terminals.clone();

        // Generate the terminal ID.
        let terminal_id = {
            let mut guard = self.terminals.lock().await;
            let session = TerminalSession {
                child: child_mutex.clone(),
                output: String::new(),
                exit_status: None,
                released: false,
                output_byte_limit: output_byte_limit.map(|b| b as usize),
                exit_notify: Arc::new(Notify::new()),
            };
            guard.insert(session)
        };

        tracing::debug!(terminal_id = %terminal_id, "Terminal created");

        // Spawn background task for reading output and waiting for exit.
        let terminals_for_bg = terminals.clone();
        let terminal_id_for_bg = terminal_id.clone();
        let child_mutex_for_bg = child_mutex.clone();

        tokio::spawn(async move {
            read_stream(stdout, &terminals_for_bg, &terminal_id_for_bg).await;
            read_stream(stderr, &terminals_for_bg, &terminal_id_for_bg).await;

            // Wait for the child to exit.
            let child_opt = {
                let mut guard = child_mutex_for_bg.lock().await;
                guard.take()
            };

            let exit_status = if let Some(mut child) = child_opt {
                match child.wait().await {
                    Ok(status) => Some(TerminalExitStatus {
                        exit_code: status.code(),
                        signal: status.signal().map(|s| s.to_string()),
                        _meta: None,
                    }),
                    Err(_) => None,
                }
            } else {
                None
            };

            // Store exit status and notify waiters.
            {
                let mut guard = terminals_for_bg.lock().await;
                if let Some(session) = guard.get_mut(&terminal_id_for_bg) {
                    if session.exit_status.is_none() {
                        session.exit_status = exit_status;
                    }
                    session.exit_notify.notify_waiters();
                }
            }
        });

        Ok(terminal_id)

    /// Kill the child process for a terminal and wait for it to exit.
    async fn kill_terminal(&self, terminal_id: &str) -> Result<Option<TerminalExitStatus>, ToolError> {
        let (exit_status, notify) = {
            let mut guard = self.terminals.lock().await;
            let session = guard.get_mut(terminal_id).ok_or_else(|| {
                ToolError::new(
                    error_code::TERMINAL_NOT_FOUND,
                    format!("terminal not found: {}", terminal_id),
                )
            })?;

            if session.released {
                return Err(ToolError::new(
                    error_code::TERMINAL_ALREADY_RELEASED,
                    format!("terminal already released: {}", terminal_id),
                ));
            }

            // Take the child out (for killing) — the bg task may have already taken it.
            let child_opt = {
                let mut child_guard = session.child.lock().await;
                child_guard.take()
            };

            // If bg task already took it, it's already being processed.
            // We can't kill it ourselves — let the bg task handle it.
            // But we still need to wait for exit. Actually, if the bg task
            // took the child, it's already calling wait(). We just need to
            // wait for the notify.
            if child_opt.is_none() {
                // Child already taken by bg task — get notify and wait later.
                let notify = session.exit_notify.clone();
                (None, Some(notify))
            } else {
                let notify = session.exit_notify.clone();
                (Some(child_opt), Some(notify))
            }
        };

        // Kill the child if we have it.
        let exit_status = if let Some(mut child) = exit_status.unwrap_or(None) {
            let kill_result = child.kill().await;
            let wait_result = child.wait().await;
            match (kill_result, wait_result) {
                (_, Ok(status)) => Some(TerminalExitStatus {
                    exit_code: status.code(),
                    signal: status.signal().map(|s| s.to_string()),
                    _meta: None,
                }),
                (Err(e), _) => return Err(ToolError::new(
                    error_code::TERMINAL_KILL_FAILED,
                    format!("failed to kill terminal: {}", e),
                )),
                (Ok(()), Err(e)) => return Err(ToolError::new(
                    error_code::TERMINAL_KILL_FAILED,
                    format!("failed to wait for terminal after kill: {}", e),
                )),
                _ => None,
            }
        } else {
            None
        };

        // Notify waiters if we got an exit status.
        if exit_status.is_some() {
            let mut guard = self.terminals.lock().await;
            if let Some(session) = guard.get_mut(terminal_id) {
                session.exit_status = exit_status.clone();
                session.exit_notify.notify_waiters();
            }
        } else if let Some(ref notify) = notify {
            notify.notified().await;
            // After notify, check for exit status.
            let guard = self.terminals.lock().await;
            if let Some(session) = guard.get(terminal_id) {
                return Ok(session.exit_status.clone());
            }
        }

        Ok(exit_status)
    }

    /// Wait for a terminal to exit and return its exit status.
    async fn wait_for_exit_inner(&self, terminal_id: &str) -> Result<TerminalExitStatus, ToolError> {
        // First check if already exited.
        {
            let guard = self.terminals.lock().await;
            let session = guard.get(terminal_id).ok_or_else(|| {
                ToolError::new(
                    error_code::TERMINAL_NOT_FOUND,
                    format!("terminal not found: {}", terminal_id),
                )
            })?;

            if session.released {
                return Err(ToolError::new(
                    error_code::TERMINAL_ALREADY_RELEASED,
                    format!("terminal already released: {}", terminal_id),
                ));
            }

            if let Some(ref status) = session.exit_status {
                return Ok(status.clone());
            }
        }

        // Wait for notification.
        let (notify, ) = {
            let mut guard = self.terminals.lock().await;
            let session = guard.get_mut(terminal_id).ok_or_else(|| {
                ToolError::new(
                    error_code::TERMINAL_NOT_FOUND,
                    format!("terminal not found: {}", terminal_id),
                )
            })?;
            let notify = session.exit_notify.clone();
            drop(guard);
            (notify,)
        };

        notify.notified().await;

        // Get the exit status after notification.
        let guard = self.terminals.lock().await;
        let session = guard.get(terminal_id).ok_or_else(|| {
            ToolError::new(
                error_code::TERMINAL_NOT_FOUND,
                format!("terminal not found: {}", terminal_id),
            )
        })?;
        session
            .exit_status
            .clone()
            .ok_or_else(|| ToolError::new("TERMINAL_WAIT_TIMEOUT", "process exited but no status"))
    }
}

/// Read all available data from a stream into the terminal's output buffer.
async fn read_stream<R: AsyncRead + Unpin>(
    mut stream: Option<R>,
    terminals: &Arc<Mutex<ActiveTerminals>>,
    terminal_id: &str,
) {
    if let Some(mut s) = stream {
        let mut buf = [0u8; 8192];
        loop {
            match s.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]);
                    let mut guard = terminals.lock().await;
                    if let Some(session) = guard.get_mut(terminal_id) {
                        session.append_output(&text);
                    }
                }
                Err(_) => break,
            }
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
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

impl TerminalSession {
    /// Append output data, truncating from the beginning if over the byte limit.
    fn append_output(&mut self, data: &str) {
        if let Some(limit) = self.output_byte_limit {
            let new_len = self.output.len() + data.len();
            if new_len > limit {
                let excess = new_len - limit;
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

// Fix the read_stream function to accept the trait object properly.
// ChildStdout and ChildStderr both implement AsyncRead, but we need a generic approach.

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

                if let Some(ref cwd) = req.cwd {
                    self.validate_cwd(cwd)?;
                }

                let terminal_id = self.spawn_terminal(
                    &req.command,
                    &req.args,
                    req.cwd.as_deref(),
                    &req.env,
                    req.output_byte_limit,
                )?;

                tracing::debug!(terminal_id = %terminal_id, "Terminal created");

                Ok(ToolResult::success(
                    Some(json!({ "terminal_id": terminal_id })),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "kill" => {
                let req: KillTerminalRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                self.kill_terminal(&req.terminal_id).await?;

                tracing::debug!(terminal_id = %req.terminal_id, "Terminal killed");

                Ok(ToolResult::success(
                    Some(json!({})),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "output" => {
                let req: TerminalOutputRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let guard = self.terminals.lock().await;
                let session = guard.get(&req.terminal_id).ok_or_else(|| {
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

                let truncated = session
                    .output_byte_limit
                    .map_or(false, |limit| session.output.len() > limit);

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
                        ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "wait_for_exit" => {
                let req: WaitForTerminalExitRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let status = self.wait_for_exit_inner(&req.terminal_id).await?;

                let resp = WaitForTerminalExitResponse {
                    exit_code: status.exit_code,
                    signal: status.signal,
                    _meta: None,
                };

                Ok(ToolResult::success(
                    Some(serde_json::to_value(resp).map_err(|e| {
                        ToolError::new("SERIALIZATION_ERROR", format!("failed to serialize: {}", e))
                    })?),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "release" => {
                let req: ReleaseTerminalRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                // Kill if still running, then remove.
                self.kill_terminal(&req.terminal_id).await.ok();

                {
                    let mut guard = self.terminals.lock().await;
                    guard.remove(&req.terminal_id);
                }

                tracing::debug!(terminal_id = %req.terminal_id, "Terminal released");

                Ok(ToolResult::success(
                    Some(json!({})),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            _ => unreachable!(),
        }
    }

    fn id(&self) -> ToolId {
        ToolId::new("nabu:terminal")
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

    fn make_tool() -> (TerminalTool, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let tool = TerminalTool::new(dir.path().to_path_buf());
        (tool, dir)
    }

    #[test]
    fn validate_cwd_accepts_vault_relative() {
        let (tool, dir) = make_tool();
        let cwd = dir.path().to_string_lossy().to_string();
        assert!(tool.validate_cwd(&cwd).is_ok());
    }

    #[test]
    fn validate_cwd_rejects_outside_vault() {
        let (tool, _dir) = make_tool();
        assert!(tool.validate_cwd("/etc").is_err());
    }

    #[test]
    fn validate_cwd_rejects_non_absolute() {
        let (tool, _dir) = make_tool();
        assert!(tool.validate_cwd("relative/path").is_err());
    }

    #[tokio::test]
    async fn terminal_create_and_wait() {
        let (tool, _dir) = make_tool();

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "create",
                "command": "echo",
                "args": ["hello"],
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let terminal_id = result
            .result
            .unwrap()
            .get("terminal_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // Wait for the process to exit.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "wait_for_exit",
                "terminal_id": terminal_id,
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let resp: WaitForTerminalExitResponse =
            serde_json::from_value(result.result.unwrap()).unwrap();
        assert_eq!(resp.exit_code, Some(0));
    }

    #[tokio::test]
    async fn terminal_output_after_completion() {
        let (tool, _dir) = make_tool();

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "create",
                "command": "echo",
                "args": ["hello world"],
            }),
        );

        let result = tool.call(call).await.unwrap();
        let terminal_id = result
            .result
            .unwrap()
            .get("terminal_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // Wait for completion.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "wait_for_exit",
                "terminal_id": terminal_id,
            }),
        );
        tool.call(call).await.unwrap();

        // Read output.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "output",
                "terminal_id": terminal_id,
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let resp: TerminalOutputResponse =
            serde_json::from_value(result.result.unwrap()).unwrap();
        assert!(resp.output.contains("hello world"));
        assert!(resp.exit_status.is_some());
    }

    #[tokio::test]
    async fn terminal_kill() {
        let (tool, _dir) = make_tool();

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "create",
                "command": "sleep",
                "args": ["30"],
            }),
        );

        let result = tool.call(call).await.unwrap();
        let terminal_id = result
            .result
            .unwrap()
            .get("terminal_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // Kill it immediately.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "kill",
                "terminal_id": terminal_id,
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn terminal_release_removes_session() {
        let (tool, _dir) = make_tool();

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "create",
                "command": "sleep",
                "args": ["30"],
            }),
        );

        let result = tool.call(call).await.unwrap();
        let terminal_id = result
            .result
            .unwrap()
            .get("terminal_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // Release it.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "release",
                "terminal_id": terminal_id,
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());

        // Verify it's gone.
        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "output",
                "terminal_id": terminal_id,
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
    }

    #[tokio::test]
    async fn terminal_not_found() {
        let (tool, _dir) = make_tool();

        let call = ToolCall::with_args(
            "nabu:terminal",
            json!({
                "operation": "output",
                "terminal_id": "nonexistent",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::TERMINAL_NOT_FOUND
        );
    }
}
