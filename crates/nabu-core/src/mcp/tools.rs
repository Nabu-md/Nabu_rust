//! # MCP Tools — Nabu Knowledge Operations
//!
//! Implements the Nabu tools exposed through the MCP `tools/list` and
//! `tools/call` methods.  Each tool reuses the existing
//! [`Tool`](crate::tool_calling::Tool) trait and is registered on a
//! [`ToolRegistry`](crate::tool_calling::ToolRegistry) so that the same
//! tool model can serve both ACP and MCP protocols.
//!
//! ## Available Tools
//!
//! | MCP name | Nabu ToolId | Description |
//! |----------|-------------|-------------|
//! | `search_note` | `nabu:search_note` | Full-text search across all notes |
//! | `read_note` | `nabu:read_note` | Read a note by vault-relative path |
//! | `write_note` | `nabu:write_note` | Write content to a note |
//! | `list_notes` | `nabu:list_notes` | List all notes in the vault |

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::indexer::Indexer;
use crate::models::{KnowledgeObject, ObjectContent, ObjectType};
use crate::storage::StorageManager;
use crate::tool_calling::models::ToolExecutionMeta;
use crate::tool_calling::{
    Tool, ToolCall, ToolError, ToolId, ToolParam, ToolParamSchema, ToolResult, ToolSpec,
    ToolRegistry,
};

pub mod error_code {
    pub const NOTE_NOT_FOUND: &str = "NOTE_NOT_FOUND";
    pub const SEARCH_FAILED: &str = "SEARCH_FAILED";
    pub const READ_FAILED: &str = "READ_FAILED";
    pub const WRITE_FAILED: &str = "WRITE_FAILED";
    pub const INVALID_PATH: &str = "INVALID_PATH";
    pub const PATH_OUTSIDE_VAULT: &str = "PATH_OUTSIDE_VAULT";
    pub const MISSING_QUERY: &str = "MISSING_QUERY";
}

fn validate_vault_path(vault_rel: &str) -> Result<(), ToolError> {
    if vault_rel.is_empty() {
        return Err(ToolError::new(
            error_code::INVALID_PATH,
            "path must not be empty",
        ));
    }

    let path = std::path::Path::new(vault_rel);
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(ToolError::new(
                error_code::PATH_OUTSIDE_VAULT,
                format!("path '{}' contains parent-directory traversal", vault_rel),
            ));
        }
    }

    if vault_rel.starts_with(".nabu") || vault_rel.contains("/.nabu") {
        return Err(ToolError::new(
            error_code::INVALID_PATH,
            "access to .nabu internal directory is not permitted",
        ));
    }

    Ok(())
}

fn content_as_str(content: &ObjectContent) -> String {
    match content {
        ObjectContent::Markdown(s)
        | ObjectContent::RichHtml(s)
        | ObjectContent::PlainText(s)
        | ObjectContent::Uri(s) => s.clone(),
        ObjectContent::Binary { filename, .. } => {
            format!("[binary: {}]", filename.as_deref().unwrap_or("unnamed"))
        }
    }
}

fn note_search_hit(obj: &KnowledgeObject, _query: &str) -> Value {
    let body = content_as_str(&obj.content);
    let snippet = if body.len() > 200 {
        format!("{}...", &body[..200])
    } else {
        body
    };
    json!({
        "id": obj.id.to_string(),
        "title": obj.metadata.title.as_deref().unwrap_or(""),
        "path": obj.metadata.vault_path.as_deref().unwrap_or(""),
        "tags": obj.tags,
        "excerpt": snippet,
    })
}

fn exec_meta(id: &str) -> ToolExecutionMeta {
    ToolExecutionMeta::from_duration(
        ToolId::new(id),
        std::time::Duration::from_millis(1),
    )
}

// ---------------------------------------------------------------------------
// SearchNoteTool
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SearchNoteTool {
    indexer: Arc<Indexer>,
    storage: Arc<StorageManager>,
    _vault_root: PathBuf,
}

impl SearchNoteTool {
    pub fn new(indexer: Arc<Indexer>, storage: Arc<StorageManager>) -> Self {
        let _vault_root = storage.vault_path().clone();
        Self {
            indexer,
            storage,
            _vault_root,
        }
    }
}

#[async_trait::async_trait]
impl Tool for SearchNoteTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "nabu:search_note",
            "Search Notes",
            "Search for notes matching a full-text query across the vault.",
        )
        .with_param(ToolParam::required(
            "query",
            ToolParamSchema::of_type("string"),
        ))
    }

    async fn call(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let args = call.arguments.unwrap_or(serde_json::Value::Null);
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if query.is_empty() {
            return Err(ToolError::new(
                error_code::MISSING_QUERY,
                "search query must not be empty",
            ));
        }

        let object_ids = self.indexer.search(query);

        let mut results: Vec<Value> = Vec::new();
        for id_str in &object_ids {
            if let Ok(uuid) = uuid::Uuid::parse_str(id_str) {
                if let Some(obj) = self.storage.load(uuid) {
                    results.push(note_search_hit(&obj, query));
                }
            }
        }

        Ok(ToolResult::success(
            Some(json!({
                "query": query,
                "count": results.len(),
                "hits": results,
            })),
            Some(exec_meta("nabu:search_note")),
        ))
    }
}

// ---------------------------------------------------------------------------
// ReadNoteTool
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ReadNoteTool {
    storage: Arc<StorageManager>,
    _vault_root: PathBuf,
}

impl ReadNoteTool {
    pub fn new(storage: Arc<StorageManager>) -> Self {
        let _vault_root = storage.vault_path().clone();
        Self {
            _vault_root,
            storage,
        }
    }
}

#[async_trait::async_trait]
impl Tool for ReadNoteTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "nabu:read_note",
            "Read Note",
            "Read a note by its vault-relative path. Resolves through Nabu's StorageManager.",
        )
        .with_param(ToolParam::required(
            "path",
            ToolParamSchema::of_type("string"),
        ))
    }

    async fn call(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let args = call.arguments.unwrap_or(serde_json::Value::Null);
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new(error_code::INVALID_PATH, "missing 'path' parameter"))?;

        validate_vault_path(path)?;

        let obj = self
            .storage
            .find_by_path(path)
            .ok_or_else(|| ToolError::new(error_code::NOTE_NOT_FOUND, format!("note not found: {}", path)))?;

        let content = content_as_str(&obj.content);

        Ok(ToolResult::success(
            Some(json!({
                "path": path,
                "title": obj.metadata.title.as_deref().unwrap_or(""),
                "content": content,
                "tags": obj.tags,
                "created_at": obj.created_at.to_rfc3339(),
                "updated_at": obj.updated_at.to_rfc3339(),
                "word_count": obj.count_words(),
            })),
            Some(exec_meta("nabu:read_note")),
        ))
    }
}

// ---------------------------------------------------------------------------
// WriteNoteTool
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct WriteNoteTool {
    storage: Arc<StorageManager>,
}

impl WriteNoteTool {
    pub fn new(storage: Arc<StorageManager>) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl Tool for WriteNoteTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "nabu:write_note",
            "Write Note",
            "Write content to a note at the given vault-relative path. Creates or overwrites the note.",
        )
        .with_param(ToolParam::required(
            "path",
            ToolParamSchema::of_type("string"),
        ))
        .with_param(ToolParam::required(
            "content",
            ToolParamSchema::of_type("string"),
        ))
    }

    async fn call(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let args = call.arguments.unwrap_or(serde_json::Value::Null);

        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new(error_code::INVALID_PATH, "missing 'path' parameter"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new(error_code::INVALID_PATH, "missing 'content' parameter"))?;

        validate_vault_path(path)?;

        let saved_path = self
            .storage
            .save_note_content(path, content)
            .map_err(|e| ToolError::new(error_code::WRITE_FAILED, e))?;

        Ok(ToolResult::success(
            Some(json!({
                "path": saved_path,
                "written": content.len(),
            })),
            Some(exec_meta("nabu:write_note")),
        ))
    }
}

// ---------------------------------------------------------------------------
// ListNotesTool
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ListNotesTool {
    storage: Arc<StorageManager>,
}

impl ListNotesTool {
    pub fn new(storage: Arc<StorageManager>) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl Tool for ListNotesTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "nabu:list_notes",
            "List Notes",
            "List all notes in the vault with summary metadata.",
        )
    }

    async fn call(&self, _call: ToolCall) -> Result<ToolResult, ToolError> {
        let objects = self.storage.load_by_type(ObjectType::Note);
        let notes: Vec<Value> = objects
            .iter()
            .map(|obj| note_search_hit(obj, ""))
            .collect();

        Ok(ToolResult::success(
            Some(json!({
                "count": notes.len(),
                "notes": notes,
            })),
            Some(exec_meta("nabu:list_notes")),
        ))
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub async fn register_nabu_tools(
    registry: &ToolRegistry,
    storage: Arc<StorageManager>,
    indexer: Arc<Indexer>,
) {
    let search_tool: Arc<dyn Tool> = Arc::new(SearchNoteTool::new(indexer, storage.clone()));
    registry.register(search_tool).await;

    let read_tool: Arc<dyn Tool> = Arc::new(ReadNoteTool::new(storage.clone()));
    registry.register(read_tool).await;

    let write_tool: Arc<dyn Tool> = Arc::new(WriteNoteTool::new(storage.clone()));
    registry.register(write_tool).await;

    let list_tool: Arc<dyn Tool> = Arc::new(ListNotesTool::new(storage));
    registry.register(list_tool).await;

    tracing::info!("Registered Nabu MCP tools: search_note, read_note, write_note, list_notes");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ObjectMetadata;
    use crate::tool_calling::models::ToolResultStatus;
    use crate::tool_calling::shared;
    use serde_json::json;
    use tempfile::tempdir;

    fn make_test_env() -> (Arc<StorageManager>, Arc<Indexer>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let storage = Arc::new(StorageManager::new(dir.path()));
        storage.initialize().unwrap();
        storage.start().unwrap();

        let indexer = Arc::new(Indexer::with_vault_path(dir.path()));
        let _ = indexer.initialize();

        (storage, indexer, dir)
    }

    fn make_note(path: &str, content: &str, title: &str) -> KnowledgeObject {
        KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown(content.to_string()))
            .with_metadata(ObjectMetadata {
                vault_path: Some(path.to_string()),
                title: Some(title.to_string()),
                ..Default::default()
            })
    }

    #[tokio::test]
    async fn search_note_tool_spec() {
        let (storage, indexer, _dir) = make_test_env();
        let tool = SearchNoteTool::new(indexer, storage);
        let spec = tool.spec();
        assert_eq!(spec.id, ToolId::new("nabu:search_note"));
        assert_eq!(spec.parameters.len(), 1);
        assert_eq!(spec.parameters[0].name, "query");
        assert!(spec.parameters[0].required);
    }

    #[tokio::test]
    async fn read_note_tool_spec() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = ReadNoteTool::new(storage);
        let spec = tool.spec();
        assert_eq!(spec.id, ToolId::new("nabu:read_note"));
    }

    #[tokio::test]
    async fn write_note_tool_spec() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = WriteNoteTool::new(storage);
        let spec = tool.spec();
        assert_eq!(spec.id, ToolId::new("nabu:write_note"));
        assert_eq!(spec.parameters.len(), 2);
    }

    #[tokio::test]
    async fn list_notes_tool_spec() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = ListNotesTool::new(storage);
        let spec = tool.spec();
        assert_eq!(spec.id, ToolId::new("nabu:list_notes"));
        assert!(spec.parameters.is_empty());
    }

    #[tokio::test]
    async fn read_note_returns_content() {
        let (storage, indexer, _dir) = make_test_env();
        let obj = make_note("test/readme.md", "Hello note", "Readme");
        storage.save(&obj).unwrap();
        indexer.index_object(&obj).unwrap();

        let tool = ReadNoteTool::new(storage);
        let call = ToolCall::with_args(
            "nabu:read_note",
            json!({ "path": "test/readme.md" }),
        );
        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let value = result.result.unwrap();
        assert_eq!(value["content"], json!("Hello note"));
        assert_eq!(value["path"], json!("test/readme.md"));
    }

    #[tokio::test]
    async fn read_note_rejects_traversal() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = ReadNoteTool::new(storage);
        let call = ToolCall::with_args(
            "nabu:read_note",
            json!({ "path": "../secret.md" }),
        );
        let result = tool.call(call).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, error_code::PATH_OUTSIDE_VAULT);
    }

    #[tokio::test]
    async fn read_note_rejects_nabu_dir() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = ReadNoteTool::new(storage);
        let call = ToolCall::with_args(
            "nabu:read_note",
            json!({ "path": ".nabu/evil.json" }),
        );
        let result = tool.call(call).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_note_missing_path_returns_error() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = ReadNoteTool::new(storage);
        let call = ToolCall::without_args("nabu:read_note");
        let result = tool.call(call).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_note_creates_and_overwrites() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = WriteNoteTool::new(storage.clone());

        let call = ToolCall::with_args(
            "nabu:write_note",
            json!({ "path": "test/new-note.md", "content": "# New Note" }),
        );
        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());

        let written = std::fs::read_to_string(storage.vault_path().join("test/new-note.md")).unwrap();
        assert_eq!(written, "# New Note");
    }

    #[tokio::test]
    async fn write_note_rejects_traversal() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool = WriteNoteTool::new(storage);
        let call = ToolCall::with_args(
            "nabu:write_note",
            json!({ "path": "../evil.md", "content": "bad" }),
        );
        let result = tool.call(call).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, error_code::PATH_OUTSIDE_VAULT);
    }

    #[tokio::test]
    async fn search_note_finds_body_match() {
        let (storage, indexer, _dir) = make_test_env();
        let obj = make_note(
            "searchable.md",
            "This body contains a unique bodytoken123 word",
            "Searchable",
        );
        storage.save(&obj).unwrap();
        indexer.index_object(&obj).unwrap();

        let tool = SearchNoteTool::new(indexer, storage);
        let call = ToolCall::with_args(
            "nabu:search_note",
            json!({ "query": "bodytoken123" }),
        );
        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let value = result.result.unwrap();
        assert_eq!(value["count"], json!(1));
        assert_eq!(value["hits"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn search_note_empty_query_errors() {
        let (storage, indexer, _dir) = make_test_env();
        let tool = SearchNoteTool::new(indexer, storage);
        let call = ToolCall::with_args(
            "nabu:search_note",
            json!({ "query": "" }),
        );
        let result = tool.call(call).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, error_code::MISSING_QUERY);
    }

    #[tokio::test]
    async fn list_notes_returns_all_notes() {
        let (storage, _indexer, _dir) = make_test_env();

        let obj1 = make_note("first.md", "First", "First Note");
        let obj2 = make_note("second.md", "Second", "Second Note");
        storage.save(&obj1).unwrap();
        storage.save(&obj2).unwrap();

        let tool = ListNotesTool::new(storage);
        let call = ToolCall::without_args("nabu:list_notes");
        let result = tool.call(call).await.unwrap();
        assert!(result.is_success());
        let value = result.result.unwrap();
        assert_eq!(value["count"], json!(2));
    }

    #[tokio::test]
    async fn register_nabu_tools_registers_all_four() {
        let (storage, indexer, _dir) = make_test_env();
        let registry = ToolRegistry::new();
        register_nabu_tools(&registry, storage, indexer).await;
        assert_eq!(registry.tool_count().await, 4);
        assert!(registry.has_tool("nabu:search_note").await);
        assert!(registry.has_tool("nabu:read_note").await);
        assert!(registry.has_tool("nabu:write_note").await);
        assert!(registry.has_tool("nabu:list_notes").await);
    }

    #[tokio::test]
    async fn registry_dispatch_read_note() {
        let (storage, indexer, _dir) = make_test_env();
        let obj = make_note("dispatched.md", "Content here", "Dispatched");
        storage.save(&obj).unwrap();
        indexer.index_object(&obj).unwrap();

        let registry = ToolRegistry::new();
        register_nabu_tools(&registry, storage, indexer).await;

        let result = registry
            .call(ToolCall::with_args(
                "nabu:read_note",
                json!({ "path": "dispatched.md" }),
            ))
            .await;
        assert!(result.is_success());
        assert_eq!(result.result.unwrap()["content"], json!("Content here"));
    }

    #[tokio::test]
    async fn registry_dispatch_unknown_tool() {
        let (storage, indexer, _dir) = make_test_env();
        let registry = ToolRegistry::new();
        register_nabu_tools(&registry, storage, indexer).await;

        let result = registry
            .call(ToolCall::without_args("nabu:nonexistent"))
            .await;
        assert_eq!(result.status, ToolResultStatus::ToolNotFound);
    }

    #[tokio::test]
    async fn registry_dispatch_missing_required_param() {
        let (storage, indexer, _dir) = make_test_env();
        let registry = ToolRegistry::new();
        register_nabu_tools(&registry, storage, indexer).await;

        let result = registry
            .call(ToolCall::with_args(
                "nabu:write_note",
                json!({ "path": "test.md" }),
            ))
            .await;
        assert_eq!(result.status, ToolResultStatus::InvalidParams);
    }

    #[tokio::test]
    async fn shared_tool_wraps_arc() {
        let (storage, _indexer, _dir) = make_test_env();
        let tool: Arc<dyn Tool + Send + Sync> = shared(Arc::new(ReadNoteTool::new(storage)));
        assert_eq!(tool.id(), ToolId::new("nabu:read_note"));
    }
}
