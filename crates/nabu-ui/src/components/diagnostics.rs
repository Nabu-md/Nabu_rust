//! # Diagnostics Panel — Dioxus
//!
//! On-demand diagnostic analysis UI.  A user pastes or edits markdown text in
//! the textarea, selects an origin (spelling engine, AI assistant, etc.), and
//! clicks "Run Diagnostics".  The panel calls the `diagnostic_requested` Tauri
//! IPC command, then renders the returned [`DiagnosticResponse`] grouped by
//! [`DiagnosticSeverity`] with range, message, suggestions, and decorations.

use crate::components::ui::feedback::{use_toast, ToastContext, ErrorPanel, LoadingBlock, SpinnerSize};
use crate::components::ui::button::{Button, ButtonVariant};
use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;

use nabu_core::diagnostic::{
    Diagnostic, DiagnosticBatch, DiagnosticCategory, DiagnosticSeverity, DiagnosticStyleMap,
    Suggestion, SuggestionApplicability, SuggestionPriority, TextPosition, TextRange,
};

/// Mirrors the backend `DiagnosticResponse` (commands.rs).
/// The batch + style map are deserialized from the IPC result so the UI can
/// render severity-resolved styles without a second round-trip.
#[derive(Debug, Deserialize)]
struct DiagnosticResponse {
    batch: DiagnosticBatch,
    #[allow(dead_code)]
    style_map: DiagnosticStyleMap,
}

/// Component props.
#[derive(Clone, PartialEq, Props)]
pub struct DiagnosticsPanelProps {
    /// Stable resource identifier such as `"vault:notes/example.md"`.
    pub resource_id: String,
}

/// Severity → CSS colour class pair for the diagnostics panel.
fn severity_class(sev: DiagnosticSeverity) -> &'static str {
    match sev {
        DiagnosticSeverity::Hint => "text-sky-400",
        DiagnosticSeverity::Information => "text-blue-400",
        DiagnosticSeverity::Warning => "text-amber-400",
        DiagnosticSeverity::Error => "text-red-400",
        DiagnosticSeverity::Critical => "text-red-600 font-bold",
    }
}

/// Human-readable label for a severity.
fn severity_label(sev: DiagnosticSeverity) -> &'static str {
    sev.label()
}

/// Render a single diagnostic entry.
fn diagnostic_item(diag: &Diagnostic) -> Element {
    let range_str = format_range(&diag.range);
    let code_str = diag.code.clone().unwrap_or_default();
    let source_str = diag.source.clone().unwrap_or_default();
    let cat_str = diag
        .category
        .map(|c| format!("{:?}", c))
        .unwrap_or_default();
    let sev_class = severity_class(diag.severity);

    rsx! {
        li { class: "border-l-2 border-gray-700 pl-4 pb-3",
            div { class: "flex items-baseline gap-3",
                span { class: "text-xs text-gray-500 w-16 shrink-0", "{range_str}" }
                span { class: "{sev_class} text-sm font-medium", severity_label(diag.severity) }
                if !code_str.is_empty() {
                    span { class: "text-xs text-gray-600", "[{code_str}]" }
                }
                if !source_str.is_empty() {
                    span { class: "text-xs text-gray-600", "· {source_str}" }
                }
                if !cat_str.is_empty() {
                    span { class: "text-xs text-gray-600", "· {cat_str}" }
                }
            }
            p { class: "mt-1 text-sm text-gray-200", "{diag.message}" }
            if !diag.suggestions.is_empty() {
                ul { class: "mt-2 space-y-1",
                    for sug in &diag.suggestions {
                        suggestion_item(sug)
                    }
                }
            }
        }
    }
}

fn suggestion_item(sug: &Suggestion) -> Element {
    let applicability_lbl = sug
        .applicability
        .unwrap_or(SuggestionApplicability::OnRequest)
        .label();
    let priority_lbl = sug
        .priority
        .unwrap_or(SuggestionPriority::Normal)
        .name();

    rsx! {
        li { class: "ml-4 list-disc text-sm",
            div { class: "flex items-center gap-2",
                span { class: "text-green-400", "🔧" }
                span { class: "font-medium text-gray-200", "{sug.title}" }
            }
            p { class: "text-xs text-gray-500",
                "Applicability: {applicability_lbl} · Priority: {priority_lbl}"
            }
            if !sug.new_text.is_empty() {
                pre { class: "mt-1 text-xs text-gray-600 bg-gray-800/50 rounded p-1 overflow-x-auto",
                    "{sug.new_text}"
                }
            }
        }
    }
}

fn format_range(range: &TextRange) -> String {
    if range.start == range.end {
        format!("L{}:{}", range.start.line + 1, range.start.character + 1)
    } else {
        format!(
            "L{}-L{} :{}-{}",
            range.start.line + 1,
            range.end.line + 1,
            range.start.character + 1,
            range.end.character + 1
        )
    }
}

/// Diagnostics tab content inside the Settings panel.
#[component]
pub fn DiagnosticsPanel(resource_id: String) -> Element {
    let toasts = use_toast();
    let text = use_signal(String::new);
    let origin = use_signal(|| "harper".to_string());
    let response: Signal<Option<DiagnosticResponse>> = use_signal(|| None);
    let loading = use_signal(|| false);
    let error = use_signal(String::new);

    let run_diagnostics = {
        let text_sig = text.clone();
        let origin_sig = origin.clone();
        let toasts_sig = toasts.clone();
        let resp_sig = response.clone();
        let loading_sig = loading.clone();
        let error_sig = error.clone();
        move |_: MouseEvent| {
            let text_val = text_sig.read().clone();
            let origin_val = origin_sig.read().clone();
            let resource_id_val = resource_id.clone();
            loading_sig.set(true);
            error_sig.set(String::new());

            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(
                    &serde_json::json!({
                        "text": text_val,
                        "resource_id": resource_id_val,
                        "origin": origin_val,
                    }),
                ).unwrap();

                match crate::ipc::tauri_invoke("diagnostic_requested", args).await {
                    Ok(result) => {
                        match serde_wasm_bindgen::from_value::<DiagnosticResponse>(result) {
                            Ok(resp) => {
                                resp_sig.set(Some(resp));
                                let count = resp.batch.diagnostic_count();
                                toasts_sig.success(
                                    "Diagnostics complete",
                                    format!("Found {count} issue(s)."),
                                );
                            }
                            Err(e) => {
                                error_sig.set(format!("Parse error: {e}"));
                                toasts_sig.error("Diagnostics failed", format!("Parse error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        error_sig.set(e.message());
                        toasts_sig.error("Diagnostics failed", e.message());
                    }
                }
                loading_sig.set(false);
            });
        }
    };

    let diag_data = response.read().clone();

    rsx! {
        div { class: "diagnostics-panel h-full flex flex-col",

            h2 { class: "text-xl font-bold mb-4", "Diagnostic Analyzer" }

            div { class: "flex gap-3 mb-4",
                div { class: "flex-1",
                    label { class: "block text-sm text-gray-400 mb-1", "Origin" }
                    select {
                        class: "input w-full",
                        value: "{origin.read()}",
                        onchange: move |ev: FormEvent| {
                            origin.set(ev.value());
                        },
                        option { value: "harper", "Harper (Spelling/Grammar)" }
                        option { value: "ai-assistant", "AI Assistant" }
                        option { value: "lsp-markdown", "LSP (Markdown)" }
                        option { value: "plugin", "Plugin" }
                    }
                }
                div { class: "flex-1",
                    label { class: "block text-sm text-gray-400 mb-1", "Resource ID" }
                    input {
                        class: "input w-full",
                        r#type: "text",
                        value: "{resource_id}",
                        readonly: true,
                    }
                }
            }

            label { class: "block text-sm text-gray-400 mb-1", "Text to analyze" }
            textarea {
                class: "input w-full h-48 font-mono text-sm",
                placeholder: "Paste or edit markdown text to run diagnostics...",
                value: "{text.read()}",
                onchange: move |ev: FormEvent| {
                    text.set(ev.value());
                },
            }

            div { class: "mt-4 flex gap-2",
                Button {
                    on_click: run_diagnostics,
                    disabled: *loading.read(),
                    variant: ButtonVariant::Primary,
                    if *loading.read() {
                        "Running..."
                    } else {
                        "Run Diagnostics"
                    }
                }
            }

            if !error.read().is_empty() {
                ErrorPanel { message: "{error.read()}" }
            }

            if *loading.read() && error.read().is_empty() {
                LoadingBlock { spinner: SpinnerSize::Md, label: Some("Analyzing...".to_string()) }
            }

            if let Some(data) = &diag_data {
                if data.batch.diagnostics.is_empty() {
                    div { class: "mt-4 p-4 text-center text-gray-500",
                        "No diagnostics found."
                    }
                } else {
                    render_diagnostic_batch(data)
                }
            }
        }
    }
}

fn render_diagnostic_batch(data: &DiagnosticResponse) -> Element {
    let batch = &data.batch;
    let severity_counts: [(DiagnosticSeverity, usize); 5] = [
        (DiagnosticSeverity::Hint, 0),
        (DiagnosticSeverity::Information, 0),
        (DiagnosticSeverity::Warning, 0),
        (DiagnosticSeverity::Error, 0),
        (DiagnosticSeverity::Critical, 0),
    ];

    let counts: std::collections::HashMap<DiagnosticSeverity, usize> =
        DiagnosticSeverity::ALL.iter().map(|&s| (s, 0)).collect();

    let mut counts = counts;
    for diag in &batch.diagnostics {
        *counts.entry(diag.severity).or_insert(0) += 1;
    }

    let total = batch.diagnostics.len();

    rsx! {
        div { class: "mt-6 border-t border-gray-700 pt-4",
            div { class: "flex justify-between items-center mb-3",
                h3 { class: "text-lg font-semibold", "Results ({total} diagnostics)" }
                span { class: "text-sm text-gray-500",
                    "Origin: {batch.origin} · Resource: {batch.resource_id}"
                }
            }

            div { class: "flex flex-wrap gap-2 mb-4",
                for &sev in DiagnosticSeverity::ALL {
                    let count = counts.get(&sev).copied().unwrap_or(0);
                    let cls = severity_class(sev);
                    let lbl = severity_label(sev);
                    rsx! {
                        span { class: "px-2 py-1 bg-gray-800 rounded text-xs",
                            span { class: "{cls}", "{lbl}" }
                            span { class: "text-gray-500 ml-1", "({count})" }
                        }
                    }
                }
            }

            for &sev in DiagnosticSeverity::ALL.iter().rev() {
                let group: Vec<&Diagnostic> = batch.diagnostics.iter()
                    .filter(|d| d.severity == sev)
                    .collect();
                if group.is_empty() {
                    continue;
                }
                let cls = severity_class(sev);
                let lbl = severity_label(sev);
                let count = group.len();
                rsx! {
                    div { class: "mb-4",
                        h4 { class: "{cls} text-md font-medium mb-2",
                            "{lbl} ({count})"
                        }
                        ul { class: "space-y-2",
                            for diag in group {
                                diagnostic_item(diag)
                            }
                        }
                    }
                }
            }
        }
    }
}
