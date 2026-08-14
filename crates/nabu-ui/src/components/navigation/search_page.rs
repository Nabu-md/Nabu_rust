//! # Search page (Dioxus)
//!
//! Full-text search backed by the backend `notes_search` command. Shows
//! explicit loading, empty, and error states so the user always knows whether
//! results are in-flight, absent, or failed.
//!
//! Uses `crate::ipc::tauri_invoke_safe` so a rejected promise (command not
//! registered, backend error, vault not configured) becomes a graceful error
//! state instead of a renderer panic.

use crate::components::contexts::{open_tab, record_recent_search, use_nav, use_workspace, NavContext};
use crate::components::ui::feedback::{use_toast, ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

/// Mirrors the backend `SearchHit` (defined in `commands.rs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub folder: String,
    pub snippet: String,
    /// Character offset of the first match within `snippet`.
    pub match_start: usize,
    /// Character offset one past the end of the first match in `snippet`.
    pub match_end: usize,
    pub modified_at: String,
}

/// Search result lifecycle states.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SearchState {
    /// Never attempted / initial.
    Idle,
    /// `notes_search` IPC is in flight.
    Loading,
    /// IPC succeeded — results may be empty.
    Loaded,
    /// IPC failed or deserialization errored.
    Failed,
}

/// Runs a search IPC call and updates the state signals.
fn run_search(
    query: String,
    hits: Signal<Vec<SearchHit>>,
    state: Signal<SearchState>,
    error_msg: Signal<String>,
    nav: NavContext,
    toasts: crate::components::ui::feedback::ToastContext,
) {
    let mut hits = hits;
    let mut state = state;
    let mut error_msg = error_msg;
    let q = query.trim().to_string();
    if q.is_empty() {
        hits.set(Vec::new());
        state.set(SearchState::Loaded);
        error_msg.set(String::new());
        return;
    }

    state.set(SearchState::Loading);
    error_msg.set(String::new());

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({
            "query": q,
        }))
        .unwrap();

        match crate::ipc::tauri_invoke_safe("notes_search", args).await {
            Ok(Some(val)) => {
                match serde_wasm_bindgen::from_value::<Vec<SearchHit>>(val) {
                    Ok(results) => {
                        hits.set(results);
                        state.set(SearchState::Loaded);
                        error_msg.set(String::new());
                        // Record the query in recent searches.
                        record_recent_search(nav, &q);
                    }
                    Err(e) => {
                        let msg = format!("Search results could not be parsed: {e}");
                        error_msg.set(msg);
                        state.set(SearchState::Failed);
                        toasts.error("Search failed", "The search results were invalid.");
                    }
                }
            }
            Ok(None) => {
                // Command resolved but returned no value.
                let msg = "Search returned no data from the backend.".to_string();
                error_msg.set(msg);
                state.set(SearchState::Failed);
                toasts.error("Search failed", "The search returned an unexpected response.");
            }
            Err(e) => {
                // IPC rejected — command not registered, backend error, etc.
                let msg = e.message();
                error_msg.set(msg);
                state.set(SearchState::Failed);
                toasts.error("Search failed", "Could not complete the search request.");
            }
        }
    });
}

/// The search page component.
#[component]
pub fn SearchPage() -> Element {
    let nav = use_nav();
    let ws = use_workspace();
    let toasts = use_toast();

    let mut query = use_signal(|| nav.search_query.read().clone());
    let mut hits = use_signal(Vec::<SearchHit>::new);
    let mut state = use_signal(|| SearchState::Idle);
    let mut error_msg = use_signal(String::new);

    // One-time initial load from the external query (if any).
    let mut initialized = use_signal(|| false);
    if !*initialized.read() {
        initialized.set(true);
        let initial = query.read().clone();
        if !initial.trim().is_empty() {
            run_search(initial, hits, state, error_msg, nav, toasts);
        }
    }

    // ── Input handler ──
    let on_input_change = move |ev: FormEvent| {
        query.set(ev.value());
    };

    let on_key_down = {
        let hits = hits.clone();
        let state = state.clone();
        let error_msg = error_msg.clone();
        move |ev: KeyboardEvent| {
            if ev.key() == Key::Enter {
                ev.prevent_default();
                let q = query.read().clone();
                run_search(q, hits, state, error_msg, nav, toasts);
            }
        }
    };

    let on_retry = {
        let hits = hits.clone();
        let state = state.clone();
        let error_msg = error_msg.clone();
        move |_| {
            let q = query.read().clone();
            run_search(q, hits, state, error_msg, nav, toasts);
        }
    };

    // ── Rendering ──
    let current_state = *state.read();
    let current_query = query.read().clone();
    let current_hits = hits.read().clone();
    let current_error = error_msg.read().clone();

    rsx! {
        div { class: "search-page h-full flex flex-col bg-gray-950 text-gray-100" }

        // ── Search input ──
        div { class: "p-4 border-b border-gray-800" }
        div { class: "relative max-w-2xl mx-auto" }
        input {
            r#type: "text",
            placeholder: "Search your vault… (press Enter)",
            class: "w-full bg-gray-800 text-gray-100 rounded-lg px-4 py-2.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none pr-10",
            value: "{current_query}",
            oninput: on_input_change,
            onkeydown: on_key_down,
            autocomplete: "off",
            spellcheck: "false",
        }
        span {
            class: "absolute right-3 top-1/2 -translate-y-1/2 text-gray-500",
            "aria-hidden": "true",
            {render_icon_view(Icon::Search)}
        }

        // ── Results area ──
        div { class: "flex-1 overflow-y-auto p-4" }
        div { class: "max-w-2xl mx-auto" }

        {match current_state {
            SearchState::Idle => rsx! {
                EmptyState {
                    icon: Some(Icon::Search),
                    title: "Search your vault".to_string(),
                    description: "Type a query and press Enter to find notes by title and content.".to_string(),
                }
            },
            SearchState::Loading => rsx! {
                LoadingBlock {
                    label: "Searching…",
                    size: SpinnerSize::Lg,
                }
            },
            SearchState::Failed => rsx! {
                ErrorPanel {
                    title: "Search failed".to_string(),
                    message: "Could not search the vault.".to_string(),
                    details: current_error,
                    on_retry: on_retry,
                    recovery: "Make sure your vault is accessible and the backend is running.".to_string(),
                }
            },
            SearchState::Loaded => {
                if current_query.trim().is_empty() {
                    rsx! {
                        EmptyState {
                            icon: Some(Icon::Search),
                            title: "Search your vault".to_string(),
                            description: "Type a query and press Enter to find notes by title and content.".to_string(),
                        }
                    }
                } else if current_hits.is_empty() {
                    rsx! {
                        EmptyState {
                            icon: Some(Icon::SearchAlt),
                            title: "No matches found".to_string(),
                            description: "Try a different search term or check your spelling.".to_string(),
                        }
                    }
                } else {
                    rsx! {
                        div { class: "space-y-1" }
                        for hit in &current_hits {
                            {
                                let path = hit.path.clone();
                                let title = hit.title.clone();
                                let folder = hit.folder.clone();
                                let snippet = hit.snippet.clone();
                                let modified = hit.modified_at.clone();
                                rsx! {
                                    div {
                                        class: "group flex items-start gap-3 px-3 py-3 rounded-lg hover:bg-gray-800/50 transition-colors border border-transparent hover:border-gray-700 cursor-pointer",
                                        onclick: move |_: MouseEvent| {
                                            open_tab(ws, &path);
                                        },
                                    }
                                    div { class: "flex-1 min-w-0" }
                                    div { class: "text-sm font-medium text-gray-200 truncate", "{title}" }
                                    {if !folder.is_empty() {
                                        rsx! { span { class: "text-xs text-gray-500", "{folder}/" } }
                                    } else { rsx!{} }}
                                    div { class: "mt-1 text-xs text-gray-400 line-clamp-2", "{snippet}" }
                                    {if !modified.is_empty() {
                                        rsx! {
                                            div { class: "mt-1 text-[10px] text-gray-600", "{modified}" }
                                        }
                                    } else { rsx!{} }}
                                    div { class: "text-xs text-gray-600 mt-1 flex items-center gap-1" }
                                    {render_icon_view(Icon::ExternalLink)}
                                }
                            }
                        }
                    }
                }
            }
        }}
    }
}
