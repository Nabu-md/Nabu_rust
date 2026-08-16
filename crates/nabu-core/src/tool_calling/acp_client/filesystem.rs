//! # ACP File-System Tools
//!
//! Implements the ACP client-side file-system methods:
//!
//! - `fs/read_text_file` — read text content from a file in the vault
//! - `fs/write_text_file` — write text content to a file in the vault
//!
//! ## Security
//!
//! Both operations enforce vault boundary protection:
//!
//! - Paths must be absolute.
//! - Paths are normalized and checked to ensure they resolve within the
//!   vault root. Path traversal techniques (e.g. `../`) are rejected.
//! - The actual file I/O is delegated to `StorageManager`, which is the
//!   single storage owner in the Nabu Capability Platform.

use super::types::{ReadTextFileRequest, WriteTextFileRequest};
use crate::storage::StorageManager;
use crate::tool_calling::{Tool, ToolCall, ToolError, ToolSpec, ToolParam, ToolParamSchema};
use crate::tool_calling::models::ToolResult;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Error codes returned by the file-system tools.
pub mod error_code {
    pub const FS_PATH_OUTSIDE_VAULT: &str = "FS_PATH_OUTSIDE_VAULT";
    pub const FS_PATH_NOT_ABSOLUTE: &str = "FS_PATH_NOT_ABSOLUTE";
    pub const FS_FILE_NOT_FOUND: &str = "FS_FILE_NOT_FOUND";
    pub const FS_READ_FAILED: &str = "FS_READ_FAILED";
    pub const FS_WRITE_FAILED: &str = "FS_WRITE_FAILED";
}

/// A tool implementing ACP `fs/read_text_file` and `fs/write_text_file`.
///
/// This single tool handles both file-system methods. The tool ID determines
/// which operation is performed:
///
/// - `nabu:fs/read_text_file` — read a file
/// - `nabu:fs/write_text_file` — write a file
///
/// Vault boundary enforcement: all paths are validated to ensure they
/// resolve within the vault root before any I/O is performed.
#[derive(Clone)]
pub struct FileSystemTool {
    storage: Arc<StorageManager>,
    vault_root: PathBuf,
}

impl FileSystemTool {
    /// Create a new FileSystemTool bound to the given storage manager's vault.
    pub fn new(storage: Arc<StorageManager>) -> Self {
        let vault_root = storage.vault_path().clone();
        Self {
            storage,
            vault_root,
        }
    }

    /// Validate that the given absolute path resolves within the vault root.
    ///
    /// - Rejects non-absolute paths.
    /// - Normalizes the path (resolving `..` and `.` components).
    /// - Ensures the normalized path is within the vault root.
    ///
    /// Returns the vault-relative path (with leading `/` stripped) on success.
    fn validate_vault_path(&self, abs_path: &str) -> Result<PathBuf, ToolError> {
        let path = Path::new(abs_path);

        if !path.is_absolute() {
            return Err(ToolError::new(
                error_code::FS_PATH_NOT_ABSOLUTE,
                format!("path '{}' is not absolute", abs_path),
            ));
        }

        let canonical = self.vault_root.join(path.strip_prefix("/").unwrap_or(path));

        // Normalize the path to resolve any `..` or `.` components.
        // We use lexically_normal-style normalization without actually
        // hitting the filesystem (which may not exist yet for write ops).
        let normalized = normalize_path(&canonical);

        // Ensure the normalized path is still within the vault root.
        let vault_canonical = normalize_path(&self.vault_root);
        if !normalized.starts_with(&vault_canonical) {
            return Err(ToolError::new(
                error_code::FS_PATH_OUTSIDE_VAULT,
                format!(
                    "path '{}' resolves outside the vault root '{}'",
                    abs_path,
                    self.vault_root.display()
                ),
            ));
        }

        // Compute the vault-relative path.
        let vault_rel = normalized
            .strip_prefix(&vault_canonical)
            .map_err(|_| {
                ToolError::new(
                    error_code::FS_PATH_OUTSIDE_VAULT,
                    format!(
                        "path '{}' resolves outside the vault root '{}'",
                        abs_path,
                        self.vault_root.display()
                    ),
                )
            })?
            .to_path_buf();

        Ok(vault_rel)
    }
}

/// Perform a simple lexical normalization of a path without touching the filesystem.
///
/// Resolves `.` and `..` components lexically (similar to `Path::canonicalize`
/// but doesn't require the path to exist). This is safe because we've already
/// validated that the path starts with the vault root.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<std::path::Component> = path
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();

    let mut result: Vec<std::path::Component> = Vec::new();
    for comp in components.drain(..) {
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
impl Tool for FileSystemTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "nabu:fs",
            "ACP File System",
            "Read and write text files within the Nabu vault",
        )
        .with_param(ToolParam::required(
            "operation",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::required(
            "path",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::optional(
            "content",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::optional(
            "line",
            ToolParamSchema::of_type("number"),
        ))
        .with_param(ToolParam::optional(
            "limit",
            ToolParamSchema::of_type("number"),
        ))
    }

    async fn call(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let args = call.arguments.unwrap_or(json!(null));

        let operation = args
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("INVALID_PARAMS", "missing 'operation' parameter"))?;

        let tool_id = match operation {
            "read_text_file" => "nabu:fs/read_text_file",
            "write_text_file" => "nabu:fs/write_text_file",
            _ => {
                return Err(ToolError::new(
                    "INVALID_PARAMS",
                    format!("unknown file-system operation: '{}'", operation),
                ));
            }
        };

        match operation {
            "read_text_file" => {
                let req: ReadTextFileRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let vault_rel = self.validate_vault_path(&req.path)?;
                let abs_path = self.vault_root.join(&vault_rel);

                if !abs_path.exists() {
                    return Err(ToolError::new(
                        error_code::FS_FILE_NOT_FOUND,
                        format!("file not found: {}", req.path),
                    ));
                }

                let content = std::fs::read_to_string(&abs_path).map_err(|e| {
                    ToolError::new(
                        error_code::FS_READ_FAILED,
                        format!("failed to read file '{}': {}", req.path, e),
                    )
                })?;

                // Apply optional line/limit for reading.
                let content = apply_line_limit(&content, req.line, req.limit);

                Ok(ToolResult::success(
                    Some(json!({
                        "content": content,
                    })),
                    Some(crate::tool_calling::ToolExecutionMeta::from_duration(
                        crate::tool_calling::ToolId::new(tool_id),
                        std::time::Duration::from_millis(1),
                    )),
                ))
            }
            "write_text_file" => {
                let req: WriteTextFileRequest =
                    serde_json::from_value(args).map_err(|e| {
                        ToolError::new("INVALID_PARAMS", format!("invalid params: {}", e))
                    })?;

                let vault_rel = self.validate_vault_path(&req.path)?;

                // Use StorageManager.save_note_content for persistence.
                // The vault-relative path is passed directly.
                let vault_rel_str = vault_rel.to_string_lossy().to_string();
                let _saved_path = self
                    .storage
                    .save_note_content(&vault_rel_str, &req.content)
                    .map_err(|e| {
                        ToolError::new(error_code::FS_WRITE_FAILED, e)
                    })?;

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
        crate::tool_calling::ToolId::new("nabu:fs")
    }

    fn name(&self) -> String {
        "ACP File System".to_string()
    }

    fn description(&self) -> String {
        "Read and write text files within the Nabu vault (ACP client-side fs methods)".to_string()
    }
}

/// Apply optional line offset and limit to the content string.
///
/// `line` is 1-based (matching ACP semantics). `limit` is the max number
/// of lines to return.
fn apply_line_limit(content: &str, line: Option<u64>, limit: Option<u64>) -> String {
    let start_line = line.unwrap_or(1).saturating_sub(1) as usize;
    let lines: Vec<&str> = content.lines().collect();

    let end_idx = if let Some(limit) = limit {
        (start_line + limit as usize).min(lines.len())
    } else {
        lines.len()
    };

    if start_line >= lines.len() {
        return String::new();
    }

    lines[start_line..end_idx].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{KnowledgeObject, ObjectContent, ObjectMetadata, ObjectType};
    use tempfile::tempdir;

    fn make_storage() -> (Arc<StorageManager>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let storage = Arc::new(StorageManager::new(dir.path()));
        (storage, dir)
    }

    #[test]
    fn validate_vault_path_accepts_vault_relative() {
        let (storage, _dir) = make_storage();
        let tool = FileSystemTool::new(storage);
        let vault_root = tool.vault_root.clone();

        let abs = vault_root.join("notes/test.md");
        let abs_str = abs.to_string_lossy().to_string();
        let result = tool.validate_vault_path(&abs_str);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_vault_path_rejects_outside_vault() {
        let (storage, _dir) = make_storage();
        let tool = FileSystemTool::new(storage);

        let result = tool.validate_vault_path("/etc/passwd");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code,
            error_code::FS_PATH_OUTSIDE_VAULT
        );
    }

    #[test]
    fn validate_vault_path_rejects_non_absolute() {
        let (storage, _dir) = make_storage();
        let tool = FileSystemTool::new(storage);

        let result = tool.validate_vault_path("relative/path.md");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code,
            error_code::FS_PATH_NOT_ABSOLUTE
        );
    }

    #[test]
    fn validate_vault_path_rejects_traversal() {
        let (storage, _dir) = make_storage();
        let tool = FileSystemTool::new(storage);

        let vault_root = tool.vault_root.to_string_lossy().to_string();
        let traversal = format!("{}/../outside.md", vault_root);
        let result = tool.validate_vault_path(&traversal);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code,
            error_code::FS_PATH_OUTSIDE_VAULT
        );
    }

    #[tokio::test]
    async fn read_text_file_returns_content() {
        let (storage, _dir) = make_storage();

        // Write a test file directly to disk in the vault.
        let file_path = storage.vault_path().join("test.md");
        std::fs::write(&file_path, "# Hello World").unwrap();

        let tool = FileSystemTool::new(storage);
        let abs_path = file_path.to_string_lossy().to_string();

        let call = ToolCall::with_args(
            "nabu:fs",
            json!({
                "operation": "read_text_file",
                "path": abs_path,
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let result_value = result.result.unwrap();
        let content = result_value.get("content").unwrap().as_str().unwrap();
        assert_eq!(content, "# Hello World");
    }

    #[tokio::test]
    async fn read_text_file_not_found() {
        let (storage, _dir) = make_storage();
        let tool = FileSystemTool::new(storage.clone());

        let path = storage.vault_path().join("nonexistent.md");
        let abs_path = path.to_string_lossy().to_string();

        let call = ToolCall::with_args(
            "nabu:fs",
            json!({
                "operation": "read_text_file",
                "path": abs_path,
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::FS_FILE_NOT_FOUND
        );
    }

    #[tokio::test]
    async fn write_text_file_saves_content() {
        let (storage, _dir) = make_storage();
        let tool = FileSystemTool::new(storage.clone());

        let abs_path = storage.clone()
            .vault_path()
            .join("output.md")
            .to_string_lossy()
            .to_string();

        let call = ToolCall::with_args(
            "nabu:fs",
            json!({
                "operation": "write_text_file",
                "path": abs_path,
                "content": "# New Note",
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());

        // Verify the content was written.
        let written = std::fs::read_to_string(storage.vault_path().join("output.md")).unwrap();
        assert_eq!(written, "# New Note");
    }

    #[tokio::test]
    async fn fs_methods_reject_outside_vault() {
        let (storage, _dir) = make_storage();
        let tool = FileSystemTool::new(storage);

        let call = ToolCall::with_args(
            "nabu:fs",
            json!({
                "operation": "read_text_file",
                "path": "/etc/passwd",
                "sessionId": "sess_1",
            }),
        );

        let result = tool.call(call).await.unwrap();
        assert!(result.is_error());
        assert_eq!(
            result.error.unwrap().code,
            error_code::FS_PATH_OUTSIDE_VAULT
        );
    }

    #[test]
    fn apply_line_limit_full_content() {
        let content = "line1\nline2\nline3";
        assert_eq!(apply_line_limit(content, None, None), content);
    }

    #[test]
    fn apply_line_limit_with_offset() {
        let content = "line1\nline2\nline3\nline4";
        let result = apply_line_limit(content, Some(3), None);
        assert_eq!(result, "line3\nline4");
    }

    #[test]
    fn apply_line_limit_with_limit() {
        let content = "line1\nline2\nline3\nline4";
        let result = apply_line_limit(content, None, Some(2));
        assert_eq!(result, "line1\nline2");
    }

    #[test]
    fn apply_line_limit_with_offset_and_limit() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let result = apply_line_limit(content, Some(2), Some(2));
        assert_eq!(result, "line2\nline3");
    }

    #[test]
    fn apply_line_limit_offset_beyond_end() {
        let content = "line1\nline2";
        let result = apply_line_limit(content, Some(10), None);
        assert_eq!(result, "");
    }
}
