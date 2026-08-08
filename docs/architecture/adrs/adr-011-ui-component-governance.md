# ADR-011 — UI Component Governance

**Status:** Accepted
**Date:** 2026-08-08
**Source:** Distilled from AUDIT_0.6, AUDIT_0.7

---

## Context

The frontend (`crates/nabu-ui`, Dioxus 0.6.3 / CSR) is organized as
feature components. AUDIT-0.6 traced the working flows (open note →
editor, session persistence, autosave); AUDIT-0.7 inventoried dead and
orphaned components.

The migration left several components declared in `components/mod.rs`
but **never instantiated or rendered** anywhere in `src/`. Keeping them
is a confusion trap — contributors cannot tell which components are live
feature surfaces versus scaffolding.

## Decision

1. **Remove or rewire dead components.** The following were found
   unreferenced (declared in `mod.rs`, zero render/`use` points) and must
   either be wired into a feature or deleted:

   | Component | File | Lines | Verdict noted by audit |
   |-----------|------|-------|------------------------|
   | `RelationEditor` | `relation_editor.rs` | 276 | Remove, or finish wiring into `graph_view` |
   | `TemplatePicker` | `template_picker.rs` | 78 | Remove (template editing lives in `template_editor.rs`) |
   | `PdfViewer` | `pdf_viewer.rs` | 15 | Remove (no frontend PDF feature) |
   | `SandboxContainer` | `sandbox.rs` | 32 | Remove |
   | `SandboxedHtml` | `sandboxed_html.rs` | 8 | Remove |
   | `ThemeToggle` | `theme_toggle.rs` | 24 | Remove (theme via settings panel) |

2. **Single state model.** The UI inherits one state approach — a
   workspace/context signal layer — and does not mix two state-management
   patterns per feature. New components subscribe to the shared signals
   and IPC bridge (ADR-008) rather than local-only mirrors.

3. **Documented IPC as the source of truth.** The frontend talks to the
   backend only through `#[tauri::command]`s (ADR-006). A UI-only
   "capability" that bypasses IPC for reads is a violation.

## Rationale

- Six orphan components (~430 lines) bloating the component tree obscure
  which UI is real and inviting new contributors to extend already-dead
  scaffolds.
- A single state model matches Dioxus `Signal` conventions already used
  and avoids the competing-patterns problem AUDIT-0.7 found in the
  backend.
- Keeping the IPC boundary strict preserves the typed contract that
  makes the frontend replaceable.

## Alternatives Considered

- **Keep the dead components** — rejected; they are not exercised, so
  their "purpose" is misleading and they rot silently.
- **Auto-wire all six into the interface** — rejected; several (PdfViewer
  PDF viewing, RelationEditor visual graph editing) represent features
  that do not yet exist, so wiring would be fabrication.

## Consequences

- The dead components are candidates for removal; the actual deletion is
  a code change to be validated per-component (each has a real dependency
  in `mod.rs` and may be referenced by tests).
- New UI work follows the shared-signal + IPC contract above.
- This ADR does **not** delete code by itself — it establishes the
  governance rule. Removal/rewire should be tracked as implementation
  tickets with the shared design.

## Verification note

The six components were listed as un-referenced at the time AUDIT-0.7
ran. Before removal, re-verify each has no production render path with a
fresh grep for its component name; a component may have been wired since
the audit.

## Future Implications

- Each UI feature owns exactly one living component; dead scaffolds are
  removed rather than preserved.
- Future component needs (PDF viewing, graph relationship editing,
  sandboxed HTML) are implemented on demand, referencing these ADR
  decisions and the shared state model.