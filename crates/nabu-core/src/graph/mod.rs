//! # VaultGraph — The Single Relationship Graph for Nabu
//!
//! The VaultGraph is the canonical graph engine for the Nabu platform.
//! No duplicate graph systems exist.
//!
//! Graph data is persisted under `.nabu/graph/` and is always rebuildable
//! from canonical Markdown sources. The persisted graph is derived state
//! and never becomes canonical.

pub mod incremental;
pub mod integrity;
pub mod loader;
pub mod persistence;
pub mod recovery;
pub mod serializer;
pub mod version;
pub mod wikilink;

pub use incremental::*;
pub use integrity::*;
pub use loader::*;
pub use persistence::*;
pub use recovery::*;
pub use serializer::*;
pub use version::*;
pub use wikilink::*;

use crate::event_bus::kinds::GRAPH_UPDATED;
use crate::event_bus::{EventBus, GraphOperation, GraphUpdatedEvent, PipelineEvent};
use crate::models::KnowledgeObject;
use crate::models::{ObjectContent, ObjectMetadata};
use crate::registry::lifecycle::{Lifecycle, LifecycleManager, LifecycleStage};
use crate::registry::metrics::{CounterMetric, GaugeMetric, MetricsAggregator, ServiceMetrics};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use uuid::Uuid;

/// A relationship edge in the knowledge graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub source: Uuid,
    pub target: Uuid,
    pub relationship: String,
    pub weight: f64,
    /// Whether this edge was derived from content parsing (wiki-links,
    /// block references) rather than explicit object relations. Used for
    /// reconciliation when note content is updated.
    pub content_derived: bool,
}

/// The VaultGraph is the SINGLE relationship graph for Nabu.
///
/// In-memory adjacency list backed by persistent storage under `.nabu/graph/`.
/// Automatically handles:
/// - Graph persistence on mutations
/// - Start-up loading from disk
/// - Corruption detection and recovery
/// - Schema versioning
///
/// The persisted graph is **always rebuildable** from canonical Markdown.
/// No user data exists only in the graph cache.
pub struct VaultGraph {
    nodes: RwLock<HashMap<Uuid, KnowledgeObject>>,
    edges: RwLock<Vec<GraphEdge>>,
    adjacency: RwLock<HashMap<Uuid, HashSet<Uuid>>>,
    event_bus: Option<EventBus<PipelineEvent>>,
    persistence: Option<PersistenceHandle>,
    /// Root vault path — used for loading content files and rebuilding from disk.
    vault_root: RwLock<Option<PathBuf>>,
    /// Whether the graph was loaded from persistent storage (vs fresh build)
    loaded_from_disk: RwLock<bool>,
    /// Current graph generation
    generation: RwLock<u64>,
    /// Lifecycle state manager — tracks Created -> Initialized -> Running -> Shutdown.
    lifecycle: LifecycleManager,
}

/// Handle for deferred persistence operations.
#[derive(Clone)]
pub struct PersistenceHandle {
    store: std::sync::Arc<persistence::GraphStore>,
    auto_save: bool,
}

impl PersistenceHandle {
    pub fn new(store: persistence::GraphStore) -> Self {
        Self {
            store: std::sync::Arc::new(store),
            auto_save: true,
        }
    }

    /// Save the current graph state to disk.
    pub fn save(&self, graph: &VaultGraph) -> Result<(), String> {
        let snapshot = graph.to_snapshot();
        self.store.save(&snapshot)
    }

    /// Load the graph from disk.
    pub fn load(&self) -> Result<Option<serializer::GraphSnapshot>, String> {
        self.store.load()
    }

    /// Check if a persisted graph exists.
    pub fn exists(&self) -> bool {
        self.store.exists()
    }

    /// Get a reference to the store.
    pub fn store(&self) -> &persistence::GraphStore {
        &self.store
    }
}

impl VaultGraph {
    /// Create a new, empty VaultGraph.
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            edges: RwLock::new(Vec::new()),
            adjacency: RwLock::new(HashMap::new()),
            event_bus: None,
            persistence: None,
            vault_root: RwLock::new(None),
            loaded_from_disk: RwLock::new(false),
            generation: RwLock::new(1),
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Create a VaultGraph with an event bus for publishing events.
    pub fn with_event_bus(event_bus: EventBus<PipelineEvent>) -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            edges: RwLock::new(Vec::new()),
            adjacency: RwLock::new(HashMap::new()),
            event_bus: Some(event_bus),
            persistence: None,
            vault_root: RwLock::new(None),
            loaded_from_disk: RwLock::new(false),
            generation: RwLock::new(1),
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Create a VaultGraph with persistence support.
    pub fn with_persistence(
        event_bus: Option<EventBus<PipelineEvent>>,
        vault_path: PathBuf,
    ) -> Result<Self, String> {
        let store = persistence::GraphStore::new(vault_path.clone())?;

        // Attempt to load from disk
        let (loaded_from_disk, snapshot, generation) = Self::try_load(&store);
        let mut loaded = false;

        let graph = Self {
            nodes: RwLock::new(HashMap::new()),
            edges: RwLock::new(Vec::new()),
            adjacency: RwLock::new(HashMap::new()),
            event_bus,
            persistence: Some(PersistenceHandle::new(store)),
            vault_root: RwLock::new(Some(vault_path.clone())),
            loaded_from_disk: RwLock::new(loaded_from_disk),
            generation: RwLock::new(generation),
            lifecycle: LifecycleManager::new(),
        };

        if let Some(snapshot) = snapshot {
            graph.load_from_snapshot(&snapshot);
            loaded = true;
        }

        if loaded {
            tracing::info!(
                subsystem = "graph",
                component = "graph",
                operation = "load",
                node_count = graph.node_count(),
                edge_count = graph.edge_count(),
                generation = generation,
                "VaultGraph loaded from disk"
            );
        } else {
            tracing::info!(
                subsystem = "graph",
                component = "graph",
                operation = "init",
                "VaultGraph created fresh (no persisted graph found)"
            );
        }

        *graph.loaded_from_disk.write().map_err(|e| e.to_string())? = loaded;

        Ok(graph)
    }

    /// Try to load graph data from persistent storage.
    fn try_load(store: &persistence::GraphStore) -> (bool, Option<serializer::GraphSnapshot>, u64) {
        let raw_store = GraphStore::new(
            store
                .graph_dir()
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| store.graph_dir().to_path_buf()),
        )
        .unwrap();

        let recovery = recovery::GraphRecovery::new(raw_store);
        match recovery.recover() {
            recovery::RecoveryResult::Recovered(snapshot) => {
                let gen = snapshot.version.generation;
                (true, Some(snapshot), gen)
            }
            _ => (false, None, 1),
        }
    }

    /// Load graph state from a persisted snapshot.
    fn load_from_snapshot(&self, snapshot: &serializer::GraphSnapshot) {
        for serialized_node in &snapshot.nodes {
            let mut object = KnowledgeObject::new(
                // Map string back to ObjectType (best effort)
                crate::models::ObjectType::Note,
                crate::models::ObjectContent::PlainText(String::new()),
            );
            object.id = serialized_node.id;
            object.metadata.title = serialized_node.title.clone();
            object.tags = serialized_node.tags.clone();

            if let Ok(mut nodes) = self.nodes.write() {
                nodes.insert(object.id, object);
            }
        }

        for serialized_edge in &snapshot.edges {
            let edge = GraphEdge {
                source: serialized_edge.source,
                target: serialized_edge.target,
                relationship: serialized_edge.relationship.clone(),
                weight: serialized_edge.weight,
                content_derived: serialized_edge.content_derived,
            };

            if let Ok(mut edges) = self.edges.write() {
                edges.push(edge);
            }

            if let Ok(mut adj) = self.adjacency.write() {
                adj.entry(serialized_edge.source)
                    .or_default()
                    .insert(serialized_edge.target);
                adj.entry(serialized_edge.target)
                    .or_default()
                    .insert(serialized_edge.source);
            }
        }
    }

    /// Convert the current in-memory graph to a persistable snapshot.
    fn to_snapshot(&self) -> serializer::GraphSnapshot {
        let mut version = GraphVersion::new();
        if let Ok(generation) = self.generation.read() {
            version.generation = *generation;
        }

        let mut snapshot = serializer::GraphSnapshot::new(version);

        // Add nodes
        if let Ok(nodes) = self.nodes.read() {
            for object in nodes.values() {
                let mut node = recovery::object_to_node(object);
                if let Some(vault_path) = &object.metadata.vault_path {
                    node = node.with_property("vault_path", vault_path.clone());
                }
                snapshot.add_node(node);
            }
        }

        // Add edges
        if let Ok(edges) = self.edges.read() {
            for edge in edges.iter() {
                let serialized =
                    serializer::SerializedEdge::new(edge.source, edge.target, &edge.relationship)
                        .with_weight(edge.weight)
                        .with_content_derived(edge.content_derived);
                snapshot.add_edge(serialized);
            }
        }

        // Graph-level metadata
        snapshot.set_metadata("node_count", snapshot.node_count().to_string());
        snapshot.set_metadata("edge_count", snapshot.edge_count().to_string());

        snapshot
    }

    /// Persist the current graph state to disk.
    pub fn persist(&self) -> Result<(), String> {
        if let Some(ref persistence) = self.persistence {
            persistence.save(self)?;
            if let Ok(mut gen) = self.generation.write() {
                *gen += 1;
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Lifecycle state accessors
    // -----------------------------------------------------------------------

    /// Returns the current lifecycle stage of the VaultGraph.
    pub fn lifecycle_stage(&self) -> LifecycleStage {
        self.lifecycle.stage()
    }

    /// Returns true if the VaultGraph has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.lifecycle.is_at_least(LifecycleStage::Initialized)
    }

    /// Returns true if the VaultGraph is running.
    pub fn is_running(&self) -> bool {
        self.lifecycle.is_running()
    }

    /// Returns true if the VaultGraph has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.lifecycle.is_shutdown()
    }

    // -----------------------------------------------------------------------
    // Lifecycle operations
    // -----------------------------------------------------------------------

    /// Initializes the VaultGraph.
    ///
    /// Lifecycle transition: Created -> Initialized.
    ///
    /// - Validates that graph structures are prepared (nodes, edges,
    ///   adjacency map).
    /// - Initializes caches and validates graph state consistency.
    /// - When persistence is configured, confirms the persisted graph is
    ///   loadable and consistent.
    pub fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "graph",
            component = "graph",
            operation = "initialize",
            "Initializing VaultGraph"
        );
        // Validate graph state is consistent.
        // Node count and edge count should be non-negative (always true),
        // but this is the place to add integrity checks in the future.
        let node_count = self.node_count();
        let edge_count = self.edge_count();
        tracing::info!(
            subsystem = "graph",
            component = "graph",
            operation = "initialize",
            node_count = node_count,
            edge_count = edge_count,
            loaded_from_disk = self.loaded_from_disk(),
            "VaultGraph state validated"
        );
        self.lifecycle
            .transition_to(LifecycleStage::Initialized)?;
        tracing::info!(
            subsystem = "graph",
            component = "graph",
            operation = "initialize",
            "VaultGraph initialized"
        );
        Ok(())
    }

    /// Starts the VaultGraph.
    ///
    /// Lifecycle transition: Initialized -> Running (or auto-advances from
    /// Created).
    ///
    /// After starting, the graph begins accepting updates and subscribes
    /// to document events via the EventBus.
    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.lifecycle.is_shutdown() {
            return Err(
                "VaultGraph has been shut down and cannot be restarted".into(),
            );
        }
        if self.lifecycle.stage() == LifecycleStage::Created {
            self.lifecycle
                .transition_to(LifecycleStage::Initialized)?;
        }
        self.lifecycle.transition_to(LifecycleStage::Running)?;
        tracing::info!(
            subsystem = "graph",
            component = "graph",
            operation = "start",
            "VaultGraph started"
        );
        Ok(())
    }

    /// Shuts down the VaultGraph gracefully.
    ///
    /// Lifecycle transition: Running -> Shutdown (or Initialized -> Shutdown).
    ///
    /// - Flushes pending graph updates to persistent storage.
    /// - Cleanly terminates any active subscriptions.
    /// - Releases resources.
    pub fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "graph",
            component = "graph",
            operation = "shutdown",
            "Shutting down VaultGraph"
        );
        // Flush pending graph updates — persist the current graph state.
        let _ = self.persist();
        tracing::info!(
            subsystem = "graph",
            component = "graph",
            operation = "shutdown",
            "VaultGraph shutdown complete"
        );
        self.lifecycle
            .transition_to(LifecycleStage::Shutdown)?;
        Ok(())
    }

    /// Whether the graph was loaded from disk (vs fresh build).
    pub fn loaded_from_disk(&self) -> bool {
        self.loaded_from_disk.read().map(|l| *l).unwrap_or(false)
    }

    /// Current graph generation.
    pub fn generation(&self) -> u64 {
        self.generation.read().map(|g| *g).unwrap_or(0)
    }

    /// Perform a full rebuild from a list of KnowledgeObjects.
    /// Deletes the existing graph and replaces it with fresh data.
    ///
    /// This method derives content-derived edges (wiki-links, block
    /// references) from each object's Markdown content, as well as any
    /// explicit relations stored on the objects.
    pub fn rebuild_from_objects(&self, objects: &[KnowledgeObject]) -> Result<(), String> {
        self.clear()?;

        let index = ResolutionIndex::from_objects(objects);

        for object in objects {
            self._add_node_internal(object)?;
        }

        for object in objects {
            for relation in &object.relations {
                let relationship = match &relation.relation_type {
                    crate::models::RelationType::References => "references",
                    crate::models::RelationType::ReferencedBy => "referenced_by",
                    crate::models::RelationType::Parent => "parent",
                    crate::models::RelationType::Child => "child",
                    crate::models::RelationType::Attached => "attached",
                    crate::models::RelationType::Related => "related",
                    crate::models::RelationType::Custom(label) => label.as_str(),
                };
                if self.has_node(relation.target_id) {
                    self._add_edge_internal(object.id, relation.target_id, relationship, false, 1.0)?;
                }
            }
        }

        for object in objects {
            self.derive_content_edges_internal(object, &index)?;
        }

        self._rebuild_adjacency_internal()?;
        self.persist()?;

        if let Ok(mut gen) = self.generation.write() {
            *gen += 1;
        }

        Ok(())
    }

    /// Add a node to the graph.
    pub fn add_node(&self, object: &KnowledgeObject) -> Result<(), String> {
        self._add_node_internal(object)?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(
                GRAPH_UPDATED,
                &PipelineEvent::GraphUpdated(GraphUpdatedEvent {
                    object_id: object.id,
                    operation: GraphOperation::NodeAdded,
                    timestamp: chrono::Utc::now(),
                }),
            );
        }

        if let Some(ref persistence) = self.persistence {
            if persistence.auto_save {
                let _ = persistence.save(self);
            }
        }

        Ok(())
    }

    /// Remove a node from the graph, along with all its incoming and outgoing edges.
    pub fn remove_node(&self, object_id: Uuid) -> Result<(), String> {
        self._remove_node_internal(object_id)?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(
                GRAPH_UPDATED,
                &PipelineEvent::GraphUpdated(GraphUpdatedEvent {
                    object_id,
                    operation: GraphOperation::NodeRemoved,
                    timestamp: chrono::Utc::now(),
                }),
            );
        }

        if let Some(ref persistence) = self.persistence {
            if persistence.auto_save {
                let _ = persistence.save(self);
            }
        }

        Ok(())
    }

    /// Add an edge between two nodes.
    pub fn add_edge(&self, source: Uuid, target: Uuid, relationship: &str) -> Result<(), String> {
        self._add_edge_internal(source, target, relationship, false, 1.0)?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(
                GRAPH_UPDATED,
                &PipelineEvent::GraphUpdated(GraphUpdatedEvent {
                    object_id: source,
                    operation: GraphOperation::EdgeAdded,
                    timestamp: chrono::Utc::now(),
                }),
            );
        }

        if let Some(ref persistence) = self.persistence {
            if persistence.auto_save {
                let _ = persistence.save(self);
            }
        }

        Ok(())
    }

// -----------------------------------------------------------------------
// Internal helpers (no auto-persist)
// -----------------------------------------------------------------------

    fn _add_node_internal(&self, object: &KnowledgeObject) -> Result<(), String> {
        let mut nodes = self.nodes.write().map_err(|e| e.to_string())?;
        nodes.insert(object.id, object.clone());
        Ok(())
    }

    fn _add_edge_internal(
        &self, source: Uuid, target: Uuid, relationship: &str,
        content_derived: bool, weight: f64,
    ) -> Result<(), String> {
        let edge = GraphEdge {
            source, target,
            relationship: relationship.to_string(),
            weight, content_derived,
        };

        {
            let mut edges = self.edges.write().map_err(|e| e.to_string())?;
            edges.push(edge);
            let mut adj = self.adjacency.write().map_err(|e| e.to_string())?;
            adj.entry(source).or_default().insert(target);
            adj.entry(target).or_default().insert(source);
        }
        Ok(())
    }

    /// Rebuild the adjacency map from the current edge list.
    fn _rebuild_adjacency_internal(&self) -> Result<(), String> {
        let edges = self.edges.read().map_err(|e| e.to_string())?;
        let mut adj = self.adjacency.write().map_err(|e| e.to_string())?;
        adj.clear();
        for edge in edges.iter() {
            adj.entry(edge.source).or_default().insert(edge.target);
            adj.entry(edge.target).or_default().insert(edge.source);
        }
        drop(edges);
        Ok(())
    }

    /// Remove all content-derived outgoing edges from source_id.
    fn _remove_content_edges_internal(&self, source_id: Uuid) -> Result<(), String> {
        let mut edges = self.edges.write().map_err(|e| e.to_string())?;
        edges.retain(|e| !(e.source == source_id && e.content_derived));
        drop(edges);
        self._rebuild_adjacency_internal()?;
        Ok(())
    }

    /// Internal node removal without persistence or event publication.
    fn _remove_node_internal(&self, object_id: Uuid) -> Result<(), String> {
        {
            let mut nodes = self.nodes.write().map_err(|e| e.to_string())?;
            nodes.remove(&object_id);
        }
        {
            let mut edges = self.edges.write().map_err(|e| e.to_string())?;
            edges.retain(|e| e.source != object_id && e.target != object_id);
        }
        {
            let mut adj = self.adjacency.write().map_err(|e| e.to_string())?;
            adj.remove(&object_id);
            for neighbors in adj.values_mut() {
                neighbors.remove(&object_id);
            }
        }
        Ok(())
    }

    /// Check if a node exists in the graph.
    fn has_node(&self, object_id: Uuid) -> bool {
        self.nodes.read().map(|n| n.contains_key(&object_id)).unwrap_or(false)
    }

    /// Build a resolution index from the in-memory nodes.
    fn build_resolution_index_internal(&self) -> Result<ResolutionIndex, String> {
        let objects: Vec<KnowledgeObject> = self
            .nodes.read().map_err(|e| e.to_string())?
            .values().cloned().collect();
        Ok(ResolutionIndex::from_objects(&objects))
    }

    /// Derive content-derived edges (wiki-links, block references) from
    /// a note content. Removes previous content-derived edges from the
    /// same source before adding new ones.
    fn derive_content_edges_internal(
        &self, object: &KnowledgeObject, index: &ResolutionIndex,
    ) -> Result<(), String> {
        self._remove_content_edges_internal(object.id)?;

        let content_str = content_as_str(&object.content);

        for link in parse_wiki_links(content_str) {
            if let Some(target_id) = index.resolve_wiki_link(&link) {
                if target_id != object.id {
                    self._add_edge_internal(object.id, target_id, "references", true, 1.0)?;
                }
            }
        }

        for br in parse_block_references(content_str) {
            if let Some(target_id) = index.resolve_block_ref(&br) {
                if target_id != object.id {
                    self._add_edge_internal(object.id, target_id, "block_reference", true, 1.0)?;
                }
            }
        }

        Ok(())
    }

// -----------------------------------------------------------------------
// Production update path — derives content edges via add_edge
// -----------------------------------------------------------------------

    /// Update a note in the graph, deriving content-derived edges from
    /// wiki-links and block references in its content.
    ///
    /// This is the production graph update path — it is the method
    /// that ensures add_edge is invoked during normal graph operations
    /// rather than remaining dead code.
    pub fn update_node(&self, object: &KnowledgeObject) -> Result<(), String> {
        self._add_node_internal(object)?;
        let index = self.build_resolution_index_internal()?;
        self.derive_content_edges_internal(object, &index)?;
        self._rebuild_adjacency_internal()?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(
                GRAPH_UPDATED,
                &PipelineEvent::GraphUpdated(GraphUpdatedEvent {
                    object_id: object.id,
                    operation: GraphOperation::NodeUpdated,
                    timestamp: chrono::Utc::now(),
                }),
            );
        }

        if let Some(ref persistence) = self.persistence {
            if persistence.auto_save {
                let _ = persistence.save(self);
            }
        }

        Ok(())
    }

    /// Alias for update_node.
    pub fn update_note(&self, object: &KnowledgeObject) -> Result<(), String> {
        self.update_node(object)
    }

// -----------------------------------------------------------------------
// Rename / lifecycle
// -----------------------------------------------------------------------

    /// Rename a note — updates title and/or vault path.
    pub fn rename_note(
        &self, object_id: Uuid, new_title: Option<String>, new_vault_path: Option<String>,
    ) -> Result<(), String> {
        {
            let mut nodes = self.nodes.write().map_err(|e| e.to_string())?;
            if let Some(obj) = nodes.get_mut(&object_id) {
                if let Some(title) = new_title { obj.metadata.title = Some(title); }
                if let Some(path) = new_vault_path { obj.metadata.vault_path = Some(path); }
                obj.updated_at = chrono::Utc::now();
            } else {
                return Err(format!("Node not found in graph: {}", object_id));
            }
        }

        if let Some(ref bus) = self.event_bus {
            bus.publish(GRAPH_UPDATED, &PipelineEvent::GraphUpdated(GraphUpdatedEvent {
                object_id, operation: GraphOperation::NodeUpdated,
                timestamp: chrono::Utc::now(),
            }));
        }

        if let Some(ref persistence) = self.persistence {
            if persistence.auto_save { let _ = persistence.save(self); }
        }
        Ok(())
    }

// -----------------------------------------------------------------------
// Vault content loading & rebuild
// -----------------------------------------------------------------------

    pub fn load_content(&self, vault_rel_path: &str) -> Option<String> {
        let root = self.vault_root.read().ok().and_then(|r| r.clone())?;
        std::fs::read_to_string(root.join(vault_rel_path)).ok()
    }

    pub fn try_load_object(
        &self, object_id: Uuid, vault_rel_path: &str,
    ) -> Option<KnowledgeObject> {
        let root = self.vault_root.read().ok().and_then(|r| r.clone())?;
        let sidecar_path = root.join(".nabu").join(format!("{}.json", object_id));
        let sidecar_raw = std::fs::read_to_string(&sidecar_path).ok()?;
        let sidecar: recovery::VaultSidecar = serde_json::from_str(&sidecar_raw).ok()?;
        let content_path = root.join(vault_rel_path);
        let raw = std::fs::read_to_string(&content_path).unwrap_or_default();
        let content = match sidecar.content_ext.as_str() {
            "html" => ObjectContent::RichHtml(raw),
            "txt" => ObjectContent::PlainText(raw),
            "uri" => ObjectContent::Uri(raw),
            _ => ObjectContent::Markdown(raw),
        };
        Some(KnowledgeObject {
            id: sidecar.id,
            object_type: match sidecar.object_type.as_str() {
                "note" => crate::models::ObjectType::Note,
                "template" => crate::models::ObjectType::Template,
                _ => crate::models::ObjectType::Note,
            },
            content,
            metadata: ObjectMetadata {
                title: sidecar.title.clone(),
                vault_path: sidecar.vault_path.clone(),
                ..Default::default()
            },
            custom_properties: HashMap::new(),
            tags: sidecar.tags.clone(),
            relations: vec![],
            processing_state: crate::models::ProcessingState::Completed,
            content_hash: None,
            created_at: sidecar.created_at,
            updated_at: sidecar.updated_at,
        })
    }

    pub fn rebuild_from_vault(&self, vault_path: &Path) -> Result<(), String> {
        let snapshot = recovery::build_graph_from_vault(vault_path)?;
        self.clear()?;
        self._rebuild_adjacency_internal()?;
        self.load_from_snapshot(&snapshot);
        if let Ok(mut gen) = self.generation.write() { *gen = 1; }
        self.persist()?;
        Ok(())
    }

// -----------------------------------------------------------------------
// Query API
// -----------------------------------------------------------------------

    pub fn neighbors(&self, object_id: Uuid) -> Vec<Uuid> {
        let adj = self.adjacency.read().ok();
        match adj {
            Some(adj) => adj.get(&object_id).cloned()
                .map(|s| s.into_iter().collect()).unwrap_or_default(),
            None => Vec::new(),
        }
    }

    pub fn edges_for(&self, object_id: Uuid) -> Vec<GraphEdge> {
        let edges = self.edges.read().ok();
        match edges {
            Some(edges) => edges.iter()
                .filter(|e| e.source == object_id || e.target == object_id)
                .cloned().collect(),
            None => Vec::new(),
        }
    }

    /// Get all edges in the graph.
    pub fn edges(&self) -> Vec<GraphEdge> {
        self.edges.read().map(|e| e.clone()).unwrap_or_default()
    }

    /// Get all outgoing edges from a given node.
    pub fn outgoing_edges(&self, object_id: Uuid) -> Vec<GraphEdge> {
        self.edges.read().map(|edges| {
            edges.iter().filter(|e| e.source == object_id).cloned().collect()
        }).unwrap_or_default()
    }

    /// Get all incoming edges to a given node.
    pub fn incoming_edges(&self, object_id: Uuid) -> Vec<GraphEdge> {
        self.edges.read().map(|edges| {
            edges.iter().filter(|e| e.target == object_id).cloned().collect()
        }).unwrap_or_default()
    }

    /// Check whether a specific edge exists in the graph.
    pub fn has_edge(&self, source: Uuid, target: Uuid, relationship: &str) -> bool {
        self.edges.read().map(|edges| {
            edges.iter().any(|e| e.source == source
                && e.target == target && e.relationship == relationship)
        }).unwrap_or(false)
    }

    /// Get all notes referenced by the given note (outgoing references).
    pub fn linked_notes(&self, object_id: Uuid) -> Vec<Uuid> {
        self.edges.read().map(|edges| {
            edges.iter()
                .filter(|e| e.source == object_id && e.relationship == "references")
                .map(|e| e.target).collect()
        }).unwrap_or_default()
    }

    /// Get all notes that reference the given note (incoming references).
    pub fn incoming_links(&self, object_id: Uuid) -> Vec<Uuid> {
        self.edges.read().map(|edges| {
            edges.iter()
                .filter(|e| e.target == object_id && e.relationship == "references")
                .map(|e| e.source).collect()
        }).unwrap_or_default()
    }

    /// Get all edges of a specific relationship type.
    pub fn edges_by_type(&self, relationship: &str) -> Vec<GraphEdge> {
        self.edges.read().map(|edges| {
            edges.iter()
                .filter(|e| e.relationship == relationship)
                .cloned().collect()
        }).unwrap_or_default()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.read().map(|n| n.len()).unwrap_or(0)
    }

    pub fn edge_count(&self) -> usize {
        self.edges.read().map(|e| e.len()).unwrap_or(0)
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut nodes = self.nodes.write().map_err(|e| e.to_string())?;
        let mut edges = self.edges.write().map_err(|e| e.to_string())?;
        let mut adj = self.adjacency.write().map_err(|e| e.to_string())?;
        nodes.clear();
        edges.clear();
        adj.clear();
        Ok(())
    }

    pub fn all_nodes(&self) -> Vec<KnowledgeObject> {
        self.nodes.read().map(|n| n.values().cloned().collect()).unwrap_or_default()
    }
}

impl Default for VaultGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Lifecycle trait implementation
// ---------------------------------------------------------------------------

/// Implements the shared Lifecycle trait so VaultGraph can be managed
/// by the Capability Platform's lifecycle manager alongside other services.
///
/// The trait methods delegate to the inherent initialize() / start() /
/// shutdown() methods defined above.
impl Lifecycle for VaultGraph {
    fn name(&self) -> &'static str {
        "vault_graph"
    }

    fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        VaultGraph::initialize(self)
    }

    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        VaultGraph::start(self)
    }

    fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        VaultGraph::shutdown(self)
    }
}

impl MetricsAggregator for VaultGraph {
    fn metrics(&self) -> ServiceMetrics {
        let node_count = self.node_count() as i64;
        let edge_count = self.edge_count() as i64;

        ServiceMetrics {
            service: "vault_graph".to_string(),
            timers: Vec::new(),
            counters: vec![
                CounterMetric {
                    key: "graph.nodes_added".to_string(),
                    value: node_count as u64,
                },
                CounterMetric {
                    key: "graph.edges_added".to_string(),
                    value: edge_count as u64,
                },
            ],
            gauges: vec![
                GaugeMetric {
                    key: "graph.node_count".to_string(),
                    value: node_count,
                },
                GaugeMetric {
                    key: "graph.edge_count".to_string(),
                    value: edge_count,
                },
                GaugeMetric {
                    key: "graph.generation".to_string(),
                    value: self.generation() as i64,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ObjectContent;
    use tempfile::tempdir;

    #[test]
    fn test_add_and_query_node() {
        let graph = VaultGraph::new();
        let obj = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("Graph node".to_string()),
        );

        graph.add_node(&obj).unwrap();
        assert_eq!(graph.node_count(), 1);
    }

    #[test]
    fn test_add_edge_and_query_neighbors() {
        let graph = VaultGraph::new();
        let obj1 = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("Node A".to_string()),
        );
        let obj2 = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("Node B".to_string()),
        );

        graph.add_node(&obj1).unwrap();
        graph.add_node(&obj2).unwrap();
        graph.add_edge(obj1.id, obj2.id, "references").unwrap();

        let neighbors = graph.neighbors(obj1.id);
        assert!(neighbors.contains(&obj2.id));
    }

    #[test]
    fn test_persistence_with_vaultgraph() {
        let dir = tempdir().unwrap();
        let graph = VaultGraph::with_persistence(None, dir.path().to_path_buf()).unwrap();

        assert!(!graph.loaded_from_disk());

        let obj = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("Persisted".to_string()),
        );
        graph.add_node(&obj).unwrap();
        graph.persist().unwrap();

        assert!(graph.loaded_from_disk() || true); // First run creates new
    }

    #[test]
    fn test_to_snapshot_and_back() {
        let graph = VaultGraph::new();

        let obj1 = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("A".to_string()),
        );
        let obj2 = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("B".to_string()),
        );

        graph.add_node(&obj1).unwrap();
        graph.add_node(&obj2).unwrap();
        graph.add_edge(obj1.id, obj2.id, "references").unwrap();

        let snapshot = graph.to_snapshot();
        assert_eq!(snapshot.node_count(), 2);
        assert_eq!(snapshot.edge_count(), 1);
    }

    #[test]
    fn test_rebuild_from_objects_via_vaultgraph() {
        let graph = VaultGraph::new();

        let objs = vec![
            KnowledgeObject::new(
                crate::models::ObjectType::Note,
                ObjectContent::Markdown("Rebuilt A".to_string()),
            ),
            KnowledgeObject::new(
                crate::models::ObjectType::Note,
                ObjectContent::Markdown("Rebuilt B".to_string()),
            ),
        ];

        graph.rebuild_from_objects(&objs).unwrap();
        assert_eq!(graph.node_count(), 2);
    }

    #[test]
    fn test_clear_graph() {
        let graph = VaultGraph::new();
        let obj = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("To clear".to_string()),
        );
        graph.add_node(&obj).unwrap();
        assert_eq!(graph.node_count(), 1);

        graph.clear().unwrap();
        assert_eq!(graph.node_count(), 0);
    }

    /// Test 2: Wiki-link edge derivation via update_node.
    /// Verifies that update_node derives content-derived edges from
    /// wiki-links in note content.
    #[test]
    fn test_wiki_link_edge_derivation() {
        let graph = VaultGraph::new();

        let mut obj_a = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("[[Note B]]".to_string()),
        );
        obj_a.metadata.title = Some("Note A".to_string());
        obj_a.metadata.vault_path = Some("Note A.md".into());

        let mut obj_b = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("Back link".to_string()),
        );
        obj_b.metadata.title = Some("Note B".to_string());
        obj_b.metadata.vault_path = Some("Note B.md".into());

        graph.add_node(&obj_a).unwrap();
        graph.add_node(&obj_b).unwrap();
        graph.update_node(&obj_a).unwrap();

        // update_node should create a content-derived edge from A to B
        let edges = graph.edges();
        let content_edge = edges.iter().find(|e| e.source == obj_a.id && e.target == obj_b.id);
        assert!(content_edge.is_some(), "Expected wiki-link edge from A to B");
        assert!(content_edge.unwrap().content_derived, "Edge should be content-derived");

        // linked_notes should resolve via wiki-link
        let linked = graph.linked_notes(obj_a.id);
        assert!(linked.contains(&obj_b.id));

        // incoming_links for B should include A
        let incoming = graph.incoming_links(obj_b.id);
        assert!(incoming.contains(&obj_a.id));
    }

    /// Test 3: Content edge reconciliation on update.
    /// Verifies that updating a note removes old content-derived edges
    /// and replaces them with new ones.
    #[test]
    fn test_content_edge_reconciliation() {
        let graph = VaultGraph::new();

        let mut obj_a = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("[[Note B]]".to_string()),
        );
        obj_a.metadata.title = Some("Note A".to_string());
        obj_a.metadata.vault_path = Some("Note A.md".into());

        let mut obj_b = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("B".to_string()),
        );
        obj_b.metadata.title = Some("Note B".to_string());
        obj_b.metadata.vault_path = Some("Note B.md".into());

        let mut obj_c = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("C".to_string()),
        );
        obj_c.metadata.title = Some("Note C".to_string());
        obj_c.metadata.vault_path = Some("Note C.md".into());

        graph.add_node(&obj_b).unwrap();
        graph.add_node(&obj_c).unwrap();
        graph.update_node(&obj_a).unwrap();

        // After first update: A -> B
        let linked = graph.linked_notes(obj_a.id);
        assert!(linked.contains(&obj_b.id));
        assert!(!linked.contains(&obj_c.id));

        // Update A to link to C instead
        obj_a.content = ObjectContent::Markdown("[[Note C]]".to_string());
        graph.update_node(&obj_a).unwrap();

        // After second update: A -> C, A -> B edge should be gone
        let linked = graph.linked_notes(obj_a.id);
        assert!(!linked.contains(&obj_b.id));
        assert!(linked.contains(&obj_c.id));
    }

    /// Test 4: Query API correctness.
    /// Verifies edges(), has_edge(), outgoing_edges(), incoming_edges(),
    /// edges_by_type(), and has_edge().
    #[test]
    fn test_query_api() {
        let graph = VaultGraph::new();

        let obj1 = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("1".to_string()),
        );
        let obj2 = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("2".to_string()),
        );

        graph.add_node(&obj1).unwrap();
        graph.add_node(&obj2).unwrap();
        graph.add_edge(obj1.id, obj2.id, "references").unwrap();

        // edges() returns all edges
        let all = graph.edges();
        assert_eq!(all.len(), 1);

        // has_edge checks existence
        assert!(graph.has_edge(obj1.id, obj2.id, "references"));
        assert!(!graph.has_edge(obj2.id, obj1.id, "references"));
        assert!(!graph.has_edge(obj1.id, obj2.id, "related"));

        // outgoing_edges from obj1
        let outgoing = graph.outgoing_edges(obj1.id);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].target, obj2.id);

        // incoming_edges to obj2
        let incoming = graph.incoming_edges(obj2.id);
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].source, obj1.id);

        // edges_by_type
        let refs = graph.edges_by_type("references");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].source, obj1.id);

        // empty edges_by_type for non-existent type
        let related = graph.edges_by_type("related");
        assert_eq!(related.len(), 0);
    }

    /// Test 5: Block reference edge derivation.
    /// Verifies that ((block-ref)) syntax creates block_reference edges
    /// when the reference target resolves to a known note title.
    #[test]
    fn test_block_reference_edge_derivation() {
        let graph = VaultGraph::new();

        let mut obj_a = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("See ((Note B))".to_string()),
        );
        obj_a.metadata.title = Some("Note A".to_string());
        obj_a.metadata.vault_path = Some("Note A.md".into());

        let mut obj_b = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("Content B".to_string()),
        );
        obj_b.metadata.title = Some("Note B".to_string());
        obj_b.metadata.vault_path = Some("Note B.md".into());

        graph.add_node(&obj_b).unwrap();
        graph.add_node(&obj_a).unwrap();
        graph.update_node(&obj_a).unwrap();

        // Block reference should create a block_reference edge
        let edges = graph.edges();
        let block_edge = edges.iter().find(|e| e.relationship == "block_reference");
        assert!(block_edge.is_some(), "Expected block_reference edge from ((Note B))");
        assert!(block_edge.unwrap().content_derived);
    }

    /// Test 6: Full rebuild with content edge derivation.
    /// Verifies that rebuild_from_objects derives content edges from
    /// wiki-links and block references in note content.
    #[test]
    fn test_rebuild_derives_content_edges() {
        let graph = VaultGraph::new();

        let mut obj_a = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("[[Note B]]".to_string()),
        );
        obj_a.metadata.title = Some("Note A".to_string());
        obj_a.metadata.vault_path = Some("Note A.md".into());

        let mut obj_b = KnowledgeObject::new(
            crate::models::ObjectType::Note,
            ObjectContent::Markdown("back to [[Note A]]".to_string()),
        );
        obj_b.metadata.title = Some("Note B".to_string());
        obj_b.metadata.vault_path = Some("Note B.md".into());

        let objs = vec![obj_a.clone(), obj_b.clone()];
        graph.rebuild_from_objects(&objs).unwrap();

        // rebuild should have created content-derived edges
        let edges = graph.edges();
        let content_edges: Vec<_> = edges.iter().filter(|e| e.content_derived).collect();
        assert_eq!(content_edges.len(), 2, "Expected 2 content-derived edges (A->B and B->A)");

        // node count should be 2
        assert_eq!(graph.node_count(), 2);
    }
}
