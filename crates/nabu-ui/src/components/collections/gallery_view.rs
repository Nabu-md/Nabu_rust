//! Gallery View component (Dioxus).
//!
//! Card-based gallery view with filtering and sorting.
//! Views are projections of `Vec<CollectionItem>` — views never own data.

use crate::components::collections::shared::types::{CollectionItem, GalleryFilter};
use crate::components::ui::icons::{render_icon_view, Icon};
use dioxus::prelude::*;

#[derive(Props, PartialEq, Clone)]
pub struct GalleryViewProps {
    pub objects: Vec<CollectionItem>,
    pub filter: GalleryFilter,
    pub on_filter_change: EventHandler<GalleryFilter>,
    pub on_open: EventHandler<String>,
}

fn get_sort_value(obj: &CollectionItem, key: &str) -> String {
    match key {
        "title" => obj.title.clone(),
        "folder" => obj.folder.clone(),
        "modified" => obj.modified_at.clone(),
        "path" => obj.path.clone(),
        _ => obj.title.clone(),
    }
}

#[component]
pub fn GalleryView(props: GalleryViewProps) -> Element {
    let filter = props.filter.clone();
    let on_open = props.on_open;

    let filtered: Vec<CollectionItem> = {
        let mut result = props.objects.clone();

        if !filter.query.is_empty() {
            let q = filter.query.to_lowercase();
            result.retain(|obj| {
                obj.title.to_lowercase().contains(&q)
                    || obj.folder.to_lowercase().contains(&q)
                    || obj.path.to_lowercase().contains(&q)
            });
        }

        if let Some(ref ot) = filter.object_type {
            result.retain(|obj| obj.folder == *ot);
        }

        if !filter.sort_by.is_empty() {
            let sort_key = filter.sort_by.clone();
            let asc = filter.sort_ascending;
            result.sort_by(|a, b| {
                let a_val = get_sort_value(a, &sort_key);
                let b_val = get_sort_value(b, &sort_key);
                let ord = a_val.cmp(&b_val);
                if asc { ord } else { ord.reverse() }
            });
        }

        result
    };

    let cards: Vec<Element> = filtered
        .iter()
        .map(|obj| {
            let obj = obj.clone();
            let title = obj.title.clone();
            let folder = obj.folder.clone();
            let modified = obj.modified_at.clone();
            let path = obj.path.clone();
            let on_open = on_open;

            rsx! {
                div {
                    class: "bg-gray-800 rounded-lg border border-gray-700 p-4 hover:border-gray-600 transition-colors cursor-pointer",
                    onclick: move |_: MouseEvent| on_open.call(path.clone()),
                }
                div { class: "flex items-start justify-between mb-2" }
                span { class: "text-xs px-2 py-0.5 rounded bg-gray-700 text-gray-300", "{folder}" }
                span { class: "text-xs text-gray-500", "{modified}" }

                h3 { class: "text-sm font-medium text-gray-200 mb-2 line-clamp-2", "{title}" }
                div { class: "text-xs text-gray-500 truncate", "{path}" }
            }
        })
        .collect();

    rsx! {
        div { class: "gallery-view p-4 overflow-y-auto h-full" }
        {if filtered.is_empty() {
            rsx! {
                div { class: "flex items-center justify-center h-64 text-gray-500" }
                "No items to display"
            }
        } else {
            rsx! {
                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4" }
                for card in cards {
                    {card}
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
    fn get_sort_value_by_key() {
        let i = item("a/b.md", "Title", "a", "2024-01-01");
        assert_eq!(get_sort_value(&i, "title"), "Title");
        assert_eq!(get_sort_value(&i, "folder"), "a");
        assert_eq!(get_sort_value(&i, "modified"), "2024-01-01");
        assert_eq!(get_sort_value(&i, "path"), "a/b.md");
        assert_eq!(get_sort_value(&i, "unknown"), "Title");
    }

    #[test]
    fn gallery_filter_default() {
        let f = GalleryFilter::default();
        assert!(f.query.is_empty());
        assert!(f.object_type.is_none());
        assert!(f.sort_by.is_empty());
        assert!(!f.sort_ascending);
    }
}
