//! Shared context type for the Collections module.
//!
//! `SearchState` is a plain struct (no Leptos signals) so it can be stored
//! in a Dioxus `Signal` and passed by value to view components.

/// Lightweight search + grouping state projected from a single source
/// (the `notes_index` payload loaded by the container).
#[derive(Clone, PartialEq, Default, Debug)]
pub struct SearchState {
    pub query: String,
}
