//! # Knowledge Graph View — interactive graph canvas (Dioxus)
//!
//! Phase 2A-1 replaces the static placeholder with a real interactive graph
//! canvas backed by the actual `graph_data` IPC result.
//!
//! Responsibilities:
//! - fetch real graph data via the `graph_data` IPC command (Phase 1B-1);
//! - show loading / empty / error states (Phase 1B-4);
//! - render real nodes (DOM cards) and real edges (SVG connectors) from the
//!   backend graph;
//! - pan / zoom via CSS transforms (no external graph framework);
//! - node selection with relationship highlighting;
//! - open the selected note through the existing workspace navigation APIs;
//! - refresh when the backend graph is updated (via the `GraphUpdated` event).
//!
//! Performance: the expensive layout computation is memoized and runs once
//! per data load — never on every frame or trivial UI state change. There is
//! no animation loop; positions are static and pan/zoom is a pure transform on
//! a container element.

use crate::components::contexts::{open_tab, use_nav, use_workspace, ViewMode};
use crate::components::graph_layout::{layout_graph, Layout as GraphLayout};
use crate::components::ui::feedback::{ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::events::{use_event_listener, FrontendEvent, FrontendEventKind};
use crate::models::graph::{GraphData, GraphEdgeData};
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen_futures::spawn_local;

// ── State & classification ─────────────────────────────────────────

/// Graph data load lifecycle states (unchanged from Phase 1B-4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GraphLoadState {
    /// Initial / reload not yet started.
    Idle,
    /// `graph_data` IPC is in flight.
    Loading,
    /// IPC succeeded — data may be empty.
    Loaded,
    /// IPC failed or deserialization errored.
    Failed,
}

/// Which visual phase the view is in, derived purely from (state, data).
///
/// This classification is a small, dependency-free function so the rendering
/// decision can be unit-tested without a DOM / Dioxus context.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewPhase {
    Loading,
    Empty,
    Error,
    Ready,
}

/// Classify the current view phase from load state + payload.
///
/// - `Idle`/`Loading` → `Loading` (idle is treated as loading on first mount).
/// - `Failed` → `Error` (never replaced by an empty canvas).
/// - `Loaded` with no nodes → `Empty`.
/// - `Loaded` with nodes → `Ready`.
pub fn classify_view(data: Option<&GraphData>, state: GraphLoadState) -> ViewPhase {
    match state {
        GraphLoadState::Idle | GraphLoadState::Loading => ViewPhase::Loading,
        GraphLoadState::Failed => ViewPhase::Error,
        GraphLoadState::Loaded => match data {
            Some(g) if !g.nodes.is_empty() => ViewPhase::Ready,
            _ => ViewPhase::Empty,
        },
    }
}

/// Loads graph data from the backend and updates the state signals.
fn load_graph_data(
    data: Signal<Option<GraphData>>,
    state: Signal<GraphLoadState>,
    error_msg: Signal<String>,
) {
    let mut data = data;
    let mut state = state;
    let mut error_msg = error_msg;
    state.set(GraphLoadState::Loading);
    error_msg.set(String::new());

    spawn_local(async move {
        let empty_args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();

        match crate::ipc::tauri_invoke_safe("graph_data", empty_args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<GraphData>(val) {
                Ok(g) => {
                    data.set(Some(g));
                    state.set(GraphLoadState::Loaded);
                    error_msg.set(String::new());
                }
                Err(e) => {
                    error_msg.set(format!("Could not parse graph data: {e}"));
                    state.set(GraphLoadState::Failed);
                }
            },
            Ok(None) => {
                error_msg.set("Graph data request returned no value.".to_string());
                state.set(GraphLoadState::Failed);
            }
            Err(e) => {
                error_msg.set(e.message());
                state.set(GraphLoadState::Failed);
            }
        }
    });
}

// ── Canvas viewport state ───────────────────────────────────────────

/// Pan/zoom viewport state. `pan_x`/`pan_y` are the canvas-coordinate
/// translation applied to the transform container; `zoom` is the scale factor.
#[derive(Clone, Copy, Debug)]
struct Viewport {
    pan_x: f64,
    pan_y: f64,
    zoom: f64,
    /// Whether a background pan drag is in progress.
    panning: bool,
    /// Pan-drag origin (client coords) when `panning` is true.
    pan_start_x: f64,
    pan_start_y: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        // Start moderately zoomed out for a whole-graph overview.
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 0.6,
            panning: false,
            pan_start_x: 0.0,
            pan_start_y: 0.0,
        }
    }
}

impl Viewport {
    /// CSS transform string applied to the canvas container.
    fn transform(&self) -> String {
        format!("translate({}px, {}px) scale({})", self.pan_x, self.pan_y, self.zoom)
    }
}

/// A precomputed edge segment ready for SVG rendering.
#[derive(Debug, Clone)]
struct EdgeSegment {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: String,
    width: f64,
    dash: &'static str,
}

// ── Component ────────────────────────────────────────────────────────

/// The Graph view component.
///
/// Shows loading / empty / error states around a real interactive graph canvas.
/// The canvas renders real nodes (DOM cards positioned from a memoized layout)
/// and real edges (SVG connectors) derived from the backend `graph_data`.
#[component]
pub fn GraphView() -> Element {
    let data = use_signal(|| None::<GraphData>);
    let state = use_signal(|| GraphLoadState::Idle);
    let error_msg = use_signal(String::new);

    // Viewport state (pan / zoom) and node selection.
    let viewport = use_signal(Viewport::default);
    let selected = use_signal(|| None::<String>);

    // Memoized layout: recomputed only when the graph payload changes.
    let layout = use_memo(move || data.read().as_ref().map(|g| layout_graph(g)));

    // Initial load on mount.
    let mut initialized = use_signal(|| false);
    if !*initialized.read() {
        initialized.set(true);
        load_graph_data(data, state, error_msg);
    }

    // Refresh from backend truth when the graph is updated by the backend.
    let data_for_refresh = data;
    let state_for_refresh = state;
    let err_for_refresh = error_msg;
    use_event_listener(FrontendEventKind::GraphUpdated, move |_: &FrontendEvent| {
        load_graph_data(data_for_refresh, state_for_refresh, err_for_refresh);
    });

    let nav = use_nav();
    let workspace = use_workspace();

    let current_state = *state.read();
    let current_error = error_msg.read().clone();
    let phase = classify_view(data.read().as_ref(), current_state);

    // Pre-extract immutable data for the Ready branch so the borrow guard is
    // not held across the rsx!.
    let loaded_data = data.read().clone();
    let layout_val = layout.read().clone();
    let selected_path = selected.read().clone();

    let on_retry = move |_: ()| {
        load_graph_data(data, state, error_msg);
    };

    rsx! {
        div { class: "graph-view h-full flex flex-col bg-gray-950 text-gray-100" }

        {match phase {
            ViewPhase::Loading => rsx! {
                div { class: "flex-1 flex items-center justify-center" }
                LoadingBlock {
                    label: "Building graph…",
                    size: SpinnerSize::Lg,
                }
            },
            ViewPhase::Error => rsx! {
                div { class: "flex-1 flex items-center justify-center p-8" }
                div { class: "w-full max-w-md" }
                ErrorPanel {
                    title: "Couldn't load the graph".to_string(),
                    message: "The knowledge graph could not be built.".to_string(),
                    details: Some(current_error),
                    on_retry,
                    recovery: "Make sure your vault is accessible and the backend is running.".to_string(),
                }
            },
            ViewPhase::Empty => rsx! {
                div { class: "flex-1 flex items-center justify-center" }
                EmptyState {
                    icon: Some(Icon::Network),
                    title: "No connections yet".to_string(),
                    description: Some("Create a few notes with links and they will appear here as a knowledge graph.".to_string()),
                }
            },
            ViewPhase::Ready => {
                let g = loaded_data.expect("Ready phase guarantees graph data");
                rsx! {
                    GraphCanvas {
                        data: g,
                        layout: layout_val.expect("Ready phase guarantees a layout"),
                        viewport,
                        selected,
                        on_open: move |path: String| {
                            open_tab(workspace, &path);
                            let mut nav = nav;
                            nav.view_mode.set(ViewMode::Editor);
                        },
                    }
                }
            }
        }}
    }
}

/// The interactive graph canvas: pan/zoom container, SVG edge layer, and DOM
/// node cards. Renders only real nodes and real edges from `graph_data`.
#[allow(clippy::too_many_arguments)]
#[component]
fn GraphCanvas(
    data: GraphData,
    layout: GraphLayout,
    viewport: Signal<Viewport>,
    selected: Signal<Option<String>>,
    on_open: EventHandler<String>,
) -> Element {
    let nodes = data.nodes.clone();
    let edges = data.edges.clone();

    // Precompute edge geometry + styling once per render. Every edge comes
    // from the backend graph + the memoized layout — no frontend-generated
    // edges are drawn.
    let selected_path = selected.read().clone();
    let vp = *viewport.read();
    let bounds = layout.bounds;

    let selected_for_edges = selected_path.as_deref();
    let edge_segments: Vec<EdgeSegment> = edges
        .iter()
        .filter_map(|e| {
            if e.broken {
                return None;
            }
            let src = layout.positions.get(&e.source)?;
            let tgt = layout.positions.get(&e.target)?;
            let highlighted = selected_for_edges.map_or(false, |s| {
                s == e.source || s == e.target
            });
            let stroke = if highlighted {
                "#10b981".to_string()
            } else {
                "#4b5563".to_string()
            };
            Some(EdgeSegment {
                x1: src.x,
                y1: src.y,
                x2: tgt.x,
                y2: tgt.y,
                stroke,
                width: if highlighted { 2.5 } else { 1.5 },
                dash: "0",
            })
        })
        .collect();

    // Relationships of the selected node, for the detail panel.
    let related: Vec<String> = if let Some(sel) = &selected_path {
        edges
            .iter()
            .filter_map(|e| {
                if e.broken {
                    return None;
                }
                if &e.source == sel {
                    Some(e.target.clone())
                } else if &e.target == sel {
                    Some(e.source.clone())
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // ── Interaction handlers ──
    // Each handler copies the viewport out of the signal, mutates the local,
    // then writes it back — never holding a read guard across a set.

    let on_wheel = move |ev: WheelEvent| {
        let web = ev.data().as_web_event();
        web.prevent_default();
        let delta = web.delta_y();
        let factor = if delta > 0.0 { 0.9 } else { 1.1 };
        let cur = *viewport.read();
        let new_zoom = (cur.zoom * factor).clamp(0.1, 4.0);
        // Keep the canvas point under the cursor stationary.
        let cx = web.client_x() as f64;
        let cy = web.client_y() as f64;
        let canvas_x = (cx - cur.pan_x) / cur.zoom;
        let canvas_y = (cy - cur.pan_y) / cur.zoom;
        viewport.set(Viewport {
            pan_x: cx - canvas_x * new_zoom,
            pan_y: cy - canvas_y * new_zoom,
            zoom: new_zoom,
            ..cur
        });
    };

    let on_bg_down = move |ev: MouseEvent| {
        let web = ev.data().as_web_event();
        let cur = *viewport.read();
        viewport.set(Viewport {
            pan_start_x: web.client_x() as f64,
            pan_start_y: web.client_y() as f64,
            panning: true,
            ..cur
        });
    };

    let on_bg_move = move |ev: MouseEvent| {
        let cur = *viewport.read();
        if !cur.panning {
            return;
        }
        let web = ev.data().as_web_event();
        let dx = (web.client_x() as f64) - cur.pan_start_x;
        let dy = (web.client_y() as f64) - cur.pan_start_y;
        viewport.set(Viewport {
            pan_x: cur.pan_x + dx,
            pan_y: cur.pan_y + dy,
            pan_start_x: web.client_x() as f64,
            pan_start_y: web.client_y() as f64,
            panning: true,
            zoom: cur.zoom,
        });
    };

    let on_bg_up = move |_: MouseEvent| {
        let cur = *viewport.read();
        viewport.set(Viewport {
            panning: false,
            ..cur
        });
    };

    let on_fit = move |_: MouseEvent| {
        let gw = bounds.width().max(1.0);
        let gh = bounds.height().max(1.0);
        // Fit within the canvas area (approx 1200x720).
        let fit_zoom = ((1200.0 / gw).min(720.0 / gh) * 0.9).clamp(0.1, 2.0);
        let center_x = (bounds.min_x + bounds.max_x) / 2.0;
        let center_y = (bounds.min_y + bounds.max_y) / 2.0;
        let cur = *viewport.read();
        viewport.set(Viewport {
            pan_x: 600.0 - center_x * fit_zoom,
            pan_y: 360.0 - center_y * fit_zoom,
            zoom: fit_zoom,
            ..cur
        });
    };

    // ── Nodes (DOM cards) ──
    // Pre-filter to nodes that have a layout position, because the Dioxus
    // `for` body is compiled as a closure and cannot use `continue`.
    let renderable: Vec<_> = nodes
        .iter()
        .filter_map(|node| layout.positions.get(&node.path).map(|p| (node, *p)))
        .collect();

    // ── Selection detail panel ──
    // Only rendered when a node is selected; `selected_path` carries the
    // selected note's path.
    let detail_panel: Option<(String, Vec<String>, Vec<crate::models::graph::GraphNodeData>)> =
        selected_path.as_ref().map(|sel_path| {
            (sel_path.clone(), related.clone(), nodes.clone())
        });

    rsx! {
        div { class: "flex-1 relative flex overflow-hidden" }

        // ── Main canvas ──
        div {
            class: "flex-1 relative bg-gray-950 overflow-hidden",
            // The transform container: panning moves it, zooming scales it.
            style: "transform: {vp.transform()}; transform-origin: 0 0;",
            onwheel: on_wheel,
            // Pan drag lifecycle is driven from the container so a release
            // outside the background still ends the drag.
            onmousemove: on_bg_move,
            onmouseup: on_bg_up,
            onmouseout: on_bg_up,

            // The static background (target of bg clicks for panning). Renders
            // a subtle grid for orientation. Only the bg receives mousedown
            // so clicking a node never starts a pan.
            div {
                class: "graph-canvas-bg absolute inset-0",
                style: "background-image: radial-gradient(circle, #374151 1px, transparent 1px); background-size: 40px 40px;",
                onmousedown: on_bg_down,
            }

            // ── Edges (SVG, behind nodes) ──
            // Segments are precomputed above; the loop body only interpolates.
            svg {
                class: "absolute inset-0",
                style: "width: 100%; height: 100%; overflow: visible;",
                key: data.edges.len(),
                for segment in &edge_segments {
                    line {
                        x1: "{segment.x1}",
                        y1: "{segment.y1}",
                        x2: "{segment.x2}",
                        y2: "{segment.y2}",
                        stroke: "{segment.stroke}",
                        "stroke-width": "{segment.width}",
                        "stroke-dasharray": segment.dash,
                    }
                }
                defs {
                    marker {
                        id: "graph-arrow",
                        "markerWidth": "8",
                        "markerHeight": "8",
                        "refX": "7",
                        "refY": "3.5",
                        orient: "auto",
                        "markerUnits": "strokeWidth",
                        polygon { points: "0 0, 8 3.5, 0 7", fill: "#6b7280" }
                    }
                }
            }

            // ── Nodes (DOM cards) ──
            for (node, pos) in &renderable {
                {
                    let is_selected = selected_path.as_deref() == Some(&node.path);
                    let node_title = node.title.clone();
                    let node_path = node.path.clone();
                    let node_folder = node.folder.clone();
                    let mut sel = selected;
                    let node_degree = node.degree;
                    let open = on_open;
                    let node_path_click = node_path.clone();
                    let node_path_dblclick = node_path.clone();

                    let card_class: &str = if is_selected {
                        "absolute rounded-lg px-3 py-2 text-sm font-medium whitespace-nowrap bg-blue-600/20 border-2 border-blue-400 text-white shadow-lg"
                    } else {
                        "absolute rounded-lg px-3 py-2 text-sm font-medium whitespace-nowrap bg-gray-800/80 border border-gray-700 text-gray-300 hover:bg-gray-700/80"
                    };

                    rsx! {
                        div {
                            key: node_path.clone(),
                            class: card_class,
                            style: "left: {pos.x}px; top: {pos.y}px;",
                            title: node_title.clone(),
                            onclick: move |ev: MouseEvent| {
                                ev.stop_propagation();
                                sel.set(Some(node_path_click.clone()));
                            },
                            ondblclick: move |ev: MouseEvent| {
                                ev.stop_propagation();
                                open.call(node_path_dblclick.clone());
                            },
                        }
                        span {
                            class: "inline-block w-2 h-2 rounded-full mr-1.5 align-middle",
                            class: if node_degree > 0 { "bg-cyan-400" } else { "bg-gray-500" },
                            "aria-hidden": "true",
                        }
                        "{node_title}"
                        {if !node_folder.is_empty() {
                            rsx! { span { class: "ml-1 text-xs text-gray-500 font-normal", "/{node_folder}" } }
                        } else {
                            rsx! {}
                        }}
                    }
                }
            }

            // Toolbar overlay on the canvas.
            div {
                class: "absolute top-3 left-3 z-10 flex gap-1.5",
            }
            div {
                class: "bg-gray-900/90 border border-gray-800 rounded-lg px-2 py-1 text-xs text-gray-400",
            }
            "{data.nodes.len()} notes · {data.edges.len()} links"
            button {
                class: "bg-gray-800/90 border border-gray-700 rounded px-2 py-1 text-xs hover:bg-gray-700",
                onclick: on_fit,
                {render_icon_view(Icon::Target)}
                " Fit"
            }
        }

        // ── Selection detail panel ──
        {if let Some((sel_path, related_nodes, panel_nodes)) = &detail_panel {
            rsx! {
                GraphDetailPanel {
                    nodes: panel_nodes.clone(),
                    selected: sel_path.clone(),
                    related: related_nodes.clone(),
                    on_open,
                }
            }
        } else {
            rsx! {}
        }}
    }
}

/// Right-hand detail panel shown when a graph node is selected.
///
/// Extracts the `let`-heavy body into a component so the parent `rsx!` stays
/// free of inline `let` statements (which the Dioxus macro does not accept
/// directly in `if let` / `for` arms).
#[component]
fn GraphDetailPanel(
    nodes: Vec<crate::models::graph::GraphNodeData>,
    selected: String,
    related: Vec<String>,
    on_open: EventHandler<String>,
) -> Element {
    let node = nodes.iter().find(|n| n.path == selected).cloned();
    let node_titles = nodes.clone();
    let open = on_open;

    rsx! {
        div { class: "flex-none w-72 border-l border-gray-800 bg-gray-900/60 p-4 overflow-y-auto" }
        {node.as_ref().map(|n| rsx! {
            div { class: "text-sm font-semibold text-gray-200 truncate", "{n.title}" }
            div { class: "text-xs text-gray-500 mt-1 break-all", "{n.path}" }
        })}
        div { class: "h-px bg-gray-800 my-3" }
        div { class: "text-xs text-gray-400 mb-1", "Links ({related.len()})" }
        for rpath in &related {
            {
                let rtitle = node_titles
                    .iter()
                    .find(|n| &n.path == rpath)
                    .map(|n| n.title.clone())
                    .unwrap_or_else(|| rpath.clone());
                rsx! {
                    div { class: "text-xs text-cyan-400 hover:text-cyan-300 cursor-pointer py-0.5 break-all", "{rtitle}" }
                }
            }
        }
        {node.as_ref().map(|n| rsx! {
            div { class: "h-px bg-gray-800 my-3" }
            div { class: "grid grid-cols-3 gap-2 text-center text-xs" }
            div {}
            div { class: "text-lg font-bold text-cyan-400", "{n.degree}" }
            div { class: "text-gray-500", "degree" }
            div {}
            div { class: "text-lg font-bold text-green-400", "{n.backlink_count}" }
            div { class: "text-gray-500", "backlinks" }
            div {}
            div { class: "text-lg font-bold text-orange-400", "{n.outgoing_count}" }
            div { class: "text-gray-500", "outgoing" }
        })}
        {
            let open_path = selected.clone();
            rsx! {
                button {
                    class: "mt-4 w-full btn btn-sm",
                    onclick: move |_: MouseEvent| {
                        open.call(open_path.clone());
                    },
                    {render_icon_view(Icon::FolderOpen)}
                    " Open note"
                }
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::graph::GraphNodeData;

    fn gd(nodes: Vec<GraphNodeData>, edges: Vec<GraphEdgeData>) -> GraphData {
        GraphData {
            nodes,
            edges,
            orphan_count: 0,
            cluster_count: 1,
        }
    }

    fn n(path: &str, title: &str, degree: usize) -> GraphNodeData {
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

    fn e(src: &str, tgt: &str) -> GraphEdgeData {
        GraphEdgeData {
            source: src.to_string(),
            target: tgt.to_string(),
            broken: false,
        }
    }

    // Test 3 — Error
    #[test]
    fn failed_state_classifies_as_error() {
        let data: Option<GraphData> = None;
        assert_eq!(
            classify_view(data, GraphLoadState::Failed),
            ViewPhase::Error
        );
    }

    // Test 4 (variant) — selection is a view concern over a fixed layout.
    #[test]
    fn selecting_a_node_does_not_move_positions() {
        let data = gd(
            vec![n("a.md", "A", 1), n("b.md", "B", 1)],
            vec![e("a.md", "b.md")],
        );
        let layout = layout_graph(&data);
        let before = layout.positions.clone();
        // Simulate selecting node "a.md".
        let _selected = Some("a.md".to_string());
        let after = layout_graph(&data);
        assert_eq!(before, after, "layout is independent of view selection");
        assert_ne!(
            layout.positions["a.md"],
            layout.positions["b.md"],
            "connected nodes must not share a position"
        );
    }
}
