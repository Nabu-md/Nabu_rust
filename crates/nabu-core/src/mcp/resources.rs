//! # MCP Resource Provider
//!
//! Exposes Nabu's knowledge base as MCP static and dynamic resources.
//!
//! ## Resource URIs
//!
//! | URI | Description |
//! |-----|-------------|
//! | `notes://all` | Summary listing of all notes in the vault |
//! | `note://{vault_path}` | Content of a single note by vault-relative path |
//! | `vault://info` | Vault-level metadata (path, counts, etc.) |
//!
//! Resources are derived from Nabu's existing [`StorageManager`] and
//! [`VaultGraph`] — no direct filesystem access bypasses the data-access layer.

use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::models::{KnowledgeObject, ObjectContent, ObjectType};
use crate::storage::StorageManager;

use super::error::{McpError, McpErrorKind};
use super::protocol::{
    ListResourcesResult, ReadResourceParams, ReadResourceResult, Resource, ResourceContents,
};

/// A registry of statically-known resource templates plus a dynamic reader
/// that resolves individual URIs against the vault.
pub struct ResourceProvider {
    storage: Arc<StorageManager>,
    vault_root: PathBuf,
}

impl ResourceProvider {
    pub fn new(storage: Arc<StorageManager>) -> Self {
        let vault_root = storage.vault_path().clone();
        Self {
            storage,
            vault_root,
        }
    }

    /// Returns the storage manager backing this provider.
    pub fn storage(&self) -> &Arc<StorageManager> {
        &self.storage
    }

    /// Returns the vault root path.
    pub fn vault_root(&self) -> &Path {
        &self.vault_root
    }

    /// Validate that a note path is safe — vault-relative, no traversal.
    fn validate_note_path(&self, vault_rel: &str) -> Result<(), McpError> {
        let path = Path::new(vault_rel);

        // Reject path traversal.
        for component in path.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(McpError::new(
                    McpErrorKind::InvalidParams,
                    format!("path '{}' contains parent-directory traversal", vault_rel),
                ));
            }
        }

        // Reject hidden files and the .nabu internal directory.
        if vault_rel.starts_with(".nabu") || vault_rel.contains("/.nabu") {
            return Err(McpError::new(
                McpErrorKind::InvalidParams,
                "access to .nabu internal directory is not permitted",
            ));
        }

        Ok(())
    }

    /// List all static resource templates available on this server.
    pub fn list_resources(&self) -> ListResourcesResult {
        let resources = vec![
            Resource {
                uri: "notes://all".to_string(),
                name: "All Notes".to_string(),
                description: Some("Complete summary of every note in the vault".to_string()),
                mime_type: Some("application/json".to_string()),
                annotations: None,
                _meta: None,
            },
            Resource {
                uri: "vault://info".to_string(),
                name: "Vault Info".to_string(),
                description: Some("Vault-level metadata".to_string()),
                mime_type: Some("application/json".to_string()),
                annotations: None,
                _meta: None,
            },
        ];

        ListResourcesResult {
            resources,
            next_cursor: None,
        }
    }

    /// Read a resource by URI.
    pub fn read_resource(
        &self,
        params: &ReadResourceParams,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = &params.uri;

        if uri == "notes://all" {
            let objects = self.storage.load_by_type(ObjectType::Note);
            let mut notes: Vec<serde_json::Value> = Vec::new();
            for obj in &objects {
                notes.push(note_summary(obj));
            }
            let text =
                serde_json::to_string_pretty(&json!({ "count": notes.len(), "notes": notes }))
                    .unwrap_or_else(|_| "{}".to_string());
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents {
                    uri: uri.clone(),
                    text: Some(text),
                    mime_type: Some("application/json".to_string()),
                    blob: None,
                    _meta: None,
                }],
            });
        }

        if uri == "vault://info" {
            let info = json!({
                "vault_path": self.vault_root.to_string_lossy(),
                "note_count": self.storage.load_by_type(ObjectType::Note).len(),
                "total_objects": self.storage.count(),
            });
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents {
                    uri: uri.clone(),
                    text: Some(serde_json::to_string_pretty(&info).unwrap_or_default()),
                    mime_type: Some("application/json".to_string()),
                    blob: None,
                    _meta: None,
                }],
            });
        }

        // Dynamic: note://{vault_path}
        if let Some(vault_rel) = uri.strip_prefix("note://") {
            self.validate_note_path(vault_rel)?;
            let object = self
                .storage
                .find_by_path(vault_rel)
                .ok_or_else(|| McpError::resource_not_found(uri))?;
            let content = content_as_text(&object.content);
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents {
                    uri: uri.clone(),
                    text: Some(content),
                    mime_type: Some(object.content.content_type_hint().to_string()),
                    blob: None,
                    _meta: None,
                }],
            });
        }

        Err(McpError::resource_not_found(uri))
    }
}

fn note_summary(obj: &KnowledgeObject) -> serde_json::Value {
    json!({
        "id": obj.id.to_string(),
        "title": obj.metadata.title.as_deref().unwrap_or(""),
        "path": obj.metadata.vault_path.as_deref().unwrap_or(""),
        "tags": obj.tags,
        "created_at": obj.created_at.to_rfc3339(),
        "updated_at": obj.updated_at.to_rfc3339(),
        "word_count": obj.count_words(),
    })
}

fn content_as_text(content: &ObjectContent) -> String {
    match content {
        ObjectContent::Markdown(s)
        | ObjectContent::RichHtml(s)
        | ObjectContent::PlainText(s)
        | ObjectContent::Uri(s) => s.clone(),
        ObjectContent::Binary { filename, .. } => {
            format!(
                "[binary content: {}]",
                filename.as_deref().unwrap_or("unnamed")
            )
        }
    }
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

    fn make_note(path: &str, content: &str) -> KnowledgeObject {
        KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown(content.to_string()),
        )
        .with_metadata(ObjectMetadata {
            vault_path: Some(path.to_string()),
            title: Some("Test Note".to_string()),
            ..Default::default()
        })
    }

    #[test]
    fn list_resources_returns_static_entries() {
        let (storage, _dir) = make_storage();
        let provider = ResourceProvider::new(storage);
        let result = provider.list_resources();
        assert_eq!(result.resources.len(), 2);
        let uris: Vec<&str> = result.resources.iter().map(|r| r.uri.as_str()).collect();
        assert!(uris.contains(&"notes://all"));
        assert!(uris.contains(&"vault://info"));
    }

    #[test]
    fn read_notes_all_returns_json() {
        let (storage, _dir) = make_storage();
        let obj = make_note("Inbox/test.md", "Hello world");
        storage.save(&obj).unwrap();

        let provider = ResourceProvider::new(storage);
        let result = provider
            .read_resource(&ReadResourceParams {
                uri: "notes://all".to_string(),
                _meta: None,
            })
            .unwrap();

        assert_eq!(result.contents.len(), 1);
        assert_eq!(
            result.contents[0].mime_type.as_deref(),
            Some("application/json")
        );
        let text = result.contents[0].text.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["notes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn read_vault_info_returns_metadata() {
        let (storage, _dir) = make_storage();
        let provider = ResourceProvider::new(storage);
        let result = provider
            .read_resource(&ReadResourceParams {
                uri: "vault://info".to_string(),
                _meta: None,
            })
            .unwrap();

        let parsed: serde_json::Value =
            serde_json::from_str(result.contents[0].text.as_ref().unwrap()).unwrap();
        assert!(parsed["vault_path"].is_string());
        assert_eq!(parsed["note_count"], 0);
    }

    #[test]
    fn read_note_by_uri_returns_content() {
        let (storage, _dir) = make_storage();
        let obj = make_note("Inbox/readme.md", "# Hello");
        storage.save(&obj).unwrap();

        let provider = ResourceProvider::new(storage);
        let result = provider
            .read_resource(&ReadResourceParams {
                uri: "note://Inbox/readme.md".to_string(),
                _meta: None,
            })
            .unwrap();

        assert_eq!(result.contents[0].text.as_deref(), Some("# Hello"));
        assert_eq!(
            result.contents[0].mime_type.as_deref(),
            Some("text/markdown")
        );
    }

    #[test]
    fn read_unknown_resource_returns_error() {
        let (storage, _dir) = make_storage();
        let provider = ResourceProvider::new(storage);
        let err = provider
            .read_resource(&ReadResourceParams {
                uri: "unknown://foo".to_string(),
                _meta: None,
            })
            .unwrap_err();
        assert_eq!(err.kind, McpErrorKind::ResourceNotFound);
    }

    #[test]
    fn read_note_nonexistent_returns_error() {
        let (storage, _dir) = make_storage();
        let provider = ResourceProvider::new(storage);
        let err = provider
            .read_resource(&ReadResourceParams {
                uri: "note://does/not/exist.md".to_string(),
                _meta: None,
            })
            .unwrap_err();
        assert_eq!(err.kind, McpErrorKind::ResourceNotFound);
    }

    #[test]
    fn validate_rejects_traversal() {
        let (storage, _dir) = make_storage();
        let provider = ResourceProvider::new(storage);
        assert!(provider.validate_note_path("foo/../bar").is_err());
    }

    #[test]
    fn validate_rejects_hidden_nabu_dir() {
        let (storage, _dir) = make_storage();
        let provider = ResourceProvider::new(storage);
        assert!(provider.validate_note_path(".nabu/something").is_err());
    }

    #[test]
    fn validate_accepts_normal_path() {
        let (storage, _dir) = make_storage();
        let provider = ResourceProvider::new(storage);
        assert!(provider.validate_note_path("Inbox/hello.md").is_ok());
    }
}
