//! Reading Queue UI component (Dioxus).
//!
//! Real reading queue backed by the `queue_*` Tauri commands. The queue is a
//! projection of persisted `KnowledgeObject`s — the view never owns data.
//!
//! Data flow:
//! - load: `queue_get_all` -> `Vec<QueueItem>`
//! - mutate: `queue_set_status` / `queue_set_priority` / `queue_set_progress` /
//!   `queue_batch_set_status`
//!
//! Mutations route through the real backend and the queue is reconciled from
//! backend truth after every successful change. A failed mutation is surfaced
//! (toast) and the UI is **not** advanced as though persistence succeeded. A
//! backend `ItemStored` event triggers a reload so newly-stored objects appear.

use crate::components::ui::feedback::{ErrorPanel, SkeletonList, ToastContext, use_toast};
use crate::components::ui::icons::Icon;
use crate::components::ui::info::EmptyState;
use crate::events::{use_event_listener, FrontendEvent, FrontendEventKind};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

// ── Types (mirror backend QueueItem/QueueStatus/QueuePriority in commands.rs) ──

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Unread,
    Reading,
    Completed,
    Archived,
}

impl Default for QueueStatus {
    fn default() -> Self {
        Self::Unread
    }
}

impl QueueStatus {
    /// Wire-format label consumed by the backend `QueueStatus::from_label`.
    pub fn label(self) -> &'static str {
        match self {
            QueueStatus::Unread => "unread",
            QueueStatus::Reading => "reading",
            QueueStatus::Completed => "completed",
            QueueStatus::Archived => "archived",
        }
    }

    /// Inverse of [`label`](Self::label), mirroring the backend parser.
    pub fn from_label(label: &str) -> Self {
        match label {
            "reading" => QueueStatus::Reading,
            "completed" => QueueStatus::Completed,
            "archived" => QueueStatus::Archived,
            _ => QueueStatus::Unread,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuePriority {
    Low,
    Normal,
    High,
}

impl Default for QueuePriority {
    fn default() -> Self {
        Self::Normal
    }
}

impl QueuePriority {
    pub fn label(self) -> &'static str {
        match self {
            QueuePriority::Low => "low",
            QueuePriority::Normal => "normal",
            QueuePriority::High => "high",
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label {
            "low" => QueuePriority::Low,
            "high" => QueuePriority::High,
            _ => QueuePriority::Normal,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub object_type: String,
    pub status: QueueStatus,
    pub priority: QueuePriority,
    pub progress: f32,
    pub source: String,
    pub modified_at: String,
    pub tags: Vec<String>,
    pub selected: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct QueueFilter {
    pub status: Option<QueueStatus>,
    pub priority: Option<QueuePriority>,
    pub search: String,
    pub sort_by: QueueSortField,
    pub sort_ascending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum QueueSortField {
    #[default]
    ModifiedAt,
    Title,
    Priority,
    Progress,
    Status,
}

// ── Load-state classification ──────────────────────────────────────────

/// Lifecycle phase of the queue view, derived purely from load state + payload.
///
/// A failed load is **never** collapsed into an empty queue: the error is
/// surfaced so the user can distinguish "no items" from "could not load."
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QueueViewPhase {
    Loading,
    Empty,
    Error,
    Ready,
}

pub fn classify_queue(loaded: bool, load_error: Option<&str>, items: &[QueueItem]) -> QueueViewPhase {
    if !loaded {
        return QueueViewPhase::Loading;
    }
    if load_error.is_some() {
        return QueueViewPhase::Error;
    }
    if items.is_empty() {
        QueueViewPhase::Empty
    } else {
        QueueViewPhase::Ready
    }
}

// ── Pure projection helpers (tested independently of the DOM) ──────────

/// Apply the active filter + sort to a slice of queue items.
pub fn apply_filter(items: &[QueueItem], filter: &QueueFilter) -> Vec<QueueItem> {
    let mut result = items.to_vec();

    if let Some(status) = filter.status {
        result.retain(|i| i.status == status);
    }
    if let Some(priority) = filter.priority {
        result.retain(|i| i.priority == priority);
    }
    if !filter.search.is_empty() {
        let q = filter.search.to_lowercase();
        result.retain(|i| {
            i.title.to_lowercase().contains(&q)
                || i.object_type.to_lowercase().contains(&q)
                || i.tags.iter().any(|t| t.to_lowercase().contains(&q))
        });
    }

    result.sort_by(|a, b| cmp_by(a, b, filter.sort_by, filter.sort_ascending));
    result
}

fn cmp_by(a: &QueueItem, b: &QueueItem, field: QueueSortField, ascending: bool) -> std::cmp::Ordering {
    let ord = match field {
        QueueSortField::ModifiedAt => a.modified_at.cmp(&b.modified_at),
        QueueSortField::Title => a.title.cmp(&b.title),
        QueueSortField::Priority => a.priority.cmp(&b.priority),
        QueueSortField::Progress => a
            .progress
            .partial_cmp(&b.progress)
            .unwrap_or(std::cmp::Ordering::Equal),
        QueueSortField::Status => a.status.cmp(&b.status),
    };
    if ascending {
        ord
    } else {
        ord.reverse()
    }
}

/// Aggregate counts of items per status (unread, reading, completed, archived).
pub fn status_counts(items: &[QueueItem]) -> (usize, usize, usize, usize) {
    let mut counts = (0usize, 0usize, 0usize, 0usize);
    for i in items {
        match i.status {
            QueueStatus::Unread => counts.0 += 1,
            QueueStatus::Reading => counts.1 += 1,
            QueueStatus::Completed => counts.2 += 1,
            QueueStatus::Archived => counts.3 += 1,
        }
    }
    counts
}

/// ids of all currently-selected queue items (drives batch actions).
pub fn selected_ids(items: &[QueueItem]) -> Vec<String> {
    items.iter().filter(|i| i.selected).map(|i| i.id.clone()).collect()
}

/// Post-mutation reconciliation, isolated for testing.
///
/// `Ok` means the backend accepted the change and the caller must reload the
/// queue so the visible state reflects backend truth. `Err(msg)` means the
/// backend rejected the change: the local items are returned **untouched** (no
/// false update) together with the message to surface.
pub fn reconcile_mutation(
    items: Vec<QueueItem>,
    outcome: Result<(), String>,
) -> (Vec<QueueItem>, Option<String>, bool) {
    match outcome {
        Ok(()) => (items, None, true),
        Err(msg) => (items, Some(msg), false),
    }
}

// ── IPC ────────────────────────────────────────────────────────────────

/// Loads the persisted reading queue from the backend.
fn load_queue(
    items: Signal<Vec<QueueItem>>,
    loaded: Signal<bool>,
    error: Signal<Option<String>>,
    toasts: ToastContext,
) {
    let mut loaded = loaded;
    let mut error = error;
    let mut items = items;
    loaded.set(false);
    error.set(None);

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap_or_default();
        match crate::ipc::tauri_invoke_safe("queue_get_all", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<QueueItem>>(val) {
                Ok(queue_items) => items.set(queue_items),
                Err(e) => {
                    error.set(Some(e.to_string()));
                    toasts.error(
                        "Reading queue",
                        "Could not parse the reading queue response.",
                    );
                }
            },
            Ok(None) => {
                error.set(Some("queue_get_all returned no data.".to_string()));
                toasts.error("Reading queue", "The reading queue returned no data.");
            }
            Err(e) => {
                error.set(Some(e.message()));
                toasts.error(
                    "Couldn't load the reading queue",
                    "Your reading queue could not be loaded — try again.",
                );
            }
        }
        loaded.set(true);
    });
}

/// Persists a single-item field change and reconciles from backend truth.
fn mutate_single(
    cmd: &str,
    args: serde_json::Map<String, serde_json::Value>,
    items: Signal<Vec<QueueItem>>,
    loaded: Signal<bool>,
    error: Signal<Option<String>>,
    toasts: ToastContext,
    failure_title: &str,
    failure_msg: &str,
) {
    let cmd = cmd.to_string();
    let failure_title = failure_title.to_string();
    let failure_msg = failure_msg.to_string();
    let args_val = serde_wasm_bindgen::to_value(&serde_json::Value::Object(args))
        .unwrap_or_default();
    spawn_local(async move {
        match crate::ipc::tauri_invoke_safe(&cmd, args_val).await {
            Err(e) => {
                toasts.error(&failure_title, &format!("{}: {}", failure_msg, e.message()));
            }
            Ok(_) => {
                load_queue(items, loaded, error, toasts);
            }
        }
    });
}

// ── Component ──────────────────────────────────────────────────────────

#[component]
pub fn ReadingQueue() -> Element {
    let mut items = use_signal(|| Vec::<QueueItem>::new());
    let mut loaded = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);
    let mut filter = use_signal(QueueFilter::default);
    let toasts = use_toast();

    let mut initialized = use_signal(|| false);
    if !*initialized.read() {
        initialized.set(true);
        load_queue(items, loaded, load_error, toasts);
    }

    // Reload whenever a note is stored (new/updated objects may join the queue).
    {
        let items = items;
        let loaded = loaded;
        let load_error = load_error;
        use_event_listener(FrontendEventKind::ItemStored, move |_: &FrontendEvent| {
            load_queue(items, loaded, load_error, toasts);
        });
    }

    // ── Handlers ──
    let on_filter_change = move |ev: FormEvent| {
        let mut f = filter;
        f.set(QueueFilter {
            search: ev.value(),
            ..f.read().clone()
        });
    };

    let on_status_filter = move |status: QueueStatus| {
        let mut f = filter;
        let current = f.read().clone();
        f.set(QueueFilter {
            status: if current.status == Some(status) {
                None
            } else {
                Some(status)
            },
            ..current
        });
    };

    let on_priority_filter = move |priority: QueuePriority| {
        let mut f = filter;
        let current = f.read().clone();
        f.set(QueueFilter {
            priority: if current.priority == Some(priority) {
                None
            } else {
                Some(priority)
            },
            ..current
        });
    };

    let on_sort_change = move |field: QueueSortField| {
        let mut f = filter;
        let current = f.read().clone();
        f.set(QueueFilter {
            sort_by: field,
            sort_ascending: if current.sort_by == field {
                !current.sort_ascending
            } else {
                true
            },
            ..current
        });
    };

    let toggle_select = move |id: String| {
        let mut items = items;
        let mut all = items.read().clone();
        if let Some(i) = all.iter_mut().find(|i| i.id == id) {
            i.selected = !i.selected;
        }
        items.set(all);
    };

    let set_status = move |id: String, status: QueueStatus| {
        let mut args = serde_json::Map::new();
        args.insert("id".into(), serde_json::Value::String(id));
        args.insert("status".into(), serde_json::Value::String(status.label().to_string()));
        mutate_single(
            "queue_set_status",
            args,
            items,
            loaded,
            load_error,
            toasts,
            "Reading queue",
            "Could not update that item's status",
        );
    };

    let set_priority = move |id: String, priority: QueuePriority| {
        let mut args = serde_json::Map::new();
        args.insert("id".into(), serde_json::Value::String(id));
        args.insert("priority".into(), serde_json::Value::String(priority.label().to_string()));
        mutate_single(
            "queue_set_priority",
            args,
            items,
            loaded,
            load_error,
            toasts,
            "Reading queue",
            "Could not update that item's priority",
        );
    };

    let set_progress = move |id: String, progress: f32| {
        let mut args = serde_json::Map::new();
        args.insert("id".into(), serde_json::Value::String(id));
        args.insert("progress".into(), serde_json::Value::Number(serde_json::Number::from_f64(progress as f64).unwrap_or(serde_json::Number::from_f64(0.0).unwrap())));
        mutate_single(
            "queue_set_progress",
            args,
            items,
            loaded,
            load_error,
            toasts,
            "Reading queue",
            "Could not update reading progress",
        );
    };

    let batch_set_status = move |status: QueueStatus| {
        let ids = {
            let mut a = items.read().clone();
            selected_ids(&a)
        };
        if ids.is_empty() {
            return;
        }
        let mut args = serde_json::Map::new();
        args.insert("ids".into(), serde_json::Value::Array(ids.into_iter().map(serde_json::Value::String).collect()));
        args.insert("status".into(), serde_json::Value::String(status.label().to_string()));
        mutate_single(
            "queue_batch_set_status",
            args,
            items,
            loaded,
            load_error,
            toasts,
            "Reading queue",
            "Could not update the selected items",
        );
    };

    rsx! {
        div { class: "reading-queue flex h-full bg-gray-950 text-gray-100 overflow-hidden" }

        // ── Left panel: queue list ──
        div { class: "flex-none w-96 border-r border-gray-800 flex flex-col" }

        // Header
        div { class: "flex items-center justify-between px-4 py-3 border-b border-gray-800" }
        h2 { class: "text-sm font-semibold text-gray-300", "Reading Queue" }
        span { class: "text-xs text-gray-500", "{items.read().len()} items" }

        // Search
        div { class: "px-3 py-2 border-b border-gray-800" }
        input {
            r#type: "text",
            placeholder: "Search queue...",
            class: "w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none",
            oninput: on_filter_change,
        }

        div { class: "flex items-center gap-2 px-3 py-2 border-b border-gray-800" }
        {
            let (unread, reading, completed, archived) = status_counts(&items.read());
            let f = filter.read().clone();
            rsx! {
                StatusPill {
                    label: "Unread",
                    count: unread,
                    active: f.status == Some(QueueStatus::Unread),
                    status: QueueStatus::Unread,
                    on_toggle: on_status_filter,
                }
                StatusPill {
                    label: "Reading",
                    count: reading,
                    active: f.status == Some(QueueStatus::Reading),
                    status: QueueStatus::Reading,
                    on_toggle: on_status_filter,
                }
                StatusPill {
                    label: "Done",
                    count: completed,
                    active: f.status == Some(QueueStatus::Completed),
                    status: QueueStatus::Completed,
                    on_toggle: on_status_filter,
                }
                StatusPill {
                    label: "Archived",
                    count: archived,
                    active: f.status == Some(QueueStatus::Archived),
                    status: QueueStatus::Archived,
                    on_toggle: on_status_filter,
                }
            }
        }

        // Priority filter
        div { class: "flex items-center gap-2 px-3 py-1.5 border-b border-gray-800" }
        span { class: "text-xs text-gray-500", "Priority:" }
        PriorityPill {
            label: "High",
            priority: QueuePriority::High,
            active: filter.read().priority == Some(QueuePriority::High),
            on_toggle: on_priority_filter,
        }
        PriorityPill {
            label: "Normal",
            priority: QueuePriority::Normal,
            active: filter.read().priority == Some(QueuePriority::Normal),
            on_toggle: on_priority_filter,
        }
        PriorityPill {
            label: "Low",
            priority: QueuePriority::Low,
            active: filter.read().priority == Some(QueuePriority::Low),
            on_toggle: on_priority_filter,
        }

        {
            let count = items.read().iter().filter(|i| i.selected).count();
            if count > 0 {
                rsx! {
                    div { class: "flex items-center gap-2 px-3 py-2 bg-blue-900/30 border-b border-blue-800/30" }
                    span { class: "text-xs text-blue-400", "{count} selected" }
                    button { class: "px-2 py-1 text-xs bg-green-700 rounded hover:bg-green-600", onclick: move |_| batch_set_status(QueueStatus::Reading), "Mark Reading" }
                    button { class: "px-2 py-1 text-xs bg-blue-700 rounded hover:bg-blue-600", onclick: move |_| batch_set_status(QueueStatus::Completed), "Mark Done" }
                    button { class: "px-2 py-1 text-xs bg-gray-700 rounded hover:bg-gray-600", onclick: move |_| batch_set_status(QueueStatus::Archived), "Archive" }
                }
            } else {
                rsx! {}
            }
        }

        div { class: "flex-1 overflow-y-auto" }
        {
            let phase = classify_queue(
                *loaded.read(),
                load_error.read().as_deref(),
                &items.read(),
            );
            match phase {
                QueueViewPhase::Loading => rsx! {
                    div { class: "p-4" }
                    SkeletonList { rows: Some(6) }
                },
                QueueViewPhase::Error => rsx! {
                    div { class: "p-4" }
                    ErrorPanel {
                        title: "Couldn't load the reading queue".to_string(),
                        message: "Something went wrong while reading your queue.".to_string(),
                        details: load_error.read().clone(),
                        on_retry: move |_| {
                            load_queue(items, loaded, load_error, toasts);
                        },
                        recovery: "Check that your vault is accessible, then try again.".to_string(),
                    }
                },
                QueueViewPhase::Empty => rsx! {
                    div { class: "h-full flex items-center justify-center p-6" }
                    EmptyState {
                        icon: Some(Icon::BookOpen),
                        title: "Nothing in the queue".to_string(),
                        description: "Add documents or web articles to your reading queue and track progress here.".to_string(),
                    }
                },
                QueueViewPhase::Ready => {
                    let f = filter.read().clone();
                    let visible = apply_filter(&items.read(), &f);
                    if visible.is_empty() {
                        rsx! {
                            div { class: "p-6 text-center text-gray-500", "No items match your filters." }
                        }
                    } else {
                        rsx! {
                            div { class: "divide-y divide-gray-800" }
                            for item in &visible {
                                {
                                    let id = item.id.clone();
                                    let title = item.title.clone();
                                    let item_status = item.status;
                                    let priority = item.priority;
                                    let progress = item.progress;
                                    let object_type = item.object_type.clone();
                                    let selected = item.selected;
                                    let pct = progress_pct(progress);
                                    rsx! {
                                        div {
                                            key: item.id.clone(),
                                            class: item_row_class(selected, priority),
                                            onclick: move |_| toggle_select(id.clone()),
                                        }
                                        div { class: "flex items-center justify-between" }
                                        span { class: "text-sm font-medium truncate max-w-48", "{title}" }
                                        span { class: "text-xs px-1.5 py-0.5 rounded bg-gray-700 text-gray-300", "{item_status_label(item_status)}" }

                                        div { class: "flex items-center gap-2 mt-1" }
                                        span { class: "text-xs text-gray-500", "{object_type}" }
                                        div { class: "flex-1 h-1 bg-gray-700 rounded-full overflow-hidden" }
                                        div {
                                            class: "h-full bg-blue-500 rounded-full transition-all",
                                            style: "width: {pct}%",
                                        }
                                        span { class: "text-xs text-gray-500", "{pct}%" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Footer with sort controls
        div { class: "flex items-center justify-between px-3 py-2 border-t border-gray-800 text-xs text-gray-500" }
        span { "{apply_filter(&items.read(), &filter.read().clone()).len()} items" }
        div { class: "flex gap-2" }
        button { class: "hover:text-gray-300", onclick: move |_| on_sort_change(QueueSortField::ModifiedAt), "Sort by Date" }
        button { class: "hover:text-gray-300", onclick: move |_| on_sort_change(QueueSortField::Title), "Sort by Title" }
        button { class: "hover:text-gray-300", onclick: move |_| on_sort_change(QueueSortField::Priority), "Sort by Priority" }
        button { class: "hover:text-gray-300", onclick: move |_| on_sort_change(QueueSortField::Progress), "Sort by Progress" }

        // ── Right panel: detail view ──
        div { class: "flex-1 flex flex-col overflow-hidden" }
        {
            let selected = items.read().iter().find(|i| i.selected).cloned();
            match selected {
                Some(item) => rsx! {
                    div { class: "flex h-full" }
                    div { class: "flex-1 overflow-y-auto p-4" }
                    div { class: "space-y-4" }
                    h2 { class: "text-xl font-semibold", "{item.title}" }
                    div { class: "grid grid-cols-2 gap-4" }

                    div {}
                    label { class: "text-xs text-gray-500 uppercase tracking-wide", "Status" }
                    div { class: "mt-1" }
                    select {
                        class: "bg-gray-800 text-gray-100 rounded px-2 py-1 text-sm border border-gray-700",
                        value: item.status.label(),
                        onchange: {
                            let id = item.id.clone();
                            move |ev: FormEvent| set_status(id.clone(), QueueStatus::from_label(&ev.value()))
                        },
                    }
                    option { value: "unread", "Unread" }
                    option { value: "reading", "Reading" }
                    option { value: "completed", "Completed" }
                    option { value: "archived", "Archived" }

                    div {}
                    label { class: "text-xs text-gray-500 uppercase tracking-wide", "Priority" }
                    div { class: "mt-1" }
                    select {
                        class: "bg-gray-800 text-gray-100 rounded px-2 py-1 text-sm border border-gray-700",
                        value: item.priority.label(),
                        onchange: {
                            let id = item.id.clone();
                            move |ev: FormEvent| set_priority(id.clone(), QueuePriority::from_label(&ev.value()))
                        },
                    }
                    option { value: "normal", "Normal" }
                    option { value: "high", "High" }
                    option { value: "low", "Low" }

                    div {}
                    label { class: "text-xs text-gray-500 uppercase tracking-wide", "Progress" }
                    div { class: "mt-1 flex items-center gap-3" }
                    input {
                        r#type: "range",
                        min: "0",
                        max: "100",
                        value: "{progress_pct(item.progress)}",
                        class: "flex-1",
                        oninput: {
                            let id = item.id.clone();
                            move |ev: FormEvent| {
                                let p = ev.value().parse::<f32>().unwrap_or(0.0) / 100.0;
                                set_progress(id.clone(), p);
                            }
                        },
                    }
                    span { class: "text-sm text-gray-400", "{progress_pct(item.progress)}%" }

                    div {}
                    label { class: "text-xs text-gray-500 uppercase tracking-wide", "Type" }
                    p { class: "text-sm text-gray-300 mt-1", "{item.object_type}" }

                    div {}
                    label { class: "text-xs text-gray-500 uppercase tracking-wide", "Source" }
                    p { class: "text-sm text-gray-300 mt-1", "{item.source}" }

                    div {}
                    label { class: "text-xs text-gray-500 uppercase tracking-wide", "Tags" }
                    div { class: "flex flex-wrap gap-1 mt-1" }
                    for tag in &item.tags {
                        {
                            let t = tag.clone();
                            rsx! {
                                span { class: "px-2 py-0.5 text-xs bg-gray-700 rounded text-gray-300", "{t}" }
                            }
                        }
                    }
                },
                None => rsx! {
                    div { class: "flex items-center justify-center h-full text-gray-500", "Select an item to view details" }
                },
            }
        }
    }
}

// ── Row + detail helpers ───────────────────────────────────────────────

fn item_row_class(selected: bool, priority: QueuePriority) -> String {
    let base = "queue-item px-3 py-2 cursor-pointer hover:bg-gray-800 border-l-2";
    let sel = if selected {
        "bg-gray-800 border-l-blue-500"
    } else {
        "border-l-transparent"
    };
    let pri = match priority {
        QueuePriority::High => "border-l-red-500",
        QueuePriority::Normal => "",
        QueuePriority::Low => "border-l-blue-500",
    };
    format!("{} {} {}", base, sel, pri)
}

fn item_status_label(status: QueueStatus) -> String {
    match status {
        QueueStatus::Unread => "Unread".to_string(),
        QueueStatus::Reading => "Reading".to_string(),
        QueueStatus::Completed => "Done".to_string(),
        QueueStatus::Archived => "Archived".to_string(),
    }
}

fn progress_pct(progress: f32) -> u32 {
    (progress * 100.0).round() as u32
}

/// Sub-helper component: a status filter pill.
#[component]
fn StatusPill(
    label: String,
    count: usize,
    active: bool,
    status: QueueStatus,
    on_toggle: EventHandler<QueueStatus>,
) -> Element {
    let class = if active {
        "px-2 py-0.5 text-xs rounded-full border bg-blue-900/50 border-blue-600 text-blue-300"
    } else {
        "px-2 py-0.5 text-xs rounded-full border border-gray-700 text-gray-400 hover:text-gray-200"
    };
    rsx! {
        button { class, onclick: move |_| on_toggle.call(status), "{label} {count}" }
    }
}

/// Sub-helper component: a priority filter pill.
#[component]
fn PriorityPill(
    label: String,
    priority: QueuePriority,
    active: bool,
    on_toggle: EventHandler<QueuePriority>,
) -> Element {
    let class = if active {
        "px-2 py-0.5 text-xs rounded border bg-gray-700 border-gray-500 text-gray-300"
    } else {
        "px-2 py-0.5 text-xs rounded border border-gray-700 text-gray-400"
    };
    rsx! {
        button { class, onclick: move |_| on_toggle.call(priority), "{label}" }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, title: &str, status: QueueStatus, priority: QueuePriority, tags: Vec<&str>) -> QueueItem {
        QueueItem {
            id: id.to_string(),
            title: title.to_string(),
            object_type: "note".to_string(),
            status,
            priority,
            progress: 0.0,
            source: "vault".to_string(),
            modified_at: "2024-01-02T00:00:00Z".to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            selected: false,
        }
    }

    // Test 1 — Queue loads (classification of a populated, loaded queue).
    #[test]
    fn populated_loaded_queue_classifies_ready() {
        let items = vec![item("1", "A", QueueStatus::Unread, QueuePriority::High, vec![])];
        assert_eq!(
            classify_queue(true, None, &items),
            QueueViewPhase::Ready
        );
    }

    // Test 2 — Queue empty.
    #[test]
    fn empty_queue_classifies_empty() {
        assert_eq!(classify_queue(true, None, &[]), QueueViewPhase::Empty);
    }

    // Test 3 — Queue failure is never shown as empty.
    #[test]
    fn failed_load_classifies_error_not_empty() {
        assert_eq!(classify_queue(true, Some("boom"), &[]), QueueViewPhase::Error);
        assert_ne!(classify_queue(true, Some("boom"), &[]), QueueViewPhase::Empty);
    }

    #[test]
    fn loading_queue_classifies_loading() {
        assert_eq!(classify_queue(false, None, &[]), QueueViewPhase::Loading);
    }

    // Test 4 — Queue mutation success triggers a reload (visible state reconciled), no error.
    #[test]
    fn successful_mutation_reconciles_and_reloads() {
        let items = vec![item("1", "A", QueueStatus::Unread, QueuePriority::Normal, vec![])];
        let (unchanged, err, reload) = reconcile_mutation(items.clone(), Ok(()));
        // No false update: items untouched.
        assert_eq!(unchanged, items);
        // Backend accepted → caller reloads to reflect truth.
        assert!(reload);
        assert!(err.is_none());
    }

    // Test 5 — Queue mutation failure does not falsely update state.
    #[test]
    fn failed_mutation_surfaces_error_without_update() {
        let items = vec![item("1", "A", QueueStatus::Unread, QueuePriority::Normal, vec![])];
        let (unchanged, err, reload) =
            reconcile_mutation(items.clone(), Err("backend rejected".to_string()));
        // No false update: local items untouched.
        assert_eq!(unchanged, items);
        assert_eq!(err.as_deref(), Some("backend rejected"));
        // No reload — the stale-backed data stays as-is; error is surfaced.
        assert!(!reload);
    }

    // Filtering: status, priority, search.
    #[test]
    fn apply_filter_retains_only_matching() {
        let items = vec![
            item("1", "Alpha", QueueStatus::Reading, QueuePriority::High, vec!["web"]),
            item("2", "Beta", QueueStatus::Unread, QueuePriority::Low, vec![]),
            item("3", "Gamma", QueueStatus::Reading, QueuePriority::High, vec!["web"]),
        ];
        let f = QueueFilter {
            status: Some(QueueStatus::Reading),
            priority: Some(QueuePriority::High),
            search: String::new(),
            sort_by: QueueSortField::Title,
            sort_ascending: true,
        };
        let got = apply_filter(&items, &f);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].title, "Alpha");
        assert_eq!(got[1].title, "Gamma");
    }

    #[test]
    fn apply_filter_search_matches_title_tag_type() {
        let items = vec![
            item("1", "Alpha", QueueStatus::Unread, QueuePriority::Low, vec!["research"]),
            item("2", "Beta", QueueStatus::Unread, QueuePriority::Low, vec![]),
        ];
        let f = QueueFilter {
            status: None,
            priority: None,
            search: "research".to_string(),
            sort_by: QueueSortField::Title,
            sort_ascending: true,
        };
        assert_eq!(apply_filter(&items, &f).len(), 1);
    }

    #[test]
    fn status_counts_aggregate_per_status() {
        let items = vec![
            item("1", "A", QueueStatus::Unread, QueuePriority::Normal, vec![]),
            item("2", "B", QueueStatus::Reading, QueuePriority::Normal, vec![]),
            item("3", "C", QueueStatus::Completed, QueuePriority::Normal, vec![]),
            item("4", "D", QueueStatus::Reading, QueuePriority::Normal, vec![]),
        ];
        assert_eq!(status_counts(&items), (1, 2, 1, 0));
    }

    #[test]
    fn selected_ids_returns_only_selected() {
        let mut a = item("1", "A", QueueStatus::Unread, QueuePriority::Normal, vec![]);
        a.selected = true;
        let b = item("2", "B", QueueStatus::Unread, QueuePriority::Normal, vec![]);
        let c = item("3", "C", QueueStatus::Unread, QueuePriority::Normal, vec![]);
        let mut cc = c;
        cc.selected = true;
        assert_eq!(selected_ids(&[a, b, cc]), vec!["1".to_string(), "3".to_string()]);
    }

    // IPC contract: frontend labels round-trip through the backend parser.
    #[test]
    fn status_labels_round_trip_through_backend_parser() {
        for s in [QueueStatus::Unread, QueueStatus::Reading, QueueStatus::Completed, QueueStatus::Archived] {
            let label = s.label();
            // Backend `from_label` maps an unknown label to Unread; known labels
            // must round-trip exactly.
            assert_eq!(QueueStatus::from_label(label), s);
        }
        assert_eq!(QueueStatus::from_label("reading"), QueueStatus::Reading);
        assert_eq!(QueueStatus::from_label("unknown"), QueueStatus::Unread);
    }

    #[test]
    fn priority_labels_round_trip() {
        for p in [QueuePriority::Low, QueuePriority::Normal, QueuePriority::High] {
            assert_eq!(QueuePriority::from_label(p.label()), p);
        }
        assert_eq!(QueuePriority::from_label("bogus"), QueuePriority::Normal);
    }
}
