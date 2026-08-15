//! Calendar View component (Dioxus).
//!
//! Date-based calendar view with month/week/day modes and filtering.
//! Views are projections of `Vec<CollectionItem>` — views never own data.

use crate::components::collections::shared::types::{CalendarFilter, CalendarViewMode, CollectionItem};
use dioxus::prelude::*;

#[derive(Props, PartialEq, Clone)]
pub struct CalendarViewProps {
    pub objects: Vec<CollectionItem>,
    pub filter: CalendarFilter,
    pub on_filter_change: EventHandler<CalendarFilter>,
    pub on_open: EventHandler<String>,
}

/// Extracts a date key (YYYY-MM-DD) from an RFC 3339 timestamp string.
fn date_key(modified_at: &str) -> String {
    modified_at
        .get(..10)
        .unwrap_or("no-date")
        .to_string()
}

/// Groups items by their modification date key.
fn group_by_date(items: &[CollectionItem], query: &str) -> Vec<(String, Vec<CollectionItem>)> {
    let q = query.to_lowercase();
    let filtered: Vec<&CollectionItem> = if q.is_empty() {
        items.iter().collect()
    } else {
        items
            .iter()
            .filter(|i| {
                i.title.to_lowercase().contains(&q)
                    || i.folder.to_lowercase().contains(&q)
                    || i.path.to_lowercase().contains(&q)
            })
            .collect()
    };

    let mut groups: std::collections::BTreeMap<String, Vec<CollectionItem>> =
        std::collections::BTreeMap::new();
    for obj in &filtered {
        let key = date_key(&obj.modified_at);
        groups.entry(key).or_default().push((*obj).clone());
    }

    groups.into_iter().collect()
}

#[component]
pub fn CalendarView(props: CalendarViewProps) -> Element {
    let filter = props.filter.clone();
    let on_filter_change = props.on_filter_change;
    let on_open = props.on_open;

    let view_mode = filter.view_mode;
    let query = filter.query.clone();

    let date_groups = group_by_date(&props.objects, &query);

    let on_query_input = move |ev: FormEvent| {
        on_filter_change.call(CalendarFilter {
            query: ev.value(),
            ..filter.clone()
        });
    };

    let switch_to_month = {
        let on_filter_change = on_filter_change;
        let filter = filter.clone();
        move |_: MouseEvent| {
            on_filter_change.call(CalendarFilter {
                view_mode: CalendarViewMode::Month,
                ..filter.clone()
            });
        }
    };

    let switch_to_week = {
        let on_filter_change = on_filter_change;
        let filter = filter.clone();
        move |_: MouseEvent| {
            on_filter_change.call(CalendarFilter {
                view_mode: CalendarViewMode::Week,
                ..filter.clone()
            });
        }
    };

    let switch_to_day = {
        let on_filter_change = on_filter_change;
        let filter = filter.clone();
        move |_: MouseEvent| {
            on_filter_change.call(CalendarFilter {
                view_mode: CalendarViewMode::Day,
                ..filter.clone()
            });
        }
    };

    let month_class = if view_mode == CalendarViewMode::Month {
        "px-3 py-1 text-xs rounded border bg-blue-700 border-blue-500 text-blue-100"
    } else {
        "px-3 py-1 text-xs rounded border border-gray-600 text-gray-400"
    };
    let week_class = if view_mode == CalendarViewMode::Week {
        "px-3 py-1 text-xs rounded border bg-blue-700 border-blue-500 text-blue-100"
    } else {
        "px-3 py-1 text-xs rounded border border-gray-600 text-gray-400"
    };
    let day_class = if view_mode == CalendarViewMode::Day {
        "px-3 py-1 text-xs rounded border bg-blue-700 border-blue-500 text-blue-100"
    } else {
        "px-3 py-1 text-xs rounded border border-gray-600 text-gray-400"
    };

    rsx! {
        div { class: "calendar-view p-4 overflow-y-auto h-full" }

        div { class: "flex items-center justify-between mb-4" }
        h2 { class: "text-lg font-semibold text-gray-200", "Calendar" }
        div { class: "flex gap-2" }
        button { class: month_class, onclick: switch_to_month, "Month" }
        button { class: week_class, onclick: switch_to_week, "Week" }
        button { class: day_class, onclick: switch_to_day, "Day" }

        div { class: "flex gap-3 mb-4" }
        input {
            r#type: "text",
            placeholder: "Search...",
            class: "flex-1 bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none",
            value: "{query}",
            oninput: on_query_input,
        }

        div { class: "grid grid-cols-7 gap-1" }
        for day in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
            {
                let day = day.to_string();
                rsx! {
                    div { class: "text-center text-xs text-gray-500 py-2 font-medium", "{day}" }
                }
            }
        }

        {if date_groups.is_empty() {
            rsx! {
                div { class: "col-span-full flex items-center justify-center h-64 text-gray-500" }
                "No items to display"
            }
        } else {
            rsx! {
                for (date_key_str, day_items) in &date_groups {
                    {
                        let key = date_key_str.clone();
                        let count = day_items.len();
                        let items: Vec<Element> = day_items
                            .iter()
                            .take(3)
                            .map(|obj| {
                                let obj = obj.clone();
                                let title = obj.title.clone();
                                let on_open = on_open;
                                rsx! {
                                    div {
                                        class: "text-xs text-blue-400 truncate",
                                        title: "{title}",
                                        onclick: move |_: MouseEvent| on_open.call(obj.path.clone()),
                                    }
                                    "{title}"
                                }
                            })
                            .collect();
                        rsx! {
                            div { class: "col-span-1 bg-gray-800 rounded border border-gray-700 p-2 min-h-[80px]" }
                            div { class: "text-xs text-gray-400 mb-1", "{key}" }
                            div { class: "text-xs text-gray-500", "{count} items" }
                            div { class: "mt-1 space-y-1" }
                            for item in items {
                                {item}
                            }
                        }
                    }
                }
            }
        }}
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, title: &str, folder: &str, modified: &str) -> CollectionItem {
        CollectionItem {
            path: path.to_string(),
            title: title.to_string(),
            folder: folder.to_string(),
            modified_at: modified.to_string(),
            pinned: false,
        }
    }

    #[test]
    fn date_key_extracts_yyyy_mm_dd() {
        assert_eq!(date_key("2024-01-15T10:30:00Z"), "2024-01-15");
        assert_eq!(date_key("2024-12-31"), "2024-12-31");
        assert_eq!(date_key(""), "no-date");
        assert_eq!(date_key("short"), "no-date");
    }

    #[test]
    fn group_by_date_sorts_keys_ascending() {
        let items = vec![
            item("a.md", "A", "", "2024-03-01T00:00:00Z"),
            item("b.md", "B", "", "2024-01-01T00:00:00Z"),
            item("c.md", "C", "", "2024-02-01T00:00:00Z"),
        ];
        let groups = group_by_date(&items, "");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].0, "2024-01-01");
        assert_eq!(groups[1].0, "2024-02-01");
        assert_eq!(groups[2].0, "2024-03-01");
    }

    #[test]
    fn group_by_date_filters_by_query() {
        let items = vec![
            item("a.md", "Alpha", "", "2024-01-01T00:00:00Z"),
            item("b.md", "Beta", "", "2024-01-01T00:00:00Z"),
        ];
        let groups = group_by_date(&items, "alpha");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 1);
    }

    #[test]
    fn calendar_filter_default() {
        let f = CalendarFilter::default();
        assert!(f.query.is_empty());
        assert!(f.object_type.is_none());
        assert_eq!(f.view_mode, CalendarViewMode::Month);
    }

    #[test]
    fn calendar_view_mode_default_is_month() {
        assert_eq!(CalendarViewMode::default(), CalendarViewMode::Month);
    }
}
