// ──────────────────────────────────────────────────────────────────────────────
// shipped/index.ts — barrel re-exports for shipped views
//
// Mirrors: ui-react/src/components/shipped/mod.rs
//
// Re-exports the Reader and Comparison view components and the shared
// `nonceIsStale` helper so consumers can import from a single path.
// ──────────────────────────────────────────────────────────────────────────────

export { ReaderView } from "../reader";
export { ComparisonView } from "../comparison";
export { DiffView } from "./DiffView";

// Re-export types used by shipped views
export type { DiffKind, DiffRow, LoadState } from "../../types";
