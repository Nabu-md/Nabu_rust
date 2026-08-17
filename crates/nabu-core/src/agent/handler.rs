//! # Nabu ACP Handler — Application-Layer Implementation of `AcpClientHandler`
//!
//! This module provides [`NabuAcpHandler`], which implements the
//! [`AcpClientHandler`](crate::acp::handler::AcpClientHandler) trait by
//! delegating to Nabu's existing subsystem services:
//!
//! - `fs/read_text_file` → reads from the vault filesystem
//! - `fs/write_text_file` → `StorageManager::save_note_content`
//! - `session/request_permission` → publishes an `AcpPermissionRequested` event
//!   to the EventBus so the frontend can prompt the user, then awaits the
//!   response via a oneshot channel
//!
//! Terminal and elicitation methods use the default trait implementations
//! (which return `UnsupportedOperation`).
//!
//! The handler is `Send + Sync` and designed to be stored as
//! `Arc<NabuAcpHandler>` so it can be shared between the `AcpClient`
//! message-loop task and the Tauri command layer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{oneshot, Mutex as TokioMutex};

use crate::acp::error::{AcpError, ErrorKind};
use crate::acp::handler::AcpClientHandler;
use crate::acp::types::{
    PermissionOutcome, ReadTextFileRequest, ReadTextFileResponse, RequestPermissionRequest,
    WriteTextFileRequest, WriteTextFileResponse,
};
use crate::event_bus::events::AcpPermissionRequestEvent;
use crate::event_bus::{EventBus, PipelineEvent};
use crate::storage::StorageManager;

/// Error returned when a user's permission response cannot be delivered.
#[derive(Debug, thiserror::Error)]
pub enum PermissionDeliveryError {
    #[error("No pending permission request matching this request_id")]
    NoPendingRequest,
    #[error("The handler that issued the request is no longer waiting")]
    HandlerGone,
}

/// Application-layer ACP client handler backed by the vault [`StorageManager`].
///
/// File-system requests are confined to the vault root.  Any path that
/// escapes the vault directory (path traversal) is rejected with
/// `ErrorKind::MalformedRequest`.
pub struct NabuAcpHandler {
    /// The canonical storage manager used to resolve and persist vault file
    /// operations.
    storage: Arc<StorageManager>,
    /// The EventBus — used to publish permission requests so the frontend
    /// can prompt the user.
    event_bus: Arc<EventBus<PipelineEvent>>,
    /// The Nabu thread UUID this handler belongs to, so the frontend can
    /// route permission responses back to the correct session.
    thread_id: uuid::Uuid,
    /// Pending permission requests awaiting a user response, keyed by
    /// `request_id`. The handler publishes a request, then awaits the
    /// corresponding oneshot receiver. A Tauri command delivers the response
    /// by looking up the sender here.
    pending_permissions: Arc<TokioMutex<HashMap<uuid::Uuid, oneshot::Sender<PermissionOutcome>>>>,
}

impl NabuAcpHandler {
    /// Creates a new handler backed by the given storage manager and EventBus.
    pub fn new(
        storage: Arc<StorageManager>,
        event_bus: Arc<EventBus<PipelineEvent>>,
        thread_id: uuid::Uuid,
    ) -> Self {
        Self {
            storage,
            event_bus,
            thread_id,
            pending_permissions: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    /// Delivers a user's permission decision to the handler awaiting it.
    ///
    /// Called by the `acp_permission_respond` Tauri command. Finds the
    /// matching oneshot sender by `request_id` and sends the outcome.
    pub async fn deliver_permission_response(
        &self,
        request_id: uuid::Uuid,
        outcome: PermissionOutcome,
    ) -> Result<(), PermissionDeliveryError> {
        let sender = {
            let mut pending = self.pending_permissions.lock().await;
            pending.remove(&request_id)
        };
        match sender {
            Some(tx) => tx
                .send(outcome)
                .map_err(|_| PermissionDeliveryError::HandlerGone),
            None => Err(PermissionDeliveryError::NoPendingRequest),
        }
    }

    /// The underlying storage manager, exposed so the session manager can
    /// verify handler ownership.
    pub fn thread_id(&self) -> uuid::Uuid {
        self.thread_id
    }

    /// Resolves a user-supplied relative path against the vault root and
    /// rejects path traversal attempts.
    ///
    /// The ACP protocol sends paths as strings that are typically relative to
    /// the session's `cwd`.  We anchor them at the vault root and canonicalise
    /// to ensure they never escape the vault boundary.
    fn resolve_vault_path(&self, path: &str) -> Result<PathBuf, AcpError> {
        let vault_root = self.storage.vault_path();
        let raw = Path::new(path);

        if raw.is_absolute() {
            let Ok(canonical) = raw.canonicalize() else {
                return Err(AcpError::new(
                    ErrorKind::Internal,
                    format!("Unable to canonicalise absolute path: {}", path),
                ));
            };
            let Ok(canonical_vault) = vault_root.canonicalize() else {
                return Err(AcpError::new(
                    ErrorKind::Internal,
                    format!(
                        "Unable to canonicalise vault root: {}",
                        vault_root.display()
                    ),
                ));
            };
            if canonical.starts_with(&canonical_vault) {
                Ok(canonical)
            } else {
                Err(AcpError::new(
                    ErrorKind::MalformedRequest,
                    format!(
                        "Path traversal detected: {} is outside vault directory {}",
                        path,
                        canonical_vault.display()
                    ),
                ))
            }
        } else {
            let resolved = vault_root.join(raw);
            for component in resolved.components() {
                if matches!(component, std::path::Component::ParentDir) {
                    return Err(AcpError::new(
                        ErrorKind::MalformedRequest,
                        format!(
                            "Path traversal detected: {} contains parent-dir reference",
                            path
                        ),
                    ));
                }
            }
            Ok(resolved)
        }
    }

    /// Converts a vault-absolute path back into a vault-relative path string
    /// suitable for `StorageManager::save_note_content`.
    fn to_vault_rel_path(&self, abs: &Path) -> Result<String, AcpError> {
        let rel = abs.strip_prefix(self.storage.vault_path()).map_err(|_| {
            AcpError::new(
                ErrorKind::MalformedRequest,
                format!(
                    "Resolved path {} is not within vault root {}",
                    abs.display(),
                    self.storage.vault_path().display()
                ),
            )
        })?;
        Ok(rel.to_string_lossy().to_string())
    }
}

#[async_trait]
impl AcpClientHandler for NabuAcpHandler {
    async fn read_text_file(
        &self,
        request: &ReadTextFileRequest,
    ) -> Result<ReadTextFileResponse, AcpError> {
        let path = self.resolve_vault_path(&request.path)?;

        let content = std::fs::read_to_string(&path).map_err(|e| {
            AcpError::new(
                ErrorKind::Internal,
                format!("Failed to read file {}: {}", request.path, e),
            )
        })?;

        Ok(ReadTextFileResponse {
            content,
            _meta: None,
        })
    }

    async fn write_text_file(
        &self,
        request: &WriteTextFileRequest,
    ) -> Result<WriteTextFileResponse, AcpError> {
        let abs_path = self.resolve_vault_path(&request.path)?;
        let rel_path = self.to_vault_rel_path(&abs_path)?;

        if let Some(parent) = abs_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AcpError::new(
                        ErrorKind::Internal,
                        format!(
                            "Failed to create parent directory {}: {}",
                            parent.display(),
                            e
                        ),
                    )
                })?;
            }
        }

        self.storage
            .save_note_content(&rel_path, &request.content)
            .map_err(|e| AcpError::new(ErrorKind::Internal, e))?;

        Ok(WriteTextFileResponse { _meta: None })
    }

    async fn request_permission(
        &self,
        request: &RequestPermissionRequest,
    ) -> Result<PermissionOutcome, AcpError> {
        let request_id = uuid::Uuid::new_v4();
        let (tx, rx) = oneshot::channel::<PermissionOutcome>();

        {
            let mut pending = self.pending_permissions.lock().await;
            pending.insert(request_id, tx);
        }

        let event = AcpPermissionRequestEvent {
            request_id,
            thread_id: self.thread_id,
            session_id: request.session_id.clone(),
            tool_call_id: request.tool_call.tool_call_id.clone(),
            tool_call_title: request.tool_call.title.clone(),
            options: request.options.clone(),
            timestamp: chrono::Utc::now(),
        };

        self.event_bus.publish(
            crate::event_bus::kinds::ACP_PERMISSION_REQUESTED,
            &PipelineEvent::AcpPermissionRequested(event),
        );

        match rx.await {
            Ok(outcome) => Ok(outcome),
            Err(_) => Ok(PermissionOutcome::Cancelled {
                _meta: Some(serde_json::json!({
                    "reason": "user response channel dropped"
                })),
            }),
        }
    }
}
