// ──────────────────────────────────────────────────────────────────────────────
// settings/types.ts — local helper types for the settings panel
//
// `AppSettings` itself lives in the root `types.ts` (wire format mirror of the
// Rust struct in `crates/nabu-ui/src/components/settings/settings_panel.rs`).
// These helpers model the snapshot + change-payload shapes the panel needs.
// ──────────────────────────────────────────────────────────────────────────────

import type { AppSettings } from "../../types";

/**
 * A snapshot of `AppSettings` plus the field that changed and its new value.
 *
 * Mirrors the per-field `settings_set` IPC contract used by the Rust
 * `setting_*` helpers (which call `settings_set_all` after each edit, but the
 * single-key `settings_set` is the wire-level primitive).
 */
export interface SettingsChange {
  key: string;
  value: unknown;
}

/**
 * Full settings payload used when the panel needs to push the entire settings
 * object back to the backend via `settings_set_all`.
 */
export interface SettingsSnapshot extends AppSettings {}

/** A { value, label } option pair for the {@link SettingSelect} helper. */
export interface SelectOption {
  value: string;
  label: string;
}
