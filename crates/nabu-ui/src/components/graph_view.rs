//! # Knowledge Graph View — state management (Dioxus)
//!
//! Phase 1B-4 establishes correct loading/empty/error state presentation for
//! the graph view. The actual interactive graph canvas rendering (force-directed
//! layout, pan/zoom, hover previews, minimap) belongs to Phase 2A-1. This
//! component handles only:
//!
//! - fetching graph data via the `graph_data` IPC command
//! - showing a loading spinner while data is in flight
//! - showing an empty state when the graph has no nodes
//! - showing an error panel (with retry) if the fetch fails
//!
//! Uses `crate::ipc::tauri_invoke_safe` so a rejected promise becomes a
//! graceful error rather than a renderer panic.

use crate::components::ui::feedback::{ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::models::graph::GraphData;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Graph data load lifecycle states.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GraphLoadState {
    /// Initial / reload not yet started.
    Idle,
    /// `graph_data` IPC is in flight.
    Loading,
    /// IPC succeeded — data may be empty.
    Loaded,
    /// IPC failed or deserialization errored.
    Failed,
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
        let empty_args =
            serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();

        match crate::ipc::tauri_invoke_safe("graph_data", empty_args).await {
            Ok(Some(val)) => {
                match serde_wasm_bindgen::from_value::<GraphData>(val) {
                    Ok(g) => {
                        data.set(Some(g));
                        state.set(GraphLoadState::Loaded);
                        error_msg.set(String::new());
                    }
                    Err(e) => {
                        error_msg.set(format!("Could not parse graph data: {e}"));
                        state.set(GraphLoadState::Failed);
                    }
                }
            }
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

/// The Graph view component.
///
/// Shows loading / empty / error / loaded states. When loaded with data,
/// renders a summary placeholder — the interactive canvas is deferred to
/// Phase 2A-1.
#[component]
pub fn GraphView() -> Element {
    let data = use_signal(|| None::<GraphData>);
    let state = use_signal(|| GraphLoadState::Idle);
    let error_msg = use_signal(String::new);

    // Initial load on mount.
    let mut initialized = use_signal(|| false);
    if !*initialized.read() {
        initialized.set(true);
        load_graph_data(data, state, error_msg);
    }

    let on_retry = move |_: ()| {
        load_graph_data(data, state, error_msg);
    };

    let current_state = *state.read();
    let current_error = error_msg.read().clone();

    // Pre-extract graph data so the match arms don't borrow the guard.
    let loaded_data = data.read().clone();

    rsx! {
        div { class: "graph-view h-full flex flex-col bg-gray-950 text-gray-100" }

        {if current_state == GraphLoadState::Loading || current_state == GraphLoadState::Idle {
            rsx! {
                div { class: "flex-1 flex items-center justify-center" }
                LoadingBlock {
                    label: "Building graph…",
                    size: SpinnerSize::Lg,
                }
            }
        } else if current_state == GraphLoadState::Failed {
            rsx! {
                div { class: "flex-1 flex items-center justify-center p-8" }
                div { class: "w-full max-w-md" }
                ErrorPanel {
                    title: "Couldn't load the graph".to_string(),
                    message: "The knowledge graph could not be built.".to_string(),
                    details: current_error,
                    on_retry: on_retry,
                    recovery: "Make sure your vault is accessible and the backend is running.".to_string(),
                }
            }
        } else {
            // Loaded — check if data is present and non-empty
            match &loaded_data {
                Some(g) if !g.nodes.is_empty() => rsx! {
                    // ── Graph loaded with content ──
                    div { class: "absolute top-3 left-3 z-10 bg-gray-900/90 border border-gray-800 rounded-lg p-2 shadow-lg text-xs text-gray-400" }
                    "{g.nodes.len()} notes · {g.edges.len()} links"

                    div {
                        class: "flex-1 flex items-center justify-center bg-gray-950",
                    }
                    div { class: "text-center max-w-md p-8" }
                    div { class: "text-3xl mb-4", {render_icon_view(Icon::Network)} }
                    h3 { class: "text-lg font-semibold text-gray-200 mb-2", "Knowledge graph ready" }
                    p { class: "text-sm text-gray-400 mb-4",
                        "The interactive graph canvas is being prepared. "
                        "Explore connections between your notes."
                    }

                    // Quick stats preview
                    div { class: "grid grid-cols-4 gap-4 text-center max-w-lg mx-auto" }
                    div {}
                    div { class: "text-2xl font-bold text-blue-400", "{g.nodes.len()}" }
                    div { class: "text-xs text-gray-500", "Notes" }

                    div {}
                    div { class: "text-2xl font-bold text-cyan-400", "{g.edges.len()}" }
                    div { class: "text-xs text-gray-500", "Connections" }

                    div {}
                    div { class: "text-2xl font-bold text-orange-400", "{g.orphan_count}" }
                    div { class: "text-xs text-gray-500", "Orphans" }

                    div {}
                    div { class: "text-2xl font-bold text-purple-400", "{g.cluster_count}" }
                    div { class: "text-xs text-gray-500", "Clusters" }
                },
                _ => rsx! {
                    // ── Graph loaded but empty (no nodes) ──
                    div { class: "flex-1 flex items-center justify-center" }
                    EmptyState {
                        icon: Some(Icon::Network),
                        title: "No connections yet".to_string(),
                        description: "Create a few notes with [[links]] and they will appear here as a knowledge graph.".to_string(),
                    }
                },
            }
        }}
    }
}
