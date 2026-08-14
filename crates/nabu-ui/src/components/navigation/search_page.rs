//! # Search page (Dioxus)
//!
//! Full-text search backed by the backend `notes_search` command. Shows
//! explicit loading, empty, and error states so the user always knows whether
//! results are in-flight, absent, or failed.
//!
//! Uses `crate::ipc::tauri_invoke_safe` so a rejected promise (command not
//! registered, backend error, vault not configured) becomes a graceful error
//! state instead of a renderer panic.

use crate::components::contexts::{record_recent_search, use_nav};
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

/// The search page component.
#[component]
pub fn SearchPage() -> Element {
    let nav = use_nav();
    let toasts = use_toast();

    let query = use_signal(|| nav.search_query.read().clone());
    let hits = use_signal(Vec::<SearchHit>::new);
    let state = use_signal(|| SearchState::Idle);
    let error_msg = use_signal(String::new);

    // ── Search worker ───────────────────────────────────────────────────
    let do_search = {
        let query = query.clone();
        let hits = hits.clone();
        let state = state.clone();
        let error_msg = error_msg.clone();
        move |q: String| {
            let q_trimmed = q.trim().to_string();
            if q_trimmed.is_empty() {
                hits.set(Vec::new());
                state.set(SearchState::Loaded);
                error_msg.set(String::new());
                return;
            }

            state.set(SearchState::Loading);
            error_msg.set(String::new());

            let hits_c = hits.clone();
            let state_c = state.clone();
            let error_c = error_msg.clone();
            let toasts_c = toasts;

            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                    "query": q_trimmed,
                }))
                .unwrap();

                match crate::ipc::tauri_invoke_safe("notes_search", args).await {
                    None => {
                        // IPC rejected — command not registered, backend error, etc.
                        let msg = "Search request failed — the backend may be unavailable.".to_string();
                        error_c.set(msg.clone());
                        state_c.set(SearchState::Failed);
                        toasts_c.error("Search failed", "Could not complete the search request.");
                    }
                    Some(val) => {
                        match serde_wasm_bindgen::from_value::<Vec<SearchHit>>(val) {
                            Ok(results) => {
                                hits_c.set(results);
                                state_c.set(SearchState::Loaded);
                                error_c.set(String::new());
                                // Record the query in recent searches.
                                record_recent_search(nav, &q_trimmed);
                            }
                            Err(e) => {
                                let msg = format!("Search results could not be parsed: {e}");
                                error_c.set(msg);
                                state_c.set(SearchState::Failed);
                                toasts_c.error("Search failed", "The search results were invalid.");
                            }
                        }
                    }
                }
            });
        }
    };

    // ── React to external query changes (from command palette / nav) ──
    let external_query = nav.search_query.read().clone();
    {
        let mut checked = use_signal(|| false);
        if !*checked.read() && !external_query.is_empty() {
            checked.set(true);
            let do_search = do_search.clone();
            let q = external_query.clone();
            do_search(q);
        }
    }

    // ── Input handler: run search on Enter ──
    let on_input_change = move |ev: FormEvent| {
        query.set(ev.value());
    };

    let on_key_down = move |ev: KeyboardEvent| {
        let ev_web = ev.data().as_web_event();
        let web = ev_web.unchecked_ref::<web_sys::KeyboardEvent>();
        if web.key() == "Enter" {
            let q = query.read().clone();
            do_search(q);
        }
    };

    let on_retry = move |_: MouseEvent| {
        let q = query.read().clone();
        do_search(q);
    };

    // ── Rendering ───────────────────────────────────────────────────────
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
            class: "w-full bg-gray-800 text-gray-100 rounded-lg px-4 py-2.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none",
            value: "{current_query}",
            oninput: on_input_change,
            onkeydown: on_key_down,
            autocomplete: "off",
            spellcheck: "false",
        }
        {render_icon_view(Icon::Search)}

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
                if current_hits.is_empty() && current_query.trim().is_empty() {
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
                                let ms = hit.match_start;
                                let me = hit.match_end;
                                let modified = hit.modified_at.clone();
                                rsx! {
                                    div {
                                        class: "group flex items-start gap-3 px-3 py-3 rounded-lg hover:bg-gray-800/50 transition-colors border border-transparent hover:border-gray-700 cursor-pointer",
                                        onclick: move |_: MouseEvent| {
                                            crate::components::contexts::open_tab(nav, &path);
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
                                    div { class: "text-xs text-gray-600" }
                                    {render_icon_view(Icon::ArrowUpRight)}
                                }
                            }
                        }
                    }
                }
            }
        }}
    }
}
