//! # Right Inspector — property metadata, backlinks, outgoing links, and unlinked mentions
//!
//! Renders a tab bar (Tags / Backlinks / Outgoing / Mentions) backed by the
//! `note_links` Tauri command. The Tags tab shows the note's frontmatter tags
//! as a badge list alongside the [`PropertyEditor`]. The remaining tabs render
//! real link data instead of static placeholder content.
//!
//! Data is reloaded via `use_effect` whenever [`WorkspaceContext::active_path`]
//! changes, following the same IPC pattern used by `note_editor.rs` and
//! `shipped/reader.rs`.

use crate::components::contexts::{open_tab, use_workspace, WorkspaceContext};
use crate::components::property_editor::PropertyEditor;
use crate::components::ui::feedback::{use_toast, SkeletonList, ErrorPanel, ToastContext};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use crate::components::ui::nav::{TabDef, Tabs};
use crate::models::graph::{MentionEntry, NoteLinks, OutgoingLink};
use crate::models::properties::{PropertyDefinition, PropertyValue};
use dioxus::prelude::*;
use serde_wasm_bindgen;
use wasm_bindgen_futures::spawn_local;

use std::collections::HashMap;

// ── Load-state classification ──────────────────────────────────────────

/// Lifecycle phase of the link-data view, derived purely from load state + payload.
///
/// A failed load is never collapsed into an empty result: the error is surfaced
/// so the user can distinguish "no link data" from "could not load."
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LinkLoadPhase {
    /// No note is currently active in the workspace.
    NoNote,
    /// Note links are being fetched from the backend.
    Loading,
    /// The IPC call failed and the error is surfaced.
    Error,
    /// Data loaded but the active tab has no entries.
    Empty,
    /// Data loaded with entries to display.
    Ready,
}

/// Classifies the current inspector phase based on signals and the active tab.
///
/// Priority: Loading > Error > NoNote > Empty > Ready.
fn classify_links_state(
    loading: bool,
    had_error: bool,
    links: Option<&NoteLinks>,
    tab: &str,
) -> LinkLoadPhase {
    if loading {
        return LinkLoadPhase::Loading;
    }
    if had_error {
        return LinkLoadPhase::Error;
    }
    let Some(links) = links else {
        return LinkLoadPhase::NoNote;
    };
    let is_empty = match tab {
        "tags" => links.tags.is_empty(),
        "backlinks" => links.backlinks.is_empty(),
        "outgoing" => links.outgoing.is_empty(),
        "mentions" => links.mentions.is_empty(),
        _ => true,
    };
    if is_empty {
        LinkLoadPhase::Empty
    } else {
        LinkLoadPhase::Ready
    }
}

// ── Signal bundle ──────────────────────────────────────────────────────

/// All reactive state owned by [`RightInspector`], bundled so the IPC helper
/// functions stay single-argument and testable.
#[derive(Clone, Copy)]
struct LinkInspectorState {
    links: Signal<Option<NoteLinks>>,
    loading: Signal<bool>,
    error: Signal<Option<String>>,
    toasts: ToastContext,
}

// ── IPC helpers ────────────────────────────────────────────────────────

/// Fetches backlinks, outgoing links, unlinked mentions, and tags for the note
/// at `path` via the `note_links` Tauri command. Updates `state` signals and
/// surfaces failures as toasts.
fn load_note_links(path: String, state: LinkInspectorState) {
    *state.loading.write_unchecked() = true;
    *state.error.write_unchecked() = None;

    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "path": path }))
            .unwrap();
        match crate::ipc::tauri_invoke_safe("note_links", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<NoteLinks>(val) {
                Ok(note_links) => {
                    *state.links.write_unchecked() = Some(note_links);
                    *state.loading.write_unchecked() = false;
                }
                Err(e) => {
                    let msg = format!("Could not parse note links: {e}");
                    *state.error.write_unchecked() = Some(msg.clone());
                    *state.loading.write_unchecked() = false;
                    *state.links.write_unchecked() = None;
                    state.toasts.error("Links failed", msg);
                }
            },
            Ok(None) => {
                *state.error.write_unchecked() =
                    Some("Note links request returned no data.".to_string());
                *state.loading.write_unchecked() = false;
                *state.links.write_unchecked() = None;
                state
                    .toasts
                    .error("Links failed", "No link data was returned by the backend.");
            }
            Err(e) => {
                let msg = e.message();
                *state.error.write_unchecked() = Some(msg.clone());
                *state.loading.write_unchecked() = false;
                *state.links.write_unchecked() = None;
                state.toasts.error("Links failed", msg);
            }
        }
    });
}

// ── Component ──────────────────────────────────────────────────────────

/// The Right Inspector panel.
///
/// Renders the inspector dock shell with the Tags / Backlinks / Outgoing /
/// Mentions tab bar. Link data is fetched via the `note_links` Tauri command
/// and re-fetched whenever the workspace's active path changes.
#[component]
pub fn RightInspector() -> Element {
    let ws = use_workspace();
    let toasts = use_toast();

    let active_tab = use_signal(|| "tags".to_string());

    let tabs = vec![
        TabDef::new("tags", "Tags").with_icon(Icon::Tag),
        TabDef::new("backlinks", "Backlinks").with_icon(Icon::Link),
        TabDef::new("outgoing", "Outgoing").with_icon(Icon::Forward),
        TabDef::new("mentions", "Mentions").with_icon(Icon::MessageCircle),
    ];

    // Note-link state signals.
    let links = use_signal(|| None::<NoteLinks>);
    let loading = use_signal(|| false);
    let error = use_signal(|| None::<String>);

    let link_state = LinkInspectorState {
        links,
        loading,
        error,
        toasts,
    };

    // Reload note links on mount and whenever the active path changes.
    {
        let state_c = link_state;
        let ws_c = ws;

        use_effect(move || {
            let path = ws_c.active_path.read().clone().unwrap_or_default();
            if path.is_empty() {
                *state_c.links.write_unchecked() = None;
                return;
            }
            load_note_links(path, state_c);
        });
    }

    // Standard property set for notes.
    let properties = vec![
        PropertyDefinition {
            id: "title".to_string(),
            display_name: "Title".to_string(),
            property_type: crate::models::properties::PropertyType::Text,
            description: Some("The note's display title.".to_string()),
            default_value: None,
            options: None,
        },
        PropertyDefinition {
            id: "created".to_string(),
            display_name: "Created".to_string(),
            property_type: crate::models::properties::PropertyType::Date,
            description: Some("When this note was first created.".to_string()),
            default_value: None,
            options: None,
        },
        PropertyDefinition {
            id: "modified".to_string(),
            display_name: "Modified".to_string(),
            property_type: crate::models::properties::PropertyType::Date,
            description: Some("When this note was last saved.".to_string()),
            default_value: None,
            options: None,
        },
        PropertyDefinition {
            id: "tags".to_string(),
            display_name: "Tags".to_string(),
            property_type: crate::models::properties::PropertyType::MultiSelect,
            description: Some("Categories for this note.".to_string()),
            default_value: None,
            options: Some(vec![
                "work".to_string(),
                "personal".to_string(),
                "reference".to_string(),
            ]),
        },
        PropertyDefinition {
            id: "status".to_string(),
            display_name: "Status".to_string(),
            property_type: crate::models::properties::PropertyType::Select,
            description: Some("Current workflow state.".to_string()),
            default_value: None,
            options: Some(vec![
                "draft".to_string(),
                "in progress".to_string(),
                "done".to_string(),
            ]),
        },
        PropertyDefinition {
            id: "url".to_string(),
            display_name: "URL".to_string(),
            property_type: crate::models::properties::PropertyType::Url,
            description: Some("Source URL (for captured notes).".to_string()),
            default_value: None,
            options: None,
        },
    ];

    let empty_values = use_signal(HashMap::<String, PropertyValue>::new);

    rsx! {
        div {
            class: "right-inspector w-64 border-l border-gray-700 bg-gray-900 h-screen flex flex-col transition-[width] duration-slow ease-standard",

            div { class: "flex border-b border-gray-700" }

            Tabs {
                tabs: tabs,
                active: active_tab,
                on_change: None,
            }

            div { class: "flex-1 overflow-y-auto p-4 text-gray-300 text-sm" }
            {inspect_content(
                &active_tab.read(),
                link_state,
                properties,
                empty_values,
                ws,
            )}
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn icon_for_tab(tab: &str) -> Icon {
    match tab {
        "tags" => Icon::Tag,
        "backlinks" => Icon::Link,
        "outgoing" => Icon::Forward,
        "mentions" => Icon::MessageCircle,
        _ => Icon::Tag,
    }
}

fn no_note_desc_for_tab(tab: &str) -> &'static str {
    match tab {
        "tags" => "Open a note to see its tags and properties.",
        "backlinks" => "Open a note to see its backlinks.",
        "outgoing" => "Open a note to see its outgoing links.",
        "mentions" => "Open a note to see its unlinked mentions.",
        _ => "Open a note to see its link data.",
    }
}

fn empty_title_for_tab(tab: &str) -> &'static str {
    match tab {
        "tags" => "No tags",
        "backlinks" => "No backlinks",
        "outgoing" => "No outgoing links",
        "mentions" => "No unlinked mentions",
        _ => "No data",
    }
}

fn empty_desc_for_tab(tab: &str) -> &'static str {
    match tab {
        "tags" => "This note has no tags in its frontmatter.",
        "backlinks" => "Nothing links to this note yet.",
        "outgoing" => "This note has no outgoing links.",
        "mentions" => "No unlinked mentions were found.",
        _ => "No data to show.",
    }
}

/// Renders content for the active inspector tab.
fn inspect_content(
    tab: &str,
    state: LinkInspectorState,
    properties: Vec<PropertyDefinition>,
    values: Signal<HashMap<String, PropertyValue>>,
    ws: WorkspaceContext,
) -> Element {
    let links_val = state.links.read().clone();
    let loading_val = *state.loading.read();
    let error_val = state.error.read().clone();

    let had_error = error_val.is_some();
    let phase = classify_links_state(loading_val, had_error, links_val.as_ref(), tab);

    match phase {
        LinkLoadPhase::Loading => rsx! { SkeletonList { rows: 5 } },

        LinkLoadPhase::Error => rsx! {
            ErrorPanel {
                title: "Could not load note links".to_string(),
                message: error_val,
                on_retry: {
                    let ws_retry = ws;
                    let state_retry = state;
                    move |_: ()| {
                        let path = ws_retry.active_path.read().clone().unwrap_or_default();
                        if !path.is_empty() {
                            load_note_links(path, state_retry);
                        }
                    }
                },
                recovery: Some("The note may not exist or the backend is unavailable.".to_string()),
            }
        },

        LinkLoadPhase::NoNote => rsx! {
            EmptyState {
                icon: Some(icon_for_tab(tab)),
                title: "No note selected".to_string(),
                description: Some(no_note_desc_for_tab(tab).to_string()),
            }
        },

        LinkLoadPhase::Empty => rsx! {
            EmptyState {
                icon: Some(icon_for_tab(tab)),
                title: empty_title_for_tab(tab).to_string(),
                description: Some(empty_desc_for_tab(tab).to_string()),
            }
        },

        LinkLoadPhase::Ready => {
            let links = links_val.as_ref().unwrap();
            match tab {
                "tags" => {
                    let tags = &links.tags;
                    let tags_section = if tags.is_empty() {
                        rsx! {
                            span { class: "text-xs text-gray-500", "No tags" }
                        }
                    } else {
                        rsx! {
                            div { class: "flex flex-wrap gap-1 mt-1" }
                            for tag in tags {
                                span {
                                    class: "px-2 py-0.5 text-xs rounded bg-gray-800 text-gray-300",
                                    "#{tag}",
                                }
                            }
                        }
                    };
                    rsx! {
                        div { class: "space-y-3" }
                        div {
                            div { class: "text-xs font-semibold uppercase tracking-wider text-gray-500", "Tags" }
                            {tags_section}
                        }
                        div { class: "text-xs font-semibold uppercase tracking-wider text-gray-500 pt-3", "Properties" }
                        PropertyEditor {
                            properties: properties,
                            values: values.read().clone(),
                            on_change: None,
                            on_validate: None,
                        }
                    }
                }

                "backlinks" => {
                    let entries = &links.backlinks;
                    if entries.is_empty() {
                        rsx! {
                            EmptyState {
                                icon: Some(Icon::Link),
                                title: "No backlinks".to_string(),
                                description: Some("Nothing links to this note yet.".to_string()),
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "space-y-1.5" }
                            for entry in entries {
                                let click_path = entry.path.clone();
                                let ws_click = ws;
                                div {
                                    class: "backlink-entry border-b border-gray-800 pb-2 last:border-0",
                                    div { class: "flex items-center justify-between" }
                                    div { class: "flex items-center gap-1.5" }
                                    {render_icon_view(Icon::FileText)}
                                    span {
                                        class: "text-blue-400 hover:text-blue-300 cursor-pointer text-sm font-medium",
                                        onclick: move |_: MouseEvent| { open_tab(ws_click, &click_path); },
                                        "{entry.title}",
                                    }
                                    span { class: "text-xs text-gray-500", "{entry.folder}" }
                                    span { class: "text-xs bg-gray-800 text-gray-400 rounded px-1.5 py-0.25", "x{entry.count}" }
                                    if !entry.snippet.is_empty() {
                                        div { class: "mt-1 text-xs text-gray-500", "{entry.snippet}" }
                                    }
                                }
                            }
                        }
                    }
                }

                "outgoing" => {
                    let entries = &links.outgoing;
                    if entries.is_empty() {
                        rsx! {
                            EmptyState {
                                icon: Some(Icon::Forward),
                                title: "No outgoing links".to_string(),
                                description: Some("This note has no outgoing links.".to_string()),
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "space-y-1.5" }
                            for link in entries {
                                let ws_click = ws;
                                let click_path = link.path.clone().unwrap_or_default();
                                let link_icon = match link.kind.as_str() {
                                    "external" => Icon::ExternalLink,
                                    "broken" => Icon::CircleX,
                                    _ => Icon::Link2,
                                };
                                let kind_class = match link.kind.as_str() {
                                    "external" => "text-xs px-1.5 py-0.25 rounded bg-blue-900/30 text-blue-400",
                                    "broken" => "text-xs px-1.5 py-0.25 rounded bg-red-900/30 text-red-400",
                                    _ => "text-xs px-1.5 py-0.25 rounded bg-gray-800 text-gray-400",
                                };
                                let is_internal = link.kind == "internal" && link.path.is_some();
                                div {
                                    class: "outgoing-link border-b border-gray-800 pb-2 last:border-0",
                                    div { class: "flex items-center justify-between" }
                                    div { class: "flex items-center gap-1.5" }
                                    {render_icon_view(link_icon)}
                                    if is_internal {
                                        span {
                                            class: "text-blue-400 hover:text-blue-300 cursor-pointer text-sm",
                                            onclick: move |_: MouseEvent| { open_tab(ws_click, &click_path); },
                                            "{link.target}",
                                        }
                                    } else {
                                        span { class: "text-sm break-all", "{link.target}" }
                                    }
                                    span { class: kind_class, "{link.kind}" }
                                    span { class: "text-xs text-gray-500", "x{link.count}" }
                                }
                            }
                        }
                    }
                }

                "mentions" => {
                    let entries = &links.mentions;
                    if entries.is_empty() {
                        rsx! {
                            EmptyState {
                                icon: Some(Icon::MessageCircle),
                                title: "No unlinked mentions".to_string(),
                                description: Some("No plain-text references to existing notes were found.".to_string()),
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "space-y-1.5" }
                            for entry in entries {
                                let click_path = entry.path.clone();
                                let ws_click = ws;
                                div {
                                    class: "mention-entry border-b border-gray-800 pb-2 last:border-0",
                                    div { class: "flex items-center justify-between" }
                                    div { class: "flex items-center gap-1.5" }
                                    {render_icon_view(Icon::MessageCircle)}
                                    if click_path.is_empty() {
                                        span { class: "text-sm", "{entry.title}" }
                                    } else {
                                        span {
                                            class: "text-blue-400 hover:text-blue-300 cursor-pointer text-sm font-medium",
                                            onclick: move |_: MouseEvent| { open_tab(ws_click, &click_path); },
                                            "{entry.title}",
                                        }
                                    }
                                    span { class: "text-xs text-gray-500", "score: {entry.score}" }
                                    if !entry.snippet.is_empty() {
                                        div { class: "mt-1 text-xs text-gray-500", "{entry.snippet}" }
                                    }
                                }
                            }
                        }
                    }
                }

                _ => rsx! {},
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::graph::{BacklinkEntry, MentionEntry, OutgoingLink};

    fn make_links(backlinks: usize, outgoing: usize, mentions: usize, tags: usize) -> NoteLinks {
        NoteLinks {
            backlinks: (0..backlinks)
                .map(|i| BacklinkEntry {
                    path: format!("note_{i}.md"),
                    title: format!("Note {i}"),
                    folder: String::new(),
                    snippet: String::new(),
                    match_start: 0,
                    match_end: 0,
                    count: 1,
                })
                .collect(),
            outgoing: (0..outgoing)
                .map(|i| OutgoingLink {
                    kind: "internal".to_string(),
                    target: format!("Note {i}"),
                    path: Some(format!("note_{i}.md")),
                    count: 1,
                })
                .collect(),
            mentions: (0..mentions)
                .map(|i| MentionEntry {
                    title: format!("Note {i}"),
                    path: format!("note_{i}.md"),
                    snippet: String::new(),
                    match_start: 0,
                    match_end: 0,
                    score: 10,
                })
                .collect(),
            tags: (0..tags).map(|i| format!("tag_{i}")).collect(),
        }
    }

    // ── Phase classification ───────────────────────────────────────────

    #[test]
    fn classify_loading_when_loading() {
        assert_eq!(
            classify_links_state(true, false, None, "backlinks"),
            LinkLoadPhase::Loading
        );
    }

    #[test]
    fn classify_loading_takes_precedence_over_error() {
        assert_eq!(
            classify_links_state(true, true, None, "tags"),
            LinkLoadPhase::Loading
        );
    }

    #[test]
    fn classify_error_when_not_loading_but_error() {
        assert_eq!(
            classify_links_state(false, true, None, "backlinks"),
            LinkLoadPhase::Error
        );
    }

    #[test]
    fn classify_no_note_when_no_data() {
        assert_eq!(
            classify_links_state(false, false, None, "tags"),
            LinkLoadPhase::NoNote
        );
    }

    #[test]
    fn classify_empty_for_empty_backlinks() {
        let links = make_links(0, 0, 0, 0);
        assert_eq!(
            classify_links_state(false, false, Some(&links), "backlinks"),
            LinkLoadPhase::Empty
        );
    }

    #[test]
    fn classify_ready_for_populated_backlinks() {
        let links = make_links(3, 0, 0, 0);
        assert_eq!(
            classify_links_state(false, false, Some(&links), "backlinks"),
            LinkLoadPhase::Ready
        );
    }

    #[test]
    fn classify_empty_tags_but_ready_backlinks() {
        let links = make_links(3, 0, 0, 0);
        assert_eq!(
            classify_links_state(false, false, Some(&links), "tags"),
            LinkLoadPhase::Empty
        );
    }

    #[test]
    fn classify_ready_for_outgoing_links() {
        let links = make_links(0, 2, 0, 1);
        assert_eq!(
            classify_links_state(false, false, Some(&links), "outgoing"),
            LinkLoadPhase::Ready
        );
    }

    #[test]
    fn classify_ready_for_tags() {
        let links = make_links(0, 0, 0, 2);
        assert_eq!(
            classify_links_state(false, false, Some(&links), "tags"),
            LinkLoadPhase::Ready
        );
    }

    #[test]
    fn classify_empty_for_empty_mentions() {
        let links = make_links(0, 0, 0, 1);
        assert_eq!(
            classify_links_state(false, false, Some(&links), "mentions"),
            LinkLoadPhase::Empty
        );
    }

    #[test]
    fn classify_ready_for_mentions() {
        let links = make_links(0, 0, 5, 0);
        assert_eq!(
            classify_links_state(false, false, Some(&links), "mentions"),
            LinkLoadPhase::Ready
        );
    }

    // ── Helper functions ───────────────────────────────────────────────

    #[test]
    fn icon_for_tab_returns_correct_icons() {
        assert_eq!(icon_for_tab("tags"), Icon::Tag);
        assert_eq!(icon_for_tab("backlinks"), Icon::Link);
        assert_eq!(icon_for_tab("outgoing"), Icon::Forward);
        assert_eq!(icon_for_tab("mentions"), Icon::MessageCircle);
        assert_eq!(icon_for_tab("unknown"), Icon::Tag);
    }

    #[test]
    fn empty_title_for_tab_returns_correct_titles() {
        assert_eq!(empty_title_for_tab("tags"), "No tags");
        assert_eq!(empty_title_for_tab("backlinks"), "No backlinks");
        assert_eq!(empty_title_for_tab("outgoing"), "No outgoing links");
        assert_eq!(empty_title_for_tab("mentions"), "No unlinked mentions");
    }

    #[test]
    fn no_note_desc_for_tab_returns_correct_descriptions() {
        assert_eq!(
            no_note_desc_for_tab("tags"),
            "Open a note to see its tags and properties."
        );
        assert_eq!(no_note_desc_for_tab("backlinks"), "Open a note to see its backlinks.");
    }
}
