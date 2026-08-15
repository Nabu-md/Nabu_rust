//! # Home screen
//!
//! Shown when no note is open in the editor. It is the vault's front door — a
//! welcome banner, a row of quick actions (today's note, new note, search,
//! statistics, inbox), the *Recently Modified* list (sourced from the NavContext
//! note index, no extra IPC required) and the user's *Favourites*.
//!
//! Opening a note here switches the view to the editor and records the visit
//! in the recent-notes history, mirroring the Quick Switcher behaviour.

use crate::components::contexts::{open_tab, record_recent_note, toggle_favourite, use_nav, use_workspace, NavContext, ViewMode, WorkspaceContext};
use crate::components::navigation::state::{load_notes_index, NoteIndexEntry};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::events::{use_event_listener, FrontendEvent, FrontendEventKind};
use crate::ipc;
use chrono::DateTime;
use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;

/// RFC 3339 → "Mon d, YYYY".
fn fmt_date(rfc: &str) -> String {
    DateTime::parse_from_rfc3339(rfc)
        .map(|dt| dt.format("%b %e, %Y").to_string().replace("  ", " "))
        .unwrap_or_else(|_| rfc.chars().take(10).collect())
}

fn today_str() -> String {
    let ms = js_sys::Date::now() as i64;
    DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string()
}

fn note_title(index: &[NoteIndexEntry], path: &str) -> String {
    for n in index.iter() {
        if n.path == path {
            return n.title.clone();
        }
    }
    basename(path)
}

/// Opens a note from the home screen.
fn open_note(nav: NavContext, ws: WorkspaceContext, path: &str) {
    open_tab(ws, path);
    record_recent_note(nav, path);
    nav.view_mode.set(ViewMode::Editor);
}

/// Creates today's daily note (or opens the existing one).
fn open_today(nav: NavContext, ws: WorkspaceContext) {
    let date = today_str();
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "date": date })).unwrap();
    spawn_local(async move {
        match ipc::tauri_invoke_safe("daily_note_for", args).await {
            Ok(Some(val)) => {
                if let Some(path) = val.as_string() {
                    open_note(nav, ws, &path);
                }
            }
            Ok(None) => tracing::warn!("home: daily_note_for returned no path"),
            Err(e) => tracing::warn!("home: daily_note_for error: {}", e.message()),
        }
    });
}

/// Creates a new untitled note and opens it.
fn open_new_note(nav: NavContext, ws: WorkspaceContext) {
    let path = "untitled.md".to_string();
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
        "path": path,
        "content": Some("# Untitled"),
    }))
    .unwrap();
    spawn_local(async move {
        match ipc::tauri_invoke_safe("note_create_file", args).await {
            Ok(_) => open_note(nav, ws, &path),
            Err(e) => tracing::warn!("home: note_create_file error: {}", e.message()),
        }
    });
}

/// Top-N notes by modification time from the vault index.
fn recent_notes(index: &[NoteIndexEntry], limit: usize) -> Vec<NoteIndexEntry> {
    let mut v: Vec<NoteIndexEntry> = index.to_vec();
    v.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    v.truncate(limit);
    v
}

#[component]
pub fn HomeScreen() -> Element {
    let nav = use_nav();
    let ws = use_workspace();

    // Refresh the index-backed list when notes change elsewhere.
    use_event_listener(FrontendEventKind::ItemStored, move |_ev: &FrontendEvent| {
        load_notes_index(nav);
    });

    let vault_name = nav.vault_name.read().clone();
    let index = nav.notes_index.read().clone();
    let favourites = nav.favourites.read().clone();
    let recent = recent_notes(&index, 10);
    let fav_rows: Vec<NoteIndexEntry> = favourites
        .iter()
        .filter_map(|p| index.iter().find(|n| n.path == *p).cloned())
        .collect();

    let vm_search = nav.view_mode;
    let vm_inbox = nav.view_mode;
    let vm_stats = nav.view_mode;

    rsx! {
        div { class: "home-screen h-full overflow-y-auto" }

        // Hero banner.
        div { class: "home-hero border-b border-gray-700 px-6 py-8" }
        h1 { class: "home-title text-3xl font-bold text-gray-100", "Welcome to {vault_name}" }
        p { class: "home-subtitle text-sm text-gray-400 mt-2 max-w-lg",
            "Your knowledge base is ready. Start a note, search everything, or open today's entry."
        }

        div { class: "home-actions px-6 py-4" }
        div { class: "flex flex-wrap gap-3" }
        HomeActionButton {
            icon: Icon::Clock, label: "Today's Note".to_string(),
            on_click: move |_: MouseEvent| { open_today(nav, ws); },
        }
        HomeActionButton {
            icon: Icon::FilePlus, label: "New Note".to_string(),
            on_click: move |_: MouseEvent| { open_new_note(nav, ws); },
        }
        HomeActionButton {
            icon: Icon::Search, label: "Search".to_string(),
            on_click: move |_: MouseEvent| { vm_search.set(ViewMode::Search); },
        }
        HomeActionButton {
            icon: Icon::Inbox, label: "Inbox".to_string(),
            on_click: move |_: MouseEvent| { vm_inbox.set(ViewMode::Inbox); },
        }
        HomeActionButton {
            icon: Icon::Database, label: "Statistics".to_string(),
            on_click: move |_: MouseEvent| { vm_stats.set(ViewMode::Statistics); },
        }

        div { class: "home-content px-6 py-6 space-y-8" }

        // Recently modified.
        section { class: "home-section" }
        h2 { class: "text-sm font-semibold text-gray-200 mb-3", "Recently Modified" }
        {render_note_list(&recent, &index, nav, &ws)}

        // Favourites.
        section { class: "home-section mt-8" }
        h2 { class: "text-sm font-semibold text-gray-200 mb-3", "Favourites" }
        {render_favourites(&fav_rows, nav, &ws)}
    }
}

#[component]
fn HomeActionButton(icon: Icon, label: String, on_click: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "home-action inline-flex items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-2 border border-gray-700 hover:bg-gray-700/50 text-sm text-gray-200",
            on_click,
            {render_icon_view(icon)}
            "{label}"
        }
    }
}

fn render_note_list(
    notes: &[NoteIndexEntry],
    index: &[NoteIndexEntry],
    nav: NavContext,
    ws: &WorkspaceContext,
) -> Element {
    if notes.is_empty() {
        return rsx! {
            EmptyState {
                icon: Some(Icon::FileText),
                title: "No notes yet".to_string(),
                description: Some("Create a note to get started.".to_string()),
            }
        };
    }
    rsx! {
        div { class: "space-y-1" }
        for n in notes {
            let path = n.path.clone();
            let title = n.title.clone();
            let folder = n.folder.clone();
            let modified = n.modified_at.clone();
            let ws = *ws; let nav_copy = nav;
            rsx! {
                div {
                    class: "flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm",
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

fn render_favourites(
    favs: &[NoteIndexEntry],
    nav: NavContext,
    ws: &WorkspaceContext,
) -> Element {
    if favs.is_empty() {
        return rsx! {
            EmptyState {
                icon: Some(Icon::BookMarked),
                title: "No favourites".to_string(),
                description: Some("Star a note to pin it here.".to_string()),
            }
        };
    }
    rsx! {
        div { class: "space-y-1" }
        for n in favs {
            let path = n.path.clone();
            let title = n.title.clone();
            let folder = n.folder.clone();
            let ws = *ws; let nav_copy = nav;
            let toggle = nav;
            rsx! {
                div {
                    class: "flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm group",
                    onclick: move |_: MouseEvent| { open_note(nav_copy, ws, &path); },
                    div { class: "flex-1 min-w-0" }
                    div { class: "text-sm text-gray-200 truncate", "{title}" }
                    if !folder.is_empty() { span { class: "text-xs text-gray-500", "{folder}/" } }
                    button {
                        class: "ml-auto opacity-0 group-hover:opacity-100 text-gray-400 hover:text-yellow-400",
                        onclick: move |_: MouseEvent| { toggle_favourite(toggle, &path); },
                        "★"
                    }
                }
            }
        }
    }
}
