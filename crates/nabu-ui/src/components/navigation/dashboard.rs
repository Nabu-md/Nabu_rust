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

use crate::components::contexts::{open_tab, record_recent_note, use_nav, use_workspace, NavContext, ViewMode, WorkspaceContext};
use crate::components::navigation::state::dashboard_section_label;
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
    graph_orphans: usize,
    graph_clusters: usize,
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
enum LoadState { Loading, Error, Loaded }
impl Default for LoadState { fn default() -> Self { Self::Loading } }

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

/// Today's date as YYYY-MM-DD (UTC) for the daily-note command.
fn today_str() -> String {
    let ms = js_sys::Date::now() as i64;
    DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string()
}

/// Opens a note: switch to editor, record recent, open tab.
fn open_note(nav: NavContext, ws: WorkspaceContext, path: &str) {
    open_tab(ws, path);
    record_recent_note(nav, path);
    nav.view_mode.set(ViewMode::Editor);
}

/// Resolves a vault-relative path to a title via the notes index.
fn note_title(index: &[crate::components::navigation::state::NoteIndexEntry], path: &str) -> String {
    for n in index.iter() {
        if n.path == path { return n.title.clone(); }
    }
    basename(path)
}

/// Loads statistics + inbox IPC; stores results into the state signals.
fn load_dashboard(
    stats: Signal<Option<Stats>>,
    stats_state: Signal<LoadState>,
    inbox_items: Signal<Vec<InboxRow>>,
    inbox_state: Signal<LoadState>,
) {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
    spawn_local(async move {
        // ── statistics ──
        stats_state.set(LoadState::Loading);
        match ipc::tauri_invoke_safe("statistics_get", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Stats>(val) {
                Ok(s) => { stats.set(Some(s)); stats_state.set(LoadState::Loaded); }
                Err(e) => { stats.set(None); stats_state.set(LoadState::Error); tracing::warn!("dashboard stats: {e}"); }
            },
            Ok(None) => { stats.set(None); stats_state.set(LoadState::Error); }
            Err(e) => { stats.set(None); stats_state.set(LoadState::Error); tracing::warn!("dashboard stats_get: {}", e.message()); }
        }

        // ── inbox ──
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

    // Initial load.
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

    rsx! {
        div { class: "dashboard h-full overflow-y-auto" }
        div { class: "dashboard-header border-b border-gray-700 px-6 py-4" }
        h1 { class: "text-2xl font-bold text-gray-100", "Dashboard" }
        p { class: "text-sm text-gray-400 mt-1", "Vault • {vault_name}" }
        div { class: "dashboard-content p-6 space-y-6" }

        {render_summary(*stats_state.read(), stats.read().as_ref())}

        for section in &sections {
            {render_section(section, nav, &ws, &index, *stats_state.read(), stats.read().as_ref(), *inbox_state.read(), &inbox_items.read())}
        }

        if *stats_state.read() == LoadState::Loading && stats.read().is_none() && sections.is_empty() {
            LoadingBlock { spinner_size: SpinnerSize::Md }
        }
    }
}

fn render_summary(state: LoadState, stats: Option<&Stats>) -> Element {
    match state {
        LoadState::Loading => rsx! { LoadingBlock { spinner_size: SpinnerSize::Sm } },
        LoadState::Error => rsx! {
            ErrorPanel {
                title: "Statistics".to_string(),
                message: "Couldn't load vault statistics.".to_string(),
            }
        },
        LoadState::Loaded => rsx! {
            div { class: "grid grid-cols-2 md:grid-cols-4 gap-4" }
            {stats.map(|s| rsx! {
                StatCard { icon: Icon::FileText, label: "Notes", value: s.note_count.to_string() }
                StatCard { icon: Icon::Folder, label: "Folders", value: s.folder_count.to_string() }
                StatCard { icon: Icon::Tag, label: "Tags", value: s.total_tags.to_string() }
                StatCard { icon: Icon::Database, label: "Graph", value: format!("{} → {}", s.graph_nodes, s.graph_edges) }
                StatCard { icon: Icon::HardDrive, label: "Storage", value: fmt_bytes(s.storage_bytes) }
                StatCard { icon: Icon::Activity, label: "Streak", value: format!("{} days", s.writing_streak_days) }
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

#[allow(clippy::too_many_arguments)]
fn render_section(
    section: &str,
    nav: NavContext,
    ws: &WorkspaceContext,
    index: &[crate::components::navigation::state::NoteIndexEntry],
    stats_state: LoadState,
    stats: Option<&Stats>,
    inbox_state: LoadState,
    inbox_items: &[InboxRow],
) -> Element {
    let label = dashboard_section_label(section);
    let widget = match section {
        "quick_actions" => render_quick_actions(nav, ws),
        "recently_modified" => {
            if stats_state == LoadState::Error {
                rsx! {
                    ErrorPanel {
                        title: "Recently Modified".to_string(),
                        message: "Couldn't load statistics.".to_string(),
                    }
                }
            } else if stats_state == LoadState::Loading {
                rsx! { LoadingBlock { spinner_size: SpinnerSize::Sm } }
            } else {
                let notes = stats
                    .map(|s| s.recently_modified.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                render_note_list(&notes, index, nav, ws)
            }
        }
        "favourites" => render_path_list(&nav.favourites.read().clone(), index, nav, ws),
        "recently_opened" => render_path_list(&nav.recent_notes.read().clone(), index, nav, ws),
        "pinned" => render_pinned(index, nav, ws),
        "inbox" => render_inbox(inbox_state, inbox_items, nav, ws),
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

fn render_quick_actions(nav: NavContext, ws: &WorkspaceContext) -> Element {
    let vm_search = nav.view_mode;
    let vm_inbox = nav.view_mode;
    let vm_stats = nav.view_mode;
    rsx! {
        div { class: "grid grid-cols-2 md:grid-cols-4 gap-3" }
        QuickActionButton {
            icon: Icon::Clock, label: "Today's Note".to_string(),
            on_click: move |_: MouseEvent| { open_today_note(nav, *ws); },
        }
        QuickActionButton {
            icon: Icon::FilePlus, label: "New Note".to_string(),
            on_click: move |_: MouseEvent| { open_new_note(nav, *ws); },
        }
        QuickActionButton {
            icon: Icon::Search, label: "Search".to_string(),
            on_click: move |_: MouseEvent| { vm_search.set(ViewMode::Search); },
        }
        QuickActionButton {
            icon: Icon::Inbox, label: "Inbox".to_string(),
            on_click: move |_: MouseEvent| { vm_inbox.set(ViewMode::Inbox); },
        }
        QuickActionButton {
            icon: Icon::Database, label: "Statistics".to_string(),
            on_click: move |_: MouseEvent| { vm_stats.set(ViewMode::Statistics); },
        }
    }
}

#[component]
fn QuickActionButton(icon: Icon, label: String, on_click: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "quick-action flex flex-col items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700 hover:bg-gray-700/50 transition-colors text-center",
            on_click,
            {render_icon_view(icon)}
            span { class: "text-xs text-gray-300", "{label}" }
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

fn render_note_list(
    notes: &[NoteStat],
    index: &[crate::components::navigation::state::NoteIndexEntry],
    nav: NavContext,
    ws: &WorkspaceContext,
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
            let path = n.path.clone();
            let title = n.title.clone();
            let folder = n.folder.clone();
            let modified = n.modified_at.clone();
            let ws = *ws; let nav_copy = nav;
            rsx! {
                div {
                    class: "flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm",
                    onclick: move |_: MouseEvent| { open_note(nav_copy, ws, &path); },
                    title,
                    div { class: "text-sm text-gray-200 truncate", "{title}" }
                    if !folder.is_empty() { span { class: "text-xs text-gray-500", "{folder}/" } }
                    div { class: "ml-auto text-xs text-gray-500", "{fmt_date(&modified)}" }
                }
            }
        }
    }
}

fn render_path_list(
    paths: &[String],
    index: &[crate::components::navigation::state::NoteIndexEntry],
    nav: NavContext,
    ws: &WorkspaceContext,
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
            let path = p.clone();
            let title = note_title(index, &path);
            let folder = index.iter().find(|n| n.path == path).map(|n| n.folder.clone()).unwrap_or_default();
            let ws = *ws; let nav_copy = nav;
            rsx! {
                div {
                    class: "flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm",
                    onclick: move |_: MouseEvent| { open_note(nav_copy, ws, &path); },
                    div { class: "flex-1 min-w-0" }
                    div { class: "text-sm text-gray-200 truncate", "{title}" }
                    if !folder.is_empty() { span { class: "text-xs text-gray-500", "{folder}/" } }
                }
            }
        }
    }
}

fn render_pinned(
    index: &[crate::components::navigation::state::NoteIndexEntry],
    nav: NavContext,
    ws: &WorkspaceContext,
) -> Element {
    let pinned: Vec<_> = index.iter().filter(|n| n.pinned).cloned().collect();
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
            let path = n.path.clone();
            let title = n.title.clone();
            let ws = *ws; let nav_copy = nav;
            rsx! {
                span {
                    class: "inline-flex items-center gap-1 rounded bg-gray-800/50 px-2 py-1 text-xs text-gray-200 border border-gray-700",
                    onclick: move |_: MouseEvent| { open_note(nav_copy, ws, &path); },
                    {render_icon_view(Icon::BookMarked)}
                    "{title}"
                }
            }
        }
    }
}

fn render_inbox(
    state: LoadState,
    items: &[InboxRow],
    nav: NavContext,
    ws: &WorkspaceContext,
) -> Element {
    match state {
        LoadState::Loading => rsx! { LoadingBlock { spinner_size: SpinnerSize::Sm } },
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
                }
            }
        }
    }
}

fn render_searches(
    searches: &[String],
    nav: NavContext,
    _ws: &WorkspaceContext,
) -> Element {
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
            let query = q.clone();
            rsx! {
                button {
                    class: "search-chip inline-flex items-center gap-1 rounded bg-gray-800/50 px-2 py-1 text-xs text-gray-300 border border-gray-700 hover:bg-gray-700/50",
                    onclick: move |_: MouseEvent| { sq.set(query.clone()); vm.set(ViewMode::Search); },
                    {render_icon_view(Icon::Search)}
                    "{query}"
                }
            }
        }
    }
}
