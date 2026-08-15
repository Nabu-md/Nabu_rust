//! Collection Container component (Dioxus).
//!
//! Manages view state, filtering, and data projection for all four collection
//! views. Data is loaded via the `notes_index` Tauri command and projected
//! into the active view (Table, Board, Gallery, Calendar).
//!
//! Views are projections of existing notes — the container never owns data.

use crate::components::collections::board_view::{BoardColumn, BoardView};
use crate::components::collections::calendar_view::CalendarView;
use crate::components::collections::gallery_view::GalleryView;
use crate::components::collections::shared::types::{
    BoardFilter, CalendarFilter, CalendarViewMode, CollectionItem, CollectionView, GalleryFilter,
    TableFilter,
};
use crate::components::collections::shared::context::SearchState;
use crate::components::collections::table_view::{ColumnConfig, TableView};
use crate::components::collections::view_switcher::ViewSwitcher;
use crate::components::contexts::{open_tab, use_workspace};
use crate::components::ui::feedback::{use_toast, ErrorPanel, SkeletonList, ToastContext};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use dioxus::prelude::*;
use serde_wasm_bindgen;
use wasm_bindgen_futures::spawn_local;

/// Container-level load state classification (mirrors reading_queue's pattern).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoadPhase {
    Loading,
    Error,
    Empty,
    Ready,
}

fn classify(loaded: bool, had_error: bool, items: &[CollectionItem]) -> LoadPhase {
    if !loaded {
        return LoadPhase::Loading;
    }
    if had_error {
        return LoadPhase::Error;
    }
    if items.is_empty() {
        LoadPhase::Empty
    } else {
        LoadPhase::Ready
    }
}

/// Loads the note index from the backend via `notes_index`.
fn load_items(
    mut items: Signal<Vec<CollectionItem>>,
    mut loaded: Signal<bool>,
    mut had_error: Signal<bool>,
    toasts: ToastContext,
) {

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap_or_default();
        match crate::ipc::tauri_invoke_safe("notes_index", args).await {
            Ok(Some(val)) => {
                match serde_wasm_bindgen::from_value::<Vec<CollectionItem>>(val) {
                    Ok(notes) => {
                        items.set(notes);
                    }
                    Err(e) => {
                        had_error.set(true);
                        toasts.error(
                            "Couldn't load collections",
                            &format!("The note index could not be parsed: {}", e),
                        );
                    }
                }
            }
            Ok(None) => {
                had_error.set(true);
                toasts.error("Couldn't load collections", "The note index returned no data.");
            }
            Err(e) => {
                had_error.set(true);
                toasts.error("Couldn't load collections", &e.message());
            }
        }
        loaded.set(true);
    });
}

#[component]
pub fn CollectionContainer() -> Element {
    let ws = use_workspace();
    let toasts = use_toast();

    let mut view = use_signal(|| CollectionView::Table);
    let mut search_state = use_signal(|| SearchState::default());
    let mut items = use_signal(Vec::<CollectionItem>::new);
    let mut loaded = use_signal(|| false);
    let mut had_error = use_signal(|| false);

    let mut initialized = use_signal(|| false);
    if !*initialized.read() {
        initialized.set(true);
        load_items(items, loaded, had_error, toasts);
    }

    let on_view_change = move |new_view: CollectionView| {
        view.set(new_view);
    };

    let on_query_change = move |ev: FormEvent| {
        search_state.set(SearchState {
            query: ev.value(),
        });
    };

    let on_retry = {
        let items = items;
        let loaded = loaded;
        let had_error = had_error;
        move |_| {
            load_items(items, loaded, had_error, toasts);
        }
    };

    let on_open_tab = move |path: String| {
        open_tab(ws, &path);
    };

    let phase = classify(
        *loaded.read(),
        *had_error.read(),
        &items.read(),
    );

    let search_state_cloned = search_state.read().clone();

    rsx! {
        div { class: "collection-container flex h-full flex-col bg-gray-950 text-gray-100" }

        ViewSwitcher {
            current_view: *view.read(),
            on_change: on_view_change,
        }

        div { class: "flex items-center gap-3 px-4 py-2 border-b border-gray-800" }
        span { class: "text-xs text-gray-500 ml-2", "Search:" }
        input {
            r#type: "text",
            placeholder: "Filter notes...",
            class: "flex-1 bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none",
            value: "{search_state_cloned.query}",
            oninput: on_query_change,
        }

        div { class: "flex-1 overflow-hidden" }

        {match phase {
            LoadPhase::Loading => rsx! {
                div { class: "p-4" }
                SkeletonList { rows: 6 }
            },
            LoadPhase::Error => rsx! {
                div { class: "p-4" }
                ErrorPanel {
                    title: "Couldn't load collections".to_string(),
                    message: "Something went wrong while reading your knowledge objects.".to_string(),
                    details: None,
                    on_retry: on_retry,
                    recovery: "Check that your vault is accessible, then try again.".to_string(),
                }
            },
            LoadPhase::Empty => rsx! {
                div { class: "h-full flex items-center justify-center p-6" }
                EmptyState {
                    icon: Some(Icon::FolderTree),
                    title: "No knowledge objects yet".to_string(),
                    description: "Collections show your structured knowledge once you start adding objects.".to_string(),
                }
            },
            LoadPhase::Ready => {
                let current_view = *view.read();
                let current_items = items.read().clone();
                let current_search = search_state.read().clone();
                let q = current_search.query.clone();

                match current_view {
                    CollectionView::Table => {
                        let columns = vec![
                            ColumnConfig {
                                key: "title".to_string(),
                                label: "Title".to_string(),
                                visible: true,
                                sortable: true,
                                width: Some("flex-1".to_string()),
                            },
                            ColumnConfig {
                                key: "folder".to_string(),
                                label: "Folder".to_string(),
                                visible: true,
                                sortable: true,
                                width: Some("w-40".to_string()),
                            },
                            ColumnConfig {
                                key: "modified".to_string(),
                                label: "Modified".to_string(),
                                visible: true,
                                sortable: true,
                                width: Some("w-32".to_string()),
                            },
                            ColumnConfig {
                                key: "path".to_string(),
                                label: "Path".to_string(),
                                visible: false,
                                sortable: true,
                                width: None,
                            },
                        ];
                        let filter = TableFilter {
                            query: q.clone(),
                            object_type: None,
                            sort_by: "modified".to_string(),
                            sort_ascending: false,
                        };
                        rsx! {
                            TableView {
                                objects: current_items,
                                columns: columns,
                                filter: filter,
                                on_filter_change: move |_: TableFilter| {},
                                on_sort: move |_: (String, bool)| {},
                                on_open: on_open_tab,
                            }
                        }
                    }
                    CollectionView::Board => {
                        let columns = vec![
                            BoardColumn {
                                id: "root".to_string(),
                                title: "Root".to_string(),
                                items: current_items
                                    .iter()
                                    .filter(|i| i.folder.is_empty())
                                    .cloned()
                                    .collect(),
                            },
                            BoardColumn {
                                id: "folder".to_string(),
                                title: "Folders".to_string(),
                                items: current_items
                                    .iter()
                                    .filter(|i| !i.folder.is_empty())
                                    .cloned()
                                    .collect(),
                            },
                            BoardColumn {
                                id: "pinned".to_string(),
                                title: "Pinned".to_string(),
                                items: current_items
                                    .iter()
                                    .filter(|i| i.pinned)
                                    .cloned()
                                    .collect(),
                            },
                        ];
                        let filter = BoardFilter {
                            query: q.clone(),
                            object_type: None,
                            group_by: "folder".to_string(),
                        };
                        rsx! {
                            BoardView {
                                objects: current_items,
                                columns: columns,
                                filter: filter,
                                on_filter_change: move |_: BoardFilter| {},
                                on_move_item: move |_: (String, String)| {},
                                on_open: on_open_tab,
                            }
                        }
                    }
                    CollectionView::Gallery => {
                        let filter = GalleryFilter {
                            query: q.clone(),
                            object_type: None,
                            sort_by: "modified".to_string(),
                            sort_ascending: false,
                        };
                        rsx! {
                            GalleryView {
                                objects: current_items,
                                filter: filter,
                                on_filter_change: move |_: GalleryFilter| {},
                                on_open: on_open_tab,
                            }
                        }
                    }
                    CollectionView::Calendar => {
                        let filter = CalendarFilter {
                            query: q.clone(),
                            object_type: None,
                            view_mode: CalendarViewMode::Month,
                            group_by: "date".to_string(),
                        };
                        rsx! {
                            CalendarView {
                                objects: current_items,
                                filter: filter,
                                on_filter_change: move |_: CalendarFilter| {},
                                on_open: on_open_tab,
                            }
                        }
                    }
                }
            }
        }}
    }
}
