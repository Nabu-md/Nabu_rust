//! Table View component (Dioxus).
//!
//! Column-based table view with filtering, sorting, and configurable columns.
//! Projections operate over `Vec<CollectionItem>` — views never own data.

use crate::components::collections::shared::types::{CollectionItem, TableFilter};
use crate::components::ui::icons::{render_icon_view, Icon};
use dioxus::prelude::*;

/// Column configuration for the table view.
#[derive(Clone, PartialEq)]
pub struct ColumnConfig {
    pub key: String,
    pub label: String,
    pub visible: bool,
    pub sortable: bool,
    pub width: Option<String>,
}

#[derive(Props, PartialEq)]
pub struct TableViewProps {
    pub objects: Vec<CollectionItem>,
    pub columns: Vec<ColumnConfig>,
    pub filter: TableFilter,
    pub on_filter_change: EventHandler<TableFilter>,
    pub on_sort: EventHandler<(String, bool)>,
    pub on_open: EventHandler<String>,
}

/// Projects a sort value from a CollectionItem by column key.
fn get_sort_value(obj: &CollectionItem, key: &str) -> String {
    match key {
        "title" => obj.title.clone(),
        "folder" => obj.folder.clone(),
        "modified" => obj.modified_at.clone(),
        "path" => obj.path.clone(),
        _ => obj.title.clone(),
    }
}

/// Projects a display value from a CollectionItem by column key.
fn get_column_value(obj: &CollectionItem, key: &str) -> String {
    match key {
        "title" => obj.title.clone(),
        "folder" => obj.folder.clone(),
        "modified" => obj.modified_at.clone(),
        "path" => obj.path.clone(),
        _ => obj.title.clone(),
    }
}

#[component]
pub fn TableView(props: &TableViewProps) -> Element {
    let filtered: Vec<CollectionItem> = {
        let f = &props.filter;
        let mut result = props.objects.clone();

        if !f.query.is_empty() {
            let q = f.query.to_lowercase();
            result.retain(|obj| {
                obj.title.to_lowercase().contains(&q)
                    || obj.folder.to_lowercase().contains(&q)
                    || obj.path.to_lowercase().contains(&q)
            });
        }

        if let Some(ref ot) = f.object_type {
            result.retain(|obj| obj.folder == *ot || obj.path.contains(ot));
        }

        if !f.sort_by.is_empty() {
            let sort_key = f.sort_by.clone();
            let asc = f.sort_ascending;
            result.sort_by(|a, b| {
                let a_val = get_sort_value(a, &sort_key);
                let b_val = get_sort_value(b, &sort_key);
                let ord = a_val.cmp(&b_val);
                if asc { ord } else { ord.reverse() }
            });
        }

        result
    };

    let visible_columns: Vec<ColumnConfig> = props
        .columns
        .iter()
        .filter(|c| c.visible)
        .cloned()
        .collect();

    let sort_by = props.filter.sort_by.clone();
    let sort_ascending = props.filter.sort_ascending;
    let on_sort = props.on_sort;
    let on_open = props.on_open;

    let header_cells: Vec<Element> = visible_columns
        .iter()
        .map(|col| {
            let col = col.clone();
            let sortable = col.sortable;
            let sort_key = col.key.clone();
            let is_sorted = sort_by == col.key;

            let class_str = if sortable {
                "px-4 py-3 cursor-pointer hover:text-gray-200".to_string()
            } else {
                "px-4 py-3".to_string()
            };

            let width_str = col.width.clone().unwrap_or_default();
            let full_class = format!("{} {}", class_str, width_str);

            let icon: Option<Element> = if sortable && is_sorted {
                if sort_ascending {
                    Some(rsx! { {render_icon_view(Icon::ChevronUp)} })
                } else {
                    Some(rsx! { {render_icon_view(Icon::ChevronDown)} })
                }
            } else {
                None
            };

            let on_click = move |_: MouseEvent| {
                if sortable {
                    on_sort.call((sort_key.clone(), !is_sorted || !sort_ascending));
                }
            };

            rsx! {
                th { class: full_class, onclick: on_click }
                div { class: "flex items-center gap-1" }
                "{col.label}"
                {icon}
            }
        })
        .collect();

    rsx! {
        div { class: "table-view w-full overflow-auto h-full" }
        table { class: "w-full text-sm text-left text-gray-300" }
        thead { class: "text-xs text-gray-400 uppercase bg-gray-800 border-b border-gray-700" }
        tr {}
        for cell in header_cells {
            {cell}
        }
        tbody { class: "divide-y divide-gray-800" }
        if filtered.is_empty() {
            rsx! {
                tr {}
                td { class: "px-4 py-8 text-center text-gray-500", colspan: "{visible_columns.len()}" }
                "No items to display"
                }
            }
        } else {
            for obj in &filtered {
                {
                    let obj = obj.clone();
                    let on_open = on_open;
                    let cells: Vec<Element> = visible_columns
                        .iter()
                        .map(|col| {
                            let value = get_column_value(&obj, &col.key);
                            let class = if col.key == "title" {
                                "px-4 py-2 text-gray-300 font-medium truncate max-w-xs"
                            } else {
                                "px-4 py-2 text-gray-300"
                            };
                            rsx! { td { class: class, "{value}" } }
                        })
                        .collect();
                    rsx! {
                        tr {
                            class: "hover:bg-gray-800/50 transition-colors",
                            onclick: move |_: MouseEvent| on_open.call(obj.path.clone()),
                        }
                        for cell in cells {
                            {cell}
                        }
                    }
                }
            }
        }
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
    fn get_sort_value_by_key() {
        let i = item("a/b.md", "Title", "a", "2024-01-01");
        assert_eq!(get_sort_value(&i, "title"), "Title");
        assert_eq!(get_sort_value(&i, "folder"), "a");
        assert_eq!(get_sort_value(&i, "modified"), "2024-01-01");
        assert_eq!(get_sort_value(&i, "path"), "a/b.md");
        assert_eq!(get_sort_value(&i, "unknown"), "Title");
    }

    #[test]
    fn get_column_value_matches_sort_value() {
        let i = item("a/b.md", "Title", "a", "2024-01-01");
        for key in ["title", "folder", "modified", "path"] {
            assert_eq!(get_column_value(&i, key), get_sort_value(&i, key));
        }
    }

    #[test]
    fn column_config_defaults() {
        let col = ColumnConfig {
            key: "title".to_string(),
            label: "Title".to_string(),
            visible: true,
            sortable: true,
            width: Some("flex-1".to_string()),
        };
        assert!(col.visible);
        assert!(col.sortable);
        assert_eq!(col.width, Some("flex-1".to_string()));
    }

    #[test]
    fn table_filter_default() {
        let f = TableFilter::default();
        assert!(f.query.is_empty());
        assert!(f.object_type.is_none());
        assert!(f.sort_by.is_empty());
        assert!(!f.sort_ascending);
    }
}
