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
//! - persist edits through `canvas_save`, surfacing failures in-view via a
//!   save-status indicator and a toast;
//! - create / delete canvases through `canvas_save` / `canvas_delete`;
//! - add notes from the vault index onto the canvas (double-click in the
//!   sidebar palette) and open them through the existing workspace APIs.

use crate::components::contexts::{open_tab, use_nav, use_workspace, NoteIndexEntry, ViewMode};
use crate::components::ui::dialog::{ConfirmDialog, PromptDialog};
use crate::components::ui::feedback::{
    ErrorPanel, LoadingBlock, SaveStateIndicator, SpinnerSize, ToastContext, ToastKind,
};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CanvasDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub edges: Vec<CanvasEdge>,
    #[serde(default)]
    pub groups: Vec<CanvasGroup>,
    pub pan_x: f64,
    pub pan_y: f64,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}

fn default_zoom() -> f64 {
    1.0
}

const DEFAULT_NODE_W: f64 = 240.0;
const DEFAULT_NODE_H: f64 = 120.0;

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

/// Compute the SVG line endpoints for an edge's two node centres.
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
/// functions stay single-argument and testable.
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
    let mut list_data = s.list_data;
    let mut list_state = s.list_state;
    let mut list_error = s.list_error;
    list_state.set(ListState::Loading);
    list_error.set(String::new());

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_list_args()).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke_safe("canvas_list", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<CanvasDef>>(val) {
                Ok(list) => {
                    list_data.set(list.clone());
                    list_state.set(ListState::Loaded);
                    list_error.set(String::new());
                    if s.active_id.read().is_none() && !list.is_empty() {
                        select_canvas(s, list[0].id.clone());
                    }
                }
                Err(e) => {
                    list_error.set(format!("Could not parse canvases: {e}"));
                    list_state.set(ListState::Failed);
                    s.toasts
                        .error("Canvas list failed", format!("Could not parse canvases: {e}"));
                }
            },
            Ok(None) => {
                list_error.set("Canvas list request returned no value.".to_string());
                list_state.set(ListState::Failed);
                s.toasts
                    .error("Canvas list failed", "No canvases were returned by the backend.".to_string());
            }
            Err(e) => {
                list_error.set(e.message());
                list_state.set(ListState::Failed);
                s.toasts.error("Canvas list failed", e.message());
            }
        }
    });
}

/// Loads a single canvas definition by id. `s.active_id` is set synchronously so
/// the view shows a loading state while the IPC is in flight.
fn select_canvas(s: CanvasState, id: String) {
    let mut active_id = s.active_id;
    let mut canvas = s.canvas;
    let mut canvas_state = s.canvas_state;
    let mut canvas_error = s.canvas_error;
    active_id.set(Some(id.clone()));
    canvas_state.set(CanvasLoadState::Loading);
    canvas_error.set(String::new());

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_get_args(&id)).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke_safe("canvas_get", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Option<CanvasDef>>(val) {
                Ok(Some(c)) => {
                    canvas.set(Some(c));
                    canvas_state.set(CanvasLoadState::Loaded);
                    canvas_error.set(String::new());
                }
                Ok(None) => {
                    canvas.set(None);
                    canvas_state.set(CanvasLoadState::Failed);
                    canvas_error.set(format!("Canvas \"{id}\" was not found."));
                }
                Err(e) => {
                    canvas.set(None);
                    canvas_state.set(CanvasLoadState::Failed);
                    canvas_error.set(format!("Could not parse the canvas: {e}"));
                }
            },
            Ok(None) => {
                canvas.set(None);
                canvas_state.set(CanvasLoadState::Failed);
                canvas_error.set(format!("Canvas \"{id}\" could not be loaded."));
            }
            Err(e) => {
                canvas.set(None);
                canvas_state.set(CanvasLoadState::Failed);
                canvas_error.set(e.message());
            }
        }
    });
}

/// Persists the currently loaded canvas. Sets `save_state` to `Saving` while
/// in flight, `Saved` on success, and `Error` (with `save_error`) on failure —
/// surfacing backend failures both in the workspace toolbar and as a toast.
fn save_canvas(s: CanvasState) {
    let mut save_state = s.save_state;
    let mut save_error = s.save_error;
    let current = s.canvas.read().clone();
    let Some(c) = current else {
        return;
    };
    save_state.set(SaveState::Saving);
    save_error.set(None);

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_save_args(&c)).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke("canvas_save", args).await {
            Ok(val) => match serde_wasm_bindgen::from_value::<()>(val) {
                Ok(()) => {
                    save_state.set(SaveState::Saved);
                    save_error.set(None);
                    s.toasts.success("Canvas saved", c.name.clone());
                }
                Err(e) => {
                    let msg = format!("Could not save canvas: {e}");
                    save_state.set(SaveState::Error);
                    save_error.set(Some(msg.clone()));
                    s.toasts.error("Save failed", msg);
                }
            },
            Err(e) => {
                let msg = e.message();
                save_state.set(SaveState::Error);
                save_error.set(Some(msg.clone()));
                s.toasts.error("Save failed", msg);
            }
        }
    });
}

/// Creates a new canvas: persists it, selects it, and refreshes the list.
fn create_canvas(s: CanvasState, name: String) {
    if name.trim().is_empty() {
        s.toasts
            .error("Invalid name", "Canvas name cannot be empty.");
        return;
    }
    let id = generate_canvas_id();
    let c = new_canvas_with_id(&name, &id);
    let mut list_data = s.list_data;
    let mut list_state = s.list_state;
    let mut active_id = s.active_id;
    let mut canvas = s.canvas;
    let mut canvas_state = s.canvas_state;
    let mut canvas_error = s.canvas_error;
    let mut save_state = s.save_state;
    let mut save_error = s.save_error;
    let name_for_toast = name.clone();
    // Optimistically surface the new canvas while it persists in the background.
    active_id.set(Some(id.clone()));
    canvas.set(Some(c.clone()));
    canvas_state.set(CanvasLoadState::Loaded);
    canvas_error.set(String::new());
    save_state.set(SaveState::Idle);
    save_error.set(None);
    // Keep the list in sync optimistically.
    list_data.with_mut(|l| l.push(c.clone()));

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&canvas_save_args(&c)).unwrap_or(JsValue::NULL);
        match crate::ipc::tauri_invoke("canvas_save", args).await {
            Ok(val) => match serde_wasm_bindgen::from_value::<()>(val) {
                Ok(()) => {
                    save_state.set(SaveState::Saved);
                    list_state.set(ListState::Loaded);
                    load_list(s);
                    s.toasts
                        .success("Canvas created", name_for_toast);
                }
                Err(e) => {
                    save_state.set(SaveState::Error);
                    s.toasts
                        .error("Create failed", format!("Could not create canvas: {e}"));
                    // Revert optimistically-selected state on failure.
                    active_id.set(None);
                    canvas.set(None);
                    canvas_state.set(CanvasLoadState::Failed);
                    list_data.with_mut(|l| l.retain(|x| x.id != id));
                }
            },
            Err(e) => {
                save_state.set(SaveState::Error);
                s.toasts
                    .error("Create failed", e.message());
                active_id.set(None);
                canvas.set(None);
                canvas_state.set(CanvasLoadState::Failed);
                list_data.with_mut(|l| l.retain(|x| x.id != id));
            }
        }
    });
}

/// Deletes a canvas by id, refreshing the list and switching the active canvas
/// when the deleted one was the one being edited.
fn delete_canvas(s: CanvasState, id: String) {
    let mut list_data = s.list_data;
    let mut list_state = s.list_state;
    let mut active_id = s.active_id;
    let mut canvas = s.canvas;
    let mut canvas_state = s.canvas_state;
    let mut canvas_error = s.canvas_error;
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
                    list_data.with_mut(|l| l.retain(|c| c.id != id));
                    if was_active {
                        active_id.set(None);
                        canvas.set(None);
                        canvas_state.set(CanvasLoadState::Idle);
                        canvas_error.set(String::new());
                        list_state.set(ListState::Loaded);
                        if let Some(nid) = next_id {
                            select_canvas(s, nid);
                        }
                    } else {
                        list_state.set(ListState::Loaded);
                    }
                    s.toasts.success("Canvas deleted", name);
                }
                Err(e) => {
                    s.toasts
                        .error("Delete failed", format!("Could not delete canvas: {e}"));
                }
            },
            Err(e) => {
                s.toasts
                    .error("Delete failed", e.message());
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

    let new_name = use_signal(String::new);
    let delete_target = use_signal(|| None::<String>);

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
    let mut initialized = use_signal(|| false);
    if !*initialized.read() {
        initialized.set(true);
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
    let save_st = *save_state.read();
    let save_err_val = save_error.read().clone();

    let notes_index = nav.notes_index.read().clone();

    // Callback: open a node's referenced note in the editor.
    let on_open_note = move |path: String| {
        open_tab(workspace, &path);
        let mut nav = nav;
        nav.view_mode.set(ViewMode::Editor);
    };

    // Callback: persist the working canvas (used by the surface + edits).
    let on_save = move |_: ()| {
        save_canvas(s);
    };

    // Callback: remove a node from the working canvas and persist.
    let on_remove_node = move |node_id: String| {
        let mut c = s.canvas.read().clone();
        if let Some(c) = c.as_mut() {
            remove_node(c, &node_id);
        }
        s.canvas.set(c);
        on_save.call(());
    };

    // Callback: add a note from the palette to the working canvas and persist.
    let on_add_note = {
        let on_save = on_save;
        move |entry: NoteIndexEntry| {
            let mut c = s.canvas.read().clone();
            if let Some(c) = c.as_mut() {
                create_node(c, &entry);
            }
            s.canvas.set(c);
            on_save.call(());
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
            onclick: move |_: MouseEvent| { new_name.set(String::new()); },
            onclick: move |_: MouseEvent| {
                // PromptDialog manages its own input; opening just flips the flag.
                let _ = ();
            },
        }

        // Canvas list (loading / empty / error / loaded).
        div { class: "overflow-y-auto max-h-64 border-b border-gray-800" }
        match list_phase {
            ListPhase::Loading => rsx! {
                div { class: "p-3", LoadingBlock { label: Some("Loading canvases…"), size: SpinnerSize::Lg } }
            },
            ListPhase::Error => rsx! {
                div { class: "p-3" }
                ErrorPanel {
                    title: "Couldn't load canvases".to_string(),
                    message: "Your saved canvases could not be retrieved from the backend.".to_string(),
                    details: Some(list_err),
                    on_retry: Some(move |_: ()| { load_list(s); }),
                    recovery: Some("Make sure your vault is accessible and try again.".to_string()),
                }
            },
            ListPhase::Empty => rsx! {
                div { class: "px-3 py-2 text-xs text-gray-500", "No canvases yet — create one with the + button." }
            },
            ListPhase::Loaded => rsx! {
                for c in &rows {
                    {
                        let id = c.id.clone();
                        let name = c.name.clone();
                        let summary = canvas_summary(c);
                        let is_active = active.as_deref() == Some(id.as_str());
                        let mut active_id = active_id;
                        let mut canvas_state = canvas_state;
                        let mut canvas_error = canvas_error;
                        let s_for_select = s;
                        let class = if is_active {
                            "flex items-center justify-between px-3 py-1.5 cursor-pointer bg-gray-800 border-l-2 border-blue-500 rounded"
                        } else {
                            "flex items-center justify-between px-3 py-1.5 cursor-pointer hover:bg-gray-800 rounded"
                        };
                        rsx! {
                            div {
                                key: id.clone(),
                                class: class,
                                ondblclick: move |_: MouseEvent| {
                                    select_canvas(s_for_select, id.clone());
                                },
                            }
                            div { class: "flex-1 min-w-0" }
                            div { class: "text-sm font-medium text-gray-200 truncate", "{name}" }
                            div { class: "text-xs text-gray-500 truncate", "{summary}" }
                            button {
                                class: "text-xs text-gray-500 hover:text-red-400",
                                "aria-label": format!("Delete canvas {name}"),
                                onclick: move |ev: MouseEvent| {
                                    ev.stop_propagation();
                                    *delete_target.write_unchecked() = Some(id.clone());
                                },
                                {render_icon_view(Icon::Trash2)}
                            }
                        }
                    }
                }
                ()
            },
        }

        // Note palette — double-click a note to drop it on the canvas.
        div { class: "flex-1 overflow-y-auto" }
        div { class: "px-3 py-2 text-xs text-gray-500 uppercase tracking-wide", "Notes" }
        if notes_index.is_empty() {
            rsx! {
                div { class: "px-3 py-2 text-xs text-gray-400", "No notes indexed to place yet." }
            }
        } else {
            rsx! {
                for entry in &notes_index {
                    {
                        let title = entry.title.clone();
                        let path = entry.path.clone();
                        let entry_clone = entry.clone();
                        rsx! {
                            div {
                                key: path.clone(),
                                class: "px-3 py-1.5 text-sm cursor-pointer hover:bg-gray-800 truncate",
                                title: "Double-click to add to canvas",
                                ondblclick: move |_: MouseEvent| {
                                    if s.canvas.read().is_some() {
                                        on_add_note(entry_clone.clone());
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
                ()
            }
        }

        // ── Canvas workspace ──
        div { class: "flex-1 relative overflow-hidden bg-gray-950" }

        match canvas_phase {
            CanvasPhase::Select => rsx! {
                div { class: "flex-1 flex items-center justify-center" }
                EmptyState {
                    icon: Some(Icon::Palette),
                    title: "No canvas selected".to_string(),
                    description: Some("Click a canvas in the list to open it, or create a new one.".to_string()),
                }
            },
            CanvasPhase::Loading => rsx! {
                div { class: "flex-1 flex items-center justify-center" }
                LoadingBlock { label: Some("Loading canvas…"), size: SpinnerSize::Lg }
            },
            CanvasPhase::Error => rsx! {
                div { class: "flex-1 flex items-center justify-center p-8" }
                div { class: "w-full max-w-md" }
                ErrorPanel {
                    title: "Couldn't load the canvas".to_string(),
                    message: "The selected canvas could not be retrieved from the backend.".to_string(),
                    details: Some(canvas_err),
                    on_retry: Some({
                        let s_for_retry = s;
                        move |_: ()| {
                            if let Some(id) = s_for_retry.active_id.read().clone() {
                                select_canvas(s_for_retry, id);
                            }
                        }
                    }),
                    recovery: Some("Try selecting another canvas or create a new one.".to_string()),
                }
            },
            CanvasPhase::Empty | CanvasPhase::Ready => {
                let canvas_def = sel_canvas.expect("Empty/Ready phase guarantees a loaded canvas");
                rsx! {
                    CanvasSurface {
                        canvas: canvas_def,
                        canvas_signal: s.canvas,
                        save_state: save_state,
                        save_error: save_error,
                        on_save: on_save,
                        on_open_note: on_open_note,
                        on_remove_node: on_remove_node,
                    }
                }
            }
        }

        // ── New canvas dialog ──
        PromptDialog {
            open: use_signal(|| false),
            title: "New Canvas".to_string(),
            message: "Name your canvas:".to_string(),
            on_submit: Some({
                let s_for_create = s;
                move |name: String| {
                    create_canvas(s_for_create, name);
                }
            }),
            on_cancel: Some(move |_: ()| {}),
        }

        // ── Delete confirmation ──
        ConfirmDialog {
            open: use_signal(|| false),  // TODO: bind to delete_target.is_some()
            title: "Delete canvas?".to_string(),
            message: {
                let name = delete_target
                    .read()
                    .as_ref()
                    .and_then(|id| rows.iter().find(|c| c.id == id))
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                format!("Remove \"{name}\"? This cannot be undone.")
            },
            danger: true,
            on_confirm: Some({
                let s_for_delete = s;
                move |_: ()| {
                    if let Some(id) = delete_target.read().clone() {
                        delete_canvas(s_for_delete, id);
                    }
                }
            }),
            on_cancel: Some(move |_: ()| {
                *delete_target.write_unchecked() = None;
            }),
        }
    }
}
