use crate::event_bus::kinds::INDEX_UPDATED;
use crate::event_bus::{EventBus, IndexOperation, IndexUpdatedEvent, PipelineEvent};
use crate::models::{KnowledgeObject, ObjectContent};
use crate::registry::lifecycle::{Lifecycle, LifecycleManager, LifecycleStage};
use crate::registry::metrics::{CounterMetric, GaugeMetric, MetricsAggregator, ServiceMetrics};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use uuid::Uuid;

/// Sub-directory under the vault root where the Indexer persists its
/// inverted index between sessions.
const INDEX_DIR_NAME: &str = ".nabu";

/// File name for the JSON-serialized inverted index.
const INDEX_FILE_NAME: &str = "search_index.json";

/// The Indexer is the SINGLE search index for the Nabu platform.
///
/// No duplicate indexing systems exist.
/// All search operations go through the Indexer.
///
/// The inverted index is kept in memory for fast queries and is also
/// persisted to disk (JSON, under `.nabu/search_index.json`) so that
/// the index survives application restarts.  `load()` restores a
/// previously persisted index; `persist()` flushes the current in-memory
/// index to disk.
pub struct Indexer {
    index: RwLock<HashMap<String, Vec<String>>>,
    event_bus: Option<EventBus<PipelineEvent>>,
    vault_path: Option<PathBuf>,
    /// Lifecycle state manager — tracks Created -> Initialized -> Running -> Shutdown.
    lifecycle: LifecycleManager,
}

impl Indexer {
    pub fn new() -> Self {
        Self {
            index: RwLock::new(HashMap::new()),
            event_bus: None,
            vault_path: None,
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Create an Indexer rooted at the given vault path so that the
    /// inverted index can be persisted to and loaded from disk.
    pub fn with_vault_path(vault_path: impl Into<PathBuf>) -> Self {
        Self {
            index: RwLock::new(HashMap::new()),
            event_bus: None,
            vault_path: Some(vault_path.into()),
            lifecycle: LifecycleManager::new(),
        }
    }

    pub fn with_event_bus(event_bus: EventBus<PipelineEvent>) -> Self {
        Self {
            index: RwLock::new(HashMap::new()),
            event_bus: Some(event_bus),
            vault_path: None,
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Create an Indexer with both a vault path (for persistence) and an
    /// event bus (for publishing index-updated events).
    pub fn with_vault_path_and_event_bus(
        vault_path: impl Into<PathBuf>,
        event_bus: EventBus<PipelineEvent>,
    ) -> Self {
        Self {
            index: RwLock::new(HashMap::new()),
            event_bus: Some(event_bus),
            vault_path: Some(vault_path.into()),
            lifecycle: LifecycleManager::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle state accessors
    // -----------------------------------------------------------------------

    /// Returns the current lifecycle stage of the indexer.
    pub fn lifecycle_stage(&self) -> LifecycleStage {
        self.lifecycle.stage()
    }

    /// Returns true if the indexer has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.lifecycle.is_at_least(LifecycleStage::Initialized)
    }

    /// Returns true if the indexer is running.
    pub fn is_running(&self) -> bool {
        self.lifecycle.is_running()
    }

    /// Returns true if the indexer has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.lifecycle.is_shutdown()
    }

    // -----------------------------------------------------------------------
    // Lifecycle operations
    // -----------------------------------------------------------------------

    /// Initializes the Indexer.
    ///
    /// Lifecycle transition: Created -> Initialized.
    ///
    /// - Prepares the search index structures.
    /// - Loads persisted index metadata from disk (when a vault path is
    ///   configured) so the in-memory index reflects on-disk state.
    /// - Initializes caches.
    pub fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "indexer",
            component = "indexer",
            operation = "initialize",
            "Initializing Indexer"
        );
        // Load persisted index from disk if a vault path is configured.
        self.load()?;
        self.lifecycle.transition_to(LifecycleStage::Initialized)?;
        tracing::info!(
            subsystem = "indexer",
            component = "indexer",
            operation = "initialize",
            "Indexer initialized"
        );
        Ok(())
    }

    /// Starts the Indexer.
    ///
    /// Lifecycle transition: Initialized -> Running (or auto-advances from
    /// Created).
    ///
    /// After starting, the indexer begins accepting indexing requests and
    /// subscribes to document events via the EventBus.
    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.lifecycle.is_shutdown() {
            return Err("Indexer has been shut down and cannot be restarted".into());
        }
        if self.lifecycle.stage() == LifecycleStage::Created {
            self.lifecycle.transition_to(LifecycleStage::Initialized)?;
        }
        self.lifecycle.transition_to(LifecycleStage::Running)?;
        tracing::info!(
            subsystem = "indexer",
            component = "indexer",
            operation = "start",
            "Indexer started"
        );
        Ok(())
    }

    /// Shuts down the Indexer gracefully.
    ///
    /// Lifecycle transition: Running -> Shutdown (or Initialized -> Shutdown).
    ///
    /// - Flushes pending index operations to disk.
    /// - Releases caches and index resources.
    /// - Terminates cleanly.
    pub fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "indexer",
            component = "indexer",
            operation = "shutdown",
            "Shutting down Indexer"
        );
        // Flush the in-memory index to disk before shutting down.
        let _ = self.persist();
        tracing::info!(
            subsystem = "indexer",
            component = "indexer",
            operation = "shutdown",
            "Indexer shutdown complete"
        );
        self.lifecycle.transition_to(LifecycleStage::Shutdown)?;
        Ok(())
    }

    /// The vault path, if configured.
    pub fn vault_path(&self) -> Option<&Path> {
        self.vault_path.as_deref()
    }

    /// Resolve the on-disk path of the persisted inverted index.
    fn index_file_path(&self) -> Option<PathBuf> {
        self.vault_path
            .as_ref()
            .map(|p| p.join(INDEX_DIR_NAME).join(INDEX_FILE_NAME))
    }

    /// Ensure the `.nabu` directory exists so the index file can be written.
    fn ensure_dir(&self) -> Result<(), String> {
        if let Some(path) = self.index_file_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    /// Persist the in-memory inverted index to disk as JSON.
    ///
    /// This is the bridge between the in-memory index and durable
    /// storage.  When a vault path is configured the index is written to
    /// `.nabu/search_index.json` inside the vault root.
    pub fn persist(&self) -> Result<(), String> {
        let path = self
            .index_file_path()
            .ok_or_else(|| "Indexer has no vault path configured".to_string())?;
        self.ensure_dir()?;

        let snapshot: HashMap<String, Vec<String>> = {
            let index = self.index.read().map_err(|e| e.to_string())?;
            index.clone()
        };

        let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load a previously persisted inverted index from disk, replacing
    /// the in-memory index.  If no vault path is configured or the index
    /// file does not yet exist this is a no-op.
    pub fn load(&self) -> Result<(), String> {
        let path = match self.index_file_path() {
            Some(p) => p,
            None => return Ok(()),
        };

        if !path.exists() {
            return Ok(());
        }

        let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let snapshot: HashMap<String, Vec<String>> =
            serde_json::from_str(&json).map_err(|e| e.to_string())?;

        let mut index = self.index.write().map_err(|e| e.to_string())?;
        *index = snapshot;
        Ok(())
    }

    /// Index (add or update) a `KnowledgeObject` for search.
    ///
    /// This is an upsert: any pre-existing postings for `object.id` are
    /// purged first, so re-indexing an object that was already indexed
    /// replaces rather than duplicates its entries. There is deliberately no
    /// separate stale-entry reconciliation subsystem around the index —
    /// correctness comes from remove-then-replace on every write, which is
    /// exactly the add/update semantics Phase 1B expects.
    pub fn index_object(&self, object: &KnowledgeObject) -> Result<(), String> {
        let mut index = self.index.write().map_err(|e| e.to_string())?;
        let object_id = object.id.to_string();

        // Evict any existing postings for this object so re-indexing never
        // accumulates stale duplicate entries (add/update semantics).
        let mut existed = false;
        for posting in index.values_mut() {
            if posting.iter().any(|id| id == &object_id) {
                existed = true;
            }
            posting.retain(|id| id != &object_id);
        }
        // Drop tokens that no longer reference any object so the on-disk
        // index stays compact instead of being papered over at query time.
        index.retain(|_, ids| !ids.is_empty());

        // Tokenize body + metadata and insert fresh postings. Tokens are
        // de-duplicated before insertion so a word shared between the title
        // and the body yields a single posting, not two.
        let tokens: HashSet<String> = tokenize_object(object).into_iter().collect();
        for token in &tokens {
            index
                .entry(token.clone())
                .or_default()
                .push(object_id.clone());
        }

        // Publish index updated event
        if let Some(ref bus) = self.event_bus {
            let operation = if existed {
                IndexOperation::Updated
            } else {
                IndexOperation::Added
            };
            bus.publish(
                INDEX_UPDATED,
                &PipelineEvent::IndexUpdated(IndexUpdatedEvent {
                    object_id: object.id,
                    operation,
                    timestamp: chrono::Utc::now(),
                }),
            );
        }

        Ok(())
    }

    /// Rebuild the index from scratch over a fresh set of objects.
    ///
    /// Clears every posting and then indexes each supplied object. This is the
    /// canonical "reindex" operation used after a full vault rescan. The
    /// in-memory index is replaced with the new set; call [`persist`](Self::persist)
    /// afterwards to durably store the result.
    pub fn reindex(&self, objects: &[KnowledgeObject]) -> Result<(), String> {
        self.clear()?;
        for obj in objects {
            self.index_object(obj)?;
        }
        Ok(())
    }

    /// Remove an object from the index.
    pub fn remove_object(&self, object_id: Uuid) -> Result<(), String> {
        let mut index = self.index.write().map_err(|e| e.to_string())?;

        index.retain(|_, ids| {
            ids.retain(|id| id != &object_id.to_string());
            !ids.is_empty()
        });

        Ok(())
    }

    /// Search the index for objects matching `query`.
    ///
    /// Stable query API consumed by Phase 1B. Returns the object IDs of
    /// every note whose indexed body/metadata contains at least one query
    /// token, using the same tokenisation rules as indexing (lower-cased,
    /// ASCII-punctuation-trimmed, >=3 chars). Multi-token queries use OR
    /// semantics; results are sorted and de-duplicated.
    pub fn search(&self, query: &str) -> Vec<String> {
        let index = match self.index.read() {
            Ok(i) => i,
            Err(_) => return Vec::new(),
        };

        let mut results: Vec<String> = Vec::new();

        // Tokenise the query with the same rules used at index time so
        // punctuation/whitespace differences don't cause missed matches for
        // body-only terms.
        let query_tokens: HashSet<String> = tokenize_str(query).into_iter().collect();

        for token in &query_tokens {
            if let Some(ids) = index.get(token) {
                results.extend(ids.clone());
            }
        }

        results.sort();
        results.dedup();
        results
    }

    /// Number of unique tokens in the index.
    pub fn token_count(&self) -> usize {
        self.index.read().map(|i| i.len()).unwrap_or(0)
    }

    /// Clear the entire index (for rebuild).
    pub fn clear(&self) -> Result<(), String> {
        let mut index = self.index.write().map_err(|e| e.to_string())?;
        index.clear();
        Ok(())
    }
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Lifecycle trait implementation
// ---------------------------------------------------------------------------

/// Implements the shared Lifecycle trait so Indexer can be managed
/// by the Capability Platform's lifecycle manager alongside other services.
///
/// The trait methods delegate to the inherent initialize() / start() /
/// shutdown() methods defined above.
impl Lifecycle for Indexer {
    fn name(&self) -> &'static str {
        "indexer"
    }

    fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        Indexer::initialize(self)
    }

    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        Indexer::start(self)
    }

    fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        Indexer::shutdown(self)
    }
}

impl MetricsAggregator for Indexer {
    fn metrics(&self) -> ServiceMetrics {
        let token_count = self.token_count() as i64;

        ServiceMetrics {
            service: "indexer".to_string(),
            timers: Vec::new(),
            counters: vec![CounterMetric {
                key: "indexer.documents_indexed".to_string(),
                value: token_count as u64,
            }],
            gauges: vec![GaugeMetric {
                key: "indexer.token_count".to_string(),
                value: token_count,
            }],
        }
    }
}

fn tokenize_object(object: &KnowledgeObject) -> Vec<String> {
    let mut tokens = Vec::new();

    // From title
    if let Some(title) = &object.metadata.title {
        tokens.extend(tokenize_str(title));
    }

    // From description
    if let Some(desc) = &object.metadata.description {
        tokens.extend(tokenize_str(desc));
    }

    // From the note body / primary content. This is what turns the Indexer
    // into a real full-text index: a unique word that appears only in the
    // body must be findable through `search`.
    tokens.extend(tokenize_content(&object.content));

    // From tags
    for tag in &object.tags {
        tokens.push(tag.to_lowercase());
    }

    // From content type
    tokens.push(object.object_type.variant_name().to_string());

    tokens
}

/// Extract the searchable text from a `KnowledgeObject`'s primary content.
///
/// Text-bearing variants are tokenised in full; `RichHtml` is tag-stripped
/// first so only visible text is indexed; for `Binary` content there is no
/// text body, so only the filename is indexed.
fn tokenize_content(content: &ObjectContent) -> Vec<String> {
    match content {
        ObjectContent::RichHtml(s) => tokenize_str(&strip_html_tags(s)),
        ObjectContent::Markdown(s) | ObjectContent::PlainText(s) | ObjectContent::Uri(s) => {
            tokenize_str(s)
        }
        ObjectContent::Binary { filename, .. } => {
            filename.as_deref().map(tokenize_str).unwrap_or_default()
        }
    }
}

/// Strip HTML tags from `html`, returning the visible text.
///
/// A small, dependency-free scan so that `RichHtml` article bodies are
/// indexed as text rather than as tag noise.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn tokenize_str(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .filter(|s| s.len() > 2) // Skip very short tokens
        .map(|s| {
            s.trim_matches(|c: char| c.is_ascii_punctuation())
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ObjectContent, ObjectMetadata, ObjectType};

    #[test]
    fn test_index_and_search() {
        let indexer = Indexer::new();
        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("Hello world".to_string()),
        )
        .with_metadata(ObjectMetadata {
            title: Some("Test Note".to_string()),
            ..Default::default()
        });

        indexer.index_object(&obj).unwrap();

        // Title token
        let results = indexer.search("test");
        assert!(
            results.contains(&obj.id.to_string()),
            "Should find object by title token"
        );

        // Body-only token (was not indexed before body support landed)
        let results = indexer.search("world");
        assert!(
            results.contains(&obj.id.to_string()),
            "Should find object by body token"
        );

        let results = indexer.search("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_body_indexing() {
        // A unique term present ONLY in the note body (never in title, tags,
        // or type) must be findable through the query API.
        let indexer = Indexer::new();
        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("This body contains a unique bodytoken123 word".to_string()),
        )
        .with_metadata(ObjectMetadata {
            title: Some("Some Title".to_string()),
            ..Default::default()
        });

        indexer.index_object(&obj).unwrap();

        let results = indexer.search("bodytoken123");
        assert!(
            results.contains(&obj.id.to_string()),
            "Should find object by a term present only in the body"
        );
    }

    #[test]
    fn test_update_replaces_stale_tokens() {
        let indexer = Indexer::new();

        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("Body with oldtoken999 content".to_string()),
        );
        indexer.index_object(&obj).unwrap();
        assert!(
            indexer.search("oldtoken999").contains(&obj.id.to_string()),
            "Old body term should be indexed"
        );

        // Re-index the SAME object (same id) with updated body content.
        let obj_updated = KnowledgeObject {
            id: obj.id,
            content: ObjectContent::Markdown("Body with newtoken111 content".to_string()),
            ..obj.clone()
        };
        indexer.index_object(&obj_updated).unwrap();

        // The old term must no longer match - no stale posting survives.
        assert!(
            !indexer.search("oldtoken999").contains(&obj.id.to_string()),
            "Old body term should be gone after update"
        );
        // The new term must match.
        assert!(
            indexer.search("newtoken111").contains(&obj.id.to_string()),
            "New body term should match after update"
        );
    }

    #[test]
    fn test_remove_from_index() {
        let indexer = Indexer::new();
        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("Something to index in the body".to_string()),
        )
        .with_metadata(ObjectMetadata {
            title: Some("Removal Test".to_string()),
            ..Default::default()
        });

        indexer.index_object(&obj).unwrap();
        // "something" appears only in the body (not in the title), proving
        // body indexing is in effect before we exercise removal.
        assert!(indexer.token_count() > 0);
        assert!(
            indexer.search("something").contains(&obj.id.to_string()),
            "Body term should be searchable before removal"
        );

        indexer.remove_object(obj.id).unwrap();

        assert!(
            !indexer.search("something").contains(&obj.id.to_string()),
            "Should not find removed object"
        );
    }

    #[test]
    fn test_reindex_rebuilds_cleanly() {
        let indexer = Indexer::new();
        let obj_a = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("alpha alphaonlyterm".to_string()),
        )
        .with_metadata(ObjectMetadata {
            title: Some("Alpha".to_string()),
            ..Default::default()
        });
        let obj_b = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("beta betaonlyterm".to_string()),
        )
        .with_metadata(ObjectMetadata {
            title: Some("Beta".to_string()),
            ..Default::default()
        });

        indexer.index_object(&obj_a).unwrap();
        assert!(indexer
            .search("alphaonlyterm")
            .contains(&obj_a.id.to_string()));

        // Rebuild the index with only obj_b - obj_a must disappear.
        indexer.reindex(&[obj_b.clone()]).unwrap();

        assert!(
            !indexer
                .search("alphaonlyterm")
                .contains(&obj_a.id.to_string()),
            "Reindex should drop documents not in the new set"
        );
        assert!(
            indexer
                .search("betaonlyterm")
                .contains(&obj_b.id.to_string()),
            "Reindex should index documents in the new set"
        );
    }

    #[test]
    fn test_persist_and_load() {
        let dir = tempfile::tempdir().unwrap();
        // Body-only term (not present in title/description/tags/type).
        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("Persistent bodytoken456 search content".to_string()),
        )
        .with_metadata(ObjectMetadata {
            title: Some("Persistent Search Test".to_string()),
            ..Default::default()
        });

        // Index and persist
        {
            let indexer = Indexer::with_vault_path(dir.path());
            indexer.index_object(&obj).unwrap();
            indexer.persist().unwrap();
        }

        // Load into a new indexer and verify the index survives
        {
            let indexer = Indexer::with_vault_path(dir.path());
            indexer.load().unwrap();

            let results = indexer.search("persistent");
            assert!(
                results.contains(&obj.id.to_string()),
                "Index should survive restart (title term)"
            );
            // Body-only term must also survive the restart.
            let results = indexer.search("bodytoken456");
            assert!(
                results.contains(&obj.id.to_string()),
                "Body term should survive restart"
            );
            assert!(indexer.token_count() > 0);
        }
    }

    #[test]
    fn test_restart_persistence_body_search() {
        // Mandatory acceptance test: a term appearing only in the note body
        // must be found after a full Indexer restart (write -> persist ->
        // drop -> reload -> query), exercising the real persistence path
        // (`.nabu/search_index.json`) rather than reconstructed state.
        let dir = tempfile::tempdir().unwrap();

        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("The body holds a unique restartterm789 word".to_string()),
        )
        .with_metadata(ObjectMetadata {
            title: Some("Restart Test".to_string()),
            ..Default::default()
        });

        // Phase 1: index + persist.
        {
            let indexer = Indexer::with_vault_path(dir.path());
            indexer.index_object(&obj).unwrap();
            indexer.persist().unwrap();
        }

        // Phase 2: the previous Indexer is dropped (scope ends). Construct a
        // fresh Indexer against the same vault path, load the on-disk index,
        // and query for the body-only term.
        {
            let indexer = Indexer::with_vault_path(dir.path());
            indexer.load().unwrap();

            let results = indexer.search("restartterm789");
            assert!(
                results.contains(&obj.id.to_string()),
                "Body-only term must be found after Indexer restart"
            );
            assert!(indexer.token_count() > 0);
        }
    }

    #[test]
    fn test_load_without_persisting_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let indexer = Indexer::with_vault_path(dir.path());
        indexer.load().unwrap();
        assert_eq!(indexer.token_count(), 0);
    }
}
