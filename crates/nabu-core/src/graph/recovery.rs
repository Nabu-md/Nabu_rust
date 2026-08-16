use crate::graph::integrity::{check_integrity, RebuildReason};
use crate::graph::loader::{load_graph, upgrade_snapshot, LoadResult};
use crate::graph::persistence::GraphStore;
use crate::graph::serializer::{GraphSnapshot, SerializedEdge, SerializedNode};
use crate::graph::version::{BuildSource, GraphVersion};
use crate::graph::wikilink::{ResolutionIndex, content_as_str, parse_block_references, parse_wiki_links};
use crate::models::{KnowledgeObject, ObjectContent, ObjectMetadata};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The graph recovery coordinator.
///
/// Handles the full recovery lifecycle:
/// 1. Detect if recovery is needed
/// 2. Attempt integrity check
/// 3. Attempt version upgrade
/// 4. Fall back to full rebuild
pub struct GraphRecovery {
    store: GraphStore,
}

impl GraphRecovery {
    /// Create a new recovery coordinator.
    pub fn new(store: GraphStore) -> Self {
        Self { store }
    }

    /// Attempt to recover the graph.
    ///
    /// Returns a loaded snapshot or triggers a rebuild.
    pub fn recover(&self) -> RecoveryResult {
        let load_result = load_graph(&self.store);

        match load_result {
            LoadResult::Loaded(snapshot) => {
                tracing::info!("Graph loaded successfully from disk");
                RecoveryResult::Recovered(snapshot)
            }

            LoadResult::NotFound => {
                tracing::info!("No persisted graph found — ready for fresh build");
                RecoveryResult::NeedsRebuild(RebuildReason::Manual)
            }

            LoadResult::RequiresUpgrade {
                mut snapshot,
                compatibility,
            } => {
                let from_version = match &compatibility {
                    crate::graph::version::VersionCompatibility::Outdated {
                        schema_version,
                        ..
                    } => *schema_version,
                    _ => 0,
                };

                match upgrade_snapshot(&mut snapshot, from_version) {
                    Ok(()) => {
                        tracing::info!(
                            "Graph upgraded from schema v{} to v{}",
                            from_version,
                            crate::graph::version::CURRENT_GRAPH_SCHEMA_VERSION
                        );
                        snapshot.version.build_source = BuildSource::SchemaUpgrade;
                        let _ = self.store.save(&snapshot);
                        RecoveryResult::Recovered(snapshot)
                    }
                    Err(e) => {
                        tracing::warn!("Graph upgrade failed: {} — will rebuild", e);
                        RecoveryResult::NeedsRebuild(RebuildReason::SchemaUpgrade {
                            from: from_version,
                            to: crate::graph::version::CURRENT_GRAPH_SCHEMA_VERSION,
                        })
                    }
                }
            }

            LoadResult::Corrupted { reason, report: _ } => {
                tracing::warn!("Graph corruption detected: {} — will rebuild", reason);
                // Try to save corrupted file for debugging
                if let Err(e) = self.store.delete() {
                    tracing::error!("Failed to delete corrupted graph: {}", e);
                }
                RecoveryResult::NeedsRebuild(RebuildReason::CorruptionDetected(reason))
            }

            LoadResult::FutureVersion { version } => {
                tracing::warn!(
                    "Graph is from future version (schema v{}) — must rebuild",
                    version.schema_version
                );
                RecoveryResult::NeedsRebuild(RebuildReason::SchemaUpgrade {
                    from: version.schema_version,
                    to: crate::graph::version::CURRENT_GRAPH_SCHEMA_VERSION,
                })
            }
        }
    }

    /// Perform a full graph rebuild from canonical Markdown.
    ///
    /// # Arguments
    /// * `rebuild_fn` — A function that produces graph nodes and edges from canonical sources.
    ///   This function is provided by the caller because only the caller knows how to
    ///   iterate the vault's Markdown files.
    pub fn rebuild(
        &self,
        rebuild_fn: impl FnOnce() -> Result<(Vec<SerializedNode>, Vec<SerializedEdge>), String>,
        build_source: BuildSource,
    ) -> Result<GraphSnapshot, String> {
        tracing::info!("Starting graph rebuild from canonical sources...");

        // Delete any existing graph state
        let _ = self.store.delete();

        // Build the new graph
        let (nodes, edges) = rebuild_fn()?;

        let mut version = GraphVersion::rebuilt(build_source);
        version.rebuild_count = 1;

        let mut snapshot = GraphSnapshot::new(version);

        for node in nodes {
            snapshot.add_node(node);
        }
        for edge in edges {
            snapshot.add_edge(edge);
        }

        // Perform integrity check
        let report = check_integrity(&snapshot);
        if !report.passed {
            return Err(format!(
                "Rebuilt graph failed integrity check: {} errors",
                report.errors.len()
            ));
        }

        // Persist
        self.store.save(&snapshot)?;

        tracing::info!(
            "Graph rebuilt: {} nodes, {} edges",
            snapshot.node_count(),
            snapshot.edge_count()
        );

        Ok(snapshot)
    }

    /// Get a reference to the underlying store.
    pub fn store(&self) -> &GraphStore {
        &self.store
    }
}

/// Result of a graph recovery attempt.
#[derive(Debug, Clone)]
pub enum RecoveryResult {
    /// Graph was successfully loaded/recovered
    Recovered(GraphSnapshot),
    /// Graph needs a full rebuild from canonical sources
    NeedsRebuild(RebuildReason),
}

impl RecoveryResult {
    pub fn needs_rebuild(&self) -> bool {
        matches!(self, RecoveryResult::NeedsRebuild(_))
    }

    pub fn snapshot(&self) -> Option<&GraphSnapshot> {
        match self {
            RecoveryResult::Recovered(snapshot) => Some(snapshot),
            RecoveryResult::NeedsRebuild(_) => None,
        }
    }
}

/// Extract minimal graph nodes from a KnowledgeObject.
/// This is used during graph rebuild.
pub fn object_to_node(object: &KnowledgeObject) -> SerializedNode {
    SerializedNode::new(
        object.id,
        object.object_type.variant_name(),
        object.metadata.title.clone(),
        object.content.content_type_hint(),
    )
    .with_property("created_at", object.created_at.to_rfc3339())
}

/// Extract graph edges from a KnowledgeObject's relations.
/// This is used during graph rebuild.
pub fn extract_edges(object: &KnowledgeObject) -> Vec<SerializedEdge> {
    let mut edges = Vec::new();

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

        edges.push(
            SerializedEdge::new(object.id, relation.target_id, relationship)
                .with_content_derived(false),
        );
    }

    edges
}

/// Extract content-derived edges (wiki-links, block references) from a
/// KnowledgeObject's Markdown content, using a resolution index to map
/// targets to UUIDs. Each edge is tagged with content_derived = true.
pub fn extract_content_edges(object: &KnowledgeObject, index: &ResolutionIndex) -> Vec<SerializedEdge> {
    let mut edges = Vec::new();
    let content_str = content_as_str(&object.content);

    for link in parse_wiki_links(content_str) {
        if let Some(target_id) = index.resolve_wiki_link(&link) {
            if target_id != object.id {
                edges.push(
                    SerializedEdge::new(object.id, target_id, "references")
                        .with_content_derived(true),
                );
            }
        }
    }

    for br in parse_block_references(content_str) {
        if let Some(target_id) = index.resolve_block_ref(&br) {
            if target_id != object.id {
                edges.push(
                    SerializedEdge::new(object.id, target_id, "block_reference")
                        .with_content_derived(true),
                );
            }
        }
    }

    edges
}

/// Build a graph snapshot from a list of KnowledgeObjects.
/// This is the standard rebuild function used during startup.
/// Derives both relation edges and content-derived edges (wiki-links,
/// block references) from each object's Markdown content.
pub fn build_graph_from_objects(
    objects: &[KnowledgeObject],
) -> (Vec<SerializedNode>, Vec<SerializedEdge>) {
    let mut nodes = Vec::with_capacity(objects.len());
    let mut edges = Vec::new();

    // Build a resolution index once for all objects
    let index = ResolutionIndex::from_objects(objects);

    for object in objects {
        nodes.push(object_to_node(object));
        edges.extend(extract_edges(object));
        edges.extend(extract_content_edges(object, &index));
    }

    (nodes, edges)
}

/// Sidecar metadata file stored alongside canonical Markdown content.
/// Contains graph-relevant metadata (title, tags, creation/update timestamps,
/// object type) without duplicating the content itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSidecar {
    pub id: uuid::Uuid,
    pub object_type: String,
    pub title: Option<String>,
    pub vault_path: Option<String>,
    pub tags: Vec<String>,
    pub content_ext: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Scan the vault for all `.nabu/*.json` sidecar files.
pub fn scan_sidecars(vault_root: &Path) -> Vec<PathBuf> {
    let mut sidecars = Vec::new();
    let sidecars_dir = vault_root.join(".nabu");
    if !sidecars_dir.is_dir() {
        return sidecars;
    }
    if let Ok(entries) = std::fs::read_dir(&sidecars_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                sidecars.push(path);
            }
        }
    }
    sidecars
}

/// Read a VaultSidecar from a JSON file on disk.
pub fn read_sidecar(path: &Path) -> Option<VaultSidecar> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<VaultSidecar>(&raw).ok()
}

/// Read the raw content of a note from its vault file path.
pub fn read_sidecar_content(vault_root: &Path, vault_path: &str) -> Option<String> {
    let path = vault_root.join(vault_path);
    std::fs::read_to_string(&path).ok()
}

/// Build a complete graph snapshot by reading canonical Markdown sources
/// from the vault. This is the startup rebuild path that derives content
/// edges from wiki-links and block references.
pub fn build_graph_from_vault(
    vault_root: &Path,
) -> Result<GraphSnapshot, String> {
    use crate::models::ObjectType;

    // Collect all objects from sidecars
    let mut objects: Vec<KnowledgeObject> = Vec::new();
    let sidecar_paths = scan_sidecars(vault_root);

    let mut sidecars: Vec<(VaultSidecar, String)> = Vec::new();

    for sidecar_path in &sidecar_paths {
        if let Some(sidecar) = read_sidecar(sidecar_path) {
            let content_str = read_sidecar_content(vault_root, sidecar.vault_path.as_deref().unwrap_or(""))
                .unwrap_or_default();
            sidecars.push((sidecar, content_str));
        }
    }

    // Build KnowledgeObjects from sidecars
    for (sidecar, content_str) in &sidecars {
        let obj_type = match sidecar.object_type.as_str() {
            "note" => ObjectType::Note,
            "template" => ObjectType::Template,
            "folder" => ObjectType::Collection,
            _ => ObjectType::Note,
        };

        let content_ext = sidecar.content_ext.as_str();
        let content = match content_ext {
            "html" => ObjectContent::RichHtml(content_str.clone()),
            "txt" => ObjectContent::PlainText(content_str.clone()),
            "uri" => ObjectContent::Uri(content_str.clone()),
            "binary" => ObjectContent::Binary {
                mime_type: "application/octet-stream".to_string(),
                data: vec![],
                filename: None,
            },
            _ => ObjectContent::Markdown(content_str.clone()),
        };

        let obj = KnowledgeObject::new(obj_type, content).with_metadata(ObjectMetadata {
            title: sidecar.title.clone(),
            vault_path: sidecar.vault_path.clone(),
            ..Default::default()
        });
        objects.push(obj);
    }

    // Build the graph snapshot using build_graph_from_objects (which
    // derives content edges from wiki-links and block references)
    let (nodes, edges) = build_graph_from_objects(&objects);

    let mut snapshot = GraphSnapshot::new(GraphVersion::rebuilt(BuildSource::Canonical));
    for node in nodes {
        snapshot.add_node(node);
    }
    for edge in edges {
        snapshot.add_edge(edge);
    }

    Ok(snapshot)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::persistence::GraphStore;
    use crate::graph::serializer::{SerializedEdge, SerializedNode};
    use crate::models::ObjectType;
    use tempfile::tempdir;

    #[test]
    fn test_recovery_empty_store() {
        let dir = tempdir().unwrap();
        let store = GraphStore::new(dir.path()).unwrap();
        let recovery = GraphRecovery::new(store);

        let result = recovery.recover();
        assert!(result.needs_rebuild());
    }

    #[test]
    fn test_recovery_valid_store() {
        let dir = tempdir().unwrap();
        let store = GraphStore::new(dir.path()).unwrap();

        // Save a valid graph
        let mut snapshot = GraphSnapshot::new(GraphVersion::new());
        let n1 = uuid::Uuid::new_v4();
        let n2 = uuid::Uuid::new_v4();
        snapshot.add_node(SerializedNode::new(n1, "note", Some("A".into()), "text"));
        snapshot.add_node(SerializedNode::new(n2, "note", Some("B".into()), "text"));
        snapshot.add_edge(SerializedEdge::new(n1, n2, "references"));
        store.save(&snapshot).unwrap();

        let recovery = GraphRecovery::new(store);
        let result = recovery.recover();

        match result {
            RecoveryResult::Recovered(s) => {
                assert_eq!(s.node_count(), 2);
            }
            other => panic!("Expected Recovered, got {:?}", other),
        }
    }

    #[test]
    fn test_full_rebuild() {
        let dir = tempdir().unwrap();
        let store = GraphStore::new(dir.path()).unwrap();
        let recovery = GraphRecovery::new(store);

        let rebuild_fn = || -> Result<(Vec<SerializedNode>, Vec<SerializedEdge>), String> {
            let mut nodes = Vec::new();
            let mut edges = Vec::new();

            let n1 = uuid::Uuid::new_v4();
            let n2 = uuid::Uuid::new_v4();

            nodes.push(SerializedNode::new(
                n1,
                "note",
                Some("Rebuilt A".into()),
                "text",
            ));
            nodes.push(SerializedNode::new(
                n2,
                "note",
                Some("Rebuilt B".into()),
                "text",
            ));
            edges.push(SerializedEdge::new(n1, n2, "references"));

            Ok((nodes, edges))
        };

        let result = recovery
            .rebuild(rebuild_fn, BuildSource::Canonical)
            .unwrap();
        assert_eq!(result.node_count(), 2);
        assert_eq!(result.edge_count(), 1);
    }

    #[test]
    fn test_object_to_node_conversion() {
        let object = KnowledgeObject::new(
            ObjectType::Note,
            crate::models::ObjectContent::Markdown("Test".to_string()),
        );

        let node = object_to_node(&object);
        assert_eq!(node.id, object.id);
        assert_eq!(node.object_type, "note");
        assert_eq!(node.content_hint, "text/markdown");
    }

    #[test]
    fn test_rebuild_from_objects() {
        let obj1 = KnowledgeObject::new(
            ObjectType::Note,
            crate::models::ObjectContent::Markdown("A".to_string()),
        );
        let obj2 = KnowledgeObject::new(
            ObjectType::Note,
            crate::models::ObjectContent::Markdown("B".to_string()),
        );

        let (nodes, edges) = build_graph_from_objects(&[obj1, obj2]);
        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 0); // No relations defined
    }
}
