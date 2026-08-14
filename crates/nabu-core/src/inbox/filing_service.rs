//! The inbox filing service.
//!
//! Turns a triaged inbox capture into a durable vault artifact (a real
//! Markdown note for textual content, or a native binary file for images /
//! audio / attachments) and drives its full lifecycle: approve, reject,
//! retry, delete.
//!
//! The service is deliberately *thin*: it consumes the public APIs of
//! [`StorageManager`] (persistence + the `ITEM_STORED` pipeline event) and
//! the [`KnowledgeObject`] model.  It does **not** reach into the `Indexer`
//! or `VaultGraph` internals — indexing of the filed note is handed off
//! through the canonical `ITEM_STORED` event (as wired upstream in the
//! `save_pipeline` subscriber), keeping this service isolated from those
//! modules while still making the note searchable.

use std::sync::Arc;

use uuid::Uuid;

use crate::models::{CustomPropertyValue, KnowledgeObject, ObjectContent, ProcessingState};
use crate::storage::StorageManager;

use super::model::{
    InboxItemStatus, binary_extension, inbox_item_status, render_markdown, resolve_destination,
    set_status, sha256_hex, slugify,
};

// ---------------------------------------------------------------------------
// Public result / error types
// ---------------------------------------------------------------------------

/// Outcome of a filing operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilingStatus {
    /// The item was filed into a real vault artifact.
    Filed,
    /// The item was already filed; the operation was a no-op (idempotent).
    AlreadyFiled,
    /// The item was rejected.
    Rejected,
    /// The item was reset for reprocessing.
    Retried,
}

/// Details returned after a filing operation.
#[derive(Debug, Clone)]
pub struct FilingResult {
    pub object_id: Uuid,
    pub vault_path: String,
    pub status: FilingStatus,
}

/// Errors produced by the filing service.
#[derive(Debug, thiserror::Error)]
pub enum FilingError {
    #[error("inbox item not found: {0}")]
    NotFound(Uuid),
    #[error("storage failure: {0}")]
    Storage(String),
}

// `String` is not `std::error::Error`, so `#[from]` (which would require it
// to be usable as a `dyn Error` source) is not usable here.  A plain `From`
// impl is all the `?` operator needs to convert the `String` errors emitted
// by `StorageManager`.
impl From<String> for FilingError {
    fn from(s: String) -> Self {
        FilingError::Storage(s)
    }
}

// ---------------------------------------------------------------------------
// FilingService
// ---------------------------------------------------------------------------

/// The filing service owns the inbox → vault transition.
///
/// Constructed with the application's shared [`StorageManager`] (which itself
/// carries the canonical [`EventBus`](crate::event_bus::EventBus)), so every
/// `save()` performed here flows through the standard pipeline: persistence
/// → `ITEM_STORED` → indexer/graph subscribers.
pub struct FilingService {
    storage: Arc<StorageManager>,
}

impl FilingService {
    pub fn new(storage: Arc<StorageManager>) -> Self {
        Self { storage }
    }

    /// The underlying storage manager (for tests / introspection).
    pub fn storage(&self) -> &StorageManager {
        &self.storage
    }

    // -----------------------------------------------------------------------
    // Approve / file
    // -----------------------------------------------------------------------

    /// Approve (file) an inbox item by id.
    ///
    /// On success the object is persisted as a real vault artifact — a `.md`
    /// note for textual captures, or its native binary file otherwise — and an
    /// `ITEM_STORED` event is published so the indexer/graph subscribers pick
    /// it up.  The inbox status is advanced to `approved` so the item leaves
    /// the *active* work queue.
    pub fn approve(&self, id: Uuid) -> Result<FilingResult, FilingError> {
        let mut obj = self.storage.load(id).ok_or(FilingError::NotFound(id))?;

        // Idempotent: an already-filed item is left as-is.
        if inbox_item_status(&obj) == InboxItemStatus::Approved
            && obj.metadata.vault_path.is_some()
        {
            return Ok(FilingResult {
                object_id: id,
                vault_path: obj.metadata.vault_path.unwrap_or_default(),
                status: FilingStatus::AlreadyFiled,
            });
        }

        self.file_object(&mut obj)
    }

    /// Core filing logic, factored out from `approve` so the *decision*
    /// (destination + content transform) can be tested without a full
    /// `StorageManager` round-trip.
    pub fn file_object(&self, obj: &mut KnowledgeObject) -> Result<FilingResult, FilingError> {
        let id = obj.id;

        // 1. Resolve the destination folder from the inbox model
        //    (user-chosen > AutoFiler suggestion > "Inbox").
        let folder = resolve_destination(obj);

        // 2. Derive a stable filename slug from the title.
        let title = obj
            .metadata
            .title
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("untitled");
        let slug = slugify(title);
        let name = if slug.is_empty() { "untitled" } else { &slug };

        // 3. Render the capture content as a Markdown note. Text captures
        //    (Markdown / PlainText / Uri / RichHtml) become `.md`; binary
        //    captures are filed as their native file so no data is lost.
        let (content, vault_rel) = match render_markdown(obj) {
            Some(markdown) => {
                let hash = sha256_hex(markdown.as_bytes());
                obj.content_hash = Some(hash);
                (
                    ObjectContent::Markdown(markdown),
                    format!("{folder}/{name}.md"),
                )
            }
            None => {
                // Binary capture — preserve natively.
                let ext = binary_extension(obj);
                let hash = match &obj.content {
                    ObjectContent::Binary { data, .. } => sha256_hex(data),
                    _ => String::new(),
                };
                obj.content_hash = Some(hash);
                (obj.content.clone(), format!("{folder}/{name}.{ext}"))
            }
        };

        // 4. Stamp the object: vault path, content, state, inbox status.
        obj.metadata.vault_path = Some(vault_rel.clone());
        obj.content = content;
        obj.processing_state = ProcessingState::Completed;
        set_status(obj, InboxItemStatus::Approved);

        // 5. Persist: writes the content file + sidecar and publishes
        //    `ITEM_STORED`, which the canonical pipeline subscriber hands off
        //    to the indexer/graph.
        let saved = self.storage.save(obj)?;

        Ok(FilingResult {
            object_id: id,
            vault_path: saved,
            status: FilingStatus::Filed,
        })
    }

    // -----------------------------------------------------------------------
    // Reject
    // -----------------------------------------------------------------------

    /// Reject an inbox item, optionally recording a reason, and hide it from
    /// the *active* work queue (status → `rejected`).
    pub fn reject(
        &self,
        id: Uuid,
        reason: Option<String>,
    ) -> Result<FilingResult, FilingError> {
        let mut obj = self.storage.load(id).ok_or(FilingError::NotFound(id))?;
        set_status(&mut obj, InboxItemStatus::Rejected);
        if let Some(reason) = reason {
            obj.custom_properties.insert(
                "rejection_reason".to_string(),
                CustomPropertyValue::Text(reason),
            );
        }
        let vault_rel = obj.metadata.vault_path.clone().unwrap_or_default();
        self.storage.save(&obj)?;
        Ok(FilingResult {
            object_id: id,
            vault_path: vault_rel,
            status: FilingStatus::Rejected,
        })
    }

    // -----------------------------------------------------------------------
    // Retry / re-queue
    // -----------------------------------------------------------------------

    /// Reset a failed / rejected item back to `pending` so the capture
    /// pipeline (or a human) can retry processing it.
    pub fn retry(&self, id: Uuid) -> Result<FilingResult, FilingError> {
        let mut obj = self.storage.load(id).ok_or(FilingError::NotFound(id))?;
        set_status(&mut obj, InboxItemStatus::Pending);
        obj.processing_state = ProcessingState::Pending;
        // Clear the failure signals so it is eligible for reprocessing.
        obj.custom_properties.remove("processing_warnings");
        obj.custom_properties.remove("classification_confidence");
        let vault_rel = obj.metadata.vault_path.clone().unwrap_or_default();
        self.storage.save(&obj)?;
        Ok(FilingResult {
            object_id: id,
            vault_path: vault_rel,
            status: FilingStatus::Retried,
        })
    }

    // -----------------------------------------------------------------------
    // Delete
    // -----------------------------------------------------------------------

    /// Permanently remove an item from the vault (content + sidecar + cache).
    pub fn delete(&self, id: Uuid) -> Result<(), FilingError> {
        self.storage.delete(id)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read helpers (consumed by the UI / commands)
    // -----------------------------------------------------------------------

    /// All objects carrying an inbox marker, regardless of status (for an
    /// inbox "trash"/history view).
    pub fn all(&self) -> Vec<KnowledgeObject> {
        self.storage
            .list_objects("", None, 10_000)
            .unwrap_or_default()
            .into_iter()
            .filter(|o| is_inbox_item(o))
            .collect()
    }

    /// Only the *active* inbox work items (pending / processing / ready).
    /// Items that have reached a terminal status (approved / rejected /
    /// failed) are intentionally excluded.
    pub fn queue(&self) -> Vec<KnowledgeObject> {
        self.storage
            .list_objects("", None, 10_000)
            .unwrap_or_default()
            .into_iter()
            .filter(|o| is_inbox_item(o) && inbox_item_status(o).is_active())
            .collect()
    }

    /// Load a single inbox item by id.
    pub fn get(&self, id: Uuid) -> Option<KnowledgeObject> {
        self.storage.load(id)
    }
}

/// An object "belongs to the inbox" if any of the inbox-marker custom
/// properties is present.  Mirrors the predicate used by the existing Tauri
/// `inbox_get_queue` command so the service and the UI agree on membership.
fn is_inbox_item(obj: &KnowledgeObject) -> bool {
    obj.custom_properties.contains_key("inbox_status")
        || obj.custom_properties.contains_key("suggested_folder")
        || obj.custom_properties.contains_key("classification")
}

#[cfg(test)]
mod tests {
    use super::{FilingService, FilingStatus};
    use crate::event_bus::kinds::ITEM_STORED;
    use crate::event_bus::{EventBus, PipelineEvent};
    use crate::indexer::Indexer;
    use crate::inbox::model::{InboxItemStatus, build_inbox_object, inbox_item_status, set_status};
    use crate::models::{
        CustomPropertyValue, KnowledgeObject, ObjectContent, ProcessingState,
    };
    use crate::storage::StorageManager;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// Mirror of the canonical `build_pipeline` from
    /// `tests/save_pipeline_integration.rs`: StorageManager + Indexer sharing
    /// an `EventBus` with `ITEM_STORED -> load -> index_object` wired up, so a
    /// `save()` here hands the object off to the indexer through the *existing*
    /// public API boundary (no modification to `indexer.rs`).
    fn build_pipeline(
        vault: PathBuf,
    ) -> (EventBus<PipelineEvent>, Arc<StorageManager>, Arc<Mutex<Indexer>>) {
        let event_bus = EventBus::<PipelineEvent>::new();
        let storage = Arc::new(StorageManager::with_event_bus(vault.clone(), event_bus.clone()));
        let indexer = Arc::new(Mutex::new(Indexer::with_vault_path_and_event_bus(
            vault,
            event_bus.clone(),
        )));

        let storage_sub = storage.clone();
        let indexer_sub = indexer.clone();
        event_bus.subscribe(ITEM_STORED, move |event: &PipelineEvent| {
            if let PipelineEvent::ItemStored(stored) = event {
                if let Some(object) = storage_sub.load(stored.object_id) {
                    if let Ok(idx) = indexer_sub.lock() {
                        let _ = idx.index_object(&object);
                    }
                }
            }
        });

        (event_bus, storage, indexer)
    }

    /// Seed a pending inbox note and return the *id* to approve.
    fn seed_inbox_note(storage: &StorageManager, title: &str, body: &str) -> uuid::Uuid {
        let obj = build_inbox_object(ObjectContent::Markdown(body.to_string()), Some(title));
        let id = obj.id;
        storage.save(&obj).expect("seed save should succeed");
        id
    }

    // -----------------------------------------------------------------------
    // Test 2 — approve creates a real Markdown note
    // -----------------------------------------------------------------------
    #[test]
    fn approve_creates_markdown_note_with_matching_content() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_path_buf();
        let (_bus, storage, _indexer) = build_pipeline(vault.clone());
        storage.start().unwrap();

        let body = "# Project notes from the field\n\nCaptured via clipboard.";
        let id = seed_inbox_note(&storage, "Field Notes", body);

        let result = FilingService::new(storage.clone())
            .approve(id)
            .expect("approve should succeed");

        assert_eq!(result.status, FilingStatus::Filed);
        assert_eq!(result.object_id, id);

        let vault_rel = &result.vault_path;
        assert!(
            vault_rel.ends_with(".md"),
            "filed text capture must be a .md note, got: {vault_rel}"
        );

        // The file on disk must contain exactly the captured content.
        let on_disk = std::fs::read_to_string(vault.join(vault_rel)).unwrap();
        assert_eq!(on_disk, body, "filed .md content must match the capture");

        // The object on disk must be marked processed / filed.
        let filed = storage.load(id).expect("filed object must still load");
        assert_eq!(
            inbox_item_status(&filed),
            InboxItemStatus::Approved,
            "approve must mark the item as approved (processed)"
        );
        assert_eq!(
            filed.processing_state,
            ProcessingState::Completed,
            "filing must complete processing state"
        );
        assert!(filed.content_hash.is_some(), "filed object must carry a content hash");
        assert_eq!(filed.content, ObjectContent::Markdown(body.to_string()));

        // The filed item has left the *active* inbox queue.
        let service = FilingService::new(storage.clone());
        let queued: Vec<_> = service.queue().into_iter().map(|o| o.id).collect();
        assert!(
            !queued.contains(&id),
            "filed item must leave the active inbox queue"
        );
    }

    // -----------------------------------------------------------------------
    // Test 3 — approved note becomes indexable (existing Indexer boundary)
    // -----------------------------------------------------------------------
    #[test]
    fn approve_hands_filed_note_off_to_indexer_for_search() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_path_buf();
        let (_bus, storage, indexer) = build_pipeline(vault);
        storage.start().unwrap();

        let body = "The quick brown fox jumps over the lazy dog";
        let id = seed_inbox_note(&storage, "Fox Note", body);

        FilingService::new(storage.clone())
            .approve(id)
            .expect("approve should succeed");

        // The ITEM_STORED event published by save() handed the object to the
        // indexer via the canonical subscriber; verify through the stable
        // `search` API (no modification to indexer.rs).
        let idx = indexer.lock().unwrap();
        let results = idx.search("brown");
        assert!(
            results.contains(&id.to_string()),
            "filed note must be searchable by its body content; got {results:?}"
        );
        assert!(
            idx.search("platypus").is_empty(),
            "an unrelated query must not match"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4 — reject / delete leave the active inbox
    // -----------------------------------------------------------------------
    #[test]
    fn reject_marks_item_rejected_and_removes_from_active_queue() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_path_buf();
        let (_bus, storage, _indexer) = build_pipeline(vault);
        storage.start().unwrap();

        let id = seed_inbox_note(&storage, "Reject Me", "some body");
        let service = FilingService::new(storage.clone());

        let result = service
            .reject(id, Some("not relevant".to_string()))
            .expect("reject should succeed");
        assert_eq!(result.status, FilingStatus::Rejected);

        let rejected = storage.load(id).unwrap();
        assert_eq!(inbox_item_status(&rejected), InboxItemStatus::Rejected);

        // Rejected is terminal → not active → absent from the active queue.
        let active: Vec<_> = service.queue().into_iter().map(|o| o.id).collect();
        assert!(!active.contains(&id), "rejected item must not be active");

        // Rejection is not deletion: the item is still retrievable.
        assert!(service.get(id).is_some());
        let rej = service.get(id).unwrap();
        assert_eq!(
            rej.custom_property_text("rejection_reason").as_deref(),
            Some("not relevant")
        );
    }

    #[test]
    fn delete_removes_item_entirely() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_path_buf();
        let (_bus, storage, _indexer) = build_pipeline(vault.clone());
        storage.start().unwrap();

        let id = seed_inbox_note(&storage, "Delete Me", "bye");
        let service = FilingService::new(storage.clone());

        // Persisted before deletion: cache + sidecar + content file.
        assert!(storage.exists(id));
        let sidecar = vault.join(".nabu").join(format!("{id}.json"));
        let content = vault.join("Inbox").join("delete-me.md");
        assert!(sidecar.exists(), "sidecar should exist before delete");
        assert!(content.exists(), "content file should exist before delete");

        service.delete(id).expect("delete should succeed");

        // Gone from storage, cache, AND disk.
        assert!(!storage.exists(id), "deleted object must vanish from storage");
        assert!(service.get(id).is_none(), "deleted object must not load");
        assert!(!sidecar.exists(), "sidecar must be removed on delete");
        assert!(!content.exists(), "content file must be removed on delete");
    }

    // -----------------------------------------------------------------------
    // Test 5 — retry restores a retriable state
    // -----------------------------------------------------------------------
    #[test]
    fn retry_returns_failed_item_to_pending() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_path_buf();
        let (_bus, storage, _indexer) = build_pipeline(vault);
        storage.start().unwrap();

        let id = seed_inbox_note(&storage, "Retry Me", "content");

        // Simulate a failed capture: status=failed + processing warnings.
        {
            let mut obj = storage.load(id).unwrap();
            set_status(&mut obj, InboxItemStatus::Failed);
            obj.processing_state = ProcessingState::Failed("ocr failed".to_string());
            obj.custom_properties.insert(
                "processing_warnings".to_string(),
                CustomPropertyValue::Text("low light".to_string()),
            );
            obj.custom_properties
                .insert("classification_confidence".to_string(), CustomPropertyValue::Number(0.2));
            storage.save(&obj).unwrap();
        }

        let service = FilingService::new(storage.clone());
        let result = service.retry(id).expect("retry should succeed");
        assert_eq!(result.status, FilingStatus::Retried);

        let retried = storage.load(id).unwrap();
        assert_eq!(
            inbox_item_status(&retried),
            InboxItemStatus::Pending,
            "retry must reset status to pending"
        );
        assert_eq!(
            retried.processing_state,
            ProcessingState::Pending,
            "retry must reset processing state to pending"
        );
        assert!(
            !retried.custom_properties.contains_key("processing_warnings"),
            "retry must clear stale warnings"
        );

        // And it is active again.
        let active: Vec<_> = service.queue().into_iter().map(|o| o.id).collect();
        assert!(active.contains(&id), "retried item must rejoin the active queue");
    }

    // -----------------------------------------------------------------------
    // Queue filtering
    // -----------------------------------------------------------------------
    #[test]
    fn queue_only_returns_active_items() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_path_buf();
        let (_bus, storage, _indexer) = build_pipeline(vault);
        storage.start().unwrap();

        let pending = seed_inbox_note(&storage, "Pending A", "a");
        let rejected = seed_inbox_note(&storage, "Rejected", "b");
        let failed = seed_inbox_note(&storage, "Failed", "c");
        let service = FilingService::new(storage.clone());

        // Two into terminal states.
        service.reject(rejected, None).unwrap();
        {
            let mut obj = storage.load(failed).unwrap();
            set_status(&mut obj, InboxItemStatus::Failed);
            obj.processing_state = ProcessingState::Failed("err".to_string());
            storage.save(&obj).unwrap();
        }

        let active: Vec<_> = service.queue().into_iter().map(|o| o.id).collect();
        assert!(
            active.contains(&pending),
            "pending item must be active"
        );
        assert!(
            !active.contains(&rejected) && !active.contains(&failed),
            "terminal items must be excluded from the active queue"
        );
        assert!(
            service.all().len() >= 3,
            "all() must report every inbox-marked object (got {})",
            service.all().len()
        );
    }

    // -----------------------------------------------------------------------
    // Idempotency
    // -----------------------------------------------------------------------
    #[test]
    fn approve_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_path_buf();
        let (_bus, storage, _indexer) = build_pipeline(vault);
        storage.start().unwrap();

        let id = seed_inbox_note(&storage, "Idempotent", "body");
        let service = FilingService::new(storage.clone());

        let first = service.approve(id).unwrap();
        assert_eq!(first.status, FilingStatus::Filed);

        // A second approve must be a no-op.
        let second = service.approve(id).unwrap();
        assert_eq!(second.status, FilingStatus::AlreadyFiled);
    }
}
