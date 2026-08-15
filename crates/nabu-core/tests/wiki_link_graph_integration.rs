//! Integration tests for wiki-link content → graph edge derivation.
//!
//! Verifies that real wiki-link content (`[[Note Title]]`) and block references
//! (`((ref))`) in KnowledgeObject Markdown bodies produce real graph edges
//! when processed through the public graph-building APIs:
//!
//! - `build_graph_from_objects` (pure function → SerializedNode/SerializedEdge)
//! - `VaultGraph::rebuild_from_objects` (full graph rebuild → in-memory edges)
//! - `VaultGraph::update_node` (content edge derivation on update)
//! - `build_graph_from_vault` (full disk-based rebuild from sidecars + content files)
//!
//! These tests exercise the public API boundary only — no private internals.
```

use nabu_core::graph::{
    build_graph_from_objects, build_graph_from_vault, VaultGraph, ResolutionIndex,
    parse_wiki_links, parse_block_references, content_as_str,
    SerializedEdge, extract_content_edges, object_to_node,
    VaultSidecar, BuildSource,
};
use nabu_core::models::{KnowledgeObject, ObjectContent, ObjectMetadata, ObjectType};
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a Markdown note with deterministic title, content, and vault path.
fn make_note(title: &str, content: &str, vault_path: Option<&str>) -> KnowledgeObject {
    let mut obj = KnowledgeObject::new(
        ObjectType::Note,
        ObjectContent::Markdown(content.to_string()),
    );
    obj.metadata.title = Some(title.to_string());
    if let Some(p) = vault_path {
        obj.metadata.vault_path = Some(p.to_string());
    }
    obj
}

// ---------------------------------------------------------------------------
// 1. build_graph_from_objects — pure function tests
// ---------------------------------------------------------------------------

/// Two notes with mutual wiki-links produce two content-derived edges
/// (A→B and B→A), each tagged `content_derived = true` with relationship
/// "references".
#[test]
fn build_graph_bidirectional_wiki_links() {
    let obj_a = make_note("Note A", "See [[Note B]] for context.", Some("Note A.md"));
    let obj_b = make_note("Note B", "Back link to [[Note A]].", Some("Note B.md"));

    let (nodes, edges) = build_graph_from_objects(&[obj_a.clone(), obj_b.clone()]);

    assert_eq!(nodes.len(), 2, "two nodes expected");
    assert_eq!(
        edges.len(), 2,
        "expected two content-derived edges (A→B and B→A), got {}",
        edges.len()
    );

    // Both edges must be content-derived with relationship "references".
    for edge in &edges {
        assert!(
            edge.content_derived,
            "edge {} → {} should be content-derived",
            edge.source, edge.target
        );
        assert_eq!(edge.relationship, "references");
    }

    // Verify the specific A→B edge.
    let a_to_b = edges.iter().find(|e| e.source == obj_a.id && e.target == obj_b.id);
    assert!(a_to_b.is_some(), "expected edge A→B from wiki-link");
    let b_to_a = edges.iter().find(|e| e.source == obj_b.id && e.target == obj_a.id);
    assert!(b_to_a.is_some(), "expected edge B→A from wiki-link");
}

/// A wiki-link to an unresolved target (no matching title or path stem)
/// produces zero edges.
#[test]
fn build_graph_unresolved_wiki_link_produces_no_edge() {
    let obj = make_note("Lonely Note", "Reference [[Ghost Note]] here.", Some("Lonely.md"));

    let (nodes, edges) = build_graph_from_objects(&[obj]);

    assert_eq!(nodes.len(), 1, "one node expected");
    assert!(
        edges.is_empty(),
        "unresolved wiki-link should produce no edges, got {}",
        edges.len()
    );
}

/// A self-referencing wiki-link (a note linking to itself by title) must NOT
/// create a self-edge — `extract_content_edges` explicitly skips `target_id == object.id`.
#[test]
fn build_graph_self_reference_excluded() {
    let obj = make_note("Self Ref", "See [[Self Ref]] for more.", Some("Self Ref.md"));

    let (_, edges) = build_graph_from_objects(&[obj.clone()]);

    let self_edges = edges.iter().filter(|e| e.source == obj.id && e.target == obj.id);
    assert_eq!(
        self_edges.count(),
        0,
        "self-referencing wiki-link must not create a self-edge"
    );
}

/// Block-reference syntax `((target))` creates edges with relationship
/// "block_reference" and `content_derived = true`.
#[test]
fn build_graph_block_reference_edges() {
    let obj_a = make_note(
        "Block Reader",
        "Transclude ((Target Block)) here.",
        Some("Block Reader.md"),
    );
    let obj_b = make_note("Target Block", "The content to embed.", Some("Target Block.md"));

    let (_, edges) = build_graph_from_objects(&[obj_a.clone(), obj_b]);

    let block_edges: Vec<_> = edges
        .iter()
        .filter(|e| e.relationship == "block_reference")
        .collect();
    assert_eq!(block_edges.len(), 1, "expected one block_reference edge");
    assert!(block_edges[0].content_derived, "block_reference edge must be content-derived");
    assert_eq!(block_edges[0].source, obj_a.id);
    assert_eq!(block_edges[0].target, obj_b.id);
}

/// Multiple wiki-links in a single note resolve to multiple edges.
#[test]
fn build_graph_multiple_wiki_links() {
    let obj_a = make_note("Hub", "Links: [[Note B]] and [[Note C]] and [[Note B]] again.", Some("Hub.md"));
    let obj_b = make_note("Note B", "B content.", Some("Note B.md"));
    let obj_c = make_note("Note C", "C content.", Some("Note C.md"));

    let (_, edges) = build_graph_from_objects(&[obj_a.clone(), obj_b.clone(), obj_c]);

    let refs: Vec<_> = edges.iter().filter(|e| e.relationship == "references").collect();
    // Two distinct targets (B and C), even though B is linked twice.
    let targets: Vec<Uuid> = refs.iter().map(|e| e.target).collect();
    assert!(
        targets.contains(&obj_b.id) && targets.contains(&obj_c.id),
        "expected edges to both B and C, got targets: {:?}",
        targets
    );
    assert_eq!(
        refs.len(),
        2,
        "expected exactly 2 reference edges (duplicate B link is deduplicated), got {}",
        refs.len()
    );
}

/// Wiki-links resolve by title and by path stem (case-insensitive).
#[test]
fn build_graph_resolution_by_title_and_path_stem() {
    let obj_by_title = make_note("Path Stem Note", "body", Some("custom-stem-name.md"));
    let obj_linker = make_note("Linker", "[[path-stem-name]]", Some("Linker.md"));

    let (_, edges) = build_graph_from_objects(&[obj_by_title.clone(), obj_linker]);

    assert_eq!(
        edges.len(),
        1,
        "wiki-link should resolve by path stem, got {} edges",
        edges.len()
    );
    assert_eq!(edges[0].target, obj_by_title.id);
}

// ---------------------------------------------------------------------------
// 2. VaultGraph::rebuild_from_objects — full graph rebuild tests
// ---------------------------------------------------------------------------

/// After `rebuild_from_objects`, the VaultGraph should contain nodes and
/// content-derived edges that match the wiki-link content in the source
/// Note objects.
#[test]
fn vaultgraph_rebuild_derives_content_edges() {
    let graph = VaultGraph::new();

    let obj_a = make_note("Graph Note A", "See [[Graph Note B]] for context.", Some("Graph Note A.md"));
    let obj_b = make_note("Graph Note B", "Referenced by [[Graph Note A]].", Some("Graph Note B.md"));

    let objs = vec![obj_a.clone(), obj_b.clone()];
    graph.rebuild_from_objects(&objs).expect("rebuild must succeed");

    assert_eq!(graph.node_count(), 2, "graph should have 2 nodes");
    assert_eq!(graph.edge_count(), 2, "graph should have 2 edges (bidirectional wiki-links)");

    // Verify directional queries.
    let linked = graph.linked_notes(obj_a.id);
    assert!(
        linked.contains(&obj_b.id),
        "linked_notes(A) should contain B"
    );
    let incoming = graph.incoming_links(obj_b.id);
    assert!(
        incoming.contains(&obj_a.id),
        "incoming_links(B) should contain A"
    );

    // Verify content-derived flag on edges.
    let edges = graph.edges();
    let content_edges: Vec<_> = edges.iter().filter(|e| e.content_derived).collect();
    assert_eq!(content_edges.len(), 2, "all edges should be content-derived");
}

/// `rebuild_from_objects` clears the graph first — calling it twice with
/// different objects should leave only the new set.
#[test]
fn vaultgraph_rebuild_replaces_existing_state() {
    let graph = VaultGraph::new();

    let obj1 = make_note("Old Note", "body", Some("Old Note.md"));
    graph.rebuild_from_objects(&[obj1.clone()]).unwrap();
    assert_eq!(graph.node_count(), 1);

    let obj2 = make_note("New Note", "body", Some("New Note.md"));
    let obj3 = make_note("Another", "body", Some("Another.md"));
    graph.rebuild_from_objects(&[obj2, obj3]).unwrap();

    assert_eq!(graph.node_count(), 2, "should have only the 2 new nodes");
    assert_eq!(graph.edge_count(), 0, "no edges between new notes");
    assert!(!graph.has_node(obj1.id), "old node should be removed");
}

/// `rebuild_from_objects` with persistent storage writes to disk and a
/// second `VaultGraph::with_persistence` reconstructs the same edge set.
#[test]
fn vaultgraph_rebuild_persists_and_reloads() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    let obj_a = make_note("Persist A", "Links [[Persist B]].", Some("Persist A.md"));
    let obj_b = make_note("Persist B", "Body.", Some("Persist B.md"));

    {
        let graph =
            VaultGraph::with_persistence(None, vault.clone()).expect("graph with persistence");
        graph
            .rebuild_from_objects(&[obj_a.clone(), obj_b.clone()])
            .expect("rebuild");
    }

    // Reopen — should load from `.nabu/graph/graph.json`.
    {
        let graph =
            VaultGraph::with_persistence(None, vault).expect("graph reopen");
        assert!(
            graph.loaded_from_disk(),
            "graph should have loaded from disk on reopen"
        );
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 2);
        assert!(
            graph.has_edge(obj_a.id, obj_b.id, "references"),
            "wiki-link edge must survive persistence round-trip"
        );
        assert!(
            graph.has_edge(obj_b.id, obj_a.id, "references"),
            "reverse wiki-link edge must survive persistence round-trip"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. VaultGraph::update_node — per-note content edge derivation
// ---------------------------------------------------------------------------

/// `update_node` on a note whose content contains wiki-links materialises
/// edges into its neighbours.
#[test]
fn vaultgraph_update_node_derives_wiki_link_edges() {
    let graph = VaultGraph::new();

    let obj_a = make_note("Updater A", "[[Updater B]]", Some("Updater A.md"));
    let obj_b = make_note("Updater B", "body", Some("Updater B.md"));

    graph.add_node(&obj_a).unwrap();
    graph.add_node(&obj_b).unwrap();
    graph.update_node(&obj_a).unwrap();

    assert!(
        graph.has_edge(obj_a.id, obj_b.id, "references"),
        "update_node should derive a wiki-link edge"
    );
    assert_eq!(graph.edge_count(), 1);
}

/// Updating a note removes stale content-derived edges and adds new ones
/// (reconciliation).
#[test]
fn vaultgraph_update_node_replaces_stale_edges() {
    let graph = VaultGraph::new();

    let obj_a = make_note("Switcher A", "[[Switcher B]]", Some("Switcher A.md"));
    let obj_b = make_note("Switcher B", "body", Some("Switcher B.md"));
    let obj_c = make_note("Switcher C", "body", Some("Switcher C.md"));

    graph.add_node(&obj_b).unwrap();
    graph.add_node(&obj_c).unwrap();
    graph.update_node(&obj_a).unwrap();

    assert!(graph.has_edge(obj_a.id, obj_b.id, "references"));
    assert!(!graph.has_edge(obj_a.id, obj_c.id, "references"));

    // Re-point A to C.
    let mut obj_a2 = obj_a.clone();
    obj_a2.content = ObjectContent::Markdown("[[Switcher C]]".to_string());
    graph.update_node(&obj_a2).unwrap();

    assert!(
        !graph.has_edge(obj_a.id, obj_b.id, "references"),
        "old wiki-link edge A→B should be removed on update"
    );
    assert!(
        graph.has_edge(obj_a.id, obj_c.id, "references"),
        "new wiki-link edge A→C should be created on update"
    );
}

// ---------------------------------------------------------------------------
// 4. build_graph_from_vault — full disk-based rebuild
// ---------------------------------------------------------------------------

/// Write a sidecar JSON file to `vault/.nabu/<id>.json`.
fn write_sidecar(vault_root: &std::path::Path, id: Uuid, title: &str, vault_path: &str) {
    let sidecar = VaultSidecar {
        id,
        object_type: "note".to_string(),
        title: Some(title.to_string()),
        vault_path: Some(vault_path.to_string()),
        tags: vec![],
        content_ext: "md".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string_pretty(&sidecar).expect("sidecar must serialize");
    let sidecar_dir = vault_root.join(".nabu");
    std::fs::create_dir_all(&sidecar_dir).expect("create .nabu dir");
    std::fs::write(sidecar_dir.join(format!("{}.json", id)), json).expect("write sidecar");
}

/// Full disk-based rebuild: write Markdown content files + sidecar JSON files
/// to a temp vault, then call `build_graph_from_vault` and verify that
/// wiki-link edges are derived from the on-disk content.
#[test]
fn build_graph_from_vault_derives_wiki_link_edges() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    // Write content files with wiki-links.
    let content_dir = vault.join("notes");
    std::fs::create_dir_all(&content_dir).unwrap();
    std::fs::write(
        content_dir.join("Note A.md"),
        "See [[Note B]] for the full story.",
    )
    .unwrap();
    std::fs::write(
        content_dir.join("Note B.md"),
        "Referenced by [[Note A]] here.",
    )
    .unwrap();

    // Write sidecars.
    write_sidecar(&vault, id_a, "Note A", "notes/Note A.md");
    write_sidecar(&vault, id_b, "Note B", "notes/Note B.md");

    let snapshot = build_graph_from_vault(&vault).expect("vault rebuild must succeed");

    assert_eq!(snapshot.node_count(), 2, "expected 2 nodes from vault");
    assert_eq!(
        snapshot.edge_count(),
        2,
        "expected 2 content-derived edges (bidirectional wiki-links)"
    );

    let content_edges: Vec<_> = snapshot
        .edges
        .iter()
        .filter(|e| e.content_derived && e.relationship == "references")
        .collect();
    assert_eq!(
        content_edges.len(),
        2,
        "expected 2 content-derived reference edges"
    );

    let a_to_b = content_edges
        .iter()
        .find(|e| e.source == id_a && e.target == id_b);
    assert!(a_to_b.is_some(), "expected A→B edge from disk content");
    let b_to_a = content_edges
        .iter()
        .find(|e| e.source == id_b && e.target == id_a);
    assert!(b_to_a.is_some(), "expected B→A edge from disk content");
}

// ---------------------------------------------------------------------------
// 5. ResolutionIndex & parse helpers
// ---------------------------------------------------------------------------

/// ResolutionIndex resolves by both title (case-insensitive) and path stem.
#[test]
fn resolution_index_case_insensitive_and_path_stem() {
    let id = Uuid::new_v4();
    let obj = make_note("My Fancy Note", "body", Some("path/to/my-fancy-note.md"));

    let index = ResolutionIndex::from_objects(&[obj]);

    // Title resolution (case-insensitive).
    assert_eq!(
        index.resolve_wiki_link("My Fancy Note"),
        Some(id),
        "should resolve by exact title"
    );
    assert_eq!(
        index.resolve_wiki_link("my fancy note"),
        Some(id),
        "should resolve by lowercased title"
    );

    // Path stem resolution.
    assert_eq!(
        index.resolve_wiki_link("my-fancy-note"),
        Some(id),
        "should resolve by path stem"
    );

    // Block ref resolution uses the same index.
    assert_eq!(
        index.resolve_block_ref("My Fancy Note"),
        Some(id),
        "should resolve block ref by title"
    );

    // Unresolved.
    assert_eq!(
        index.resolve_wiki_link("Does Not Exist"),
        None,
        "unresolved link should return None"
    );
}

/// `parse_wiki_links` handles inline code spans with embedded brackets.
#[test]
fn parse_wiki_links_respects_code_spans() {
    let text = r#"See [[Real Note]] and `[[fake note]]` here."#;
    let links = parse_wiki_links(text);
    assert_eq!(links, vec!["Real Note".to_string()], "should skip bracket-like content in code spans");
}

/// `parse_wiki_links` returns an empty vector when no wiki-links are present.
#[test]
fn parse_wiki_links_empty_when_no_links() {
    let links = parse_wiki_links("just plain text with no links");
    assert!(links.is_empty());
}

/// `parse_block_references` handles multiple `((ref))` patterns.
#[test]
fn parse_block_references_multiple() {
    let text = "embed ((ref-1)) and ((ref-2)) here";
    let refs = parse_block_references(text);
    assert_eq!(refs, vec!["ref-1".to_string(), "ref-2".to_string()]);
}

/// `content_as_str` projects text content variants to `&str` and returns
/// empty for binary.
#[test]
fn content_as_str_projects_text_variants() {
    let md = ObjectContent::Markdown("markdown body".to_string());
    assert_eq!(content_as_str(&md), "markdown body");

    let plain = ObjectContent::PlainText("plain text".to_string());
    assert_eq!(content_as_str(&plain), "plain text");

    let uri = ObjectContent::Uri("https://example.com".to_string());
    assert_eq!(content_as_str(&uri), "https://example.com");

    let bin = ObjectContent::Binary {
        mime_type: "image/png".to_string(),
        data: vec![1, 2, 3],
        filename: None,
    };
    assert_eq!(content_as_str(&bin), "", "binary content has no text body");
}

// ---------------------------------------------------------------------------
// 6. object_to_node — node serialization preserves metadata
// ---------------------------------------------------------------------------

/// `object_to_node` serializes a KnowledgeObject into a SerializedNode with
/// the correct type, title, and content hint.
#[test]
fn object_to_node_preserves_metadata() {
    let obj = make_note("Serialized Node", "# Title\n\nBody.", Some("Serialized.md"));
    let node = object_to_node(&obj);

    assert_eq!(node.id, obj.id);
    assert_eq!(node.object_type, "note");
    assert_eq!(node.title, obj.metadata.title);
    assert_eq!(node.content_hint, "text/markdown");
    assert!(node.properties.contains_key("created_at"));
}

// ---------------------------------------------------------------------------
// 7. Edge cases: mixed relations + content edges
// ---------------------------------------------------------------------------

/// Objects with explicit `ObjectRelation`s PLUS wiki-link content in their
/// bodies get both kinds of edges: relation edges (content_derived=false) and
/// content-derived edges (content_derived=true).
#[test]
fn build_graph_mixed_relation_and_content_edges() {
    use nabu_core::models::{ObjectRelation, RelationType};

    let target_id = Uuid::new_v4();
    let obj_a = make_note("Source", "[[Target Wiki]]", Some("Source.md"));
    let mut obj_b = make_note("Target Wiki", "body", Some("Target.md"));
    obj_b.relations.push(ObjectRelation {
        target_id,
        relation_type: RelationType::Related,
        label: None,
    });

    let (nodes, edges) = build_graph_from_objects(&[obj_a.clone(), obj_b.clone()]);

    assert_eq!(nodes.len(), 2);

    // Relation edge from obj_b → target_id (explicit relation).
    let relation_edges: Vec<_> = edges.iter().filter(|e| !e.content_derived).collect();
    assert_eq!(relation_edges.len(), 1, "expected 1 explicit relation edge");
    assert_eq!(relation_edges[0].source, obj_b.id);
    assert_eq!(relation_edges[0].target, target_id);

    // Content edge from obj_a → obj_b (wiki-link).
    let content_edges: Vec<_> = edges.iter().filter(|e| e.content_derived).collect();
    assert_eq!(content_edges.len(), 1, "expected 1 content-derived edge");
    assert_eq!(content_edges[0].source, obj_a.id);
    assert_eq!(content_edges[0].target, obj_b.id);
}
