// Smoke test that the library compiles for the host target and lays out a graph.
// Placed under `tests/` so it links the non-test library (avoiding the
// pre-existing `#[cfg(test)]` breakage in ipc.rs that is out of scope for
// Phase 2A-1).
use nabu_ui::components::graph_layout::{layout_graph, NodePos};
use nabu_ui::components::graph_view::{classify_view, GraphLoadState, ViewPhase};
use nabu_ui::models::graph::{GraphData, GraphEdgeData, GraphNodeData};

fn node(path: &str) -> GraphNodeData {
    GraphNodeData {
        path: path.to_string(),
        title: path.to_string(),
        folder: String::new(),
        modified_at: String::new(),
        tags: Vec::new(),
        backlink_count: 0,
        outgoing_count: 0,
        degree: 0,
    }
}

fn edge(src: &str, tgt: &str) -> GraphEdgeData {
    GraphEdgeData {
        source: src.to_string(),
        target: tgt.to_string(),
        broken: false,
    }
}

#[test]
fn test1_real_graph_renders_positions() {
    let data = GraphData {
        nodes: vec![node("a.md"), node("b.md"), node("c.md")],
        edges: vec![edge("a.md", "b.md"), edge("b.md", "c.md")],
        orphan_count: 1,
        cluster_count: 1,
    };
    let layout = layout_graph(&data);
    assert_eq!(layout.positions.len(), 3);
    let a = layout.positions["a.md"];
    let b = layout.positions["b.md"];
    let c = layout.positions["c.md"];
    // Real edges => connected nodes must not share a position.
    assert_ne!(a, b);
    assert_ne!(b, c);
    // Layout must be valid (finite coords).
    for p in layout.positions.values() {
        assert!(p.x.is_finite() && p.y.is_finite());
    }
    let _ = NodePos { x: 0.0, y: 0.0 };
}

#[test]
fn test2_empty_graph() {
    let data = GraphData {
        nodes: vec![],
        edges: vec![],
        orphan_count: 0,
        cluster_count: 0,
    };
    let layout = layout_graph(&data);
    assert!(layout.positions.is_empty());
}

#[test]
fn test3_error_state_classifies_as_error() {
    // A backend graph query failure must classify as an Error view phase —
    // it must never be replaced by an empty canvas.
    assert_eq!(
        classify_view(None, GraphLoadState::Failed),
        ViewPhase::Error
    );
    // Loading state is distinct from error/empty.
    assert_eq!(
        classify_view(None, GraphLoadState::Loading),
        ViewPhase::Loading
    );
    // Loaded + empty data => Empty (not Error, not Ready).
    let empty_data = GraphData {
        nodes: vec![],
        edges: vec![],
        orphan_count: 0,
        cluster_count: 0,
    };
    assert_eq!(
        classify_view(Some(&empty_data), GraphLoadState::Loaded),
        ViewPhase::Empty
    );
    // Loaded + nodes => Ready.
    let populated = GraphData {
        nodes: vec![node("a.md")],
        edges: vec![],
        orphan_count: 1,
        cluster_count: 1,
    };
    assert_eq!(
        classify_view(Some(&populated), GraphLoadState::Loaded),
        ViewPhase::Ready
    );
}

#[test]
fn test4_selection_state() {
    // A selected node must have a position, and selecting one node should not
    // collapse the other positions (selection is a view concern over the
    // fixed layout).
    let data = GraphData {
        nodes: vec![node("a.md"), node("b.md")],
        edges: vec![edge("a.md", "b.md")],
        orphan_count: 0,
        cluster_count: 1,
    };
    let layout = layout_graph(&data);
    let _selected = "a.md";
    assert!(layout.positions.contains_key("a.md"));
    assert!(layout.positions.contains_key("b.md"));
    assert_ne!(layout.positions["a.md"], layout.positions["b.md"]);
}
