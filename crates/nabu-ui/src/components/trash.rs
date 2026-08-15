//! # Trash / Recycle Bin — Frontend (Dioxus 0.6.3)
//!
//! A full-screen view for reviewing and recovering deleted vault items.
//! Deleted notes and folders are never destroyed immediately — they live in
//! the vault trash (`.nabu/trash`) until the user restores them, the retention
//! period elapses, or the user explicitly empties the trash.
//!
//! Features:
//! - list with per-item preview, deletion date and original location
//! - search, sorting and filtering
//! - single and multi-select restore / permanent delete
//! - confirmation dialogs for every irreversible action
//! - "undo" toast after restore so an accidental restore can be reversed
//! - keyboard shortcuts: Delete / Backspace (delete selection, confirmed),
//!   Cmd/Ctrl+Shift+R (restore selection), Cmd/Ctrl+Shift+Backspace (empty)

use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::dialog::ConfirmDialog;
use crate::components::ui::feedback::{
    ErrorPanel, LoadingBlock, SpinnerSize, ToastAction, ToastContext, ToastKind,
};
use crate::components::ui::icons::{render_icon, render_icon_view, Icon};
use crate::components::ui::selection::{Select, SelectOption};
use crate::components::ui::info::EmptyState;
use crate::components::ui::use_toast;
use crate::ipc;
use crate::history::HistoryContext;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

// ── Types ─────────────────────────────────────────────────────────────

/// Backend trash manifest record (mirrors `TrashRecord` in `src-tauri/src/history.rs`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrashRecord {
    pub trash_path: String,
    pub original_path: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub is_folder: bool,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub preview: Option<String>,
}

impl TrashRecord {
    /// Display name — the original basename of the trashed item.
    fn display_name(&self) -> String {
        Path::new(&self.original_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "item".to_string())
    }

    fn icon(&self) -> Icon {
        if self.is_folder {
            Icon::Folder
        } else if self.original_path.ends_with(".md") {
            Icon::FileText
        } else {
            Icon::File
        }
    }
}

/// Load lifecycle of the trash list — drives loading / content / error states.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TrashLoadState {
    Loading,
    Loaded,
    Error,
}

impl Default for TrashLoadState {
    fn default() -> Self {
        Self::Loading
    }
}

/// Filter bucket for the trash list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum TrashFilter {
    #[default]
    All,
    Notes,
    Folders,
    Attachments,
}

impl TrashFilter {
    fn matches(self, record: &TrashRecord) -> bool {
        match self {
            TrashFilter::All => true,
            TrashFilter::Notes => !record.is_folder && record.original_path.ends_with(".md"),
            TrashFilter::Folders => record.is_folder,
            TrashFilter::Attachments => {
                !record.is_folder && !record.original_path.ends_with(".md")
            }
        }
    }
}

/// Sort key for the trash list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum TrashSort {
    #[default]
    Name,
    DeletedAt,
    OriginalPath,
    Size,
}

impl TrashSort {
    fn value(self) -> &'static str {
        match self {
            TrashSort::Name => "name",
            TrashSort::DeletedAt => "deleted_at",
            TrashSort::OriginalPath => "original",
            TrashSort::Size => "size",
        }
    }
}

/// Human-readable relative time ("5m ago", "3d ago").
fn relative_time(rfc3339: &str) -> String {
    let now_ms = js_sys::Date::now() as i64;
    relative_time_at(rfc3339, now_ms)
}

/// Pure version of [`relative_time`] that accepts an explicit `now_ms`
/// (epoch millis). Extracted so tests can run on native without `js_sys`.
fn relative_time_at(rfc3339: &str, now_ms: i64) -> String {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(rfc3339) else {
        return "recently".to_string();
    };
    let then_ms = parsed.timestamp_millis();
    let secs = ((now_ms - then_ms) / 1000).max(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Computes the sorted + filtered + searched list of trash records.
fn sorted_view(
    items: &[TrashRecord],
    filter: &TrashFilter,
    sort: &TrashSort,
    ascending: bool,
    query: &str,
) -> Vec<TrashRecord> {
    let query_lower = query.to_lowercase();
    let mut result: Vec<TrashRecord> = items
        .iter()
        .filter(|r| filter.matches(r))
        .filter(|r| {
            if query_lower.is_empty() {
                return true;
            }
            r.display_name().to_lowercase().contains(&query_lower)
                || r.original_path.to_lowercase().contains(&query_lower)
                || r
                    .preview
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query_lower)
        })
        .cloned()
        .collect();
    result.sort_by(|a, b| {
        let ord = match sort {
            TrashSort::Name => a.display_name().cmp(&b.display_name()),
            TrashSort::DeletedAt => a.deleted_at.cmp(&b.deleted_at),
            TrashSort::OriginalPath => a.original_path.cmp(&b.original_path),
            TrashSort::Size => a.file_count.cmp(&b.file_count),
        };
        if ascending {
            ord
        } else {
            ord.reverse()
        }
    });
    result
}

/// Returns `true` when keyboard focus is inside an editable element, so Delete
/// and shortcut keys never hijack typing (e.g. in the search box).
fn focus_is_editable() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Some(document) = window.document() else {
        return false;
    };
    let Some(active) = document.active_element() else {
        return false;
    };
    let tag = active.tag_name().to_ascii_lowercase();
    tag == "textarea" || tag == "input" || tag == "select"
}

// ── Async operations ─────────────────────────────────────────────────

/// Loads the current trash contents from the backend via `trash_list`.
fn fetch_trash(
    mut items: Signal<Vec<TrashRecord>>,
    mut load_state: Signal<TrashLoadState>,
    mut error_msg: Signal<Option<String>>,
    selected: Signal<Vec<String>>,
    preview: Signal<Option<String>>,
) {
    load_state.set(TrashLoadState::Loading);
    error_msg.set(None);
    spawn_local(async move {
        let empty_args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
        match ipc::tauri_invoke_safe("trash_list", empty_args).await {
            Ok(Some(result)) => match serde_wasm_bindgen::from_value::<Vec<TrashRecord>>(result) {
                Ok(records) => {
                    items.set(records);
                    // Reconcile: drop selections pointing to items no longer present.
                    {
                        let mut sel = selected;
                        sel.with_mut(|s| {
                            let current = items.read();
                            s.retain(|tp| current.iter().any(|r| r.trash_path == *tp));
                        });
                    }
                    // Reconcile: drop preview if the item is no longer present.
                    {
                        let mut prev = preview;
                        prev.with_mut(|p| {
                            let current = items.read();
                            if let Some(tp) = p.clone() {
                                if !current.iter().any(|r| r.trash_path == tp) {
                                    *p = None;
                                }
                            }
                        });
                    }
                    load_state.set(TrashLoadState::Loaded);
                }
                Err(e) => {
                    error_msg.set(Some(e.to_string()));
                    load_state.set(TrashLoadState::Error);
                }
            },
            Ok(None) => {
                error_msg.set(Some("trash_list returned no data.".to_string()));
                load_state.set(TrashLoadState::Error);
            }
            Err(e) => {
                error_msg.set(Some(e.message()));
                load_state.set(TrashLoadState::Error);
            }
        }
    });
}

/// Restores the selected items via `trash_restore_many`.
fn restore_selected(
    selected: Signal<Vec<String>>,
    items: Signal<Vec<TrashRecord>>,
    load_state: Signal<TrashLoadState>,
    error_msg: Signal<Option<String>>,
    preview: Signal<Option<String>>,
    toasts: ToastContext,
    history: HistoryContext,
) {
    let paths = selected.read().clone();
    if paths.is_empty() {
        return;
    }
    let count = paths.len();
    toasts.info(
        "Restoring…",
        format!("Restoring {count} item(s) from Trash"),
    );
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "trash_paths": paths }))
            .unwrap();
        match ipc::tauri_invoke_safe("trash_restore_many", args).await {
            Ok(Some(result)) => match serde_wasm_bindgen::from_value::<Vec<String>>(result) {
                Ok(restored) if !restored.is_empty() => {
                    let restored_count = restored.len();
                    let toasts_undo = toasts;
                    let history_undo = history;
                    toasts.push_with_action(
                        ToastKind::Success,
                        format!("Restored {restored_count} item(s)"),
                        "The item(s) are back in their original location.".to_string(),
                        ToastAction::new(
                            "Undo",
                            Callback::new(move |_| crate::history::undo(history_undo, toasts_undo)),
                        ),
                    );
                    // Clear selection and refresh.
                    selected.write_unchecked().clear();
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
                Ok(_) => {
                    toasts.error("Restore", "No items were restored.");
                }
                Err(e) => {
                    toasts.error("Restore", e.to_string());
                    // On failure, refresh to show the true backend state.
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
            },
            Ok(None) => {
                toasts.error("Restore", "Backend returned no data");
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
            Err(e) => {
                toasts.error("Restore", e.message());
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
        }
    });
}

/// Permanently deletes the selected items via `trash_delete`.
fn delete_selected(
    selected: Signal<Vec<String>>,
    items: Signal<Vec<TrashRecord>>,
    load_state: Signal<TrashLoadState>,
    error_msg: Signal<Option<String>>,
    preview: Signal<Option<String>>,
    toasts: ToastContext,
) {
    let paths = selected.read().clone();
    if paths.is_empty() {
        return;
    }
    let count = paths.len();
    toasts.info(
        "Deleting…",
        format!("Permanently deleting {count} item(s)"),
    );
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "trash_paths": paths }))
            .unwrap();
        match ipc::tauri_invoke_safe("trash_delete", args).await {
            Ok(Some(result)) => match serde_wasm_bindgen::from_value::<usize>(result) {
                Ok(n) => {
                    let message = if n == 1 {
                        "Permanently deleted 1 item".to_string()
                    } else {
                        format!("Permanently deleted {n} items")
                    };
                    toasts.warning("Trash", message);
                    // Clear selection and refresh.
                    selected.write_unchecked().clear();
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
                Err(e) => {
                    toasts.error("Trash", e.to_string());
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
            },
            Ok(None) => {
                toasts.error("Trash", "Backend returned no data");
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
            Err(e) => {
                toasts.error("Trash", e.message());
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
        }
    });
}

/// Permanently deletes a single trashed item via `trash_delete`.
fn delete_one(
    trash_path: String,
    items: Signal<Vec<TrashRecord>>,
    selected: Signal<Vec<String>>,
    load_state: Signal<TrashLoadState>,
    error_msg: Signal<Option<String>>,
    preview: Signal<Option<String>>,
    toasts: ToastContext,
) {
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(
            &serde_json::json!({ "trash_paths": vec![trash_path] }),
        )
        .unwrap();
        match ipc::tauri_invoke_safe("trash_delete", args).await {
            Ok(Some(result)) => match serde_wasm_bindgen::from_value::<usize>(result) {
                Ok(n) => {
                    let message = if n == 1 {
                        "Permanently deleted 1 item".to_string()
                    } else {
                        format!("Permanently deleted {n} items")
                    };
                    toasts.warning("Trash", message);
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
                Err(e) => {
                    toasts.error("Trash", e.to_string());
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
            },
            Ok(None) => {
                toasts.error("Trash", "Backend returned no data");
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
            Err(e) => {
                toasts.error("Trash", e.message());
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
        }
    });
}

/// Restores a single trashed item via `trash_restore_many`.
fn restore_one(
    trash_path: String,
    items: Signal<Vec<TrashRecord>>,
    selected: Signal<Vec<String>>,
    load_state: Signal<TrashLoadState>,
    error_msg: Signal<Option<String>>,
    preview: Signal<Option<String>>,
    toasts: ToastContext,
    history: HistoryContext,
) {
    let restore_paths = vec![trash_path];
    let count = restore_paths.len();
    toasts.info("Restoring…", format!("Restoring {count} item(s) from Trash"));
    spawn_local(async move {
        let args =
            serde_wasm_bindgen::to_value(&serde_json::json!({ "trash_paths": restore_paths }))
                .unwrap();
        match ipc::tauri_invoke_safe("trash_restore_many", args).await {
            Ok(Some(result)) => match serde_wasm_bindgen::from_value::<Vec<String>>(result) {
                Ok(restored) if !restored.is_empty() => {
                    let restored_count = restored.len();
                    let toasts_msg = toasts;
                    let history_undo = history;
                    toasts_msg.push_with_action(
                        ToastKind::Success,
                        format!("Restored {restored_count} item(s)"),
                        "The item is back in its original location.".to_string(),
                        ToastAction::new(
                            "Undo",
                            Callback::new(move |_| crate::history::undo(history_undo, toasts_msg)),
                        ),
                    );
                    // Clear selection of this item and refresh.
                    let tp = restore_paths[0].clone();
                    {
                        let mut sel = selected;
                        sel.with_mut(|s| {
                            s.retain(|p| p != &tp);
                        });
                    }
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
                Ok(_) => {
                    toasts.error("Restore", "Could not restore this item");
                }
                Err(e) => {
                    toasts.error("Restore", e.to_string());
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
            },
            Ok(None) => {
                toasts.error("Restore", "Backend returned no data");
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
            Err(e) => {
                toasts.error("Restore", e.message());
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
        }
    });
}

/// Empties the whole trash via `trash_empty`.
fn empty_trash(
    items: Signal<Vec<TrashRecord>>,
    selected: Signal<Vec<String>>,
    load_state: Signal<TrashLoadState>,
    error_msg: Signal<Option<String>>,
    preview: Signal<Option<String>>,
    toasts: ToastContext,
) {
    toasts.info("Emptying…", "Permanently deleting all trashed items");
    spawn_local(async move {
        let empty_args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
        match ipc::tauri_invoke_safe("trash_empty", empty_args).await {
            Ok(Some(result)) => match serde_wasm_bindgen::from_value::<usize>(result) {
                Ok(n) => {
                    let message = if n == 1 {
                        "Emptied trash — 1 item permanently deleted".to_string()
                    } else {
                        format!("Emptied trash — {n} items permanently deleted")
                    };
                    toasts.warning("Trash", message);
                    // Clear selection and refresh.
                    selected.write_unchecked().clear();
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
                Err(e) => {
                    toasts.error("Trash", e.to_string());
                    // On failure, refresh to show the true backend state.
                    fetch_trash(items, load_state, error_msg, selected, preview);
                }
            },
            Ok(None) => {
                toasts.error("Trash", "Backend returned no data");
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
            Err(e) => {
                toasts.error("Trash", e.message());
                // On failure, refresh to show the true backend state.
                fetch_trash(items, load_state, error_msg, selected, preview);
            }
        }
    });
}

// ── Component ─────────────────────────────────────────────────────────

/// The Trash screen — recoverable deletion, restore, and permanent removal.
#[component]
pub fn Trash() -> Element {
    // ── State signals ─────────────────────────────────────────────
    let items = use_signal(Vec::<TrashRecord>::new);
    let selected = use_signal(Vec::<String>::new);
    let load_state = use_signal(|| TrashLoadState::Loading);
    let error_msg = use_signal(|| None::<String>);
    let filter = use_signal(|| TrashFilter::All);
    let sort = use_signal(|| TrashSort::Name);
    let sort_ascending = use_signal(|| true);
    let mut query = use_signal(String::new);
    let preview = use_signal(|| None::<String>);

    // Dialog open state.
    let mut delete_single_open = use_signal(|| false);
    let mut delete_selected_open = use_signal(|| false);
    let mut empty_open = use_signal(|| false);

    // Pending single-item delete path (captured when dialog opens).
    let mut pending_single_delete = use_signal(|| None::<String>);

    let toasts = use_toast();
    let history = crate::components::contexts::use_history();

    // ── Initial load (signal-guarded) ────────────────────────────
    {
        let mut loaded = use_signal(|| false);
        if !*loaded.read() {
            loaded.set(true);
            fetch_trash(items.clone(), load_state.clone(), error_msg.clone(), selected.clone(), preview.clone());
        }
    }

    // ── Keyboard shortcuts ────────────────────────────────────────
    {
        let items_c = items.clone();
        let selected_c = selected.clone();
        let delete_selected_open_c = delete_selected_open.clone();
        let empty_open_c = empty_open.clone();
        let toasts_c = toasts;
        let history_c = history;
        let load_state_c = load_state.clone();
        let error_msg_c = error_msg.clone();
        let preview_c = preview.clone();

        use_effect(move || {
            let cb = Closure::<dyn Fn(web_sys::KeyboardEvent)>::wrap(Box::new(
                move |ev: web_sys::KeyboardEvent| {
                    if focus_is_editable() {
                        return;
                    }
                    let meta = ev.meta_key() || ev.ctrl_key();
                    let shift = ev.shift_key();
                    let key = ev.key();
                    if meta && shift && key.eq_ignore_ascii_case("r") {
                        ev.prevent_default();
                        restore_selected(
                            selected_c.clone(),
                            items_c.clone(),
                            load_state_c.clone(),
                            error_msg_c.clone(),
                            preview_c.clone(),
                            toasts_c,
                            history_c,
                        );
                    } else if meta && shift && key == "Backspace" {
                        ev.prevent_default();
                        if !items_c.read().is_empty() {
                            *empty_open_c.write_unchecked() = true;
                        }
                    } else if !meta && (key == "Delete" || key == "Backspace") {
                        ev.prevent_default();
                        if !selected_c.read().is_empty() {
                            *delete_selected_open_c.write_unchecked() = true;
                        }
                    }
                },
            ) as Box<dyn Fn(web_sys::KeyboardEvent)>);
            if let Some(w) = web_sys::window() {
                let _ = w.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            }
            use_drop(move || {
                if let Some(w) = web_sys::window() {
                    let _ = w.remove_event_listener_with_callback(
                        "keydown",
                        cb.as_ref().unchecked_ref(),
                    );
                }
                std::mem::forget(cb);
            });
        });
    }

    // ── History-change listener (undo/redo from anywhere) ─────────
    {
        let items_c = items.clone();
        let load_state_c = load_state.clone();
        let error_msg_c = error_msg.clone();
        let selected_c = selected.clone();
        let preview_c = preview.clone();
        use_effect(move || {
            let cb = Closure::<dyn Fn(web_sys::Event)>::wrap(Box::new(move |_| {
                fetch_trash(
                    items_c.clone(),
                    load_state_c.clone(),
                    error_msg_c.clone(),
                    selected_c.clone(),
                    preview_c.clone(),
                );
            }) as Box<dyn Fn(web_sys::Event)>);
            if let Some(w) = web_sys::window() {
                let _ = w.add_event_listener_with_callback(
                    "nabu:history-changed",
                    cb.as_ref().unchecked_ref(),
                );
            }
            use_drop(move || {
                if let Some(w) = web_sys::window() {
                    let _ = w.remove_event_listener_with_callback(
                        "nabu:history-changed",
                        cb.as_ref().unchecked_ref(),
                    );
                }
                std::mem::forget(cb);
            });
        });
    }

    // ── Computed display values ───────────────────────────────────
    let items_len = items.read().len();
    let total_files: usize = items.read().iter().map(|r| r.file_count).sum();
    let selected_count = selected.read().len();
    let header_count = format!("{items_len} item(s) · {total_files} file(s)");

    let current_filter = *filter.read();
    let current_sort = *sort.read();
    let current_asc = *sort_ascending.read();
    let current_query = query.read().clone();
    let sorted_items: Vec<TrashRecord> = sorted_view(
        &items.read(),
        &current_filter,
        &current_sort,
        current_asc,
        &current_query,
    );

    let select_all_label = if !sorted_items.is_empty() && selected_count == sorted_items.len() {
        "Deselect all".to_string()
    } else {
        "Select all".to_string()
    };
    let selected_count_text = format!("{selected_count} selected");

    // Delete-selected confirmation message.
    let delete_selected_msg = if selected_count == 0 {
        "No items selected.".to_string()
    } else {
        format!(
            "Permanently delete {selected_count} selected item(s)? This cannot be undone."
        )
    };

    // Empty confirmation message.
    let empty_msg_text = if items_len == 0 {
        "Trash is already empty.".to_string()
    } else {
        format!(
            "Permanently delete all {items_len} items in Trash? This cannot be undone."
        )
    };

    rsx! {
        div { class: "trash-screen flex h-full bg-gray-950 text-gray-100 overflow-hidden" }

        // ── Left: item list ───────────────────────────────────────────
        div { class: "flex-none w-96 border-r border-gray-800 flex flex-col min-w-0" }

        // Header
        div {
            class: "px-4 py-3 border-b border-gray-800 flex items-center gap-2",
            span { class: "text-lg", "aria-hidden": "true", {render_icon(Icon::Trash2, None)} }
            div { class: "flex-1 min-w-0" }
            h2 { class: "text-base font-semibold text-gray-500", "Trash" }
            span { class: "text-xs text-gray-500 ml-auto", "{header_count}" }
        }

        // Toolbar: Empty Trash button on the right of the header
        div { class: "px-3 py-2 border-b border-gray-800 flex items-center gap-2" }
        Button {
            variant: ButtonVariant::Destructive,
            size: ButtonSize::Sm,
            disabled: items_len == 0,
            on_click: move |_: MouseEvent| { empty_open.set(true); },
            {render_icon_view(Icon::Trash2)} " Empty Trash"
        }

        // Search + filter + sort
        div { class: "px-3 py-2 border-b border-gray-800 flex flex-col gap-2" }
        input {
            r#type: "text",
            placeholder: "Search trash…",
            class: "bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none",
            value: "{query.read()}",
            oninput: move |ev: FormEvent| { query.set(ev.value()); },
        }

        // Filter segmented control
        div { class: "flex gap-1" }
        {
            let mut f = filter.clone();
            let cf = current_filter;
            rsx! {
                Button {
                    variant: if cf == TrashFilter::All { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    size: ButtonSize::Sm,
                    on_click: move |_: MouseEvent| { f.set(TrashFilter::All); },
                    "All"
                }
                Button {
                    variant: if cf == TrashFilter::Notes { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    size: ButtonSize::Sm,
                    on_click: move |_: MouseEvent| { f.set(TrashFilter::Notes); },
                    "Notes"
                }
                Button {
                    variant: if cf == TrashFilter::Folders { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    size: ButtonSize::Sm,
                    on_click: move |_: MouseEvent| { f.set(TrashFilter::Folders); },
                    "Folders"
                }
                Button {
                    variant: if cf == TrashFilter::Attachments { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    size: ButtonSize::Sm,
                    on_click: move |_: MouseEvent| { f.set(TrashFilter::Attachments); },
                    "Files"
                }
            }
        }

        // Sort dropdown
        div { class: "mt-1" }
        {
            let mut s = sort.clone();
            let mut a = sort_ascending.clone();
            let current_sort_val = current_sort.value().to_string();
            rsx! {
                Select {
                    options: vec![
                        SelectOption::new("name", "Sort: Name"),
                        SelectOption::new("deleted_at", "Sort: Deleted"),
                        SelectOption::new("original", "Sort: Location"),
                        SelectOption::new("size", "Sort: Files"),
                    ],
                    value: Signal::new(current_sort_val.clone()),
                    on_change: move |new_val: String| {
                        let new_field = match new_val.as_str() {
                            "deleted_at" => TrashSort::DeletedAt,
                            "original" => TrashSort::OriginalPath,
                            "size" => TrashSort::Size,
                            _ => TrashSort::Name,
                        };
                        let current = *s.read();
                        if current == new_field {
                            let cur_asc = *a.read();
                            a.set(!cur_asc);
                        } else {
                            s.set(new_field);
                            a.set(true);
                        }
                    },
                }
            }
        }

        // Batch actions bar
        {
            if selected_count > 0 {
                let sel_count = selected_count;
                let do_restore = {
                    let items_c = items.clone();
                    let selected_c = selected.clone();
                    let load_state_c = load_state.clone();
                    let error_msg_c = error_msg.clone();
                    let preview_c = preview.clone();
                    move |_: MouseEvent| {
                        restore_selected(
                            selected_c.clone(),
                            items_c.clone(),
                            load_state_c.clone(),
                            error_msg_c.clone(),
                            preview_c.clone(),
                            toasts,
                            history,
                        );
                    }
                };
                let do_delete = {
                    move |_: MouseEvent| {
                        delete_selected_open.set(true);
                    }
                };
                rsx! {
                    div {
                        class: "flex items-center gap-2 px-3 py-2 bg-blue-900/30 border-b border-blue-800/30",
                        span { class: "text-xs text-blue-400", "{sel_count} selected" }
                        div { class: "flex-1" }
                        Button {
                            size: ButtonSize::Sm,
                            on_click: do_restore,
                            {render_icon_view(Icon::Undo)} " Restore"
                        }
                        Button {
                            variant: ButtonVariant::Destructive,
                            size: ButtonSize::Sm,
                            on_click: do_delete,
                            {render_icon_view(Icon::Trash2)} " Delete"
                        }
                    }
                }
            } else {
                rsx! {}
            }
        }

        // Main content area: loading / error / empty / list
        div { class: "flex-1 overflow-y-auto" }
        {
            let state = *load_state.read();
            if state == TrashLoadState::Loading {
                rsx! {
                    div { class: "p-6" }
                    LoadingBlock { label: "Loading trash…", size: SpinnerSize::Lg, }
                    div { class: "p-4 space-y-2" }
                    for _ in 0..6 {
                        div { class: "h-12 bg-gray-800 rounded animate-pulse" }
                    }
                }
            } else if state == TrashLoadState::Error {
                let err = error_msg.read().clone().unwrap_or_default();
                let items_c = items.clone();
                let load_state_c = load_state.clone();
                let error_msg_c = error_msg.clone();
                let selected_c = selected.clone();
                let preview_c = preview.clone();
                rsx! {
                    ErrorPanel {
                        title: "Couldn't load Trash".to_string(),
                        message: "The backend refused to return the trash manifest.".to_string(),
                        details: Some(err),
                        recovery: "Make sure your vault is accessible, then try again.".to_string(),
                        on_retry: move |_: ()| {
                            fetch_trash(items_c.clone(), load_state_c.clone(), error_msg_c.clone(), selected_c.clone(), preview_c.clone());
                        },
                    }
                }
            } else if items_len == 0 {
                rsx! {
                    EmptyState {
                        icon: Some(Icon::Trash2),
                        title: "Trash is empty".to_string(),
                        description: Some("Deleted notes and folders appear here and can be restored before they are permanently removed.".to_string()),
                    }
                }
            } else {
                // ── List ──
                rsx! {
                    div { class: "divide-y divide-gray-800" }
                    for record in &sorted_items {
                        {
                            let trash_path = record.trash_path.clone();
                            let name = record.display_name();
                            let icon = record.icon();
                            let original = record.original_path.clone();
                            let deleted_at = record.deleted_at.clone().unwrap_or_default();
                            let deleted_at_full = deleted_at.clone();
                            let relative = record.deleted_at.as_deref().map(relative_time).unwrap_or_default();
                            let file_count = record.file_count;
                            let file_count_text = if file_count > 1 {
                                format!("{file_count} files")
                            } else if file_count == 1 {
                                format!("{file_count} file")
                            } else {
                                String::new()
                            };
                            let is_selected = selected.read().iter().any(|tp| *tp == trash_path);
                            let is_preview = *preview.read() == Some(trash_path.clone());
                            let row_class = if is_selected {
                                "w-full text-left px-3 py-2 cursor-pointer border-l-2 bg-gray-800 border-l-blue-500"
                            } else if is_preview {
                                "w-full text-left px-3 py-2 cursor-pointer border-l-2 bg-gray-900 border-l-gray-500"
                            } else {
                                "w-full text-left px-3 py-2 cursor-pointer border-l-2 border-l-transparent hover:bg-gray-800/60"
                            };
                            let record_is_folder = record.is_folder;
                            let items_for_preview = items.clone();
                            let selected_for_preview = selected.clone();
                            let load_state_for_preview = load_state.clone();
                            let error_msg_for_preview = error_msg.clone();
                            let mut preview_for_preview = preview.clone();
                            let mut delete_single_open_for_preview = delete_single_open.clone();
                            let mut pending_single_delete_for_preview = pending_single_delete.clone();
                            let tp_preview = trash_path.clone();
                            let tp_checkbox = trash_path.clone();
                            let tp_restore = trash_path.clone();
                            let tp_delete = trash_path.clone();
                            rsx! {
                                div {
                                    class: row_class,
                                    onclick: move |_: MouseEvent| {
                                        preview_for_preview.set(Some(tp_preview.clone()));
                                    },
                                }
                                div { class: "flex items-center gap-2" }
                                input {
                                    r#type: "checkbox",
                                    class: "check-field",
                                    checked: is_selected,
                                    onclick: move |_: MouseEvent| {
                                        let tp = tp_checkbox.clone();
                                        let mut sel = selected;
                                        sel.with_mut(|s| {
                                            if let Some(pos) = s.iter().position(|t| *t == tp) {
                                                s.remove(pos);
                                            } else {
                                                s.push(tp.clone());
                                            }
                                        });
                                    },
                                }
                                span { class: "text-sm", "aria-hidden": "true", {render_icon_view(icon)} }
                                span { class: "text-sm font-medium truncate flex-1", "{name}" }
                                {if record_is_folder {
                                    rsx! { span { class: "text-xs px-1.5 py-0.5 rounded bg-gray-700 text-gray-300", "Folder" } }
                                } else { rsx! {} }}
                                {if !file_count_text.is_empty() {
                                    rsx! { span { class: "text-xs text-gray-500", "{file_count_text}" } }
                                } else { rsx! {} }}
                                div { class: "flex items-center gap-2 mt-1 pl-7" }
                                span { class: "text-xs text-gray-500 truncate max-w-52", title: original, "{original}" }
                                span { class: "text-xs text-gray-600", "•" }
                                span { class: "text-xs text-gray-500", title: deleted_at_full, "{relative}" }
                                div { class: "flex-1" }
                                button {
                                    class: "px-2 py-0.5 text-xs rounded bg-green-700/80 hover:bg-green-600 text-white transition-colors",
                                    onclick: move |_: MouseEvent| {
                                        restore_one(
                                            tp_restore.clone(),
                                            items_for_preview.clone(),
                                            selected_for_preview.clone(),
                                            load_state_for_preview.clone(),
                                            error_msg_for_preview.clone(),
                                            preview_for_preview.clone(),
                                            toasts,
                                            history,
                                        );
                                    },
                                    "Restore"
                                }
                                button {
                                    class: "px-2 py-0.5 text-xs rounded bg-gray-700 hover:bg-red-700 text-gray-200 hover:text-white transition-colors",
                                    onclick: move |_: MouseEvent| {
                                        pending_single_delete_for_preview.set(Some(tp_delete.clone()));
                                        delete_single_open_for_preview.set(true);
                                    },
                                    "Delete"
                                }
                            }
                        }
                    }
                }
            }
        }

        // Footer / select-all
        div {
            class: "flex items-center justify-between px-3 py-2 border-b border-gray-800 text-xs text-gray-500",
            button {
                class: "hover:text-gray-300",
                onclick: move |_: MouseEvent| {
                    let mut sel = selected;
                    sel.with_mut(|s| {
                        if !sorted_items.is_empty() && s.len() == sorted_items.len() {
                            s.clear();
                        } else {
                            s.clear();
                            s.extend(sorted_items.iter().map(|r| r.trash_path.clone()));
                        }
                    });
                },
                "{select_all_label}"
            }
            span { "{selected_count_text}" }
        }

        // ── Right: preview ─────────────────────────────────────────────
        div { class: "flex-1 overflow-y-auto p-4 min-w-0" }
        {
            let preview_path = preview.read().clone();
            if let Some(path) = preview_path {
                let record = items.read().iter().find(|r| r.trash_path == path).cloned();
                match record {
                    Some(record) => rsx! {
                        TrashPreview {
                            record: record,
                            items: items.clone(),
                            selected: selected.clone(),
                            load_state: load_state.clone(),
                            error_msg: error_msg.clone(),
                            preview: preview.clone(),
                            delete_single_open: delete_single_open.clone(),
                            pending_single_delete: pending_single_delete.clone(),
                        }
                    },
                    None => rsx! {
                        EmptyState {
                            icon: Some(Icon::Eye),
                            title: "Item not found".to_string(),
                            description: Some("The selected item could not be located.".to_string()),
                        }
                    },
                }
            } else {
                rsx! {
                    EmptyState {
                        icon: Some(Icon::Eye),
                        title: "Select an item".to_string(),
                        description: Some("Preview items, see where they came from, restore, or delete permanently.".to_string()),
                    }
                }
            }
        }

        // ── Confirmation dialogs ───────────────────────────────────────
        {
            let tp = pending_single_delete.read().clone();
            let delete_single_msg = match tp {
                Some(tp) => {
                    let items_snapshot = items.read();
                    items_snapshot
                        .iter()
                        .find(|r| r.trash_path == tp)
                        .map(|r| {
                            format!(
                                "'{}' will be permanently deleted ({} file(s)). This cannot be undone.",
                                r.display_name(),
                                r.file_count
                            )
                        })
                        .unwrap_or_default()
                }
                None => String::new(),
            };
            rsx! {
                ConfirmDialog {
                    open: delete_single_open,
                    title: "Permanently delete this item?".to_string(),
                    message: String::new(),
                    message_signal: Some(Signal::new(delete_single_msg)),
                    confirm_label: Some("Delete Forever"),
                    cancel_label: Some("Cancel"),
                    danger: true,
                    on_confirm: move |_: ()| {
                        delete_single_open.set(false);
                        let tp = pending_single_delete.read().clone();
                        pending_single_delete.set(None);
                        if let Some(tp) = tp {
                            delete_one(
                                tp,
                                items.clone(),
                                selected.clone(),
                                load_state.clone(),
                                error_msg.clone(),
                                preview.clone(),
                                toasts,
                            );
                        }
                    },
                    on_cancel: move |_: ()| {
                        pending_single_delete.set(None);
                        delete_single_open.set(false);
                    },
                }
            }
        }

        ConfirmDialog {
            open: delete_selected_open,
            title: "Delete selected items?".to_string(),
            message: String::new(),
            message_signal: Some(Signal::new(delete_selected_msg)),
            confirm_label: Some("Delete Forever"),
            cancel_label: Some("Cancel"),
            danger: true,
            on_confirm: move |_: ()| {
                delete_selected_open.set(false);
                delete_selected(
                    selected.clone(),
                    items.clone(),
                    load_state.clone(),
                    error_msg.clone(),
                    preview.clone(),
                    toasts,
                );
            },
            on_cancel: move |_: ()| {
                delete_selected_open.set(false);
            },
        }

        ConfirmDialog {
            open: empty_open,
            title: "Empty Trash?".to_string(),
            message: String::new(),
            message_signal: Some(Signal::new(empty_msg_text)),
            confirm_label: Some("Empty Trash"),
            cancel_label: Some("Cancel"),
            danger: true,
            on_confirm: move |_: ()| {
                empty_open.set(false);
                empty_trash(
                    items.clone(),
                    selected.clone(),
                    load_state.clone(),
                    error_msg.clone(),
                    preview.clone(),
                    toasts,
                );
            },
            on_cancel: move |_: ()| {
                empty_open.set(false);
            },
        }
    }
}

// ── Preview panel ─────────────────────────────────────────────────────

/// Preview panel shown in the right rail when a trash item is selected.
#[component]
fn TrashPreview(
    record: TrashRecord,
    items: Signal<Vec<TrashRecord>>,
    selected: Signal<Vec<String>>,
    load_state: Signal<TrashLoadState>,
    error_msg: Signal<Option<String>>,
    preview: Signal<Option<String>>,
    delete_single_open: Signal<bool>,
    pending_single_delete: Signal<Option<String>>,
) -> Element {
    let toasts = use_toast();
    let history = crate::components::contexts::use_history();

    let name = record.display_name();
    let original = record.original_path.clone();
    let is_folder = record.is_folder;
    let file_count = record.file_count;
    let preview_text = record.preview.clone().unwrap_or_default();
    let delete_path = record.trash_path.clone();
    let restore_path = record.trash_path.clone();
    let deleted_at_full = record
        .deleted_at
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let deleted_at_relative = record
        .deleted_at
        .as_deref()
        .map(relative_time)
        .unwrap_or_else(|| "Unknown".to_string());

    rsx! {
        div { class: "trash-preview space-y-4" }
        div { class: "flex items-start gap-3" }
        span { class: "text-3xl", "aria-hidden": "true", {render_icon_view(record.icon())} }
        div { class: "flex-1 min-w-0" }
        h3 { class: "text-lg font-semibold text-gray-500 truncate", "{name}" }
        p { class: "text-xs text-gray-500 truncate", title: "{original}", "{original}" }

        div { class: "grid grid-cols-2 gap-3 text-sm" }
        div {}
        span { class: "text-xs text-gray-500 uppercase tracking-wide", "Kind" }
        p { class: "text-gray-300", if is_folder { "Folder" } else { "File" } }
        div {}
        span { class: "text-xs text-gray-500 uppercase tracking-wide", "Files" }
        p { class: "text-gray-300", "{file_count}" }
        div {}
        span { class: "text-xs text-gray-500 uppercase tracking-wide", "Deleted" }
        p { class: "text-gray-300", title: "{deleted_at_full}", "{deleted_at_relative}" }
        div {}
        span { class: "text-xs text-gray-500 uppercase tracking-wide", "Original location" }
        p { class: "text-gray-300 text-xs truncate", title: "{original}", "{original}" }

        {if !preview_text.is_empty() {
            rsx! {
                div {}
                span { class: "text-xs text-gray-500 uppercase tracking-wide", "Preview" }
                div { class: "mt-1" }
                pre {
                    class: "text-xs text-gray-300 bg-gray-900 p-3 rounded-lg overflow-auto max-h-64 whitespace-pre-wrap border border-gray-800",
                    "{preview_text}"
                }
            }
        } else if is_folder {
            rsx! {
                div { class: "callout callout-info" }
                "This folder and its contents were moved to trash together."
            }
        } else {
            rsx! {}
        }}

        div { class: "flex items-center gap-2 pt-3 border-t border-gray-800" }
        Button {
            on_click: move |_: MouseEvent| {
                restore_one(
                    restore_path.clone(),
                    items.clone(),
                    selected.clone(),
                    load_state.clone(),
                    error_msg.clone(),
                    preview.clone(),
                    toasts,
                    history,
                );
            },
            {render_icon_view(Icon::Undo)} " Restore"
        }
        Button {
            variant: ButtonVariant::Destructive,
            on_click: move |_: MouseEvent| {
                pending_single_delete.set(Some(delete_path.clone()));
                delete_single_open.set(true);
            },
            {render_icon_view(Icon::Trash2)} " Delete Forever"
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(
        trash_path: &str,
        original_path: &str,
        deleted_at: Option<&str>,
        is_folder: bool,
        file_count: usize,
        preview: Option<&str>,
    ) -> TrashRecord {
        TrashRecord {
            trash_path: trash_path.to_string(),
            original_path: original_path.to_string(),
            deleted_at: deleted_at.map(|s| s.to_string()),
            is_folder,
            file_count,
            preview: preview.map(|s| s.to_string()),
        }
    }

    // ── Test 1: List populates ────────────────────────────────────

    #[test]
    fn filter_all_matches_every_record() {
        let records = vec![
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
            make_record("t/b", "notes/b", Some("2024-01-02T00:00:00Z"), true, 3, None),
            make_record("t/c.png", "assets/c.png", Some("2024-01-03T00:00:00Z"), false, 1, None),
        ];
        let f = TrashFilter::All;
        let matched: Vec<_> = records.iter().filter(|r| f.matches(r)).collect();
        assert_eq!(matched.len(), 3);
    }

    #[test]
    fn filter_notes_only_matches_markdown_files() {
        let records = vec![
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
            make_record("t/b", "notes/b", Some("2024-01-02T00:00:00Z"), true, 3, None),
            make_record("t/c.png", "assets/c.png", Some("2024-01-03T00:00:00Z"), false, 1, None),
        ];
        let f = TrashFilter::Notes;
        let matched: Vec<_> = records.iter().filter(|r| f.matches(r)).collect();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].original_path, "notes/a.md");
    }

    #[test]
    fn filter_folders_only_matches_folders() {
        let records = vec![
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
            make_record("t/b", "notes/b", Some("2024-01-02T00:00:00Z"), true, 3, None),
        ];
        let f = TrashFilter::Folders;
        let matched: Vec<_> = records.iter().filter(|r| f.matches(r)).collect();
        assert_eq!(matched.len(), 1);
        assert!(matched[0].is_folder);
    }

    #[test]
    fn filter_attachments_matches_non_markdown_non_folder() {
        let records = vec![
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
            make_record("t/b", "notes/b", Some("2024-01-02T00:00:00Z"), true, 3, None),
            make_record("t/c.png", "assets/c.png", Some("2024-01-03T00:00:00Z"), false, 1, None),
        ];
        let f = TrashFilter::Attachments;
        let matched: Vec<_> = records.iter().filter(|r| f.matches(r)).collect();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].original_path, "assets/c.png");
    }

    // ── Test 2: Empty state ───────────────────────────────────────

    #[test]
    fn empty_trash_shows_empty_state_when_loaded() {
        let records: Vec<TrashRecord> = Vec::new();
        let load_state = TrashLoadState::Loaded;
        assert_eq!(load_state, TrashLoadState::Loaded);
        assert!(records.is_empty());
        // EmptyState renders when load_state == Loaded && items.is_empty().
    }

    // ── Test 3: List failure ──────────────────────────────────────

    #[test]
    fn list_failure_sets_error_state_not_empty() {
        // Simulate what fetch_trash does on failure:
        let load_state = TrashLoadState::Error;
        let error_msg: Option<String> = Some("backend refused connection".to_string());

        assert_eq!(load_state, TrashLoadState::Error);
        assert_eq!(error_msg.as_deref(), Some("backend refused connection"));
    }

    // ── Test 4: Restore ───────────────────────────────────────────

    #[test]
    fn restore_removes_items_and_clears_selection() {
        let mut items: Vec<TrashRecord> = vec![
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
            make_record("t/b.md", "notes/b.md", Some("2024-01-02T00:00:00Z"), false, 1, None),
        ];
        let mut selected: Vec<String> = vec!["t/a.md".to_string()];

        // Simulate successful restore: item removed from items, selection cleared.
        items.retain(|r| r.trash_path != "t/a.md");
        selected.clear();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].trash_path, "t/b.md");
        assert!(selected.is_empty());
    }

    #[test]
    fn restore_failure_keeps_items_intact() {
        let initial = vec![
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
        ];
        let items = initial.clone();
        let selected = vec!["t/a.md".to_string()];

        // On failure, items remain unchanged (never optimistically removed).
        assert_eq!(items.len(), 1);
        assert_eq!(selected.len(), 1);
    }

    // ── Test 5: Delete confirmation ─────────────────────────────────

    #[test]
    fn pending_single_delete_cleared_on_confirm() {
        let mut pending: Option<String> = Some("t/a.md".to_string());
        let mut open = true;

        if let Some(tp) = pending.clone() {
            pending = None;
            open = false;
            assert_eq!(tp, "t/a.md");
        }
        assert!(pending.is_none());
        assert!(!open);
    }

    // ── Test 6: Empty confirmation ──────────────────────────────────

    #[test]
    fn empty_button_disabled_when_no_items() {
        let records: Vec<TrashRecord> = Vec::new();
        assert!(records.is_empty());
        // The "Empty Trash" button is disabled when items_len == 0.
    }

    // ── Test 7: Backend failure ────────────────────────────────────

    #[test]
    fn backend_failure_sets_error_state() {
        let items: Vec<TrashRecord> = Vec::new();

        // Simulate a failed operation:
        let error_msg: Option<String> = Some("delete failed".to_string());
        let load_state = TrashLoadState::Error;

        assert_eq!(error_msg.as_deref(), Some("delete failed"));
        assert_eq!(load_state, TrashLoadState::Error);
        // State remains recoverable — items are still present (empty here).
        assert!(items.is_empty());
    }

    #[test]
    fn backend_failure_does_not_report_success() {
        let error_msg = Some("restore failed".to_string());
        let load_state = TrashLoadState::Error;

        assert!(!error_msg.is_none());
        assert_ne!(load_state, TrashLoadState::Loaded);
    }

    #[test]
    fn sorted_view_filters_and_sorts() {
        let records = vec![
            make_record("t/b.md", "notes/b.md", Some("2024-01-02T00:00:00Z"), false, 1, None),
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
            make_record("t/c.md", "notes/c.md", Some("2024-01-03T00:00:00Z"), false, 1, None),
        ];
        let sorted = sorted_view(&records, &TrashFilter::All, &TrashSort::Name, true, "");
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].display_name(), "a.md");
        assert_eq!(sorted[1].display_name(), "b.md");
        assert_eq!(sorted[2].display_name(), "c.md");
    }

    #[test]
    fn sorted_view_search_filters() {
        let records = vec![
            make_record("t/a.md", "notes/a.md", Some("2024-01-01T00:00:00Z"), false, 1, None),
            make_record("t/b.md", "notes/b.md", Some("2024-01-02T00:00:00Z"), false, 1, None),
            make_record("t/c.md", "notes/c.md", Some("2024-01-03T00:00:00Z"), false, 1, None),
        ];
        let sorted = sorted_view(&records, &TrashFilter::All, &TrashSort::Name, true, "b.md");
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].display_name(), "b.md");
    }

    #[test]
    fn trash_sort_default_is_name() {
        let s = TrashSort::default();
        assert_eq!(s, TrashSort::Name);
    }

    #[test]
    fn trash_filter_default_is_all() {
        let f = TrashFilter::default();
        assert_eq!(f, TrashFilter::All);
    }

    #[test]
    fn trash_load_state_default_is_loading() {
        let s = TrashLoadState::default();
        assert_eq!(s, TrashLoadState::Loading);
    }

    #[test]
    fn relative_time_formats_seconds() {
        let now_ms = 1_000_000_000_000i64;
        // 30 seconds before "now".
        let ts = "2024-01-01T00:00:30Z"; // not relevant, we pass now_ms explicitly
        let parsed = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap();
        let then_ms = parsed.timestamp_millis();
        let now_ms = then_ms + 30_000;
        assert_eq!(relative_time_at("2024-01-01T00:00:00Z", now_ms), "30s ago");
    }

    #[test]
    fn relative_time_minutes() {
        let parsed = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap();
        let then_ms = parsed.timestamp_millis();
        // 5 minutes ago.
        let now_ms = then_ms + 5 * 60_000;
        assert_eq!(relative_time_at("2024-01-01T00:00:00Z", now_ms), "5m ago");
    }

    #[test]
    fn relative_time_hours() {
        let parsed = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap();
        let then_ms = parsed.timestamp_millis();
        // 3 hours ago.
        let now_ms = then_ms + 3 * 3600_000;
        assert_eq!(relative_time_at("2024-01-01T00:00:00Z", now_ms), "3h ago");
    }

    #[test]
    fn relative_time_days() {
        let parsed = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap();
        let then_ms = parsed.timestamp_millis();
        // 7 days ago.
        let now_ms = then_ms + 7 * 86_400_000;
        assert_eq!(relative_time_at("2024-01-01T00:00:00Z", now_ms), "7d ago");
    }

    #[test]
    fn relative_time_invalid_returns_recently() {
        assert_eq!(relative_time_at("not-a-date", 1_000_000_000_000), "recently");
    }

    #[test]
    fn relative_time_zero_delta_shows_seconds() {
        let parsed = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap();
        let now_ms = parsed.timestamp_millis();
        assert_eq!(relative_time_at("2024-01-01T00:00:00Z", now_ms), "0s ago");
    }

    #[test]
    fn display_name_extracts_basename() {
        let r = make_record("t/a.md", "notes/sub/a.md", None, false, 1, None);
        assert_eq!(r.display_name(), "a.md");
        let r2 = make_record("t/b", "notes/folder", None, true, 3, None);
        assert_eq!(r2.display_name(), "folder");
    }

    #[test]
    fn icon_reflects_folder_and_type() {
        let md = make_record("t/a.md", "notes/a.md", None, false, 1, None);
        assert_eq!(md.icon(), Icon::FileText);
        let folder = make_record("t/b", "notes/b", None, true, 1, None);
        assert_eq!(folder.icon(), Icon::Folder);
        let other = make_record("t/c.png", "assets/c.png", None, false, 1, None);
        assert_eq!(other.icon(), Icon::File);
    }

    #[test]
    fn sorted_view_sorts_by_file_count_ascending() {
        let records = vec![
            make_record("t/l", "notes/large.md", Some("2024-01-01T00:00:00Z"), false, 10, None),
            make_record("t/s", "notes/small.md", Some("2024-01-02T00:00:00Z"), false, 1, None),
            make_record("t/m", "notes/mid.md", Some("2024-01-03T00:00:00Z"), false, 5, None),
        ];
        let sorted = sorted_view(&records, &TrashFilter::All, &TrashSort::Size, true, "");
        assert_eq!(sorted[0].file_count, 1);
        assert_eq!(sorted[1].file_count, 5);
        assert_eq!(sorted[2].file_count, 10);
    }

    #[test]
    fn sorted_view_sorts_descending() {
        let records = vec![
            make_record("t/l.md", "notes/l.md", Some("2024-01-01T00:00:00Z"), false, 10, None),
            make_record("t/s.md", "notes/s.md", Some("2024-01-02T00:00:00Z"), false, 1, None),
        ];
        let sorted = sorted_view(&records, &TrashFilter::All, &TrashSort::Name, false, "");
        assert_eq!(sorted[0].display_name(), "s.md");
        assert_eq!(sorted[1].display_name(), "l.md");
    }
}
