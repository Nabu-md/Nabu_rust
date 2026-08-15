//! Board View component (Dioxus).
//!
//! Kanban-style board view with columns, filtering, and drag-and-drop
//! reordering between columns. Views are projections of `Vec<CollectionItem>`.

use crate::components::collections::shared::types::{CollectionItem, BoardFilter};
use crate::components::ui::icons::{render_icon_view, Icon};
use dioxus::prelude::*;
use dioxus::web::WebEventExt;

#[derive(Clone, PartialEq, Default)]
pub struct BoardColumn {
    pub id: String,
    pub title: String,
    pub items: Vec<CollectionItem>,
}

#[derive(Clone, PartialEq, Default)]
pub struct BoardFilter {
    pub query: String,
    pub object_type: Option<String>,
    pub group_by: String,
}

#[derive(Props, PartialEq)]
pub struct BoardViewProps {
    pub objects: Vec<CollectionItem>,
    pub columns: Vec<BoardColumn>,
    pub filter: BoardFilter,
    pub on_filter_change: EventHandler<BoardFilter>,
    pub on_move_item: EventHandler<(String, String)>,
    pub on_open: EventHandler<String>,
}

/// Groups items for the board view. With `CollectionItem` we group by
/// `folder` (the only categorical field available); a fallback "root" bucket
/// catches items with no folder.
fn group_items(items: &[CollectionItem], group_by: &str, query: &str) -> Vec<BoardColumn> {
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

    if group_by == "folder" {
        let mut groups: std::collections::BTreeMap<String, Vec<CollectionItem>> =
            std::collections::BTreeMap::new();
        for obj in &filtered {
            let key = if obj.folder.is_empty() {
                "(root)".to_string()
            } else {
                obj.folder.clone()
            };
            groups.entry(key).or_default().push((*obj).clone());
        }

        groups
            .into_iter()
            .map(|(k, items)| BoardColumn {
                id: k.clone(),
                title: k,
                items,
            })
            .collect()
    } else {
        vec![BoardColumn {
            id: "all".to_string(),
            title: "All".to_string(),
            items: filtered.into_iter().cloned().collect(),
        }]
    }
}

#[component]
pub fn BoardView(props: &BoardViewProps) -> Element {
    let filter = props.filter.clone();
    let on_open = props.on_open;
    let on_move_item = props.on_move_item;

    let columns_data: Vec<BoardColumn> = if props.columns.is_empty() {
        group_items(&props.objects, &filter.group_by, &filter.query)
    } else {
        props.columns.clone()
    };

    let on_drag_start = move |ev: DragEvent, item_id: String| {
        let web = ev.data().as_web_event();
        if let Some(dt) = web.data_transfer() {
            let _ = dt.set_data("text/plain", &item_id);
        }
    };

    let on_drag_over = move |ev: DragEvent| {
        ev.prevent_default();
    };

    let on_drop = move |ev: DragEvent, column_id: String| {
        ev.prevent_default();
        let web = ev.data().as_web_event();
        if let Some(dt) = web.data_transfer() {
            if let Ok(data) = dt.get_data("text/plain") {
                if !data.is_empty() {
                    on_move_item.call((data, column_id));
                }
            }
        }
    };

    let column_elements: Vec<Element> = columns_data
        .iter()
        .map(|col| {
            let col_title = col.title.clone();
            let col_count = col.items.len();
            let column_id = col.id.clone();
            let col_items: Vec<Element> = col
                .items
                .iter()
                .map(|obj| {
                    let obj = obj.clone();
                    let object_id = obj.path.clone();
                    let title = obj.title.clone();
                    let obj_type = obj.folder.clone();
                    let on_open = on_open;
                    let on_drag_start = &on_drag_start;

                    rsx! {
                        div {
                            class: "bg-gray-700 rounded p-2 border border-gray-600 cursor-grab hover:border-gray-500 transition-colors",
                            draggable: "true",
                            ondragstart: move |ev: DragEvent| on_drag_start(ev, object_id.clone()),
                            onclick: move |_: MouseEvent| on_open.call(obj.path.clone()),
                        }
                        div { class: "text-sm font-medium text-gray-200", "{title}" }
                        div { class: "text-xs text-gray-500 mt-1", "{obj_type}" }
                    }
                })
                .collect();

            rsx! {
                div {
                    class: "flex-none w-72 bg-gray-800 rounded-lg border border-gray-700 flex flex-col max-h-full",
                    ondragover: on_drag_over,
                    ondrop: move |ev: DragEvent| on_drop(ev, column_id.clone()),
                }
                div { class: "px-3 py-2 border-b border-gray-700 flex items-center justify-between" }
                span { class: "text-sm font-medium text-gray-300", "{col_title}" }
                span { class: "text-xs text-gray-500", "{col_count}" }

                div { class: "flex-1 overflow-y-auto p-2 space-y-2" }
                for item in col_items {
                    {item}
                }
            }
        })
        .collect();

    rsx! {
        div { class: "board-view flex gap-4 overflow-x-auto p-4 h-full" }
        for col in column_elements {
            {col}
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, title: &str, folder: &str) -> CollectionItem {
        CollectionItem {
            path: path.to_string(),
            title: title.to_string(),
            folder: folder.to_string(),
            modified_at: "2024-01-01".to_string(),
            pinned: false,
        }
    }

    #[test]
    fn group_by_folder_groups_correctly() {
        let items = vec![
            item("a.md", "A", ""),
            item("sub/b.md", "B", "sub"),
            item("sub/c.md", "C", "sub"),
            item("other/x.md", "X", "other"),
        ];
        let groups = group_items(&items, "folder", "");
        assert_eq!(groups.len(), 3);
        let by_id: std::collections::HashMap<_, _> = groups
            .iter()
            .map(|g| (g.id.as_str(), g.items.len()))
            .collect();
        assert_eq!(by_id.get("(root)"), Some(&1));
        assert_eq!(by_id.get("sub"), Some(&2));
        assert_eq!(by_id.get("other"), Some(&1));
    }

    #[test]
    fn group_by_folder_filters_by_query() {
        let items = vec![
            item("a.md", "Alpha", ""),
            item("sub/b.md", "Beta", "sub"),
        ];
        let groups = group_items(&items, "folder", "alpha");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "(root)");
        assert_eq!(groups[0].items.len(), 1);
    }

    #[test]
    fn group_by_other_returns_single_all_column() {
        let items = vec![
            item("a.md", "A", ""),
            item("b.md", "B", "sub"),
        ];
        let groups = group_items(&items, "type", "");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "all");
        assert_eq!(groups[0].items.len(), 2);
    }

    #[test]
    fn board_column_default_is_empty() {
        let col = BoardColumn::default();
        assert!(col.id.is_empty());
        assert!(col.title.is_empty());
        assert!(col.items.is_empty());
    }

    #[test]
    fn board_filter_default() {
        let f = BoardFilter::default();
        assert!(f.query.is_empty());
        assert!(f.object_type.is_none());
        assert!(f.group_by.is_empty());
    }
}
