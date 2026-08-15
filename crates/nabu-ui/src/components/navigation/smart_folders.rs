//! # Smart folders page
//!
//! Virtual collections powered by saved queries. The folder list is read from
//! the NavContext `smart_folders` signal (persisted in settings). Selecting a
//! folder runs the backend `smart_folder_evaluate` command and renders the
//! matching notes. New folders are created via the `save_smart_folder` helper
//! (updates the NavContext + persists to settings); deleted folders use
//! `remove_smart_folder`.

use crate::components::contexts::{
    open_tab, record_recent_note, remove_smart_folder, save_smart_folder, use_nav, use_workspace,
    NavContext, ViewMode, WorkspaceContext,
};
use crate::components::navigation::state::NoteIndexEntry;
use crate::components::ui::feedback::{use_toast, ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::events::{use_event_listener, FrontendEvent, FrontendEventKind};
use crate::ipc;
use crate::models::organisation::SmartFolder;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Lifecycle of the `smart_folder_evaluate` IPC call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoadState {
    Loading,
    Error,
    Loaded,
}
impl Default for LoadState {
    fn default() -> Self {
        Self::Loading
    }
}

fn fmt_date(rfc: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc)
        .map(|dt| dt.format("%b %e, %Y").to_string().replace("  ", " "))
        .unwrap_or_else(|| rfc.chars().take(10).collect())
}

fn open_note(nav: NavContext, ws: WorkspaceContext, path: &str) {
    open_tab(ws, path);
    record_recent_note(nav, path);
    nav.view_mode.set(ViewMode::Editor);
}

/// Evaluates a saved smart folder's query and stores the notes.
fn evaluate_folder(
    folder: SmartFolder,
    results: Signal<Vec<NoteIndexEntry>>,
    state: Signal<LoadState>,
) {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "query": folder.query })).unwrap();
    state.set(LoadState::Loading);
    spawn_local(async move {
        match ipc::tauri_invoke_safe("smart_folder_evaluate", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<NoteIndexEntry>>(val) {
                Ok(list) => {
                    results.set(list);
                    state.set(LoadState::Loaded);
                }
                Err(e) => {
                    results.set(Vec::new());
                    state.set(LoadState::Error);
                    tracing::warn!("smart folder: {e}");
                }
            },
            Ok(None) => {
                results.set(Vec::new());
                state.set(LoadState::Error);
            }
            Err(e) => {
                results.set(Vec::new());
                state.set(LoadState::Error);
                tracing::warn!("smart folder: {}", e.message());
            }
        }
    });
}

#[component]
pub fn SmartFoldersPage() -> Element {
    let nav = use_nav();
    let ws = use_workspace();
    let toasts = use_toast();

    let selected = use_signal(|| None::<SmartFolder>);
    let results = use_signal(Vec::<NoteIndexEntry>::new);
    let results_state = use_signal(LoadState::default);
    let show_form = use_signal(|| false);
    let new_name = use_signal(String::new);
    let new_query = use_signal(String::new);

    // Re-evaluate the selected folder after edits elsewhere.
    use_event_listener(FrontendEventKind::ItemStored, move |_ev: &FrontendEvent| {
        let s = selected.read().clone();
        if let Some(f) = s {
            evaluate_folder(f, results, results_state);
        }
    });

    let folders = nav.smart_folders.read().clone();
    let sel = selected.read().clone();
    let show = show_form.read();
    let sel_none = sel.is_none();

    rsx! {
        div { class: "smart-folders-page h-full overflow-y-auto" }

        div { class: "sf-toolbar flex items-center justify-between px-6 py-4 border-b border-gray-700" }
        h1 { class: "text-lg font-semibold text-gray-100", "Smart Folders" }
        button {
            class: "sf-new inline-flex items-center gap-1 rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50 border border-gray-700",
            onclick: move |_: MouseEvent| { show_form.set(!show_form.read()); },
            "{render_icon_view(Icon::Plus)}"
            "New"
        }

        div { class: "sf-body px-6 py-4" }

        // Create form.
        if show {
            rsx! {
                div { class: "sf-form mb-4 flex items-end gap-3 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700" }
                div { class: "flex-1" }
                input {
                    r#type: "text",
                    placeholder: "Folder name",
                    class: "w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none",
                    value: "{new_name.read()}",
                    oninput: move |ev: FormEvent| { new_name.set(ev.value()); },
                }
                input {
                    r#type: "text",
                    placeholder: "tag:work folder:projects after:2024-01-01",
                    class: "w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none",
                    value: "{new_query.read()}",
                    oninput: move |ev: FormEvent| { new_query.set(ev.value()); },
                }
                button {
                    class: "sf-submit rounded px-3 py-1.5 text-sm text-gray-900 bg-blue-500 hover:bg-blue-400 font-medium",
                    onclick: move |_: MouseEvent| {
                        let name = new_name.read().clone();
                        let query = new_query.read().clone();
                        if !name.is_empty() && !query.is_empty() {
                            let id = format!("sf_{}", js_sys::Date::now());
                            let folder = SmartFolder {
                                id, name, icon: "folder-tree".to_string(), query, pinned: false
                            };
                            save_smart_folder(nav, folder);
                            new_name.set(String::new());
                            new_query.set(String::new());
                            show_form.set(false);
                            toasts.success("Smart Folder", format!("Created \"{}\"", new_name.read()));
                        }
                    },
                    "Create"
                }
                button {
                    class: "sf-cancel rounded px-3 py-1.5 text-sm text-gray-300 hover:bg-gray-700/50 border border-gray-700",
                    onclick: move |_: MouseEvent| { show_form.set(false); },
                    "Cancel"
                }
            }
        }

        // Folder list.
        {if folders.is_empty() {
            rsx! {
                EmptyState {
                    icon: Some(Icon::FolderTree),
                    title: "No smart folders yet".to_string(),
                    description: Some("Create one to filter your vault by tag, folder, date or text.".to_string()),
                }
            }
        } else {
            rsx! {
                div { class: "sf-list space-y-1" }
                for f in &folders {
                    let f_clone = f.clone();
                    let id = f.id.clone();
                    let ws = ws;
                    rsx! {
                        div {
                            class: "sf-item flex items-center justify-between px-3 py-2 rounded-lg hover:bg-gray-800/50 border border-gray-700 text-sm group",
                            onclick: move |_: MouseEvent| {
                                let s = selected;
                                let r = results;
                                let rst = results_state;
                                s.set(Some(f_clone.clone()));
                                evaluate_folder(f_clone, r, rst);
                            },
                        }
                        div { class: "flex items-center gap-2" }
                        {render_icon_view(Icon::FolderTree)}
                        div { class: "flex-1 min-w-0" }
                        div { class: "text-sm text-gray-200 truncate", "{f.name}" }
                        if f.pinned { span { class: "text-xs text-yellow-400", "★" } }
                        div { class: "text-xs text-gray-500 truncate", "query: {f.query}" }
                        div { class: "sf-actions ml-auto opacity-0 group-hover:opacity-100" }
                        button {
                            class: "sf-delete rounded px-1.5 py-0.5 text-xs text-gray-400 hover:text-red-400",
                            onclick: move |_: MouseEvent| { remove_smart_folder(nav, id.clone()); toasts.success("Smart Folder", "Deleted.".to_string()); },
                            title: "Delete smart folder",
                            "{render_icon_view(Icon::X)}"
                        }
                    }
                }
            }
        }}

        // Results.
        div { class: "sf-results mt-6" }
        h2 { class: "text-sm font-semibold text-gray-200 mb-3",
            if let Some(f) = sel.as_ref() {
                let n = results.read().len();
                format!("\"{name}\" — {n} match(es)", name = f.name)
            } else { "Select a smart folder".to_string() }
        }
        {if sel_none {
            rsx! {
                EmptyState {
                    icon: Some(Icon::Search),
                    title: "No folder selected".to_string(),
                    description: Some("Click a smart folder to evaluate its query.".to_string()),
                }
            }
        } else {
            rsx! {
                {match *results_state.read() {
                    LoadState::Loading => rsx! { LoadingBlock { spinner_size: SpinnerSize::Sm } },
                    LoadState::Error => rsx! {
                        ErrorPanel {
                            title: "Smart folder".to_string(),
                            message: "Couldn't evaluate the query.".to_string(),
                        }
                    },
                    LoadState::Loaded => {
                        let res = results.read().clone();
                        if res.is_empty() {
                            rsx! {
                                EmptyState {
                                    icon: Some(Icon::Search),
                                    title: "No matches".to_string(),
                                    description: Some("Try adjusting the query.".to_string()),
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "space-y-1" }
                                for n in &res {
                                    let path = n.path.clone();
                                    let title = n.title.clone();
                                    let folder = n.folder.clone();
                                    let modified = n.modified_at.clone();
                                    let nav_copy = nav;
                                    rsx! {
                                        div {
                                            class: "sf-result flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm",
                                            onclick: move |_: MouseEvent| { open_note(nav_copy, ws, &path); },
                                            div { class: "flex-1 min-w-0" }
                                            div { class: "text-sm text-gray-200 truncate", "{title}" }
                                            if !folder.is_empty() { span { class: "text-xs text-gray-500", "{folder}/" } }
                                            div { class: "ml-auto text-xs text-gray-500", "{fmt_date(&modified)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }}
            }
        }}
    }
}
