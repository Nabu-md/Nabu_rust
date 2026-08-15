//! # Reader View (Dioxus) — shipped
//!
//! Distraction-free reading experience. Loads the active note via the
//! `note_read` backend IPC and renders it as formatted HTML through a
//! lightweight inline markdown renderer. Reader preferences (font size,
//! line width, theme, focus mode) are persisted via the settings store.
//!
//! Load lifecycle mirrors `note_editor.rs`: `note_read` returns an empty string
//! for a non-existent note (treated as a new/missing note, not an error). A
//! race-safety nonce guards against stale IPC results when the active path
//! changes faster than a load completes.

use crate::components::contexts::use_workspace;
use crate::components::recovery::LoadState;
use crate::components::ui::feedback::{ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

// ── Reader settings (persisted) ────────────────────────────────────────────

const READER_SETTINGS_KEY: &str = "nabu.reader.settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReaderSettings {
    #[serde(default = "default_font_size")]
    font_size: u32,
    #[serde(default = "default_line_width")]
    line_width: u32,
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default)]
    focus_mode: bool,
}

fn default_font_size() -> u32 {
    18
}
fn default_line_width() -> u32 {
    720
}
fn default_theme() -> String {
    "dark".to_string()
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            line_width: default_line_width(),
            theme: default_theme(),
            focus_mode: false,
        }
    }
}

fn persist_reader_settings(settings: &ReaderSettings) {
    let value = serde_json::to_value(settings).unwrap_or_default();
    let args = serde_wasm_bindgen::to_value(
        &serde_json::json!({ "key": READER_SETTINGS_KEY, "value": value }),
    )
    .unwrap();
    spawn_local(async move {
        let _ = crate::ipc::tauri_invoke("settings_set", args).await;
    });
}

// ── Phase classification ────────────────────────────────────────────────────

/// Read-phase of the Reader content area.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReaderPhase {
    Loading,
    Empty,
    Error,
    Loaded,
}

/// Pure-classification: maps reactive signal states to a single phase.
///
/// * `loaded`  — true when the load state is `LoadState::Loaded`.
/// * `note_missing` — true when the note was not found on disk.
/// * `load_error` — `Some(msg)` with a non-empty string on failure.
/// Error takes precedence over every other state.
pub fn classify_reader_phase(
    loaded: bool,
    note_missing: bool,
    load_error: Option<&str>,
) -> ReaderPhase {
    if let Some(e) = load_error {
        if !e.is_empty() {
            return ReaderPhase::Error;
        }
    }
    if !loaded {
        return ReaderPhase::Loading;
    }
    if note_missing {
        return ReaderPhase::Empty;
    }
    ReaderPhase::Loaded
}

// ── Markdown renderer (pure Rust — no Leptos/Dioxus dependency) ─────────────

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn find_double(chars: &[char], start: usize, marker: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == marker && chars[i + 1] == marker {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_single(chars: &[char], start: usize, marker: char) -> Option<usize> {
    chars[start..].iter().position(|&c| c == marker).map(|p| start + p)
}

/// Renders inline markdown: bold, italic, code, links, wikilinks.
fn render_inline(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Bold: **text** or __text__
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_double(&chars, i + 2, '*') {
                let inner: String = chars[i + 2..end].iter().collect();
                result.push_str(&format!("<strong>{}</strong>", render_inline(&inner)));
                i = end + 2;
                continue;
            }
        }
        // Italic: *text* or _text_
        if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            if let Some(end) = find_single(&chars, i + 1, marker) {
                let inner: String = chars[i + 1..end].iter().collect();
                result.push_str(&format!("<em>{}</em>", render_inline(&inner)));
                i = end + 1;
                continue;
            }
        }
        // Inline code: `text`
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                result.push_str(&format!("<code class=\"inline-code\">{}</code>", inner));
                i = i + 1 + end + 1;
                continue;
            }
        }
        // Wikilink: [[text]]
        if i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == '[' {
            if let Some(close) = chars[i + 2..].windows(2).position(|w| w == [']', ']']) {
                let end = i + 2 + close;
                let inner: String = chars[i + 2..end].iter().collect();
                let target = inner.split('|').next().unwrap_or(&inner).to_string();
                result.push_str(&format!(
                    "<a class=\"wikilink\" href=\"#\" data-path=\"{}\">{}</a>",
                    target, inner
                ));
                i = end + 2;
                continue;
            }
        }
        // Link: [text](url)
        if chars[i] == '[' {
            if let Some(close) = chars[i + 1..].iter().position(|&c| c == ']') {
                if chars.get(i + 1 + close + 1) == Some(&'(') {
                    if let Some(close_paren) =
                        chars[i + 1 + close + 2..].iter().position(|&c| c == ')')
                    {
                        let text_part: String = chars[i + 1..i + 1 + close].iter().collect();
                        let url_part: String =
                            chars[i + 1 + close + 2..i + 1 + close + 2 + close_paren]
                                .iter()
                                .collect();
                        result.push_str(&format!(
                            "<a href=\"{}\" target=\"_blank\" rel=\"noopener\">{}</a>",
                            url_part, text_part
                        ));
                        i = i + 1 + close + 2 + close_paren + 1;
                        continue;
                    }
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Renders a markdown string as safe HTML. Escapes HTML entities first,
/// then applies formatting — so no raw HTML can be injected.
fn render_markdown(md: &str) -> String {
    let escaped = html_escape(md);
    let mut html = String::with_capacity(escaped.len() * 2);
    let mut in_code_block = false;
    let mut in_table = false;
    let mut table_header_done = false;
    let mut lines = escaped.lines().peekable();

    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            if in_code_block {
                html.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                let lang = line.trim_start_matches('`').trim();
                html.push_str(&format!(
                    "<pre><code class=\"code-block\" data-lang=\"{}\">",
                    lang
                ));
                in_code_block = true;
            }
            continue;
        }
        if in_code_block {
            html.push_str(line);
            html.push('\n');
            continue;
        }

        if line.contains('|') && line.trim().starts_with('|') {
            if let Some(next) = lines.peek() {
                if next.contains("---") && next.contains('|') {
                    if !in_table {
                        html.push_str("<table class=\"md-table\">\n");
                        in_table = true;
                        table_header_done = false;
                    }
                    if !table_header_done {
                        html.push_str("<thead><tr>");
                        for cell in line.trim_matches('|').split('|') {
                            html.push_str(&format!("<th>{}</th>", cell.trim()));
                        }
                        html.push_str("</tr></thead><tbody>\n");
                        table_header_done = true;
                        lines.next();
                        continue;
                    }
                }
            }
            if in_table {
                html.push_str("<tr>");
                for cell in line.trim_matches('|').split('|') {
                    html.push_str(&format!("<td>{}</td>", cell.trim()));
                }
                html.push_str("</tr>\n");
                continue;
            }
        }
        if in_table {
            html.push_str("</tbody></table>\n");
            in_table = false;
        }

        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            html.push_str(&format!("<h1>{}</h1>\n", render_inline(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            html.push_str(&format!("<h2>{}</h2>\n", render_inline(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            html.push_str(&format!("<h3>{}</h3>\n", render_inline(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#### ") {
            html.push_str(&format!("<h4>{}</h4>\n", render_inline(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("##### ") {
            html.push_str(&format!("<h5>{}</h5>\n", render_inline(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("###### ") {
            html.push_str(&format!("<h6>{}</h6>\n", render_inline(rest)));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("> ") {
            html.push_str(&format!("<blockquote>{}</blockquote>\n", render_inline(rest)));
            continue;
        }

        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            html.push_str("<hr/>\n");
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
            html.push_str(&format!(
                "<div class=\"task-item task-unchecked\"><input type=\"checkbox\" disabled /> {}</div>\n",
                render_inline(rest)
            ));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- [x] ") {
            html.push_str(&format!(
                "<div class=\"task-item task-checked\"><input type=\"checkbox\" checked disabled /> {}</div>\n",
                render_inline(rest)
            ));
            continue;
        }

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item = trimmed.trim_start_matches(['-', '*']).trim();
            html.push_str(&format!("<li>{}</li>\n", render_inline(item)));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("1. ") {
            html.push_str(&format!("<li>{}</li>\n", render_inline(rest)));
            continue;
        }

        if trimmed.is_empty() {
            html.push('\n');
            continue;
        }

        html.push_str(&format!("<p>{}</p>\n", render_inline(line)));
    }

    if in_code_block {
        html.push_str("</code></pre>\n");
    }
    if in_table {
        html.push_str("</tbody></table>\n");
    }

    html
}

// ── IPC load helper ─────────────────────────────────────────────────────────

/// Loads a note's content from the `note_read` backend command.
///
/// A race-safety nonce guards against stale results: each call increments
/// `nonce` and captures the value; the async callback checks that the nonce
/// still matches before writing to any signal.
fn load_note_content(
    path: String,
    nonce: Signal<u32>,
    content: Signal<String>,
    content_state: Signal<LoadState>,
    load_error: Signal<Option<String>>,
    note_missing: Signal<bool>,
) {
    let mut n = nonce;
    n.with_mut(|v| *v = v.wrapping_add(1));
    let this_nonce = *nonce.peek();

    spawn_local(async move {
        *content_state.write_unchecked() = LoadState::Loading;
        *load_error.write_unchecked() = None;
        *note_missing.write_unchecked() = false;

        let args =
            serde_wasm_bindgen::to_value(&serde_json::json!({ "path": path.clone() })).unwrap();
        match crate::ipc::tauri_invoke_safe("note_read", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<String>(val) {
                Ok(saved) => {
                    if super::nonce_is_stale(*nonce.peek(), this_nonce) {
                        return;
                    }
                    if !saved.is_empty() {
                        *content.write_unchecked() = saved;
                        *note_missing.write_unchecked() = false;
                    } else {
                        *content.write_unchecked() = String::new();
                        *note_missing.write_unchecked() = true;
                    }
                    *content_state.write_unchecked() = LoadState::Loaded;
                }
                Err(e) => {
                    if super::nonce_is_stale(*nonce.peek(), this_nonce) {
                        return;
                    }
                    *load_error.write_unchecked() =
                        Some(format!("Note content could not be parsed: {e}"));
                    *content_state.write_unchecked() = LoadState::Failed;
                }
            },
            Ok(None) => {
                if super::nonce_is_stale(*nonce.peek(), this_nonce) {
                    return;
                }
                *load_error.write_unchecked() =
                    Some("note_read returned no data.".to_string());
                *content_state.write_unchecked() = LoadState::Failed;
            }
            Err(e) => {
                if super::nonce_is_stale(*nonce.peek(), this_nonce) {
                    return;
                }
                *load_error.write_unchecked() = Some(e.message());
                *content_state.write_unchecked() = LoadState::Failed;
            }
        }
    });
}

// ── Component ───────────────────────────────────────────────────────────────

/// Distraction-free Reader view (`ViewMode::Reader`).
///
/// Loads the active note from the workspace via `note_read` and renders it
/// through an inline markdown→HTML renderer.  Reader preferences are persisted
/// in the settings store.
#[component]
pub fn ReaderView() -> Element {
    let ws = use_workspace();

    // ── Content / load state ──
    let content = use_signal(String::new);
    let content_state = use_signal(|| LoadState::Idle);
    let load_error = use_signal(|| None::<String>);
    let note_missing = use_signal(|| false);
    let nonce = use_signal(|| 0u32);

    // ── Settings ──
    let settings = use_signal(ReaderSettings::default);
    let mut show_settings = use_signal(|| false);

    // ── Load reader settings on mount (runs once) ──
    {
        let settings_for_load = settings;
        use_effect(move || {
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(
                    &serde_json::json!({ "key": READER_SETTINGS_KEY }),
                )
                .unwrap();
                let result = crate::ipc::tauri_invoke("settings_get", args).await;
                if let Ok(s) = serde_wasm_bindgen::from_value::<ReaderSettings>(result) {
                    settings_for_load.set(s);
                }
            });
        });
    }

    // ── Load note content when the active path changes ──
    {
        let active_path = ws.active_path;
        let content_l = content;
        let state_l = content_state;
        let error_l = load_error;
        let missing_l = note_missing;
        let nonce_l = nonce;

        use_effect(move || {
            let path = active_path.read().clone().unwrap_or_default();
            if path.is_empty() {
                nonce_l.with_mut(|n| *n = n.wrapping_add(1));
                *content_l.write_unchecked() = String::new();
                *state_l.write_unchecked() = LoadState::Idle;
                *error_l.write_unchecked() = None;
                *missing_l.write_unchecked() = false;
                return;
            }
            load_note_content(path, nonce_l, content_l, state_l, error_l, missing_l);
        });
    }

    // ── Pre-compute render values ──
    let active_path_str = ws.active_path.read().clone().unwrap_or_default();

    let cs = *content_state.read();
    let is_content_loaded = cs == LoadState::Loaded;
    let load_error_opt = load_error.read().clone();
    let is_missing = *note_missing.read();
    let show_panel = show_settings.with_mut(|s| *s); // peek

    let font_size = settings.read().font_size;
    let line_width = settings.read().line_width;
    let theme = settings.read().theme.clone();
    let focus_mode = settings.read().focus_mode;

    let phase = if active_path_str.is_empty() {
        ReaderPhase::Empty // no note selected
    } else {
        classify_reader_phase(is_content_loaded, is_missing, load_error_opt.as_deref())
    };

    let prose_class = if focus_mode {
        "reader-prose reader-focus-mode"
    } else {
        "reader-prose"
    };

    let content_html = if phase == ReaderPhase::Loaded {
        Some(render_markdown(&content.read().clone()))
    } else {
        None
    };

    // Precompute class strings for buttons
    let focus_btn_class = if focus_mode {
        "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
    } else {
        "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
    };
    let settings_btn_class = if show_panel {
        "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
    } else {
        "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
    };
    let settings_style = format!("max-width: {}px;", line_width);
    let content_style = format!(
        "max-width: {}px; font-size: {}px; line-height: 1.7;",
        line_width, font_size
    );

    // ── Setting handlers ──
    let on_font_size = {
        let settings_ref = settings;
        move |ev: FormEvent| {
            let val: u32 = ev.value().parse().unwrap_or(18);
            settings_ref.write_unchecked().font_size = val;
            let current = settings_ref.read().clone();
            persist_reader_settings(&current);
        }
    };

    let on_line_width = {
        let settings_ref = settings;
        move |ev: FormEvent| {
            let val: u32 = ev.value().parse().unwrap_or(720);
            settings_ref.write_unchecked().line_width = val;
            let current = settings_ref.read().clone();
            persist_reader_settings(&current);
        }
    };

    let on_focus_toggle = {
        let settings_ref = settings;
        move |_: MouseEvent| {
            settings_ref.write_unchecked().focus_mode = !settings_ref.read().focus_mode;
            let current = settings_ref.read().clone();
            persist_reader_settings(&current);
        }
    };

    let on_retry_load = {
        let active_path = ws.active_path;
        move |_: ()| {
            if let Some(path) = active_path.read().clone() {
                load_note_content(
                    path,
                    nonce,
                    content,
                    content_state,
                    load_error,
                    note_missing,
                );
            }
        }
    };

    rsx! {
        div {
            class: "reader-view h-full overflow-auto bg-gray-950 text-gray-100",
            "data-theme": "{theme}",

            div {
                class: "sticky top-0 z-10 flex items-center justify-between px-4 py-2 bg-gray-950/80 backdrop-blur border-b border-gray-800/50",
                div { class: "flex items-center gap-3" }
                span {
                    class: "text-sm text-gray-400 truncate max-w-xs",
                    "{active_path_str}",
                }
                div { class: "flex items-center gap-2" }
                button {
                    class: "{focus_btn_class}",
                    onclick: on_focus_toggle,
                    title: "Toggle focus mode",
                    {render_icon_view(Icon::Target) }
                    " Focus"
                }
                button {
                    class: "{settings_btn_class}",
                    onclick: move |_: MouseEvent| {
                        *show_settings.write_unchecked() = !*show_settings.peek();
                    },
                    title: "Reader settings",
                    {render_icon_view(Icon::Settings) }
                }
            }

            // Settings panel
            {if show_panel {
                rsx! {
                    div {
                        class: "sticky top-12 z-10 mx-auto bg-gray-900 border border-gray-700 rounded-lg p-4 mb-4",
                        style: "{settings_style}",

                        div { class: "space-y-3" }
                        div {}
                        label {
                            class: "text-xs text-gray-500 uppercase tracking-wide",
                            "Font Size"
                        }
                        div { class: "flex items-center gap-2 mt-1" }
                        input {
                            r#type: "range",
                            min: "14",
                            max: "28",
                            value: "{font_size}",
                            class: "flex-1",
                            oninput: on_font_size,
                        }
                        span { class: "text-sm text-gray-400", "{font_size}px" }
                        div {}
                        label {
                            class: "text-xs text-gray-500 uppercase tracking-wide",
                            "Line Width"
                        }
                        div { class: "flex items-center gap-2 mt-1" }
                        input {
                            r#type: "range",
                            min: "480",
                            max: "960",
                            step: "40",
                            value: "{line_width}",
                            class: "flex-1",
                            oninput: on_line_width,
                        }
                        span { class: "text-sm text-gray-400", "{line_width}px" }
                        div {}
                        label {
                            class: "text-xs text-gray-500 uppercase tracking-wide",
                            "Theme"
                        }
                        div { class: "flex gap-2 mt-1" }
                        {
                            let themes = ["dark", "sepia", "light"];
                            rsx! {
                                for theme_name in themes {
                                    {
                                        let t = (*theme_name).to_string();
                                        let is_active = theme == t;
                                        let settings_for_btn = settings;
                                        let btn_class = if is_active {
                                            "px-3 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                                        } else {
                                            "px-3 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
                                        };
                                        rsx! {
                                            button {
                                                class: "{btn_class}",
                                                onclick: move |_: MouseEvent| {
                                                    settings_for_btn.write_unchecked().theme = t.clone();
                                                    let current = settings_for_btn.read().clone();
                                                    persist_reader_settings(&current);
                                                },
                                                "{t}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                rsx! {}
            }}

            // Content area
            div {
                class: "reader-content mx-auto px-8 py-8",
                style: "{content_style}",
                {if active_path_str.is_empty() {
                    rsx! {
                        div {
                            class: "flex h-full items-center justify-center py-20",
                            EmptyState {
                                icon: Icon::BookOpen,
                                title: "No note selected".to_string(),
                                description: "Open a note and switch to Reader mode to start reading.".to_string(),
                            }
                        }
                    }
                } else if phase == ReaderPhase::Loading {
                    rsx! {
                        div { class: "flex items-center justify-center py-20" }
                        LoadingBlock {
                            label: "Loading note…",
                            size: SpinnerSize::Md,
                        }
                    }
                } else if phase == ReaderPhase::Error {
                    rsx! {
                        div { class: "flex-1 flex items-center justify-center p-6" }
                        div { class: "w-full max-w-md" }
                        ErrorPanel {
                            title: "Couldn't open note".to_string(),
                            message: "The note content could not be loaded.".to_string(),
                            details: load_error_opt,
                            recovery: "Make sure the note is accessible and the backend is running, then retry.".to_string(),
                            on_retry: Some(on_retry_load),
                        }
                    }
                } else if phase == ReaderPhase::Empty {
                    rsx! {
                        div {
                            class: "flex h-full items-center justify-center py-20",
                            EmptyState {
                                icon: Icon::BookText,
                                title: "Note not found".to_string(),
                                description: "The selected note doesn't exist yet. Create it in the Editor view.".to_string(),
                            }
                        }
                    }
                } else {
                    let html = content_html.clone().unwrap_or_default();
                    rsx! {
                        div {
                            class: "{prose_class}",
                            dangerous_inner_html: html,
                        }
                    }
                }}
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::recovery::diff_view::{DiffKind, DiffRow};

    // ── Phase classification ──

    #[test]
    fn phase_loading_when_not_loaded() {
        assert_eq!(
            classify_reader_phase(false, false, None),
            ReaderPhase::Loading
        );
    }

    #[test]
    fn phase_empty_when_note_missing() {
        assert_eq!(
            classify_reader_phase(true, true, None),
            ReaderPhase::Empty
        );
    }

    #[test]
    fn phase_loaded_when_content_present() {
        assert_eq!(
            classify_reader_phase(true, false, None),
            ReaderPhase::Loaded
        );
    }

    #[test]
    fn phase_error_takes_precedence() {
        assert_eq!(
            classify_reader_phase(true, false, Some("boom")),
            ReaderPhase::Error
        );
    }

    #[test]
    fn phase_empty_error_string_is_ignored() {
        assert_eq!(
            classify_reader_phase(true, true, Some("")),
            ReaderPhase::Empty
        );
    }

    // ── Nonce race-safety ──

    #[test]
    fn stale_nonce_discards_result() {
        assert!(nonce_is_stale(3, 2));
    }

    #[test]
    fn current_nonce_accepts_result() {
        assert!(!nonce_is_stale(5, 5));
    }

    // ── Markdown renderer ──

    #[test]
    fn markdown_renders_headings() {
        let html = render_markdown("# Title\n## Sub\n");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<h2>Sub</h2>"));
    }

    #[test]
    fn markdown_renders_bold_and_italic() {
        let html = render_markdown("**bold** and *italic*");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn markdown_escapes_html() {
        let html = render_markdown("<script>alert('xss')</script>");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn markdown_renders_wikilinks() {
        let html = render_markdown("[[My Note]]");
        assert!(html.contains("class=\"wikilink\""));
        assert!(html.contains("data-path=\"My Note\""));
    }

    #[test]
    fn markdown_renders_code_block() {
        let html = render_markdown("```rust\nfn main() {}\n```");
        assert!(html.contains("<pre><code"));
        assert!(html.contains("fn main() {}"));
    }

    #[test]
    fn markdown_renders_link() {
        let html = render_markdown("[example](https://example.com)");
        assert!(html.contains("href=\"https://example.com\""));
        assert!(html.contains(">example<"));
    }

    // ── Comparison has_changes (shared helper) ──

    fn make_row(kind: DiffKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffRow {
        DiffRow {
            kind,
            old_line: old,
            new_line: new,
            text: text.to_string(),
        }
    }

    #[test]
    fn has_changes_detects_added_rows() {
        let rows = vec![
            make_row(DiffKind::Same, Some(1), Some(1), "same line"),
            make_row(DiffKind::Added, None, Some(2), "new line"),
        ];
        assert!(super::comparison::has_changes(&rows));
    }

    #[test]
    fn has_changes_detects_no_changes() {
        let rows = vec![
            make_row(DiffKind::Same, Some(1), Some(1), "same line"),
            make_row(DiffKind::Same, Some(2), Some(2), "same line 2"),
        ];
        assert!(!super::comparison::has_changes(&rows));
    }

    #[test]
    fn has_changes_empty_vec() {
        assert!(!super::comparison::has_changes(&[]));
    }
}
