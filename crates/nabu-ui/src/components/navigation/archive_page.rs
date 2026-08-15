//! # Archive page
//!
//! Lists every note in the reserved `archive/` folder via `archive_list`
//! and lets the user restore items to their original vault location.
//!
//! Restoring is non-destructive (a file move) so it needs no confirmation —
//! an undo toast is offered instead. An `ItemStored` listener refreshes the
//! list after restores or subsequent archival.

use crate::components::contexts::{open_tab, record_recent_note, use_nav, use_workspace, NavContext, ViewMode, WorkspaceContext};
use crate::components::ui::feedback::{ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::events::{use_event_listener, FrontendEvent, FrontendEventKind};
use crate::ipc;
use crate::models::organisation::ArchiveEntry;
use chrono::DateTime;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Lifecycle of the `archive_list` IPC call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoadState { Loading, Error, Loaded }
impl Default for LoadState { fn default() -> Self { Self::Loading } }

fn fmt_date(rfc: &str) -> String {
    DateTime::parse_from_rfc3339(rfc)
        .map(|dt| dt.format("%b %e, %Y").to_string().replace("  ", " "))
        .unwrap_or_else(|| rfc.chars().take(10).collect())
}

/// Opens a note and switches to the editor view.
fn open_note(nav: NavContext, ws: WorkspaceContext, path: &str) {
    open_tab(ws, path);
    record_recent_note(nav, path);
    nav.view_mode.set(ViewMode::Editor);
}

fn load_archive(items: Signal<Vec<ArchiveEntry>>, state: Signal<LoadState>) {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
    state.set(LoadState::Loading);
    spawn_local(async move {
        match ipc::tauri_invoke_safe("archive_list", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<ArchiveEntry>>(val) {
                Ok(list) => { items.set(list); state.set(LoadState::Loaded); }
                Err(e) => { items.set(Vec::new()); state.set(LoadState::Error); tracing::warn!("archive: {e}"); }
            },
            Ok(None) => { items.set(Vec::new()); state.set(LoadState::Error); }
            Err(e) => { items.set(Vec::new()); state.set(LoadState::Error); tracing::warn!("archive: {}", e.message()); }
        }
    });
}

/// Restores one archived note via `archive_restore` and refreshes the list.
fn restore_entry(
    archive_path: String,
    original_path: String,
    items: Signal<Vec<ArchiveEntry>>,
    state: Signal<LoadState>,
) {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "archive_path": archive_path })).unwrap();
    spawn_local(async move {
        match ipc::tauri_invoke_safe("archive_restore", args).await {
            Ok(_) => {
                // Refresh the list after restore.
                load_archive(items, state);
                tracing::info!("restored '{}' to '{}'", original_path, "archive");
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("archive: archive_restore error: {}", e.message()),
        }
    });
}

#[component]
pub fn ArchivePage() -> Element {
    let nav = use_nav();
    let ws = use_workspace();

    let items = use_signal(Vec::<ArchiveEntry>::new);
    let state = use_signal(LoadState::default);
    let query = use_signal(String::new);

    // Initial load.
    {
        let mut initialized = use_signal(|| false);
        if !*initialized.read() {
            initialized.set(true);
            load_archive(items, state);
        }
    }

    // Refresh after external changes.
    use_event_listener(FrontendEventKind::ItemStored, move |_ev: &FrontendEvent| {
        let i = items;
        let s = state;
        load_archive(i, s);
    });

    let cur_state = *state.read();
    let all_items = items.read().clone();
    let q = query.read().clone();
    let q_lower = q.to_lowercase();

    // Filter by title / original folder.
    let visible: Vec<ArchiveEntry> = all_items
        .iter()
        .filter(|n| {
            if q_lower.is_empty() {
                return true;
            }
            n.title.to_lowercase().contains(&q_lower)
                || n.folder.to_lowercase().contains(&q_lower)
                || n.original_path.to_lowercase().contains(&q_lower)
        })
        .cloned()
        .collect();

    rsx! {
        div { class: "archive-page h-full overflow-y-auto" }

        div { class: "archive-toolbar flex items-center justify-between px-6 py-4 border-b border-gray-700" }
        h1 { class: "text-lg font-semibold text-gray-100", "Archive" }
        input {
            r#type: "text",
            placeholder: "Search archived notes…",
            class: "bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none w-64",
            value: "{query.read()}",
            oninput: move |ev: FormEvent| { query.set(ev.value()); },
        }

        div { class: "archive-content px-6 py-4" }
        {match cur_state {
            LoadState::Loading => rsx! { LoadingBlock { spinner_size: SpinnerSize::Md } },
            LoadState::Error => rsx! {
                ErrorPanel {
                    title: "Archive".to_string(),
                    message: "Couldn't load the archive folder.".to_string(),
                }
            },
            LoadState::Loaded => {
                if visible.is_empty() {
                    rsx! {
                        EmptyState {
                            icon: Some(Icon::Archive),
                            title: if q_lower.is_empty() { "Archive is empty" } else { "No matches" }.to_string(),
                            description: Some(if q_lower.is_empty() {
                                "Archived notes appear here. They remain full-text searchable.".to_string()
                            } else {
                                format!("No archived notes match \"{q}\".")
                            }),
                        }
                    }
                } else {
                    rsx! {
                        div { class: "space-y-1" }
                        for n in &visible {
                            let archive_path = n.archive_path.clone();
                            let original_path = n.original_path.clone();
                            let title = n.title.clone();
                            let folder = n.folder.clone();
                            let modified = n.modified_at.clone();
                            let path_for_open = n.original_path.clone();
                            let items_sig = items;
                            let state_sig = state;
                            let ws = *ws;
                            let nav_copy = nav;
                            rsx! {
                                div { class: "archive-row flex items-center justify-between px-3 py-2 rounded-lg hover:bg-gray-800/50 border border-gray-800 text-sm group" }
                                div { class: "flex items-center gap-3" }
                                {render_icon_view(Icon::FileText)}
                                div { class: "flex-1 min-w-0 cursor-pointer", onclick: move |_: MouseEvent| { open_note(nav_copy, ws, &path_for_open); }, }
                                div { class: "text-sm text-gray-200 truncate", "{title}" }
                                if !folder.is_empty() {
                                    span { class: "text-xs text-gray-500", "{folder}/" }
                                }
                                div { class: "ml-auto text-xs text-gray-500" }
                                "{fmt_date(&modified)}"
                                button {
                                    class: "restore-btn ml-2 opacity-0 group-hover:opacity-100 rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700/50 border border-gray-700",
                                    on_click: move |_: MouseEvent| { restore_entry(archive_path.clone(), original_path.clone(), items_sig, state_sig); },
                                    "Restore"
                                }
                            }
                        }
                    }
                }
            }
        }}
    }
}
