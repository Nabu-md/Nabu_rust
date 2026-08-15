//! # Graph layout — pure, deterministic positioning for the knowledge graph
//!
//! The layout engine is intentionally dependency-free and runs on the main
//! thread only when graph data changes (never on every render frame). It turns
//! a [`GraphData`] payload into a set of fixed `(x, y)` positions keyed by node
//! path, producing a usable static layout that satisfies the requirements:
//!
//! * a few nodes — well-spaced, readable;
//! * many nodes — components are spread out so they don't overlap;
//! * high-degree nodes — spring repulsion prevents clustering;
//! * disconnected components — each component is laid out independently and
//!   placed on a virtual grid, so no two components share a position.
//!
//! There is **no animation loop**: positions are computed once per data load
//! and stay fixed. Pan/zoom is handled by the view via CSS transforms, exactly
//! like the existing canvas component pattern.

use crate::models::graph::GraphData;
#[cfg(test)]
use crate::models::graph::{GraphEdgeData, GraphNodeData};
use std::collections::{HashMap, HashSet, VecDeque};

/// Position of a node after layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodePos {
    pub x: f64,
    pub y: f64,
}

/// Result of laying out a graph: a position per node path, plus the bounding
/// box of all nodes (useful for viewport centering and fit-to-screen).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    /// Node path → position.
    pub positions: HashMap<String, NodePos>,
    /// Logical bounds of the laid-out graph in canvas coordinates.
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

/// Spacing constants for the layout.
const NODE_SPACING: f64 = 220.0; // ideal distance between connected nodes
const REPULSION_RADIUS: f64 = 80.0; // distance at which node-node repulsion fades
const REPULSION_STRENGTH: f64 = 4000.0; // repulsion impulse scale
const ATTRACTION_STRENGTH: f64 = 0.06; // spring pull toward ideal edge length
const RELAX_ITERATIONS: usize = 40; // fixed relaxation passes — no animation loop
const ORPHAN_SPREAD: f64 = 320.0; // spacing between orphan nodes
const COMPONENT_GAP: f64 = 900.0; // spacing between disconnected components

/// Compute a full layout for the given graph data.
///
/// This is the single entry point the view calls when data loads. The result
/// is cached by the caller for the lifetime of the data so the expensive work
/// is done once per load, not per frame.
pub fn layout_graph(data: &GraphData) -> Layout {
    if data.nodes.is_empty() {
        return Layout::default();
    }

    // Index nodes by path for O(1) lookup. Edges use paths as identities.
    let mut positions: HashMap<String, NodePos> = HashMap::with_capacity(data.nodes.len());
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    let mut degree: HashMap<String, usize> = HashMap::new();
    for node in &data.nodes {
        positions.insert(node.path.clone(), NodePos { x: 0.0, y: 0.0 });
        adjacency.entry(node.path.clone()).or_default();
        degree.insert(node.path.clone(), node.degree);
    }

    // Build adjacency from real edges. Broken edges (target doesn't resolve
    // to a known node) are skipped — they aren't part of the node graph.
    let node_set: HashSet<&String> = positions.keys().collect();
    let mut real_edges: Vec<(&str, &str)> = Vec::new();
    for edge in &data.edges {
        if edge.broken {
            continue;
        }
        if node_set.contains(&edge.source) && node_set.contains(&edge.target) {
            adjacency
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
            adjacency
                .entry(edge.target.clone())
                .or_default()
                .push(edge.source.clone());
            real_edges.push((&edge.source, &edge.target));
        }
    }

    // --- Connected components ---
    let components =
        connected_components(&positions.keys().cloned().collect::<Vec<_>>(), &adjacency);

    // --- Layout each component independently ---
    let mut origin_x = 0.0_f64;
    let mut origin_y = 0.0_f64;
    let mut row_height = 0.0_f64;
    let mut column_width = 0.0_f64;
    let mut grid_col = 0_usize;
    let per_row = 3; // components per row before wrapping
    let mut placed = 0_usize;

    for comp in &components {
        let comp_positions = layout_component(comp, &adjacency, &real_edges, &degree);
        // Compute this component's local bounds.
        let (min_x, max_x, min_y, max_y) = comp_bounds(comp, &comp_positions);
        let w = max_x - min_x;
        let h = max_y - min_y;
        // Translate so the component's top-left is at (origin_x, origin_y).
        let dx = origin_x - min_x;
        let dy = origin_y - min_y;
        for path in comp {
            if let Some(p) = comp_positions.get(path) {
                let pos = positions.get_mut(path).unwrap();
                pos.x = p.x + dx;
                pos.y = p.y + dy;
            }
        }
        row_height = row_height.max(h);
        column_width = column_width.max(w);
        placed += 1;

        if placed >= per_row {
            // Wrap to a new row.
            origin_x = 0.0;
            origin_y += row_height + COMPONENT_GAP;
            row_height = 0.0;
            column_width = 0.0;
            placed = 0;
            grid_col = 0;
        } else {
            origin_x += column_width + COMPONENT_GAP;
            grid_col += 1;
        }
    }

    // --- Compute global bounds for viewport centering ---
    let (min_x, max_x, min_y, max_y) = positions.values().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |acc, p| {
            (
                acc.0.min(p.x),
                acc.1.max(p.x),
                acc.2.min(p.y),
                acc.3.max(p.y),
            )
        },
    );

    Layout {
        positions,
        bounds: Bounds {
            min_x,
            max_x,
            min_y,
            max_y,
        },
    }
}

/// Lay out a single connected component.
///
/// Uses a layered approach: pick the highest-degree node as the root, fan its
/// neighbours in a circle, then relax with a fixed number of spring-physics
/// passes (attraction along edges, repulsion between nearby nodes). Fixed
/// iteration count guarantees termination and bounded CPU.
fn layout_component(
    nodes: &[String],
    adjacency: &HashMap<String, Vec<String>>,
    edges: &[(&str, &str)],
    degree: &HashMap<String, usize>,
) -> HashMap<String, NodePos> {
    let mut pos: HashMap<String, NodePos> = HashMap::with_capacity(nodes.len());

    if nodes.is_empty() {
        return pos;
    }

    if nodes.len() == 1 {
        pos.insert(nodes[0].clone(), NodePos { x: 0.0, y: 0.0 });
        return pos;
    }

    // Choose root: highest degree (ties broken by path for determinism).
    let root = nodes
        .iter()
        .max_by_key(|p| {
            (
                degree.get(*p).copied().unwrap_or(0),
                std::cmp::Reverse(p.clone()),
            )
        })
        .cloned()
        .unwrap();

    // BFS to assign layers (shortest path from root).
    let mut layer: HashMap<String, usize> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    layer.insert(root.clone(), 0);
    queue.push_back(root.clone());
    while let Some(current) = queue.pop_front() {
        let l = layer[&current] + 1;
        for neighbor in adjacency.get(&current).into_iter().flatten() {
            if !layer.contains_key(neighbor) {
                layer.insert(neighbor.clone(), l);
                queue.push_back(neighbor.clone());
            }
        }
    }

    // Initial placement: each layer on a concentric circle around the root.
    // Root at center; layer-1 nodes on a circle of radius NODE_SPACING; each
    // subsequent layer adds another ring. Nodes in the same layer are spread
    // evenly around the ring.
    let max_layer = layer.values().copied().max().unwrap_or(0);
    // Group nodes by layer.
    let mut by_layer: HashMap<usize, Vec<String>> = HashMap::new();
    for (path, &l) in &layer {
        by_layer.entry(l).or_default().push(path.clone());
    }
    for (_l, group) in &mut by_layer {
        group.sort(); // deterministic ordering
    }

    pos.insert(root.clone(), NodePos { x: 0.0, y: 0.0 });

    let layer_radius = NODE_SPACING * 0.9;
    for l in 1..=max_layer {
        let group = match by_layer.get(&(l as usize)) {
            Some(g) => g,
            None => continue,
        };
        let radius = layer_radius * (l as f64);
        let n = group.len() as f64;
        for (i, path) in group.iter().enumerate() {
            // Stagger so nodes don't stack. Use golden-ratio-ish angle offset
            // per layer for determinism without collisions.
            let angle = (2.0 * std::f64::consts::PI * i as f64 / n) + (l as f64 * 0.7);
            let jiggle = (l as f64) * 13.0_f64.sin();
            pos.insert(
                path.clone(),
                NodePos {
                    x: radius * angle.cos() + jiggle,
                    y: radius * angle.sin() - jiggle,
                },
            );
        }
    }

    // --- Spring relaxation (fixed iterations) ---
    // Attraction along edges (Hooke's law toward ideal length), repulsion
    // between all pairs within REPULSION_RADIUS. Bounded by RELAX_ITERATIONS
    // so there is never an uncontrolled loop.
    relax(
        &mut pos,
        edges,
        REPULSION_RADIUS,
        REPULSION_STRENGTH,
        ATTRACTION_STRENGTH,
        NODE_SPACING,
        RELAX_ITERATIONS,
    );

    pos
}

/// Fixed-iteration spring relaxation. No animation, no `requestAnimationFrame`.
///
/// Uses index-based displacement arrays so the borrow checker is satisfied
/// without interior mutability gymnastics — two different indices can both be
/// read/written without aliasing.
fn relax(
    positions: &mut HashMap<String, NodePos>,
    edges: &[(&str, &str)],
    repulsion_radius: f64,
    repulsion_strength: f64,
    attraction_strength: f64,
    ideal_length: f64,
    iterations: usize,
) {
    if positions.is_empty() || iterations == 0 {
        return;
    }
    // Stable ordering so indices map deterministically to paths.
    let paths: Vec<String> = {
        let mut keys: Vec<String> = positions.keys().cloned().collect();
        keys.sort();
        keys
    };
    let n = paths.len();
    let mut index_of: HashMap<String, usize> = HashMap::with_capacity(n);
    for (i, p) in paths.iter().enumerate() {
        index_of.insert(p.clone(), i);
    }
    // Current positions as a parallel array for fast indexed access.
    let mut pts: Vec<NodePos> = paths.iter().map(|p| positions[p]).collect();
    // Displacement accumulator, indexed.
    let mut disp: Vec<(f64, f64)> = vec![(0.0, 0.0); n];

    for _ in 0..iterations {
        // Reset displacement.
        for d in disp.iter_mut() {
            *d = (0.0, 0.0);
        }

        // Repulsion: all pairs within radius.
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pts[i].x - pts[j].x;
                let dy = pts[i].y - pts[j].y;
                let dist_sq = dx * dx + dy * dy;
                let (fx, fy) = if dist_sq < 1.0 {
                    // Coincident nodes — nudge apart deterministically.
                    (1.0, 0.0)
                } else if dist_sq > repulsion_radius * repulsion_radius {
                    continue;
                } else {
                    let dist = dist_sq.sqrt();
                    let force = repulsion_strength / (dist * dist);
                    ((dx / dist) * force, (dy / dist) * force)
                };
                disp[i].0 += fx;
                disp[i].1 += fy;
                disp[j].0 -= fx;
                disp[j].1 -= fy;
            }
        }

        // Attraction along edges.
        for (src, tgt) in edges {
            if let (Some(&si), Some(&ti)) = (index_of.get(*src), index_of.get(*tgt)) {
                let dx = pts[si].x - pts[ti].x;
                let dy = pts[si].y - pts[ti].y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < 1.0 {
                    continue;
                }
                let dist = dist_sq.sqrt();
                let force = attraction_strength * (dist - ideal_length);
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;
                disp[si].0 -= fx;
                disp[si].1 -= fy;
                disp[ti].0 += fx;
                disp[ti].1 += fy;
            }
        }

        // Apply displacement with cooling (gradually dampened).
        let cooling = 0.1 + (0.02 * (iterations as f64));
        for i in 0..n {
            let (dx, dy) = disp[i];
            let mag = (dx * dx + dy * dy).sqrt().max(0.01);
            let limit = mag.min(cooling);
            pts[i].x += (dx / mag) * limit;
            pts[i].y += (dy / mag) * limit;
        }
    }

    // Write back.
    for (i, p) in paths.iter().enumerate() {
        if let Some(pos) = positions.get_mut(p) {
            *pos = pts[i];
        }
    }
}

/// Find connected components via BFS over the adjacency map.
fn connected_components(
    nodes: &[String],
    adjacency: &HashMap<String, Vec<String>>,
) -> Vec<Vec<String>> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut components: Vec<Vec<String>> = Vec::new();
    for start in nodes {
        if visited.contains(start) {
            continue;
        }
        let mut comp: Vec<String> = Vec::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(start.clone());
        visited.insert(start.clone());
        while let Some(current) = queue.pop_front() {
            comp.push(current.clone());
            for neighbor in adjacency.get(&current).into_iter().flatten() {
                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
        // Sort for deterministic component ordering.
        comp.sort();
        components.push(comp);
    }
    // Deterministic component order.
    components
}

/// Bounds of a component's positions.
fn comp_bounds(nodes: &[String], positions: &HashMap<String, NodePos>) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for path in nodes {
        if let Some(p) = positions.get(path) {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
    }
    (min_x, max_x, min_y, max_y)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn node(path: &str, title: &str, degree: usize) -> GraphNodeData {
        GraphNodeData {
            path: path.to_string(),
            title: title.to_string(),
            folder: String::new(),
            modified_at: String::new(),
            tags: Vec::new(),
            backlink_count: 0,
            outgoing_count: 0,
            degree,
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
    fn layout_single_node_is_origin() {
        let data = GraphData {
            nodes: vec![node("a.md", "A", 0)],
            edges: vec![],
            orphan_count: 1,
            cluster_count: 1,
        };
        let layout = layout_graph(&data);
        let pos = layout.positions.get("a.md").unwrap();
        assert_eq!(*pos, NodePos { x: 0.0, y: 0.0 });
    }

    #[test]
    fn layout_empty_graph_has_no_positions() {
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
    fn layout_two_connected_nodes_get_separate_positions() {
        let data = GraphData {
            nodes: vec![node("a.md", "A", 1), node("b.md", "B", 1)],
            edges: vec![edge("a.md", "b.md")],
            orphan_count: 0,
            cluster_count: 1,
        };
        let layout = layout_graph(&data);
        let a = layout.positions["a.md"];
        let b = layout.positions["b.md"];
        assert_ne!(a, b, "connected nodes must not share a position");
    }

    #[test]
    fn layout_deterministic_for_same_input() {
        let data = GraphData {
            nodes: vec![
                node("a.md", "A", 2),
                node("b.md", "B", 1),
                node("c.md", "C", 1),
                node("d.md", "D", 1),
            ],
            edges: vec![
                edge("a.md", "b.md"),
                edge("a.md", "c.md"),
                edge("a.md", "d.md"),
            ],
            orphan_count: 0,
            cluster_count: 1,
        };
        let l1 = layout_graph(&data);
        let l2 = layout_graph(&data);
        for (path, p1) in &l1.positions {
            let p2 = &l2.positions[path];
            assert_eq!(p1, p2, "layout must be deterministic for path {path}");
        }
    }

    #[test]
    fn layout_disconnected_components_separate() {
        // Two independent pairs with no edge between them.
        let data = GraphData {
            nodes: vec![
                node("a.md", "A", 1),
                node("b.md", "B", 1),
                node("c.md", "C", 1),
                node("d.md", "D", 1),
            ],
            edges: vec![edge("a.md", "b.md"), edge("c.md", "d.md")],
            orphan_count: 0,
            cluster_count: 2,
        };
        let layout = layout_graph(&data);
        // Within a pair nodes should be separated.
        let d_ab = distance(&layout.positions["a.md"], &layout.positions["b.md"]);
        let d_cd = distance(&layout.positions["c.md"], &layout.positions["d.md"]);
        assert!(d_ab > 1.0, "intra-component distance should be > 0");
        assert!(d_cd > 1.0, "intra-component distance should be > 0");
        // Cross-component centroids should be distinct (not overlapping).
        let centroid1 = centroid(&[layout.positions["a.md"], layout.positions["b.md"]]);
        let centroid2 = centroid(&[layout.positions["c.md"], layout.positions["d.md"]]);
        let across = distance(&centroid1, &centroid2);
        assert!(
            across > NODE_SPACING,
            "disconnected components must be spread apart, got {across}"
        );
    }

    #[test]
    fn layout_orphans_get_distinct_positions() {
        let data = GraphData {
            nodes: vec![
                node("a.md", "A", 0),
                node("b.md", "B", 0),
                node("c.md", "C", 0),
            ],
            edges: vec![],
            orphan_count: 3,
            cluster_count: 3,
        };
        let layout = layout_graph(&data);
        let mut positions: Vec<NodePos> = layout.positions.values().copied().collect();
        // No two orphans share a position.
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                assert_ne!(
                    positions[i], positions[j],
                    "orphans must not overlap: {i} == {j}"
                );
            }
        }
    }

    #[test]
    fn layout_many_nodes_all_have_positions() {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for i in 0..50 {
            let path = format!("note_{i}.md");
            let deg = if i > 0 { 1 } else { 49 };
            nodes.push(node(&path, &format!("Note {i}"), deg));
            if i > 0 {
                edges.push(edge("note_0.md", &path));
            }
        }
        let data = GraphData {
            nodes,
            edges,
            orphan_count: 0,
            cluster_count: 1,
        };
        let layout = layout_graph(&data);
        assert_eq!(layout.positions.len(), 50);
        // Central hub should be near origin after layout.
        let hub = layout.positions["note_0.md"];
        let hub_dist = (hub.x * hub.x + hub.y * hub.y).sqrt();
        assert!(
            hub_dist < 500.0,
            "hub should be near center, dist={hub_dist}"
        );
    }

    fn distance(a: &NodePos, b: &NodePos) -> f64 {
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        (dx * dx + dy * dy).sqrt()
    }

    fn centroid(nodes: &[NodePos]) -> NodePos {
        if nodes.is_empty() {
            return NodePos { x: 0.0, y: 0.0 };
        }
        let n = nodes.len() as f64;
        let mut x = 0.0;
        let mut y = 0.0;
        for p in nodes {
            x += p.x;
            y += p.y;
        }
        NodePos { x: x / n, y: y / n }
    }
}
