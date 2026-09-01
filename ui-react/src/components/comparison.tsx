// ──────────────────────────────────────────────────────────────────────────────
// comparison.tsx — side-by-side diff view
//
// Mirrors: ui-react/src/components/shipped/comparison.rs (ComparisonView)
//
// Supports two modes:
//  - **Two Notes** — diff any two notes by vault-relative path (`notes_diff`).
//  - **Revisions** — diff two versions of the same note (`versions_diff`).
//
// Both modes surface loading / success / empty / error states and use the
// canonical IPC + LoadState patterns established by note_editor.rs and
// version_history.rs. A race-safety nonce guards against stale IPC results.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback } from "react";
import {
  notesDiff,
  versionsDiff,
  versionsList,
} from "../ipc";
import { useNav, useToast } from "../context";
import type {
  DiffRow,
  VersionMeta,
  NoteIndexEntry,
  LoadState,
} from "../types";
import { nonceIsStale } from "../types";
import { DiffView } from "./shipped/DiffView";

// ── Types ─────────────────────────────────────────────────────────────────────

/** Two comparison modes: compare any two notes, or compare revisions of one. */
type CompareMode = "notes" | "revisions";

/** Load phase of the diff result. */
type ComparisonPhase = "idle" | "loading" | "nodiff" | "error" | "loaded";

// ── Pure helper functions ────────────────────────────────────────────────────

/**
 * Returns `true` if the diff rows contain at least one Added or Removed
 * row — i.e. the two notes/versions are not identical.
 */
function hasChanges(rows: DiffRow[]): boolean {
  return rows.some((r) => r.kind !== "same");
}

/**
 * Pure-classification: maps the comparison's reactive state to a single phase.
 *
 * Error takes precedence over every other state.
 */
function classifyComparisonPhase(
  diffState: LoadState,
  loadError: string | null,
  diffRows: DiffRow[] | null,
): ComparisonPhase {
  if (loadError && loadError.length > 0) {
    return "error";
  }
  switch (diffState) {
    case "idle":
      return "idle";
    case "loading":
      return "loading";
    case "failed":
      return "error";
    case "loaded":
      return diffRows && hasChanges(diffRows) ? "loaded" : "nodiff";
    default:
      return "idle";
  }
}

/** Formats a revision label for display in the DiffView header. */
function revisionLabel(path: string, version: string | null): string {
  return version ? `${path} @ ${version}` : `${path} (current)`;
}

// ── Versions load helper ─────────────────────────────────────────────────────

/**
 * Loads the versions list for a note via `versions_list`. A nonce guards
 * against stale results when the user quickly switches notes in Revisions mode.
 */
function loadVersionsForComparison(
  path: string,
  nonce: { current: number },
  setVersions: (v: VersionMeta[]) => void,
  setVersionsState: (v: LoadState) => void,
  setVersionsError: (v: string | null) => void,
): void {
  nonce.current += 1;
  const thisNonce = nonce.current;

  setVersionsState("loading");
  setVersionsError(null);

  void (async () => {
    try {
      const v = await versionsList(path);
      if (nonceIsStale(nonce.current, thisNonce)) return;
      setVersions(v);
      setVersionsState("loaded");
    } catch (err) {
      if (nonceIsStale(nonce.current, thisNonce)) return;
      setVersionsError(
        err instanceof Error ? err.message : "versions_list returned no data.",
      );
      setVersionsState("failed");
    }
  })();
}

// ── Component ──────────────────────────────────────────────────────────────────

/**
 * Side-by-side Comparison view (`ViewMode::Comparison`).
 *
 * Supports two modes:
 * - **Two Notes** — diff any two notes by vault-relative path (`notes_diff`).
 * - **Revisions** — diff two versions of the same note (`versions_diff`).
 */
export function ComparisonView() {
  const nav = useNav();
  const toasts = useToast();

  // ── Mode ──
  const [mode, setMode] = useState<CompareMode>("notes");

  // ── Diff result ──
  const [diffState, setDiffState] = useState<LoadState>("idle");
  const [diffRows, setDiffRows] = useState<DiffRow[] | null>(null);
  const [diffError, setDiffError] = useState<string | null>(null);
  const diffNonceRef = useRef(0);

  // ── Notes mode selections ──
  const [noteA, setNoteA] = useState("");
  const [noteB, setNoteB] = useState("");

  // ── Revisions mode selections ──
  const [revisionPath, setRevisionPath] = useState("");
  const [versions, setVersions] = useState<VersionMeta[]>([]);
  const [versionsState, setVersionsState] = useState<LoadState>("idle");
  const [versionsError, setVersionsError] = useState<string | null>(null);
  const versionsNonceRef = useRef(0);
  const [versionA, setVersionA] = useState<string | null>(null);
  const [versionB, setVersionB] = useState<string | null>(null);

  // ── Load versions when revision_path changes ──
  useEffect(() => {
    if (!revisionPath) return;
    loadVersionsForComparison(
      revisionPath,
      versionsNonceRef,
      setVersions,
      setVersionsState,
      setVersionsError,
    );
  }, [revisionPath]);

  // ── Compare button handler ──
  const onCompare = useCallback(() => {
    const canCompare =
      mode === "notes"
        ? !!noteA && !!noteB
        : !!revisionPath;

    if (!canCompare) {
      toasts.toast("Select two notes or a revision to compare.", {
        variant: "info",
      });
      return;
    }

    setDiffState("loading");
    setDiffError(null);
    setDiffRows(null);
    diffNonceRef.current += 1;
    const thisNonce = diffNonceRef.current;

    void (async () => {
      try {
        let rows: DiffRow[];
        if (mode === "notes") {
          rows = await notesDiff(noteA, noteB);
        } else {
          rows = await versionsDiff(
            revisionPath,
            versionA,
            versionB,
          );
        }
        if (nonceIsStale(diffNonceRef.current, thisNonce)) return;
        setDiffRows(rows);
        setDiffState("loaded");
      } catch (err) {
        if (nonceIsStale(diffNonceRef.current, thisNonce)) return;
        setDiffError(
          err instanceof Error
            ? err.message
            : "No data returned.",
        );
        setDiffState("failed");
      }
    })();
  }, [mode, noteA, noteB, revisionPath, versionA, versionB]);

  // ── Retry handler ──
  const onRetry = useCallback(() => {
    void onCompare();
  }, [onCompare]);

  // ── Mode toggle handlers ──
  const switchToNotes = useCallback(() => {
    setMode("notes");
    setDiffState("idle");
    setDiffRows(null);
    setDiffError(null);
  }, []);

  const switchToRevisions = useCallback(() => {
    setMode("revisions");
    setDiffState("idle");
    setDiffRows(null);
    setDiffError(null);
  }, []);

  // ── Pre-compute render values ──
  const notes: NoteIndexEntry[] = nav.notesIndex;
  const selA = noteA;
  const selB = noteB;
  const revPath = revisionPath;
  const versionsList = versions;
  const vs = versionsState;
  const ve = versionsError ?? "";
  const selVersionA = versionA;
  const selVersionB = versionB;

  const diffStateVal = diffState;
  const diffErrOpt = diffError;
  const rowsOpt = diffRows;

  const phase = classifyComparisonPhase(
    diffStateVal,
    diffErrOpt,
    rowsOpt,
  );

  const labelA =
    mode === "notes"
      ? selA || "Note A"
      : revisionLabel(revPath, selVersionA);

  const labelB =
    mode === "notes"
      ? selB || "Note B"
      : revisionLabel(revPath, selVersionB);

  const diffCount =
    rowsOpt?.filter((r) => r.kind !== "same").length ?? 0;

  // Versions loading sub-phase (for Revisions mode)
  const versionsLoading =
    vs === "loading" || vs === "idle" || vs === "failed";

  return (
    <div className="comparison-view flex flex-col h-full bg-gray-950 text-gray-100 overflow-hidden">
      {/* ── Header ── */}
      <div className="flex-none px-4 py-3 border-b border-gray-800">
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-sm font-semibold text-gray-300">
            Comparison View
          </h2>
          <div className="flex items-center gap-2" />
        </div>

        {/* Mode toggle */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={switchToNotes}
            className={
              mode === "notes"
                ? "px-3 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                : "px-3 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
            }
          >
            Two Notes
          </button>
          <button
            type="button"
            onClick={switchToRevisions}
            className={
              mode === "revisions"
                ? "px-3 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                : "px-3 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
            }
          >
            Revisions
          </button>
        </div>

        {/* Selection controls */}
        {mode === "notes" ? (
          <div className="grid grid-cols-2 gap-3 mt-3">
            <div>
              <label className="text-xs text-gray-500 uppercase tracking-wide">
                Note A
              </label>
              <select
                className="w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700 mt-1"
                value={selA}
                onChange={(ev) => setNoteA(ev.target.value)}
              >
                <option value="">Select note A…</option>
                {notes.map((note) => (
                  <option key={`a-${note.path}`} value={note.path}>
                    {note.title}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-xs text-gray-500 uppercase tracking-wide">
                Note B
              </label>
              <select
                className="w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700 mt-1"
                value={selB}
                onChange={(ev) => setNoteB(ev.target.value)}
              >
                <option value="">Select note B…</option>
                {notes.map((note) => (
                  <option key={`b-${note.path}`} value={note.path}>
                    {note.title}
                  </option>
                ))}
              </select>
            </div>
          </div>
        ) : (
          <>
            <div className="grid grid-cols-3 gap-3 mt-3">
              <div>
                <label className="text-xs text-gray-500 uppercase tracking-wide">
                  Note
                </label>
                <select
                  className="w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700 mt-1"
                  value={revPath}
                  onChange={(ev) => {
                    setRevisionPath(ev.target.value);
                    setVersionA(null);
                    setVersionB(null);
                  }}
                >
                  <option value="">Select note…</option>
                  {notes.map((note) => (
                    <option key={note.path} value={note.path}>
                      {note.title}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label className="text-xs text-gray-500 uppercase tracking-wide">
                  Version A
                </label>
                <select
                  className="w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700 mt-1"
                  value={selVersionA ?? ""}
                  onChange={(ev) => {
                    const val = ev.target.value;
                    setVersionA(val || null);
                  }}
                >
                  <option value="">Current</option>
                  {!versionsLoading &&
                    versionsList.slice().reverse().map((v) => (
                      <option key={`a-${v.id}`} value={v.id}>
                        {`${v.created_at} (${v.char_count} chars)`}
                      </option>
                    ))}
                </select>
              </div>

              <div>
                <label className="text-xs text-gray-500 uppercase tracking-wide">
                  Version B
                </label>
                <select
                  className="w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700 mt-1"
                  value={selVersionB ?? ""}
                  onChange={(ev) => {
                    const val = ev.target.value;
                    setVersionB(val || null);
                  }}
                >
                  <option value="">Current</option>
                  {!versionsLoading &&
                    versionsList.slice().reverse().map((v) => (
                      <option key={`b-${v.id}`} value={v.id}>
                        {`${v.created_at} (${v.char_count} chars)`}
                      </option>
                    ))}
                </select>
              </div>
            </div>

            {mode === "revisions" && ve && (
              <div className="mt-2 text-xs text-red-400/80">{ve}</div>
            )}
          </>
        )}

        {/* Action bar */}
        <div className="flex items-center justify-between mt-3">
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onCompare}
              className="px-3 py-1.5 text-sm bg-blue-600 rounded hover:bg-blue-500"
            >
              Compare
            </button>
            {diffCount > 0 && (
              <span className="text-xs text-gray-400">
                {diffCount} differences
              </span>
            )}
          </div>
        </div>
      </div>

      {/* ── Diff content ── */}
      <div className="flex-1 overflow-hidden">
        {phase === "idle" && (
          <div className="h-full flex items-center justify-center">
            <div className="text-center">
              <div className="text-3xl mb-2">🔍</div>
              <h3 className="text-lg font-medium text-gray-300 mb-1">
                Select two notes or versions
              </h3>
              <p className="text-sm text-gray-500">
                Choose notes (or revisions) and click Compare to see the diff.
              </p>
            </div>
          </div>
        )}

        {phase === "loading" && (
          <div className="p-4">
            <div className="space-y-2">
              {Array.from({ length: 8 }).map((_, i) => (
                <div
                  key={i}
                  className="h-3 bg-gray-800 rounded animate-pulse"
                  style={{ width: `${80 - i * 8}%` }}
                />
              ))}
            </div>
          </div>
        )}

        {phase === "error" && (
          <div className="p-4">
            <div className="p-4 border border-red-900/50 bg-red-950/30 rounded">
              <h3 className="text-red-300 font-semibold mb-1">
                Comparison failed
              </h3>
              <p className="text-sm text-red-300/80 mb-2">
                Could not compute the diff.
              </p>
              {diffErrOpt && (
                <pre className="text-xs text-red-400/60 bg-red-950/50 p-2 rounded mb-3 overflow-auto">
                  {diffErrOpt}
                </pre>
              )}
              <p className="text-xs text-red-300/70 mb-3">
                Make sure both notes exist and are accessible.
              </p>
              <button
                type="button"
                onClick={onRetry}
                className="px-3 py-1 text-sm bg-blue-600 rounded hover:bg-blue-500"
              >
                Retry
              </button>
            </div>
          </div>
        )}

        {phase === "nodiff" && (
          <div className="h-full flex items-center justify-center">
            <div className="text-center">
              <div className="text-3xl mb-2">✅</div>
              <h3 className="text-lg font-medium text-gray-300 mb-1">
                No differences
              </h3>
              <p className="text-sm text-gray-500">
                The selected notes or versions are identical.
              </p>
            </div>
          </div>
        )}

        {phase === "loaded" && rowsOpt && (
          <div className="p-4 overflow-auto">
            <DiffView rows={rowsOpt} oldLabel={labelA} newLabel={labelB} />
          </div>
        )}
      </div>
    </div>
  );
}
