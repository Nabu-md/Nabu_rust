//! # Canvas View — infinite pannable / zoomable workspace (Dioxus)
//!
//! An infinite canvas that *references* existing notes rather than duplicating
//! their content. Nodes are positioned cards pointing at vault-relative note
//! paths; edges are visual connectors; groups are labelled bounding boxes.
//!
//! Canvas definitions are persisted as JSON in the settings store
//! (`nabu.canvases`) via the `canvas_list` / `canvas_get` / `canvas_save` /
//! `canvas_delete` IPC commands.
//!
//! Responsibilities:
//! - list saved canvases from the backend (`canvas_list`);
//! - load the full definition of a canvas (`canvas_get`);
//! - pan / zoom / drag nodes, mutating the in-memory `CanvasDef`;
//! - persist edits through `canvas_save`, surfacing failures in-view (save
//!   status indicator + toast);
//! - create / delete canvases (`canvas_save` / `canvas_delete`);
//! - add notes from the vault index onto the canvas (double-click in the
//!   sidebar palette) and open them through the existing workspace APIs.

use crate::components::contexts::{open_tab, use_nav, use_workspace, NoteIndexEntry, ViewMode};
use crate::components::ui::dialog::PromptDialog;
use crate::components::ui::feedback::{
    ErrorPanel, LoadingBlock, Spinner, SpinnerSize, ToastContext, use_toast,
};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

// ── Types (mirror the backend `CanvasDef` family) ──────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: String,
    pub note_path: String,
    pub title: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default = "default_node_kind")]
    pub kind: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub text: String,
}

fn default_node_kind() -> String {
    "note".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasGroup {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub edges: Vec<CanvasEdge>,
    #[serde(default)]
    pub groups: Vec<CanvasGroup>,
    #[serde(default)]
    pub pan_x: f64,
    #[serde(default)]
    pub pan_y: f64,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}

fn default_zoom() -> f64 {
    1.0
}

const DEFAULT_NODE_W: f64 = 240.0;
const DEFAULT_NODE_H: f64 = 120.0;

impl Default for CanvasDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: default_zoom(),
        }
    }
}

// ── State enums & classification ──────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListState {
    Idle,
    Loading,
    Loaded,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CanvasLoadState {
    Idle,
    Loading,
    Loaded,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveState {
    Idle,
    Saving,
    Saved,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListPhase {
    Loading,
    Empty,
    Error,
    Loaded,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CanvasPhase {
    Select,
    Loading,
    Error,
    Empty,
    Ready,
}

/// Which visual phase the canvas list is in, derived from (state, count).
pub fn classify_list(state: ListState, count: usize) -> ListPhase {
    match state {
        ListState::Idle | ListState::Loading => ListPhase::Loading,
        ListState::Failed => ListPhase::Error,
        ListState::Loaded => {
            if count == 0 {
                ListPhase::Empty
            } else {
                ListPhase::Loaded
            }
        }
    }
}

/// Which visual phase the canvas workspace is in, derived from the load state,
/// the currently loaded canvas, and the active selection.
pub fn classify_canvas(
    state: CanvasLoadState,
    canvas: Option<&CanvasDef>,
    active_id: Option<&str>,
) -> CanvasPhase {
    match state {
        CanvasLoadState::Idle | CanvasLoadState::Loading => {
            if active_id.is_some() {
                CanvasPhase::Loading
            } else {
                CanvasPhase::Select
            }
        }
        CanvasLoadState::Failed => CanvasPhase::Error,
        CanvasLoadState::Loaded => match canvas {
            Some(c) if !c.nodes.is_empty() => CanvasPhase::Ready,
            Some(_) => CanvasPhase::Empty,
            None => CanvasPhase::Select,
        },
    }
}

// ── Pure model helpers ────────────────────────────────────────────────

/// Builds a new (unsaved) canvas definition.
pub fn new_canvas_with_id(name: impl Into<String>, id: impl Into<String>) -> CanvasDef {
    CanvasDef {
        id: id.into(),
        name: name.into(),
        nodes: Vec::new(),
        edges: Vec::new(),
        groups: Vec::new(),
        pan_x: 0.0,
        pan_y: 0.0,
        zoom: default_zoom(),
    }
}

/// A short human-readable summary of a canvas, for the list row subtitle.
pub fn canvas_summary(c: &CanvasDef) -> String {
    if c.nodes.is_empty() {
        "blank canvas".to_string()
    } else {
        format!("{} nodes · {} connections", c.nodes.len(), c.edges.len())
    }
}

/// Cascading placement for a new node near the viewport centre (canvas coords).
pub fn next_node_position(canvas: &CanvasDef, index: usize) -> (f64, f64) {
    let offset = index as f64 * 30.0;
    let cx = -canvas.pan_x / canvas.zoom + offset;
    let cy = -canvas.pan_y / canvas.zoom + offset;
    (cx, cy)
}

/// Appends a new note node to the canvas at a cascaded position.
pub fn create_node(canvas: &mut CanvasDef, entry: &NoteIndexEntry) {
    let count = canvas.nodes.len();
    let (x, y) = next_node_position(canvas, count);
    let id = format!("n{}", count + 1);
    canvas.nodes.push(CanvasNode {
        id,
        note_path: entry.path.clone(),
        title: entry.title.clone(),
        x,
        y,
        width: None,
        height: None,
        kind: default_node_kind(),
        source: String::new(),
        text: String::new(),
    });
}

/// Removes a node and every edge attached to it.
pub fn remove_node(canvas: &mut CanvasDef, node_id: &str) {
    canvas.nodes.retain(|n| n.id != node_id);
    canvas
        .edges
        .retain(|e| e.source != node_id && e.target != node_id);
}

/// Rendered bounding box of a node (x, y, width, height).
pub fn node_view_rect(node: &CanvasNode) -> (f64, f64, f64, f64) {
    let w = node.width.unwrap_or(DEFAULT_NODE_W);
    let h = node.height.unwrap_or(DEFAULT_NODE_H);
    (node.x, node.y, w, h)
}

/// Compute the SVG line endpoints (centres) for an edge's two nodes.
pub fn edge_endpoints<'a>(
    canvas: &'a CanvasDef,
    edge: &CanvasEdge,
) -> Option<(f64, f64, f64, f64)> {
    let source = canvas.nodes.iter().find(|n| n.id == edge.source)?;
    let target = canvas.nodes.iter().find(|n| n.id == edge.target)?;
    let (sx, sy, sw, sh) = node_view_rect(source);
    let (tx, ty, tw, th) = node_view_rect(target);
    Some((sx + sw / 2.0, sy + sh / 2.0, tx + tw / 2.0, ty + th / 2.0))
}

/// Generates a client-side id for a new canvas.
pub fn generate_canvas_id() -> String {
    format!("canvas-{}", uuid::Uuid::new_v4())
}

// ── IPC argument builders ─────────────────────────────────────────────

fn canvas_list_args() -> serde_json::Value {
    serde_json::json!({})
}

fn canvas_get_args(id: &str) -> serde_json::Value {
    serde_json::json!({ "id": id })
}

fn canvas_save_args(canvas: &CanvasDef) -> serde_json::Value {
    serde_json::json!({ "canvas": canvas })
}

fn canvas_delete_args(id: &str) -> serde_json::Value {
    serde_json::json!({ "id": id })
}

// ── Signal bundle ─────────────────────────────────────────────────────

/// All reactive state owned by [`CanvasView`], bundled so the IPC helper
/// functions stay concise.
#[derive(Clone, Copy)]
struct CanvasState {
    list_data: Signal<Vec<CanvasDef>>,
    list_state: Signal<ListState>,
    list_error: Signal<String>,
    active_id: Signal<Option<String>>,
    canvas: Signal<Option<CanvasDef>>,
    canvas_state: Signal<CanvasLoadState>,
    canvas_error: Signal<String>,
    save_state: Signal<SaveState>,
    save_error: Signal<Option<String>>,
    toasts: ToastContext,
}

// ── IPC helpers ───────────────────────────────────────────────────────

/// Loads the list of canvases and, when the list is non-empty and no canvas
/// is active yet, auto-selects the first one.
fn load_list(s: CanvasState) {
    *s.list_state.write_unchecked() = ListState::Loading;
    *s.list_error.write_unchecked() = String::new();

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_list_args()).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke_safe("canvas_list", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<CanvasDef>>(val) {
                Ok(list) => {
                    *s.list_data.write_unchecked() = list.clone();
                    *s.list_state.write_unchecked() = ListState::Loaded;
                    *s.list_error.write_unchecked() = String::new();
                    if s.active_id.read().is_none() && !list.is_empty() {
                        select_canvas(s, list[0].id.clone());
                    }
                }
                Err(e) => {
                    *s.list_error.write_unchecked() = format!("Could not parse canvases: {e}");
                    *s.list_state.write_unchecked() = ListState::Failed;
                    s.toasts
                        .error("Canvas list failed", format!("Could not parse canvases: {e}"));
                }
            },
            Ok(None) => {
                *s.list_error.write_unchecked() =
                    "Canvas list request returned no value.".to_string();
                *s.list_state.write_unchecked() = ListState::Failed;
                s.toasts.error(
                    "Canvas list failed",
                    "No canvases were returned by the backend.".to_string(),
                );
            }
            Err(e) => {
                *s.list_error.write_unchecked() = e.message();
                *s.list_state.write_unchecked() = ListState::Failed;
                s.toasts.error("Canvas list failed", e.message());
            }
        }
    });
}

/// Loads a single canvas definition by id. `s.active_id` is set synchronously so
/// the view shows a loading state while the IPC is in flight.
fn select_canvas(s: CanvasState, id: String) {
    *s.active_id.write_unchecked() = Some(id.clone());
    *s.canvas_state.write_unchecked() = CanvasLoadState::Loading;
    *s.canvas_error.write_unchecked() = String::new();

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_get_args(&id)).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke_safe("canvas_get", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Option<CanvasDef>>(val) {
                Ok(Some(c)) => {
                    *s.canvas.write_unchecked() = Some(c);
                    *s.canvas_state.write_unchecked() = CanvasLoadState::Loaded;
                    *s.canvas_error.write_unchecked() = String::new();
                }
                Ok(None) => {
                    *s.canvas.write_unchecked() = None;
                    *s.canvas_state.write_unchecked() = CanvasLoadState::Failed;
                    *s.canvas_error.write_unchecked() = format!("Canvas \"{id}\" was not found.");
                }
                Err(e) => {
                    *s.canvas.write_unchecked() = None;
                    *s.canvas_state.write_unchecked() = CanvasLoadState::Failed;
                    *s.canvas_error.write_unchecked() = format!("Could not parse the canvas: {e}");
                }
            },
            Ok(None) => {
                *s.canvas.write_unchecked() = None;
                *s.canvas_state.write_unchecked() = CanvasLoadState::Failed;
                *s.canvas_error.write_unchecked() = format!("Canvas \"{id}\" could not be loaded.");
            }
            Err(e) => {
                *s.canvas.write_unchecked() = None;
                *s.canvas_state.write_unchecked() = CanvasLoadState::Failed;
                *s.canvas_error.write_unchecked() = e.message();
            }
        }
    });
}

/// Persists the currently loaded canvas. Sets `save_state` to `Saving` while in
/// flight, `Saved` on success, and `Error` (with `save_error`) on failure —
/// surfacing backend failures both in the workspace toolbar and as a toast.
fn save_canvas(s: CanvasState) {
    let current = s.canvas.read().clone();
    let Some(c) = current else {
        return;
    };
    *s.save_state.write_unchecked() = SaveState::Saving;
    *s.save_error.write_unchecked() = None;
    let name = c.name.clone();

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_save_args(&c)).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke("canvas_save", args).await {
            Ok(val) => match serde_wasm_bindgen::from_value::<()>(val) {
                Ok(()) => {
                    *s.save_state.write_unchecked() = SaveState::Saved;
                    *s.save_error.write_unchecked() = None;
                    s.toasts.success("Canvas saved", name);
                }
                Err(e) => {
                    let msg = format!("Could not save canvas: {e}");
                    *s.save_state.write_unchecked() = SaveState::Error;
                    *s.save_error.write_unchecked() = Some(msg.clone());
                    s.toasts.error("Save failed", msg);
                }
            },
            Err(e) => {
                let msg = e.message();
                *s.save_state.write_unchecked() = SaveState::Error;
                *s.save_error.write_unchecked() = Some(msg.clone());
                s.toasts.error("Save failed", msg);
            }
        }
    });
}

/// Creates a new canvas: optimistically selects it, then persists it in the
/// background. On persistence failure it reverts the optimistic state.
fn create_canvas(s: CanvasState, name: String) {
    if name.trim().is_empty() {
        s.toasts.error("Invalid name", "Canvas name cannot be empty.");
        return;
    }
    let id = generate_canvas_id();
    let c = new_canvas_with_id(&name, &id);
    let name_for_toast = name.clone();

    // Optimistically surface the new canvas while it persists in the background.
    *s.active_id.write_unchecked() = Some(id.clone());
    *s.canvas.write_unchecked() = Some(c.clone());
    *s.canvas_state.write_unchecked() = CanvasLoadState::Loaded;
    *s.canvas_error.write_unchecked() = String::new();
    *s.save_state.write_unchecked() = SaveState::Idle;
    *s.save_error.write_unchecked() = None;
    {
        let mut v = s.list_data.peek().clone();
        v.push(c.clone());
        *s.list_data.write_unchecked() = v;
    }

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_save_args(&c)).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke("canvas_save", args).await {
            Ok(val) => match serde_wasm_bindgen::from_value::<()>(val) {
                Ok(()) => {
                    *s.save_state.write_unchecked() = SaveState::Saved;
                    *s.list_data.write_unchecked() = s.list_data.peek().clone();
                    load_list(s);
                    s.toasts.success("Canvas created", name_for_toast);
                }
                Err(e) => {
                    *s.save_state.write_unchecked() = SaveState::Error;
                    s.toasts
                        .error("Create failed", format!("Could not create canvas: {e}"));
                    *s.active_id.write_unchecked() = None;
                    *s.canvas.write_unchecked() = None;
                    *s.canvas_state.write_unchecked() = CanvasLoadState::Failed;
                    let mut v = s.list_data.peek().clone();
                    v.retain(|x| x.id != id);
                    *s.list_data.write_unchecked() = v;
                }
            },
            Err(e) => {
                *s.save_state.write_unchecked() = SaveState::Error;
                s.toasts.error("Create failed", e.message());
                *s.active_id.write_unchecked() = None;
                *s.canvas.write_unchecked() = None;
                *s.canvas_state.write_unchecked() = CanvasLoadState::Failed;
                let mut v = s.list_data.peek().clone();
                v.retain(|x| x.id != id);
                *s.list_data.write_unchecked() = v;
            }
        }
    });
}

/// Deletes a canvas by id, refreshing the list and switching the active canvas
/// when the deleted one was the one being edited.
fn delete_canvas(s: CanvasState, id: String) {
    let name = s
        .list_data
        .read()
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| id.clone());
    let was_active = s.active_id.read().as_deref() == Some(id.as_str());
    let next_id = if was_active {
        s.list_data
            .read()
            .iter()
            .find(|c| c.id != id)
            .map(|c| c.id.clone())
    } else {
        None
    };

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_delete_args(&id)).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke("canvas_delete", args).await {
            Ok(val) => match serde_wasm_bindgen::from_value::<()>(val) {
                Ok(()) => {
                    let mut v = s.list_data.peek().clone();
                    v.retain(|c| c.id != id);
                    *s.list_data.write_unchecked() = v;
                    if was_active {
                        *s.active_id.write_unchecked() = None;
                        *s.canvas.write_unchecked() = None;
                        *s.canvas_state.write_unchecked() = CanvasLoadState::Idle;
                        *s.canvas_error.write_unchecked() = String::new();
                        if let Some(nid) = next_id {
                            select_canvas(s, nid);
                        }
                    }
                    s.toasts.success("Canvas deleted", name);
                }
                Err(e) => {
                    s.toasts
                        .error("Delete failed", format!("Could not delete canvas: {e}"));
                }
            },
            Err(e) => {
                s.toasts.error("Delete failed", e.message());
            }
        }
    });
}

// ── Component ─────────────────────────────────────────────────────────

/// The Canvas view component.
#[component]
pub fn CanvasView() -> Element {
    let nav = use_nav();
    let workspace = use_workspace();
    let toasts = use_toast();

    let list_data = use_signal(Vec::<CanvasDef>::new);
    let list_state = use_signal(|| ListState::Idle);
    let list_error = use_signal(String::new);

    let active_id = use_signal(|| None::<String>);
    let canvas = use_signal(|| None::<CanvasDef>);
    let canvas_state = use_signal(|| CanvasLoadState::Idle);
    let canvas_error = use_signal(String::new);

    let save_state = use_signal(|| SaveState::Idle);
    let save_error = use_signal(|| None::<String>);

    let show_new_dialog = use_signal(|| false);

    let s = CanvasState {
        list_data,
        list_state,
        list_error,
        active_id,
        canvas,
        canvas_state,
        canvas_error,
        save_state,
        save_error,
        toasts,
    };

    // Initial list load on mount (mirrors `GraphView`'s mount pattern).
    let initialized = use_signal(|| false);
    if !*initialized.read() {
        *initialized.write_unchecked() = true;
        load_list(s);
    }

    // ── Derived classification (computed before rsx; guards dropped here) ──
    let list_phase = classify_list(*list_state.read(), list_data.read().len());
    let list_err = list_error.read().clone();
    let rows = list_data.read().clone();
    let active = active_id.read().clone();
    let sel_canvas = canvas.read().clone();
    let canvas_phase =
        classify_canvas(*canvas_state.read(), sel_canvas.as_ref(), active.as_deref());
    let canvas_err = canvas_error.read().clone();
    let notes_index = nav.notes_index.read().clone();

    let on_list_retry = move |_: ()| {
        load_list(s);
    };
    let on_canvas_retry = move |_: ()| {
        if let Some(id) = s.active_id.read().clone() {
            select_canvas(s, id);
        }
    };

    // ── Precomputed conditional blocks (avoid bare match/if as rsx children)
    let list_phase_node: Element = match list_phase {
        ListPhase::Loading => rsx! {
            div { class: "p-3", LoadingBlock { label: Some("Loading canvases…"), size: SpinnerSize::Lg } }
        },
        ListPhase::Error => rsx! {
            div { class: "p-3" }
            ErrorPanel {
                title: "Couldn't load canvases".to_string(),
                message: "Your saved canvases could not be retrieved from the backend.".to_string(),
                details: Some(list_err),
                on_retry: on_list_retry,
                recovery: Some("Make sure your vault is accessible and try again.".to_string()),
            }
        },
        ListPhase::Empty => rsx! {
            div { class: "px-3 py-2 text-xs text-gray-500", "No canvases yet — create one with the + button above." }
        },
        ListPhase::Loaded => rsx! {
            for c in &rows {
                {
                    let id = c.id.clone();
                    let name = c.name.clone();
                    let summary = canvas_summary(c);
                    let is_active = active.as_deref() == Some(id.as_str());
                    let class = if is_active {
                        "flex items-center justify-between px-3 py-1.5 cursor-pointer bg-gray-800 border-l-2 border-blue-500 rounded"
                    } else {
                        "flex items-center justify-between px-3 py-1.5 cursor-pointer hover:bg-gray-800 rounded"
                    };
                    let s_for_select = s;
                    let id_for_dblclick = id.clone();
                    let id_for_key = id.clone();
                    let id_for_delete = id.clone();
                    rsx! {
                        div {
                            key: id_for_key,
                            class: class,
                ondoubleclick: move |_: MouseEvent| {
                    select_canvas(s_for_select, id_for_dblclick.clone());
                },
                        }
                        div { class: "flex-1 min-w-0" }
                        div {
                            class: "flex-1 min-w-0",
                            div { class: "text-sm font-medium text-gray-200 truncate", "{name}" },
                            div { class: "text-xs text-gray-500 truncate", "{summary}" },
                        }
                        button {
                            class: "ml-2 text-xs text-gray-500 hover:text-red-400",
                            "aria-label": format!("Delete canvas {name}"),
                            onclick: move |ev: MouseEvent| {
                                ev.stop_propagation();
                                delete_canvas(s, id_for_delete.clone());
                            },
                            {render_icon_view(Icon::Trash2)}
                        }
                    }
                }
            }
        },
    };

    let palette_node: Element = if notes_index.is_empty() {
        rsx! {
            div { class: "px-3 py-2 text-xs text-gray-400", "No notes indexed to place yet." }
        }
    } else {
        rsx! {
            for entry in &notes_index {
                {
                    let title = entry.title.clone();
                    let entry_clone = entry.clone();
                    rsx! {
                        div {
                            key: entry.path.clone(),
                            class: "px-3 py-1.5 text-sm cursor-pointer hover:bg-gray-800 truncate",
                            title: "Double-click to add to canvas",
                            ondoubleclick: move |_: MouseEvent| {
                                if s.canvas.read().is_some() {
                                    let mut c = s.canvas.read().clone();
                                    if let Some(c) = c.as_mut() {
                                        create_node(c, &entry_clone);
                                    }
                                    *s.canvas.write_unchecked() = c;
                                    save_canvas(s);
                                } else {
                                    toasts.info(
                                        "No canvas selected",
                                        "Select a canvas before placing notes on it.",
                                    );
                                }
                            },
                        }
                        "{title}"
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "canvas-view flex h-full bg-gray-950 text-gray-100 overflow-hidden" }

        // ── Left sidebar: canvas list + note palette ──
        div { class: "flex-none w-64 border-r border-gray-800 flex flex-col" }

        div {
            class: "flex items-center justify-between px-3 py-2 border-b border-gray-800",
        }
        h2 { class: "text-sm font-semibold text-gray-300", "Canvases" }
        button {
            class: "px-2 py-1 text-xs bg-blue-600 rounded hover:bg-blue-500",
            "aria-label": "New canvas",
            onclick: move |_: MouseEvent| { *show_new_dialog.write_unchecked() = true; },
            {render_icon_view(Icon::Plus)}
            " New"
        }

        div { class: "overflow-y-auto max-h-64 border-b border-gray-800" }
        {list_phase_node}

        // Note palette
        div { class: "flex-1 overflow-y-auto" }
        div { class: "px-3 py-2 text-xs text-gray-500 uppercase tracking-wide", "Notes" }
        {palette_node}

        // ── Canvas workspace ──
        div { class: "flex-1 relative overflow-hidden bg-gray-950" }
        {match canvas_phase {
            CanvasPhase::Select => rsx! {
                div { class: "absolute inset-0 flex items-center justify-center" }
                EmptyState {
                    icon: Some(Icon::Palette),
                    title: "No canvas selected".to_string(),
                    description: Some("Click a canvas in the list to open it, or create a new one.".to_string()),
                }
            },
            CanvasPhase::Loading => rsx! {
                div { class: "absolute inset-0 flex items-center justify-center" }
                LoadingBlock { label: Some("Loading canvas…"), size: SpinnerSize::Lg }
            },
            CanvasPhase::Error => rsx! {
                div { class: "absolute inset-0 flex items-center justify-center p-8" }
                div { class: "w-full max-w-md" }
                ErrorPanel {
                    title: "Couldn't load the canvas".to_string(),
                    message: "The selected canvas could not be retrieved from the backend.".to_string(),
                    details: Some(canvas_err),
                    on_retry: on_canvas_retry,
                    recovery: Some("Try selecting another canvas or create a new one.".to_string()),
                }
            },
            CanvasPhase::Empty | CanvasPhase::Ready => {
                rsx! {
                    CanvasSurface {
                        canvas: s.canvas,
                        save_state: save_state,
                        save_error: save_error,
                        on_save: move |_: ()| { save_canvas(s); },
                        on_open_note: move |path: String| {
                            open_tab(workspace, &path);
                            *nav.view_mode.write_unchecked() = ViewMode::Editor;
                        },
                        on_remove_node: move |node_id: String| {
                            let mut c = s.canvas.read().clone();
                            if let Some(c) = c.as_mut() {
                                remove_node(c, &node_id);
                            }
                            *s.canvas.write_unchecked() = c;
                            save_canvas(s);
                        },
                    }
                }
            }
        }}

        // ── New canvas dialog ──
        PromptDialog {
            open: show_new_dialog,
            title: "New Canvas".to_string(),
            message: "Name your canvas:".to_string(),
            confirm_label: Some("Create"),
            on_submit: move |name: String| { create_canvas(s, name); },
            on_cancel: move |_: ()| {},
        }
    }
}

/// The interactive canvas surface: a pannable/zoomable transform container with
/// SVG edge connectors, positioned DOM node cards, and a screen-fixed toolbar.
#[allow(clippy::too_many_arguments)]
#[component]
fn CanvasSurface(
    canvas: Signal<Option<CanvasDef>>,
    save_state: Signal<SaveState>,
    save_error: Signal<Option<String>>,
    on_save: EventHandler<()>,
    on_open_note: EventHandler<String>,
    on_remove_node: EventHandler<String>,
) -> Element {
    // Interaction state (owned by the surface; not persisted on its own).
    let dragging = use_signal(|| None::<String>);
    let drag_origin = use_signal(|| (0.0f64, 0.0f64, 0.0f64, 0.0f64));
    let panning = use_signal(|| false);
    let pan_origin = use_signal(|| (0.0f64, 0.0f64));
    let pan_base = use_signal(|| (0.0f64, 0.0f64));

    let c = canvas
        .peek()
        .clone()
        .expect("CanvasSurface renders only when a canvas is loaded");
    let nodes = c.nodes.clone();
    let transform = format!("translate({}px, {}px) scale({})", c.pan_x, c.pan_y, c.zoom);
    let zoom_pct = (c.zoom * 100.0).round();

    // Precompute edge geometry once per render.
    let edge_segs: Vec<(String, f64, f64, f64, f64)> = c
        .edges
        .iter()
        .filter_map(|e| {
            edge_endpoints(&c, e).map(|(x1, y1, x2, y2)| (e.id.clone(), x1, y1, x2, y2))
        })
        .collect();

    let save_indicator: Element = match *save_state.peek() {
        SaveState::Idle => rsx! { span { class: "text-xs text-gray-500", "Saved" } },
        SaveState::Saving => rsx! {
            span { class: "flex items-center gap-1 text-xs text-amber-400",
                Spinner { size: SpinnerSize::Sm }
                " Saving…"
            }
        },
        SaveState::Saved => rsx! {
            span { class: "flex items-center gap-1 text-xs text-green-400",
                {render_icon_view(Icon::CircleCheck)}
                " Saved"
            }
        },
        SaveState::Error => rsx! {
            span { class: "flex items-center gap-1 text-xs text-red-400",
                {render_icon_view(Icon::CircleX)}
                " Save failed"
            }
        },
    };

    let save_err_indicator: Element = match save_error.peek().clone() {
        Some(m) => rsx! { span { class: "text-xs text-red-300 max-w-40 truncate", title: "{m}", "{m}" } },
        None => rsx! {},
    };

    rsx! {
        div {
            class: "absolute inset-0",
            style: "transform: {transform}; transform-origin: 0 0;",
            onwheel: move |ev: WheelEvent| {
                let web = ev.data().as_web_event();
                web.prevent_default();
                let delta = web.delta_y();
                let factor = if delta > 0.0 { 0.9 } else { 1.1 };
                let mut cc = canvas.peek().clone();
                if let Some(cc) = cc.as_mut() {
                    cc.zoom = (cc.zoom * factor).clamp(0.1, 5.0);
                }
                *canvas.write_unchecked() = cc;
                on_save.call(());
            },
            // Pan drag starts only on the background; nodes stop propagation.
            onmousedown: move |ev: MouseEvent| {
                let web = ev.data().as_web_event();
                let (px, py) = canvas
                    .peek()
                    .as_ref()
                    .map(|c| (c.pan_x, c.pan_y))
                    .unwrap_or((0.0, 0.0));
                *pan_origin.write_unchecked() = (web.client_x() as f64, web.client_y() as f64);
                *pan_base.write_unchecked() = (px, py);
                *panning.write_unchecked() = true;
                *dragging.write_unchecked() = None;
            },
            onmousemove: move |ev: MouseEvent| {
                let web = ev.data().as_web_event();
                let drag = dragging.peek().clone();
                if let Some(node_id) = drag.as_deref() {
                    let (sx, sy, nx, ny) = *drag_origin.peek();
                    let zoom = canvas.peek().as_ref().map(|c| c.zoom).unwrap_or(1.0);
                    let dx = (web.client_x() as f64 - sx) / zoom;
                    let dy = (web.client_y() as f64 - sy) / zoom;
                    let mut cc = canvas.peek().clone();
                    if let Some(cc) = cc.as_mut() {
                        if let Some(node) = cc.nodes.iter_mut().find(|n| n.id == node_id) {
                            node.x = nx + dx;
                            node.y = ny + dy;
                        }
                    }
                    *canvas.write_unchecked() = cc;
                } else if *panning.peek() {
                    let (ox, oy) = *pan_origin.peek();
                    let (bx, by) = *pan_base.peek();
                    let dx = web.client_x() as f64 - ox;
                    let dy = web.client_y() as f64 - oy;
                    let mut cc = canvas.peek().clone();
                    if let Some(cc) = cc.as_mut() {
                        cc.pan_x = bx + dx;
                        cc.pan_y = by + dy;
                    }
                    *canvas.write_unchecked() = cc;
                }
            },
            onmouseup: move |_: MouseEvent| {
                let was = dragging.peek().is_some() || *panning.peek();
                *dragging.write_unchecked() = None;
                *panning.write_unchecked() = false;
                if was {
                    on_save.call(());
                }
            },
            onmouseleave: move |_: MouseEvent| {
                *dragging.write_unchecked() = None;
                *panning.write_unchecked() = false;
            },
        }

        // Subtle grid for orientation (transformed with the canvas).
        div {
            class: "canvas-grid absolute inset-0",
            style: "background-image: radial-gradient(circle, #374151 1px, transparent 1px); background-size: 40px 40px;",
        }

        // Groups (rendered behind nodes).
        for g in &c.groups {
            {
                rsx! {
                    div {
                        key: g.id.clone(),
                        class: "absolute border-2 border-dashed border-gray-700 rounded-lg bg-gray-800/20",
                        style: "left: {g.x}px; top: {g.y}px; width: {g.width}px; height: {g.height}px;",
                    }
                }
            }
        }

        // Edges (SVG connectors), behind nodes.
        svg {
            class: "absolute inset-0",
            style: "width: 100%; height: 100%; overflow: visible;",
            key: c.edges.len(),
            for seg in &edge_segs {
                {
                    let x1 = seg.1;
                    let y1 = seg.2;
                    let x2 = seg.3;
                    let y2 = seg.4;
                    rsx! {
                        line {
                            key: seg.0.clone(),
                            x1: "{x1}",
                            y1: "{y1}",
                            x2: "{x2}",
                            y2: "{y2}",
                            stroke: "#4b5563",
                            "stroke-width": "2",
                            fill: "none",
                        }
                    }
                }
            }
            defs {
                marker {
                    id: "canvas-arrow",
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

        // Nodes (DOM cards).
        for node in &nodes {
            {
                let node_id = node.id.clone();
                let node_title = node.title.clone();
                let node_path = node.note_path.clone();
                let node_kind = node.kind.clone();
                let (nx, ny) = (node.x, node.y);
                let w = node.width.unwrap_or(DEFAULT_NODE_W);
                let h = node.height.unwrap_or(DEFAULT_NODE_H);
                let node_id_for_down = node_id.clone();
                let path_for_open = node_path.clone();
                let node_id_for_key = node_id.clone();
                rsx! {
                    div {
                        key: node_id_for_key,
                        class: "absolute bg-gray-800 border border-gray-600 rounded-lg shadow-lg cursor-move hover:border-blue-500 transition-colors",
                        style: "left: {nx}px; top: {ny}px; min-width: 180px; width: {w}px; min-height: {h}px;",
                        title: node_title.clone(),
                        onmousedown: move |ev: MouseEvent| {
                            let web = ev.data().as_web_event();
                            ev.stop_propagation();
                            *drag_origin.write_unchecked() = (
                                web.client_x() as f64,
                                web.client_y() as f64,
                                nx,
                                ny,
                            );
                            *dragging.write_unchecked() = Some(node_id_for_down.clone());
                            *panning.write_unchecked() = false;
                        },
                    ondoubleclick: move |_: MouseEvent| {
                        on_open_note.call(path_for_open.clone());
                    },
                    }
                    div { class: "flex items-center justify-between px-2 py-1 border-gray-700" }
                    span {
                        class: "text-xs font-medium text-gray-300 truncate",
                        title: node_path.clone(),
                    }
                    "{node_title}"
                    button {
                        class: "text-xs text-gray-500 hover:text-red-400",
                        "aria-label": "Remove node",
                        onmousedown: move |ev: MouseEvent| { ev.stop_propagation(); },
                        onclick: move |_: MouseEvent| {
                            on_remove_node.call(node_id.clone());
                        },
                        {render_icon_view(Icon::X)}
                    }
                    div { class: "px-2 py-1 text-xs text-gray-500", "{node_kind}" }
                }
            }
        }

        // ── Screen-fixed toolbar (not transformed) ──
        div {
            class: "absolute top-3 right-3 z-20 flex items-center gap-1.5 bg-gray-800/90 border border-gray-700 rounded-lg px-2 py-1",
        }
        button {
            class: "w-7 h-7 flex items-center justify-center hover:bg-gray-700 rounded",
            "aria-label": "Zoom out",
            onclick: move |_: MouseEvent| {
                let mut cc = canvas.peek().clone();
                if let Some(cc) = cc.as_mut() {
                    cc.zoom = (cc.zoom * 0.8).clamp(0.1, 5.0);
                }
                *canvas.write_unchecked() = cc;
                on_save.call(());
            },
            {render_icon_view(Icon::Minus)}
        }
        button {
            class: "w-7 h-7 flex items-center justify-center hover:bg-gray-700 rounded",
            "aria-label": "Reset view",
            onclick: move |_: MouseEvent| {
                let mut cc = canvas.peek().clone();
                if let Some(cc) = cc.as_mut() {
                    cc.pan_x = 0.0;
                    cc.pan_y = 0.0;
                    cc.zoom = 1.0;
                }
                *canvas.write_unchecked() = cc;
                on_save.call(());
            },
            {render_icon_view(Icon::Target)}
        }
        button {
            class: "w-7 h-7 flex items-center justify-center hover:bg-gray-700 rounded",
            "aria-label": "Zoom in",
            onclick: move |_: MouseEvent| {
                let mut cc = canvas.peek().clone();
                if let Some(cc) = cc.as_mut() {
                    cc.zoom = (cc.zoom * 1.2).clamp(0.1, 5.0);
                }
                *canvas.write_unchecked() = cc;
                on_save.call(());
            },
            {render_icon_view(Icon::Plus)}
        }
        div { class: "h-4 w-px bg-gray-700" }
        {save_indicator}
        {save_err_indicator}
        div { class: "text-xs text-gray-500 w-10 text-right", "{zoom_pct}%" }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, path: &str, x: f64, y: f64) -> CanvasNode {
        CanvasNode {
            id: id.to_string(),
            note_path: path.to_string(),
            title: path.to_string(),
            x,
            y,
            width: None,
            height: None,
            kind: "note".to_string(),
            source: String::new(),
            text: String::new(),
        }
    }

    fn edge(id: &str, source: &str, target: &str) -> CanvasEdge {
        CanvasEdge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            label: String::new(),
        }
    }

    fn entry(path: &str, title: &str) -> NoteIndexEntry {
        NoteIndexEntry {
            path: path.to_string(),
            title: title.to_string(),
            folder: String::new(),
            modified_at: String::new(),
            pinned: false,
        }
    }

    #[test]
    fn classify_list_transitions() {
        assert_eq!(classify_list(ListState::Idle, 0), ListPhase::Loading);
        assert_eq!(classify_list(ListState::Loading, 5), ListPhase::Loading);
        assert_eq!(classify_list(ListState::Loaded, 0), ListPhase::Empty);
        assert_eq!(classify_list(ListState::Loaded, 3), ListPhase::Loaded);
        assert_eq!(classify_list(ListState::Failed, 0), ListPhase::Error);
    }

    #[test]
    fn classify_canvas_phases() {
        let empty = new_canvas_with_id("Empty", "c1");
        let loaded = CanvasDef {
            id: "c2".to_string(),
            name: "Loaded".to_string(),
            nodes: vec![node("n1", "a.md", 0.0, 0.0)],
            ..Default::default()
        };
        assert_eq!(
            classify_canvas(CanvasLoadState::Idle, None, None),
            CanvasPhase::Select
        );
        assert_eq!(
            classify_canvas(CanvasLoadState::Loading, None, Some("c1")),
            CanvasPhase::Loading
        );
        assert_eq!(
            classify_canvas(CanvasLoadState::Loaded, Some(&empty), Some("c1")),
            CanvasPhase::Empty
        );
        assert_eq!(
            classify_canvas(CanvasLoadState::Loaded, Some(&loaded), Some("c2")),
            CanvasPhase::Ready
        );
        assert_eq!(
            classify_canvas(CanvasLoadState::Failed, Some(&loaded), Some("c2")),
            CanvasPhase::Error
        );
    }

    #[test]
    fn new_canvas_has_defaults() {
        let c = new_canvas_with_id("My Canvas", "abc");
        assert_eq!(c.name, "My Canvas");
        assert_eq!(c.id, "abc");
        assert!(c.nodes.is_empty());
        assert!(c.edges.is_empty());
        assert_eq!(c.zoom, 1.0);
        assert_eq!(c.pan_x, 0.0);
        assert_eq!(c.pan_y, 0.0);
    }

    #[test]
    fn next_node_position_cascades() {
        let c = new_canvas_with_id("c", "1");
        let (x0, y0) = next_node_position(&c, 0);
        let (x1, y1) = next_node_position(&c, 1);
        assert!(x0.abs() < 1e-9 && y0.abs() < 1e-9);
        assert!((x1 - 30.0).abs() < 1e-9 && (y1 - 30.0).abs() < 1e-9);
    }

    #[test]
    fn next_node_position_respects_pan_and_zoom() {
        let c = CanvasDef {
            pan_x: 100.0,
            pan_y: 50.0,
            zoom: 2.0,
            ..Default::default()
        };
        let (x, y) = next_node_position(&c, 0);
        assert!((x - (-50.0)).abs() < 1e-9);
        assert!((y - (-25.0)).abs() < 1e-9);
    }

    #[test]
    fn create_node_appends_and_ids_sequentially() {
        let mut c = new_canvas_with_id("c", "1");
        create_node(&mut c, &entry("note/a.md", "A"));
        create_node(&mut c, &entry("note/b.md", "B"));
        assert_eq!(c.nodes.len(), 2);
        assert_eq!(c.nodes[0].id, "n1");
        assert_eq!(c.nodes[1].id, "n2");
        assert_eq!(c.nodes[0].note_path, "note/a.md");
        assert_eq!(c.nodes[1].note_path, "note/b.md");
    }

    #[test]
    fn remove_node_cascades_to_edges() {
        let mut c = CanvasDef {
            id: "c".to_string(),
            name: "c".to_string(),
            nodes: vec![node("n1", "a.md", 0.0, 0.0), node("n2", "b.md", 10.0, 10.0)],
            edges: vec![edge("e1", "n1", "n2")],
            ..Default::default()
        };
        remove_node(&mut c, "n1");
        assert!(c.nodes.iter().all(|n| n.id != "n1"));
        assert!(c.edges.is_empty());
    }

    #[test]
    fn edge_endpoints_uses_node_centres() {
        let c = CanvasDef {
            nodes: vec![node("n1", "a.md", 0.0, 0.0), node("n2", "b.md", 100.0, 60.0)],
            edges: vec![edge("e1", "n1", "n2")],
            ..Default::default()
        };
        let (x1, y1, x2, y2) = edge_endpoints(&c, &c.edges[0]).unwrap();
        assert!((x1 - (0.0 + DEFAULT_NODE_W / 2.0)).abs() < 1e-9);
        assert!((y1 - (0.0 + DEFAULT_NODE_H / 2.0)).abs() < 1e-9);
        assert!((x2 - (100.0 + DEFAULT_NODE_W / 2.0)).abs() < 1e-9);
        assert!((y2 - (60.0 + DEFAULT_NODE_H / 2.0)).abs() < 1e-9);
    }

    #[test]
    fn edge_endpoints_missing_node_is_none() {
        let c = CanvasDef {
            nodes: vec![node("n1", "a.md", 0.0, 0.0)],
            edges: vec![edge("e1", "n1", "missing")],
            ..Default::default()
        };
        assert!(edge_endpoints(&c, &c.edges[0]).is_none());
    }

    #[test]
    fn canvas_summary_reflects_contents() {
        let mut c = new_canvas_with_id("c", "1");
        assert_eq!(canvas_summary(&c), "blank canvas");
        c.nodes.push(node("n1", "a.md", 0.0, 0.0));
        assert_eq!(canvas_summary(&c), "1 nodes · 0 connections");
    }

    #[test]
    fn generate_canvas_id_is_unique_and_prefixed() {
        let a = generate_canvas_id();
        let b = generate_canvas_id();
        assert!(a.starts_with("canvas-"));
        assert_ne!(a, b);
    }
}
