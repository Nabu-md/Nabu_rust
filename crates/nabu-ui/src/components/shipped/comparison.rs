//! # Comparison View (Dioxus) — shipped
//!
//! Side-by-side diff between two notes (`notes_diff`) or two revisions of
//! the same note (`versions_diff`). Reuses the shared [`DiffView`](crate::components::recovery::diff_view::DiffView)
//! component for rendering.
//!
//! Load lifecycle follows the `LoadState` pattern from `version_history.rs`.
//! A race-safety nonce guards against stale IPC results.

use crate::components::contexts::use_nav;
use crate::components::recovery::diff_view::{DiffKind, DiffRow, DiffView};
use crate::components::recovery::version_history::VersionMeta;
use crate::components::recovery::LoadState;
use crate::components::ui::feedback::{use_toast, ErrorPanel, SkeletonList};
use crate::components::ui::icons::{render_icon_view, Icon};
use crate::components::ui::info::EmptyState;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

// ── Types ───────────────────────────────────────────────────────────────────

/// Two comparison modes: compare any two notes, or compare revisions of one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompareMode {
    Notes,
    Revisions,
}

impl Default for CompareMode {
    fn default() -> Self {
        Self::Notes
    }
}

/// Load phase of the diff result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ComparisonPhase {
    Idle,
    Loading,
    NoDiff,
    Error,
    Loaded,
}

// ── Pure helper functions ───────────────────────────────────────────────────

/// Returns `true` if the diff rows contain at least one Added or Removed
/// row — i.e. the two notes/versions are not identical.
pub fn has_changes(rows: &[DiffRow]) -> bool {
    rows.iter().any(|r| r.kind != DiffKind::Same)
}

/// Pure-classification: maps the comparison's reactive signal states to a
/// single phase.
///
/// * `diff_state` — the `LoadState` of the most recent comparison.
/// * `load_error` — `Some(msg)` with a non-empty string on failure.
/// * `diff_rows` — `Some(rows)` when a comparison completed successfully.
pub fn classify_comparison_phase(
    diff_state: LoadState,
    load_error: Option<&str>,
    diff_rows: Option<&Vec<DiffRow>>,
) -> ComparisonPhase {
    if let Some(e) = load_error {
        if !e.is_empty() {
            return ComparisonPhase::Error;
        }
    }
    match diff_state {
        LoadState::Idle => ComparisonPhase::Idle,
        LoadState::Loading => ComparisonPhase::Loading,
        LoadState::Failed => ComparisonPhase::Error,
        LoadState::Loaded => match diff_rows {
            Some(rows) if has_changes(rows) => ComparisonPhase::Loaded,
            _ => ComparisonPhase::NoDiff,
        },
    }
}

/// Formats a revision label for display in the `DiffView` header.
fn revision_label(path: &str, version: &Option<String>) -> String {
    match version {
        Some(id) => format!("{} @ {}", path, id),
        None => format!("{} (current)", path),
    }
}

// ── Versions load helper ────────────────────────────────────────────────────

/// Loads the versions list for a note via `versions_list`. A nonce guards
/// against stale results when the user quickly switches notes in Revisions mode.
fn load_versions_for_comparison(
    path: String,
    versions: Signal<Vec<VersionMeta>>,
    versions_state: Signal<LoadState>,
    versions_error: Signal<Option<String>>,
    versions_nonce: Signal<u32>,
) {
    let mut n = versions_nonce;
    n.with_mut(|v| *v = v.wrapping_add(1));
    let this_nonce = *versions_nonce.peek();

    spawn_local(async move {
        *versions_state.write_unchecked() = LoadState::Loading;
        *versions_error.write_unchecked() = None;

        let args =
            serde_wasm_bindgen::to_value(&serde_json::json!({ "path": path.clone() })).unwrap();
        match crate::ipc::tauri_invoke_safe("versions_list", args).await {
            Ok(Some(val)) => match serde_wasm_bindgen::from_value::<Vec<VersionMeta>>(val) {
                Ok(v) => {
                    if !super::nonce_is_stale(*versions_nonce.peek(), this_nonce) {
                        *versions.write_unchecked() = v;
                        *versions_state.write_unchecked() = LoadState::Loaded;
                    }
                }
                Err(e) => {
                    if !super::nonce_is_stale(*versions_nonce.peek(), this_nonce) {
                        *versions_error.write_unchecked() =
                            Some(format!("Versions could not be parsed: {e}"));
                        *versions_state.write_unchecked() = LoadState::Failed;
                    }
                }
            },
            Ok(None) => {
                if !super::nonce_is_stale(*versions_nonce.peek(), this_nonce) {
                    *versions_error.write_unchecked() =
                        Some("versions_list returned no data.".to_string());
                    *versions_state.write_unchecked() = LoadState::Failed;
                }
            }
            Err(e) => {
                if !super::nonce_is_stale(*versions_nonce.peek(), this_nonce) {
                    *versions_error.write_unchecked() = Some(e.message());
                    *versions_state.write_unchecked() = LoadState::Failed;
                }
            }
        }
    });
}

// ── Component ───────────────────────────────────────────────────────────────

/// Side-by-side Comparison view (`ViewMode::Comparison`).
///
/// Supports two modes:
/// - **Two Notes** — diff any two notes by vault-relative path (`notes_diff`).
/// - **Revisions** — diff two versions of the same note (`versions_diff`).
#[component]
pub fn ComparisonView() -> Element {
    let nav = use_nav();
    let toasts = use_toast();

    // ── Mode ──
    let mut mode = use_signal(|| CompareMode::Notes);

    // ── Diff result ──
    let diff_state = use_signal(|| LoadState::Idle);
    let diff_rows = use_signal(|| None::<Vec<DiffRow>>);
    let diff_error = use_signal(|| None::<String>);
    let diff_nonce = use_signal(|| 0u32);

    // ── Notes mode selections ──
    let note_a = use_signal(|| String::new());
    let note_b = use_signal(|| String::new());

    // ── Revisions mode selections ──
    let revision_path = use_signal(|| String::new());
    let versions = use_signal(|| Vec::<VersionMeta>::new());
    let versions_state = use_signal(|| LoadState::Idle);
    let versions_error = use_signal(|| None::<String>);
    let versions_nonce = use_signal(|| 0u32);
    let version_a = use_signal(|| None::<String>);
    let version_b = use_signal(|| None::<String>);

    // ── Load versions when revision_path changes ──
    {
        let rp = revision_path;
        let v = versions;
        let vs = versions_state;
        let ve = versions_error;
        let vn = versions_nonce;

        use_effect(move || {
            let path = rp.read().clone();
            if path.is_empty() {
                return;
            }
            load_versions_for_comparison(path, v, vs, ve, vn);
        });
    }

    // ── Compare button handler ──
    let on_compare = {
        let mode_c = mode;
        let note_a_c = note_a;
        let note_b_c = note_b;
        let revision_path_c = revision_path;
        let version_a_c = version_a;
        let version_b_c = version_b;
        let diff_state_c = diff_state;
        let diff_rows_c = diff_rows;
        let diff_error_c = diff_error;
        let diff_nonce_c = diff_nonce;
        let toasts_c = toasts;

        move |_: MouseEvent| {
            let current_mode = *mode_c.read();
            let path_a = note_a_c.read().clone();
            let path_b = note_b_c.read().clone();
            let rev_path = revision_path_c.read().clone();
            let v_a = version_a_c.read().clone();
            let v_b = version_b_c.read().clone();

            let can_compare = match current_mode {
                CompareMode::Notes => !path_a.is_empty() && !path_b.is_empty(),
                CompareMode::Revisions => !rev_path.is_empty(),
            };

            if !can_compare {
                toasts_c.info(
                    "Comparison",
                    "Select two notes or a revision to compare.",
                );
                return;
            }

            // Kick off the comparison.
            diff_state_c.with_mut(|s| *s = LoadState::Loading);
            diff_nonce_c.with_mut(|n| *n = n.wrapping_add(1));
            let this_nonce = *diff_nonce_c.peek();

            let cmd = match current_mode {
                CompareMode::Notes => "notes_diff",
                CompareMode::Revisions => "versions_diff",
            };

            spawn_local(async move {
                let args = match current_mode {
                    CompareMode::Notes => serde_wasm_bindgen::to_value(
                        &serde_json::json!({ "path_a": path_a, "path_b": path_b }),
                    ),
                    CompareMode::Revisions => serde_wasm_bindgen::to_value(
                        &serde_json::json!({
                            "path": rev_path,
                            "id_a": v_a,
                            "id_b": v_b,
                        }),
                    ),
                }
                .unwrap();

                let result = crate::ipc::tauri_invoke(cmd, args).await;
                match serde_wasm_bindgen::from_value::<Vec<DiffRow>>(result) {
                    Ok(rows) => {
                        if !super::nonce_is_stale(*diff_nonce_c.peek(), this_nonce) {
                            *diff_rows_c.write_unchecked() = Some(rows);
                            *diff_state_c.write_unchecked() = LoadState::Loaded;
                        }
                    }
                    Err(e) => {
                        if !super::nonce_is_stale(*diff_nonce_c.peek(), this_nonce) {
                            *diff_error_c.write_unchecked() = Some(e.to_string());
                            *diff_state_c.write_unchecked() = LoadState::Failed;
                        }
                    }
                }
            });
        }
    };

    let on_retry = {
        let mode_r = mode;
        move |_: ()| {
            let current_mode = *mode_r.read();
            let path_a = note_a.read().clone();
            let path_b = note_b.read().clone();
            let rev_path = revision_path.read().clone();
            let v_a = version_a.read().clone();
            let v_b = version_b.read().clone();

            let cmd = match current_mode {
                CompareMode::Notes => "notes_diff",
                CompareMode::Revisions => "versions_diff",
            };

            diff_state.with_mut(|s| *s = LoadState::Loading);
            diff_nonce.with_mut(|n| *n = n.wrapping_add(1));
            let this_nonce = *diff_nonce.peek();

            spawn_local(async move {
                let args = match current_mode {
                    CompareMode::Notes => serde_wasm_bindgen::to_value(
                        &serde_json::json!({ "path_a": path_a, "path_b": path_b }),
                    ),
                    CompareMode::Revisions => serde_wasm_bindgen::to_value(
                        &serde_json::json!({
                            "path": rev_path,
                            "id_a": v_a,
                            "id_b": v_b,
                        }),
                    ),
                }
                .unwrap();

                let result = crate::ipc::tauri_invoke(cmd, args).await;
                match serde_wasm_bindgen::from_value::<Vec<DiffRow>>(result) {
                    Ok(rows) => {
                        if !super::nonce_is_stale(*diff_nonce.peek(), this_nonce) {
                            *diff_rows.write_unchecked() = Some(rows);
                            *diff_state.write_unchecked() = LoadState::Loaded;
                        }
                    }
                    Err(e) => {
                        if !super::nonce_is_stale(*diff_nonce.peek(), this_nonce) {
                            *diff_error.write_unchecked() = Some(e.to_string());
                            *diff_state.write_unchecked() = LoadState::Failed;
                        }
                    }
                }
            });
        }
    };

    // ── Mode toggle handlers ──
    let on_mode_notes = move |_: MouseEvent| {
        mode.with_mut(|m| *m = CompareMode::Notes);
    };
    let on_mode_revisions = move |_: MouseEvent| {
        mode.with_mut(|m| *m = CompareMode::Revisions);
    };

    // ── Pre-compute render values ──
    let current_mode = *mode.read();
    let notes: Vec<crate::components::navigation::state::NoteIndexEntry> =
        nav.notes_index.read().clone();
    let sel_a = note_a.read().clone();
    let sel_b = note_b.read().clone();
    let rev_path = revision_path.read().clone();
    let versions_list: Vec<VersionMeta> = versions.read().clone();
    let vs = *versions_state.read();
    let ve = versions_error.read().clone().unwrap_or_default();
    let sel_version_a = version_a.read().clone();
    let sel_version_b = version_b.read().clone();

    let diff_state_val = *diff_state.read();
    let diff_err_opt = diff_error.read().clone();
    let rows_opt = diff_rows.read().clone();

    let phase = classify_comparison_phase(diff_state_val, diff_err_opt.as_deref(), rows_opt.as_ref());

    // Labels for DiffView
    let (label_a, label_b) = match current_mode {
        CompareMode::Notes => (
            if sel_a.is_empty() { "Note A".to_string() } else { sel_a.clone() },
            if sel_b.is_empty() { "Note B".to_string() } else { sel_b.clone() },
        ),
        CompareMode::Revisions => (
            revision_label(&rev_path, &sel_version_a),
            revision_label(&rev_path, &sel_version_b),
        ),
    };

    let diff_count = rows_opt
        .as_ref()
        .map(|rows| {
            rows.iter()
                .filter(|r| r.kind != DiffKind::Same)
                .count()
        })
        .unwrap_or(0);

    // Versions loading sub-phase (for Revisions mode)
    let versions_phase = if current_mode == CompareMode::Revisions && !rev_path.is_empty() {
        if !ve.is_empty() {
            ComparisonPhase::Error
        } else if vs == LoadState::Loading || vs == LoadState::Idle {
            ComparisonPhase::Loading
        } else {
            ComparisonPhase::Loaded
        }
    } else {
        ComparisonPhase::Loaded
    };

    rsx! {
        div {
            class: "comparison-view flex flex-col h-full bg-gray-950 text-gray-100 overflow-hidden",

            // ── Header ──
            div {
                class: "flex-none px-4 py-3 border-b border-gray-800",

                div { class: "flex items-center justify-between mb-3" }
                h2 { class: "text-sm font-semibold text-gray-300", "Comparison View" }

                div { class: "flex items-center gap-2" }
                button {
                    class: { format!(
                        "px-3 py-1 text-xs rounded border {}",
                        if current_mode == CompareMode::Notes {
                            "bg-blue-900/50 border-blue-600 text-blue-300"
                        } else {
                            "border-gray-700 text-gray-400 hover:text-gray-200"
                        }
                    ) },
                    onclick: on_mode_notes,
                    "Two Notes"
                }
                button {
                    class: { format!(
                        "px-3 py-1 text-xs rounded border {}",
                        if current_mode == CompareMode::Revisions {
                            "bg-blue-900/50 border-blue-600 text-blue-300"
                        } else {
                            "border-gray-700 text-gray-400 hover:text-gray-200"
                        }
                    ) },
                    onclick: on_mode_revisions,
                    "Revisions"
                }

                // Selection controls
                if current_mode == CompareMode::Notes {
                    div { class: "grid grid-cols-2 gap-3" }
                    div {}
                    label {
                        class: "text-xs text-gray-500 uppercase tracking-wide",
                        "Note A"
                    }
                    select {
                        class: "w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700",
                        value: "{sel_a}",
                        onchange: move |ev: FormEvent| {
                            note_a.set(ev.value());
                        },
                        option { value: "", "Select note A…" }
                        for note in notes.iter() {
                            {
                                let path = note.path.clone();
                                let title = note.title.clone();
                                rsx! {
                                    option { value: "{path}", "{title}" }
                                }
                            }
                        }
                    }

                    div {}
                    label {
                        class: "text-xs text-gray-500 uppercase tracking-wide",
                        "Note B"
                    }
                    select {
                        class: "w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700",
                        value: "{sel_b}",
                        onchange: move |ev: FormEvent| {
                            note_b.set(ev.value());
                        },
                        option { value: "", "Select note B…" }
                        for note in notes.iter() {
                            {
                                let path = note.path.clone();
                                let title = note.title.clone();
                                rsx! {
                                    option { value: "{path}", "{title}" }
                                }
                            }
                        }
                    }
                } else {
                    // Revisions mode
                    div { class: "grid grid-cols-3 gap-3" }

                    div {}
                    label {
                        class: "text-xs text-gray-500 uppercase tracking-wide",
                        "Note"
                    }
                    select {
                        class: "w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700",
                        value: "{rev_path}",
                        onchange: move |ev: FormEvent| {
                            let path = ev.value();
                            revision_path.set(path.clone());
                            // Reset version selections
                            version_a.set(None);
                            version_b.set(None);
                        },
                        option { value: "", "Select note…" }
                        for note in notes.iter() {
                            {
                                let path = note.path.clone();
                                let title = note.title.clone();
                                rsx! {
                                    option { value: "{path}", "{title}" }
                                }
                            }
                        }
                    }

                    div {}
                    label {
                        class: "text-xs text-gray-500 uppercase tracking-wide",
                        "Version A"
                    }
                    select {
                        class: "w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700",
                        value: "{sel_version_a.as_deref().unwrap_or("")}",
                        onchange: move |ev: FormEvent| {
                            let val = ev.value();
                            version_a.set(if val.is_empty() { None } else { Some(val) });
                        },
                        option { value: "", "Current" }
                        if vs == LoadState::Loading || vs == LoadState::Idle {
                            rsx! {}
                        } else {
                            for v in versions_list.iter().rev() {
                                {
                                    let id = v.id.clone();
                                    let label = format!(
                                        "{} ({} chars)",
                                        &v.created_at, v.char_count
                                    );
                                    rsx! {
                                        option { value: "{id}", "{label}" }
                                    }
                                }
                            }
                        }
                    }

                    div {}
                    label {
                        class: "text-xs text-gray-500 uppercase tracking-wide",
                        "Version B"
                    }
                    select {
                        class: "w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700",
                        value: "{sel_version_b.as_deref().unwrap_or("")}",
                        onchange: move |ev: FormEvent| {
                            let val = ev.value();
                            version_b.set(if val.is_empty() { None } else { Some(val) });
                        },
                        option { value: "", "Current" }
                        for v in versions_list.iter().rev() {
                            {
                                let id = v.id.clone();
                                let label = format!(
                                    "{} ({} chars)",
                                    &v.created_at, v.char_count
                                );
                                rsx! {
                                    option { value: "{id}", "{label}" }
                                }
                            }
                        }
                    }
                }

                // Action bar
                div { class: "flex items-center justify-between mt-3" }
                div { class: "flex items-center gap-3" }
                button {
                    class: "px-3 py-1.5 text-sm bg-blue-600 rounded hover:bg-blue-500",
                    onclick: on_compare,
                    "Compare"
                }
                if diff_count > 0 {
                    span { class: "text-xs text-gray-400", "{diff_count} differences" }
                }
            }

            // ── Diff content ──
            div { class: "flex-1 overflow-hidden" }

            if phase == ComparisonPhase::Idle {
                rsx! {
                    div {
                        class: "h-full flex items-center justify-center",
                        EmptyState {
                            icon: Icon::GitCompare,
                            title: "Select two notes or versions".to_string(),
                            description: "Choose notes (or revisions) and click Compare to see the diff.".to_string(),
                        }
                    }
                }
            } else if phase == ComparisonPhase::Loading {
                rsx! {
                    div { class: "p-4" }
                    SkeletonList { rows: 8 }
                }
            } else if phase == ComparisonPhase::Error {
                rsx! {
                    div { class: "p-4" }
                    ErrorPanel {
                        title: "Comparison failed".to_string(),
                        message: "Could not compute the diff.".to_string(),
                        details: diff_err,
                        recovery: "Make sure both notes exist and are accessible.".to_string(),
                        on_retry: on_retry,
                    }
                }
            } else if phase == ComparisonPhase::NoDiff {
                rsx! {
                    div {
                        class: "h-full flex items-center justify-center",
                        EmptyState {
                            icon: Icon::TrendingUp,
                            title: "No differences".to_string(),
                            description: "The selected notes or versions are identical.".to_string(),
                        }
                    }
                }
            } else {
                // ComparisonPhase::Loaded
                {
                    let rows = rows_opt.clone().unwrap_or_default();
                    rsx! {
                        DiffView {
                            rows: rows,
                            old_label: label_a,
                            new_label: label_b,
                        }
                    }
                }
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper ──

    fn make_row(kind: DiffKind, text: &str) -> DiffRow {
        DiffRow {
            kind,
            old_line: None,
            new_line: None,
            text: text.to_string(),
        }
    }

    // ── Phase classification ──

    #[test]
    fn phase_idle_before_first_compare() {
        assert_eq!(
            classify_comparison_phase(LoadState::Idle, None, None),
            ComparisonPhase::Idle
        );
    }

    #[test]
    fn phase_loading_during_ipc() {
        assert_eq!(
            classify_comparison_phase(LoadState::Loading, None, None),
            ComparisonPhase::Loading
        );
    }

    #[test]
    fn phase_error_takes_precedence() {
        let rows = vec![make_row(DiffKind::Same, "same")];
        assert_eq!(
            classify_comparison_phase(
                LoadState::Loaded,
                Some("boom"),
                Some(&rows),
            ),
            ComparisonPhase::Error
        );
    }

    #[test]
    fn phase_loaded_with_changes() {
        let rows = vec![
            make_row(DiffKind::Same, "same"),
            make_row(DiffKind::Added, "new"),
        ];
        assert_eq!(
            classify_comparison_phase(LoadState::Loaded, None, Some(&rows)),
            ComparisonPhase::Loaded
        );
    }

    #[test]
    fn phase_no_diff_when_all_same() {
        let rows = vec![
            make_row(DiffKind::Same, "same"),
            make_row(DiffKind::Same, "same2"),
        ];
        assert_eq!(
            classify_comparison_phase(LoadState::Loaded, None, Some(&rows)),
            ComparisonPhase::NoDiff
        );
    }

    #[test]
    fn phase_no_diff_when_empty_result() {
        assert_eq!(
            classify_comparison_phase(LoadState::Loaded, None, Some(&[])),
            ComparisonPhase::NoDiff
        );
    }

    #[test]
    fn phase_no_diff_when_none_result() {
        assert_eq!(
            classify_comparison_phase(LoadState::Loaded, None, None),
            ComparisonPhase::NoDiff
        );
    }

    #[test]
    fn phase_error_on_failed_load() {
        assert_eq!(
            classify_comparison_phase(LoadState::Failed, None, None),
            ComparisonPhase::Error
        );
    }

    // ── has_changes ──

    #[test]
    fn has_changes_detects_added() {
        let rows = vec![make_row(DiffKind::Added, "new line")];
        assert!(has_changes(&rows));
    }

    #[test]
    fn has_changes_detects_removed() {
        let rows = vec![make_row(DiffKind::Removed, "old line")];
        assert!(has_changes(&rows));
    }

    #[test]
    fn has_changes_no_changes_all_same() {
        let rows = vec![
            make_row(DiffKind::Same, "a"),
            make_row(DiffKind::Same, "b"),
        ];
        assert!(!has_changes(&rows));
    }

    #[test]
    fn has_changes_empty() {
        assert!(!has_changes(&[]));
    }

    // ── nonce_is_stale ──

    #[test]
    fn stale_nonce() {
        assert!(super::nonce_is_stale(2, 1));
    }

    #[test]
    fn fresh_nonce() {
        assert!(!super::nonce_is_stale(7, 7));
    }

    // ── revision_label ──

    #[test]
    fn revision_label_with_id() {
        assert_eq!(
            revision_label("notes/file.md", &Some("abc123".to_string())),
            "notes/file.md @ abc123"
        );
    }

    #[test]
    fn revision_label_current() {
        assert_eq!(
            revision_label("notes/file.md", &None),
            "notes/file.md (current)"
        );
    }
}
