//! # Dashboard
//!
//! A configurable home view built from the enabled `dashboard_sections`
//! widget set. Widgets pull from NavContext discovery data — favourites,
//! recent notes, recent searches, pinned notes — plus two IPC calls loaded
//! once and refreshed via the event bus:
//!
//! - `statistics_get` → summary cards + the *Recently Modified* widget.
//! - `inbox_get_queue` → the *Inbox* widget (pending captures).
//!
//! All IPC goes through `crate::ipc::tauri_invoke_safe` so a rejected promise
//! becomes a graceful error state instead of a renderer panic.

use crate::components::contexts::{
    open_tab, record_recent_note, use_nav, use_workspace, NavContext, ViewMode, WorkspaceContext,
};
use crate::components::navigation::state::{dashboard_section_label, NoteIndexEntry};
use crate::components::ui::feedback::{ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::events::{use_event_listener, FrontendEvent, FrontendEventKind};
use crate::ipc;
use chrono::DateTime;
use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;

/// Subset of `VaultStatistics` rendered on the dashboard. All fields are
/// `#[serde(default)]` so a backend shape change degrades gracefully.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Stats {
    note_count: usize,
    folder_count: usize,
    tag_count: usize,
    total_tags: usize,
    graph_nodes: usize,
    graph_edges: usize,
    storage_bytes: u64,
    writing_streak_days: usize,
    active_days_last_30: usize,
    recently_modified: Vec<NoteStat>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct NoteStat {
    path: String,
    title: String,
    folder: String,
    modified_at: String,
    created_at: Option<String>,
    size: usize,
}

/// Minimal projection of an inbox item for the dashboard widget.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct InboxRow {
    id: String,
    title: String,
    object_type: String,
    suggested_folder: Option<String>,
    confidence: Option<f64>,
    status: String,
}

/// Lifecycle of the IPC loads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoadState {
    Loading,
    Error,
    Loaded,
}
impl Default for LoadState {
    fn default() -> Self { Self::Loading }
}

fn fmt_bytes(n: u64) -> String {
    const U: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    if n == 0 { return "0 B".to_string(); }
    let mut b = n as f64;
    let mut i = 0usize;
    while b >= 1024.0 && i < U.len() - 1 { b /= 1024.0; i += 1; }
    if i == 0 { format!("{} {}", n, U[0]) } else { format!("{:.1} {}", b, U[i]) }
}

/// RFC 3339 → "Mon d, YYYY".
fn fmt_date(rfc: &str) -> String {
    DateTime::parse_from_rfc3339(rfc)
        .map(|dt| dt.format("%b %e, %Y").to_string().replace("  ", " "))
        .unwrap_or_else(|_| rfc.chars().take(10).collect())
}

/// Today's date as YYYY-MM-DD (UTC).
fn today_str() -> String {
    let ms = js_sys::Date::now() as i64;
    DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string()
}

/// Opens a note: switch to editor, record the visit, open a tab.
fn open_note(nav: NavContext, ws: WorkspaceContext, path: &str) {
    open_tab(ws, path);
    record_recent_note(nav, path);
    nav.view_mode.set(ViewMode::Editor);
}

/// Resolve a vault-relative path to a title via the notes index.
fn note_title(index: &[crate::components::navigation::state::NoteIndexEntry], path: &str) -> String {
    for n in index.iter() {
        if n.path == path { return n.title.clone(); }
    }
    basename(path)
}

/// Loads statistics + inbox IPC; stores results into the state signals.
fn load_dashboard(
    mut stats: Signal<Option<Stats>>,
    mut stats_state: Signal<LoadState>,
    mut inbox_items: Signal<Vec<InboxRow>>,
    mut inbox_state: Signal<LoadState>,
) {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
    spawn_local(async move {
        stats_state.set(LoadState::Loading);
        match ipc::tauri_invoke_safe("statistics_get", args.clone()).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Stats>(val) {
                Ok(s) => { stats.set(Some(s)); stats_state.set(LoadState::Loaded); }
                Err(e) => { stats.set(None); stats_state.set(LoadState::Error); tracing::warn!("dashboard stats: {e}"); }
            },
            Ok(None) => { stats.set(None); stats_state.set(LoadState::Error); }
            Err(e) => { stats.set(None); stats_state.set(LoadState::Error); tracing::warn!("dashboard stats_get: {}", e.message()); }
        }

        inbox_state.set(LoadState::Loading);
        match ipc::tauri_invoke_safe("inbox_get_queue", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<InboxRow>>(val) {
                Ok(items) => { inbox_items.set(items); inbox_state.set(LoadState::Loaded); }
                Err(e) => { inbox_items.set(Vec::new()); inbox_state.set(LoadState::Error); tracing::warn!("dashboard inbox: {e}"); }
            },
            Ok(None) => { inbox_items.set(Vec::new()); inbox_state.set(LoadState::Error); }
            Err(e) => { inbox_items.set(Vec::new()); inbox_state.set(LoadState::Error); tracing::warn!("dashboard inbox_get: {}", e.message()); }
        }
    });
}

#[component]
pub fn Dashboard() -> Element {
    let nav = use_nav();
    let ws = use_workspace();

    let stats = use_signal(|| None::<Stats>);
    let stats_state = use_signal(LoadState::default);
    let inbox_items = use_signal(Vec::<InboxRow>::new);
    let inbox_state = use_signal(LoadState::default);

    // Initial load (runs once).
    {
        let s = stats; let ss = stats_state; let i = inbox_items; let ist = inbox_state;
        load_dashboard(s, ss, i, ist);
    }

    // Refresh on store events.
    use_event_listener(FrontendEventKind::ItemStored, move |_ev: &FrontendEvent| {
        let s = stats; let ss = stats_state; let i = inbox_items; let ist = inbox_state;
        load_dashboard(s, ss, i, ist);
    });

    let vault_name = nav.vault_name.read().clone();
    let sections = nav.dashboard_sections.read().clone();
    let index = nav.notes_index.read().clone();
    let stats_state_val = *stats_state.read();
    let stats_snapshot = stats.read().clone();
    let inbox_state_val = *inbox_state.read();
    let inbox_snapshot = inbox_items.read().clone();

    rsx! {
        div { class: "dashboard h-full overflow-y-auto" }
        div { class: "dashboard-header border-b border-gray-700 px-6 py-4" }
        h1 { class: "text-2xl font-bold text-gray-100", "Dashboard" }
        p { class: "text-sm text-gray-400 mt-1", "Vault • {vault_name}" }
        div { class: "dashboard-content p-6 space-y-6" }

        {render_summary(stats_state_val, stats_snapshot.as_ref())}

        for section in &sections {
            {render_section(section, nav, ws, &index, stats_state_val, stats_snapshot.as_ref(), inbox_state_val, &inbox_snapshot)}
        }

        if stats_state_val == LoadState::Loading && stats_snapshot.is_none() && sections.is_empty() {
            LoadingBlock { size: SpinnerSize::Md }
        }
    }
}

fn render_summary(state: LoadState, stats: Option<&Stats>) -> Element {
    match state {
        LoadState::Loading => rsx! { LoadingBlock { size: SpinnerSize::Sm } },
        LoadState::Error => rsx! {
            ErrorPanel {
                title: "Statistics".to_string(),
                message: "Couldn't load vault statistics.".to_string(),
            }
        },
        LoadState::Loaded => rsx! {
            div { class: "grid grid-cols-2 md:grid-cols-4 gap-4" }
            {stats.map(|s| rsx! {
                StatCard { icon: Icon::FileText, label: "Notes".to_string(), value: s.note_count.to_string() }
                StatCard { icon: Icon::Folder, label: "Folders".to_string(), value: s.folder_count.to_string() }
                StatCard { icon: Icon::Tag, label: "Tags".to_string(), value: s.total_tags.to_string() }
                StatCard { icon: Icon::Database, label: "Graph".to_string(), value: format!("{} → {}", s.graph_nodes, s.graph_edges) }
                StatCard { icon: Icon::HardDrive, label: "Storage".to_string(), value: fmt_bytes(s.storage_bytes) }
                StatCard { icon: Icon::Activity, label: "Streak".to_string(), value: format!("{} days", s.writing_streak_days) }
            }).unwrap_or(rsx! {})}
        },
    }
}

#[component]
fn StatCard(icon: Icon, label: String, value: String) -> Element {
    rsx! {
        div { class: "stat-card flex items-center gap-3 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700" }
        {render_icon_view(icon)}
        div { class: "flex-1 min-w-0" }
        div { class: "text-xs text-gray-400", "{label}" }
        div { class: "text-lg font-semibold text-gray-100 truncate", "{value}" }
    }
}

fn render_section(
    section: &str,
    nav: NavContext,
    ws: WorkspaceContext,
    index: &[crate::components::navigation::state::NoteIndexEntry],
    stats_state: LoadState,
    stats: Option<&Stats>,
    inbox_state: LoadState,
    inbox_items: &[InboxRow],
) -> Element {
    let label = dashboard_section_label(section);
    let widget = match section {
        "quick_actions" => render_quick_actions(nav, ws),
        "recently_modified" => render_recent_modified(stats_state, stats, index, nav, ws),
        "favourites" => render_path_list(&nav.favourites.read().clone(), index, nav, ws),
        "recently_opened" => render_path_list(&nav.recent_notes.read().clone(), index, nav, ws),
        "pinned" => render_pinned(index, nav, ws),
        "inbox" => render_inbox(inbox_state, inbox_items),
        "recent_searches" => render_searches(&nav.recent_searches.read().clone(), nav, ws),
        "summary" => rsx! {},
        _ => rsx! {
            div { class: "text-xs text-gray-500", "Unknown section: {section}" }
        },
    };

    rsx! {
        section { class: "dashboard-section" }
        div { class: "section-header flex items-center gap-2 mb-3" }
        {render_icon_view(Icon::Info)}
        h2 { class: "text-sm font-semibold text-gray-200", "{label}" }
        {widget}
    }
}

fn render_quick_actions(nav: NavContext, ws: WorkspaceContext) -> Element {
    let vm_search = nav.view_mode;
    let vm_inbox = nav.view_mode;
    let vm_stats = nav.view_mode;
    rsx! {
        div { class: "grid grid-cols-2 md:grid-cols-4 gap-3" }
        button {
            class: "quick-action flex flex-col items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700 hover:bg-gray-700/50 transition-colors text-center",
            onclick: move |_: MouseEvent| { open_today_note(nav, ws); },
            {render_icon_view(Icon::Clock)}
            span { class: "text-xs text-gray-300", "Today's Note" }
        }
        button {
            class: "quick-action flex flex-col items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700 hover:bg-gray-700/50 transition-colors text-center",
            onclick: move |_: MouseEvent| { open_new_note(nav, ws); },
            {render_icon_view(Icon::FilePlus)}
            span { class: "text-xs text-gray-300", "New Note" }
        }
        button {
            class: "quick-action flex flex-col items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700 hover:bg-gray-700/50 transition-colors text-center",
            onclick: move |_: MouseEvent| { vm_search.set(ViewMode::Search); },
            {render_icon_view(Icon::Search)}
            span { class: "text-xs text-gray-300", "Search" }
        }
        button {
            class: "quick-action flex flex-col items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700 hover:bg-gray-700/50 transition-colors text-center",
            onclick: move |_: MouseEvent| { vm_inbox.set(ViewMode::Inbox); },
            {render_icon_view(Icon::Inbox)}
            span { class: "text-xs text-gray-300", "Inbox" }
        }
        button {
            class: "quick-action flex flex-col items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700 hover:bg-gray-700/50 transition-colors text-center",
            onclick: move |_: MouseEvent| { vm_stats.set(ViewMode::Statistics); },
            {render_icon_view(Icon::Database)}
            span { class: "text-xs text-gray-300", "Statistics" }
        }
    }
}

fn open_today_note(nav: NavContext, ws: WorkspaceContext) {
    let date = today_str();
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "date": date })).unwrap();
    spawn_local(async move {
        match ipc::tauri_invoke_safe("daily_note_for", args).await {
            Ok(Some(val)) => {
                if let Some(path) = val.as_string() { open_note(nav, ws, &path); }
            }
            Ok(None) => tracing::warn!("dashboard: daily_note_for returned no path"),
            Err(e) => tracing::warn!("dashboard: daily_note_for error: {}", e.message()),
        }
    });
}

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
            Err(e) => tracing::warn!("dashboard: note_create_file error: {}", e.message()),
        }
    });
}

fn render_recent_modified(
    state: LoadState,
    stats: Option<&Stats>,
    index: &[crate::components::navigation::state::NoteIndexEntry],
    nav: NavContext,
    ws: WorkspaceContext,
) -> Element {
    match state {
        LoadState::Loading => rsx! { LoadingBlock { size: SpinnerSize::Sm } },
        LoadState::Error => rsx! {
            ErrorPanel {
                title: "Recently Modified".to_string(),
                message: "Couldn't load statistics.".to_string(),
            }
        },
        LoadState::Loaded => {
            let notes = stats
                .map(|s| s.recently_modified.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            render_note_list(&notes, nav, ws)
        }
    }
}

fn render_note_list(
    notes: &[NoteStat],
    nav: NavContext,
    ws: WorkspaceContext,
) -> Element {
    if notes.is_empty() {
        return rsx! {
            EmptyState {
                icon: Some(Icon::FileText),
                title: "No recent notes".to_string(),
                description: Some("No activity yet.".to_string()),
            }
        };
    }
    rsx! {
        div { class: "space-y-1" }
        for n in notes {
            {render_note_row(n, nav, ws)}
        }
    }
}

fn render_note_row(n: &NoteStat, nav: NavContext, ws: WorkspaceContext) -> Element {
    let path = n.path.clone();
    let title = n.title.clone();
    let folder = n.folder.clone();
    let modified = n.modified_at.clone();
    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm",
            onclick: move |_: MouseEvent| { open_note(nav, ws, &path); },
            div { class: "flex-1 min-w-0" }
            div { class: "text-sm text-gray-200 truncate", "{title}" }
            if !folder.is_empty() { span { class: "text-xs text-gray-500", "{folder}/" } }
            div { class: "ml-auto text-xs text-gray-500", "{fmt_date(&modified)}" }
        }
    }
}

fn render_path_list(
    paths: &[String],
    index: &[crate::components::navigation::state::NoteIndexEntry],
    nav: NavContext,
    ws: WorkspaceContext,
) -> Element {
    if paths.is_empty() {
        return rsx! {
            EmptyState {
                icon: Some(Icon::FileText),
                title: "Empty".to_string(),
                description: Some("No items yet.".to_string()),
            }
        };
    }
    rsx! {
        div { class: "space-y-1" }
        for p in paths {
            {render_path_row(p, index, nav, ws)}
        }
    }
}

fn render_path_row(p: &str, index: &[crate::components::navigation::state::NoteIndexEntry], nav: NavContext, ws: WorkspaceContext) -> Element {
    let path = p.to_string();
    let title = note_title(index, p);
    let folder = index.iter().find(|n| n.path == p).map(|n| n.folder.clone()).unwrap_or_default();
    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm",
            onclick: move |_: MouseEvent| { open_note(nav, ws, &path); },
            div { class: "flex-1 min-w-0" }
            div { class: "text-sm text-gray-200 truncate", "{title}" }
            if !folder.is_empty() { span { class: "text-xs text-gray-500", "{folder}/" } }
        }
    }
}

fn render_pinned(
    index: &[crate::components::navigation::state::NoteIndexEntry],
    nav: NavContext,
    ws: WorkspaceContext,
) -> Element {
    let pinned: Vec<NoteIndexEntry> = index.iter().filter(|n| n.pinned).cloned().collect();
    if pinned.is_empty() {
        return rsx! {
            EmptyState {
                icon: Some(Icon::BookMarked),
                title: "No pinned notes".to_string(),
                description: Some("Pin a note to see it here.".to_string()),
            }
        };
    }
    rsx! {
        div { class: "flex flex-wrap gap-2" }
        for n in &pinned {
            {render_pinned_tag(n, nav, ws)}
        }
    }
}

fn render_pinned_tag(n: &crate::components::navigation::state::NoteIndexEntry, nav: NavContext, ws: WorkspaceContext) -> Element {
    let path = n.path.clone();
    let title = n.title.clone();
    rsx! {
        span {
            class: "inline-flex items-center gap-1 rounded bg-gray-800/50 px-2 py-1 text-xs text-gray-200 border border-gray-700 cursor-pointer",
            onclick: move |_: MouseEvent| { open_note(nav, ws, &path); },
            {render_icon_view(Icon::BookMarked)}
            "{title}"
        }
    }
}

fn render_inbox(
    state: LoadState,
    items: &[InboxRow],
) -> Element {
    match state {
        LoadState::Loading => rsx! { LoadingBlock { size: SpinnerSize::Sm } },
        LoadState::Error => rsx! {
            ErrorPanel {
                title: "Inbox".to_string(),
                message: "Couldn't load pending captures.".to_string(),
            }
        },
        LoadState::Loaded => {
            if items.is_empty() {
                rsx! {
                    EmptyState {
                        icon: Some(Icon::Inbox),
                        title: "Inbox is clean".to_string(),
                        description: Some("No pending captures.".to_string()),
                    }
                }
            } else {
                rsx! {
                    div { class: "space-y-1" }
                    for item in items {
                        {render_inbox_row(item)}
                    }
                }
            }
        }
    }
}

fn render_inbox_row(item: &InboxRow) -> Element {
    let title = item.title.clone();
    let object_type = item.object_type.clone();
    let suggested = item.suggested_folder.clone();
    rsx! {
        div { class: "flex items-center gap-3 px-3 py-2 rounded-lg bg-gray-800/30 border border-gray-700 text-sm" }
        {render_icon_view(Icon::FileText)}
        div { class: "flex-1 min-w-0" }
        div { class: "text-sm text-gray-200 truncate", "{title}" }
        span { class: "text-xs text-gray-500", "{object_type}" }
        if let Some(folder) = &suggested {
            span { class: "text-xs text-blue-400", "→ {folder}" }
        }
    }
}

fn render_searches(searches: &[String], nav: NavContext, _ws: WorkspaceContext) -> Element {
    if searches.is_empty() {
        return rsx! {
            EmptyState {
                icon: Some(Icon::Search),
                title: "No recent searches".to_string(),
                description: Some("Searches you run will appear here.".to_string()),
            }
        };
    }
    let vm = nav.view_mode;
    let sq = nav.search_query;
    rsx! {
        div { class: "flex flex-wrap gap-2" }
        for q in searches {
            {render_search_chip(q, vm, sq)}
        }
    }
}

fn render_search_chip(q: &str, vm: Signal<ViewMode>, sq: Signal<String>) -> Element {
    let query = q.to_string();
    rsx! {
        button {
            class: "search-chip inline-flex items-center gap-1 rounded bg-gray-800/50 px-2 py-1 text-xs text-gray-300 border border-gray-700 hover:bg-gray-700/50",
            onclick: move |_: MouseEvent| { sq.set(query.clone()); vm.set(ViewMode::Search); },
            {render_icon_view(Icon::Search)}
            "{query}"
        }
    }
}
