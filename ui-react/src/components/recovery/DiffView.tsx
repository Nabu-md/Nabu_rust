// ──────────────────────────────────────────────────────────────────────────────
// recovery/DiffView.tsx — side-by-side diff row renderer
//
// Mirrors: crates/nabu-ui/src/components/recovery/diff_view.rs (DiffView)
//
// Renders DiffRow[] returned by the backend versions_diff / notes_diff
// commands as a two-column comparison. Each row shows old/new line numbers,
// a marker (+ / − / blank), and the text content with a CSS class that
// reflects the diff kind (added / removed / same).
//
// Pure presentational component — no IPC, no state.
// ──────────────────────────────────────────────────────────────────────────────

import type { DiffRow } from "../../types";

/** Props for {@link DiffView}. */
export interface DiffViewProps {
  /** The diff rows to render. */
  rows: DiffRow[];
  /** Label for the left (old) column. */
  oldLabel: string;
  /** Label for the right (new) column. */
  newLabel: string;
}

/**
 * Side-by-side diff viewer.
 *
 * Pure presentational component — no IPC, no state. Consumes DiffRow[]
 * from the parent (VersionHistoryView / ComparisonView).
 */
export function DiffView({ rows, oldLabel, newLabel }: DiffViewProps) {
  return (
    <div className="diff-view">
      <div className="diff-headers">
        <div className="diff-header diff-header-old">{oldLabel}</div>
        <div className="diff-header diff-header-new">{newLabel}</div>
      </div>

      <div className="diff-body">
        {rows.map((row, i) => {
          const display = row.text.length === 0 ? " " : row.text;
          const oldLine = row.old_line ?? "";
          const newLine = row.new_line ?? "";

          // CSS classes per diff kind (matching the Dioxus reference).
          let oldClass: string;
          let newClass: string;
          let tag: string;
          if (row.kind === "added") {
            oldClass = "diff-cell dim";
            newClass = "diff-cell diff-added";
            tag = "+";
          } else if (row.kind === "removed") {
            oldClass = "diff-cell diff-removed";
            newClass = "diff-cell dim";
            tag = "−";
          } else {
            oldClass = "";
            newClass = "";
            tag = "";
          }

          // The marker shows only on the side that changed.
          const markOld = row.kind === "added" ? "" : tag;
          const markNew = row.kind === "removed" ? "" : tag;

          return (
            <div key={i} className="diff-row">
              <div className={`diff-cell diff-old ${oldClass}`}>
                <span className="diff-lineno">{oldLine}</span>
                <span className="diff-mark" aria-hidden="true">
                  {markOld}
                </span>
                <span className="diff-text">{display}</span>
              </div>
              <div className={`diff-cell diff-new ${newClass}`}>
                <span className="diff-lineno">{newLine}</span>
                <span className="diff-mark" aria-hidden="true">
                  {markNew}
                </span>
                <span className="diff-text">{display}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
