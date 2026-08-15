//! View switcher component (Dioxus).
//!
//! A row of buttons that lets the user toggle between Table, Board, Gallery,
//! and Calendar views.

use crate::components::collections::shared::types::CollectionView;
use dioxus::prelude::*;

#[derive(Props, PartialEq)]
pub struct ViewSwitcherProps {
    pub current_view: CollectionView,
    pub on_change: EventHandler<CollectionView>,
}

#[component]
pub fn ViewSwitcher(props: &ViewSwitcherProps) -> Element {
    rsx! {
        div { class: "view-switcher flex items-center gap-1 p-2 bg-gray-800 border-b border-gray-700" }
        for view in CollectionView::all() {
            {
                let view = *view;
                let active = props.current_view == view;
                let label = view.label();
                let class = if active {
                    "px-3 py-1.5 text-xs rounded-md bg-blue-600 text-white border border-blue-500"
                } else {
                    "px-3 py-1.5 text-xs rounded-md border border-gray-700 text-gray-400 hover:text-gray-200 hover:border-gray-600"
                };
                rsx! {
                    button {
                        class: class,
                        onclick: move |_| props.on_change.call(view),
                        "{label}"
                    }
                }
            }
        }
    }
}
