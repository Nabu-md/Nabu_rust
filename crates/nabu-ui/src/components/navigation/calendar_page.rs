//! # Calendar page
//!
//! Date-based navigation workspace. Loads notes dated within the selected
//! month via the `calendar_notes` command, lays them out on a month grid, and
//! lets the user open any note or create/open that day's daily note.
//!
//! Month navigation (prev / today / next) re-fetches for the new month. An
//! `ItemStored` listener refreshes the current month's notes after edits.

use crate::components::contexts::{
    open_tab, record_recent_note, use_nav, use_workspace, NavContext, ViewMode, WorkspaceContext,
};
use crate::components::ui::feedback::{ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::events::{use_event_listener, FrontendEvent, FrontendEventKind};
use crate::ipc;
use crate::models::organisation::CalendarEntry;
use chrono::{Datelike, NaiveDate};
use dioxus::prelude::*;
use std::collections::HashMap;
use wasm_bindgen_futures::spawn_local;

/// Lifecycle of the `calendar_notes` IPC call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoadState { Loading, Error, Loaded }
impl Default for LoadState { fn default() -> Self { Self::Loading } }

fn today_str() -> String {
    let ms = js_sys::Date::now() as i64;
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn current_month() -> String {
    today_str().chars().take(7).collect()
}

fn shift_month(month: &str, delta: i32) -> String {
    let y: i32 = month[..4].parse().unwrap_or(2024);
    let m: u32 = month[5..7].parse().unwrap_or(1);
    let total = y * 12 + (m as i32 - 1) + delta;
    let ny = total.div_euclid(12);
    let nm = total.rem_euclid(12) as u32 + 1;
    format!("{:04}-{:02}", ny, nm)
}

fn month_label(month: &str) -> String {
    let y: i32 = month[..4].parse().unwrap_or(2024);
    let m: u32 = month[5..7].parse().unwrap_or(1);
    NaiveDate::from_ymd_opt(y, m, 1)
        .map(|d| d.format("%B %Y").to_string())
        .unwrap_or_else(|| month.to_string())
}

/// Day of week where Sunday = 0, using a simple algorithm (Tomohiko Sakamoto's).
fn naive_weekday_sunday(y: i32, m: u32, d: u32) -> usize {
    let dd = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    ((yy + yy / 4 - yy / 100 + yy / 400 + dd[(m - 1) as usize] as i32 + d as i32) % 7) as usize
}

fn open_note(mut nav: NavContext, ws: WorkspaceContext, path: &str) {
    open_tab(ws, path);
    record_recent_note(nav, path);
    nav.view_mode.set(ViewMode::Editor);
}

fn load_month(month: String, mut entries: Signal<Vec<CalendarEntry>>, mut state: Signal<LoadState>) {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "month": month })).unwrap();
    state.set(LoadState::Loading);
    spawn_local(async move {
        match ipc::tauri_invoke_safe("calendar_notes", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<CalendarEntry>>(val) {
                Ok(list) => { entries.set(list); state.set(LoadState::Loaded); }
                Err(e) => { entries.set(Vec::new()); state.set(LoadState::Error); tracing::warn!("calendar: {e}"); }
            },
            Ok(None) => { entries.set(Vec::new()); state.set(LoadState::Error); }
            Err(e) => { entries.set(Vec::new()); state.set(LoadState::Error); tracing::warn!("calendar: {}", e.message()); }
        }
    });
}

fn open_daily_note(nav: NavContext, ws: WorkspaceContext, date: String) {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "date": date })).unwrap();
    spawn_local(async move {
        match ipc::tauri_invoke_safe("daily_note_for", args).await {
            Ok(Some(val)) => {
                if let Some(path) = val.as_string() { open_note(nav, ws, &path); }
            }
            Ok(None) => tracing::warn!("calendar: daily_note_for returned no path"),
            Err(e) => tracing::warn!("calendar: daily_note_for error: {}", e.message()),
        }
    });
}

#[component]
pub fn CalendarPage() -> Element {
    let mut nav = use_nav();
    let ws = use_workspace();

    let mut month = use_signal(current_month);
    let mut entries = use_signal(Vec::<CalendarEntry>::new);
    let mut state = use_signal(LoadState::default);

    // Initial load (runs once).
    {
        let mut initialized = use_signal(|| false);
        if !*initialized.read() {
            initialized.set(true);
            load_month(month.read().clone(), entries, state);
        }
    }

    // Refresh after external edits.
    use_event_listener(FrontendEventKind::ItemStored, move |_ev: &FrontendEvent| {
        let m = month.read().clone();
        let mut e = entries;
        let mut s = state;
        load_month(m, e, s);
    });

    let month_str = month.read().clone();
    let notes = entries.read().clone();
    let cur_state = *state.read();
    let label = month_label(&month_str);
    let today = today_str();
    let prev_month = shift_month(&month_str, -1);
    let next_month = shift_month(&month_str, 1);

    // Group notes by date (YYYY-MM-DD).
    let by_date: HashMap<String, Vec<CalendarEntry>> = {
        let mut m: HashMap<String, Vec<CalendarEntry>> = HashMap::new();
        for n in notes.iter() {
            m.entry(n.date.clone()).or_default().push(n.clone());
        }
        m
    };

    // Grid geometry.
    let y: i32 = month_str[..4].parse().unwrap_or(2024);
    let m_mon: u32 = month_str[5..7].parse().unwrap_or(1);
    let first = NaiveDate::from_ymd_opt(y, m_mon, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
    let next_first = if m_mon == 12 {
        NaiveDate::from_ymd_opt(y + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(y, m_mon + 1, 1).unwrap()
    };
    let days_in_month = (next_first - first).num_days() as usize;
    let start_offset = naive_weekday_sunday(y, m_mon, 1);
    let total_cells = start_offset + days_in_month;
    let weeks = (total_cells + 6) / 7;

    rsx! {
        div { class: "calendar-page h-full overflow-y-auto" }

        // Toolbar.
        div { class: "calendar-toolbar flex items-center justify-between px-6 py-4 border-b border-gray-700" }
        div { class: "flex items-center gap-2" }
        button {
            class: "cal-btn rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50",
            onclick: move |_: MouseEvent| {
                let m = prev_month.clone();
                month.set(m.clone());
                load_month(m, entries, state);
            },
            {render_icon_view(Icon::Clock)}
            "Prev"
        }
        h1 { class: "text-lg font-semibold text-gray-100", "{label}" }
        button {
            class: "cal-btn rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50",
            onclick: move |_: MouseEvent| {
                let mm = current_month();
                month.set(mm.clone());
                load_month(mm, entries, state);
            },
            "Today"
        }
        button {
            class: "cal-btn rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50",
            onclick: move |_: MouseEvent| {
                let mm = next_month.clone();
                month.set(mm.clone());
                load_month(mm, entries, state);
            },
            "Next"
        }

        // Body.
        {match cur_state {
            LoadState::Loading => rsx! { LoadingBlock { size: SpinnerSize::Md } },
            LoadState::Error => rsx! {
                ErrorPanel {
                    title: "Calendar".to_string(),
                    message: "Couldn't load notes for this month.".to_string(),
                }
            },
            LoadState::Loaded => rsx! {
                div { class: "calendar-body px-6 py-4" }
                {if notes.is_empty() {
                    rsx! {
                        EmptyState {
                            icon: Some(Icon::Calendar),
                            title: "No notes this month".to_string(),
                            description: Some(format!("No dated notes in {}.", label)),
                        }
                    }
                } else { rsx!{} }}

                table { class: "cal-table w-full border-collapse text-sm" }
                thead { class: "cal-weekday-h border-b border-gray-700" }
                tr {
                    th { "Sun" } th { "Mon" } th { "Tue" } th { "Wed" } th { "Thu" } th { "Fri" } th { "Sat" }
                }
                tbody {
                    for week in 0..weeks {
                tr {
                    class: "cal-row",
                    for col in 0..7usize {
                                {
                                    let i = week * 7 + col;
                                    let day_num = if i < start_offset || i >= start_offset + days_in_month {
                                        None
                                    } else {
                                        Some(i - start_offset + 1)
                                    };
                                    render_day_cell(day_num, y, m_mon, &by_date, &today, nav, ws)
                                }
                            }
                        }
                    }
                }
            },
        }}
    }
}

fn render_day_cell(
    day: Option<usize>,
    y: i32,
    m_mon: u32,
    by_date: &HashMap<String, Vec<CalendarEntry>>,
    today: &str,
    nav: NavContext,
    ws: WorkspaceContext,
) -> Element {
    match day {
        None => rsx! {
            td { class: "cal-day cal-day-empty border border-gray-800 p-1 align-top h-20" }
        },
        Some(d) => {
            let date_str = format!("{:04}-{:02}-{:02}", y, m_mon, d);
            let is_today = date_str == today;
            let day_notes: Vec<CalendarEntry> = by_date.get(&date_str).cloned().unwrap_or_else(|| Vec::new());
            let cell_class = if is_today {
                "cal-day cal-day-today border border-blue-500 p-1 align-top text-xs h-20"
            } else {
                "cal-day border border-gray-800 p-1 align-top text-xs h-20"
            };
            rsx! {
                td { class: cell_class }
                div { class: "day-header" }
                div {
                    class: "day-number text-gray-400 cursor-pointer",
                    onclick: move |_: MouseEvent| { let mut n = nav; open_daily_note(n, ws, date_str.clone()); },
                    "{d}"
                }
                if !day_notes.is_empty() {
                    div { class: "day-notes space-y-0.5" }
                    for n in &day_notes {
                        {render_calendar_note(n, nav, ws)}
                    }
                }
            }
        }
    }
}

fn render_calendar_note(n: &CalendarEntry, nav: NavContext, ws: WorkspaceContext) -> Element {
    let path = n.path.clone();
    let title = n.title.clone();
    rsx! {
        div {
            class: "day-note truncate text-blue-400 hover:text-blue-300 cursor-pointer",
            onclick: move |_: MouseEvent| { let mut nn = nav; open_note(nn, ws, &path); },
            title: "{title}",
            "{truncate_title(&title, 18)}"
        }
    }
}

fn truncate_title(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
