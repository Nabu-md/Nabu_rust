use nabu_core::graph::VaultGraph;
use nabu_core::models::{KnowledgeObject, ObjectContent, ObjectType};
use tempfile::tempdir;

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

#[test]
fn debug_persistence_roundtrip() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    let obj_a = make_note("Persist A", "Links [[Persist B]].", Some("Persist A.md"));
    let obj_b = make_note("Persist B", "Body.", Some("Persist B.md"));

    eprintln!("obj_a id: {}, obj_b id: {}", obj_a.id, obj_b.id);

    {
        let graph =
            VaultGraph::with_persistence(None, vault.clone()).expect("graph with persistence");
        eprintln!("Before rebuild: node_count={}, edge_count={}", graph.node_count(), graph.edge_count());
        
        graph
            .rebuild_from_objects(&[obj_a.clone(), obj_b.clone()])
            .expect("rebuild");
        
        eprintln!("After rebuild: node_count={}, edge_count={}", graph.node_count(), graph.edge_count());
        eprintln!("loaded_from_disk: {}", graph.loaded_from_disk());
    }

    // Check file on disk
    let graph_file = vault.join(".nabu").join("graph").join("graph.json");
    eprintln!("Graph file exists: {}", graph_file.exists());
    if graph_file.exists() {
        let content = std::fs::read_to_string(&graph_file).unwrap();
        eprintln!("File content ({} bytes): {}", content.len(), content);
    }

    // Reopen
    {
        let graph =
            VaultGraph::with_persistence(None, vault).expect("graph reopen");
        eprintln!("After reopen: node_count={}, edge_count={}", graph.node_count(), graph.edge_count());
        eprintln!("loaded_from_disk: {}", graph.loaded_from_disk());
        
        if graph.node_count() == 2 {
            let nodes = graph.all_nodes();
            for n in &nodes {
                eprintln!("  node: id={}, title={:?}", n.id, n.metadata.title);
            }
        }
    }
}
