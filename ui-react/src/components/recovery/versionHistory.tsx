// ──────────────────────────────────────────────────────────────────────────────
// recovery/versionHistory.tsx — Version History screen
//
// Mirrors: ui-react/src/components/recovery/version_history.rs (VersionHistory)
//
// Browse the snapshots of a note: list versions, preview their content, diff
// two revisions, restore an old version (undoable), duplicate it as a new
// note, and capture a manual snapshot.
//
// Layout:
//  - left:  the snapshot browser — every note that has versions
//  - middle: the version timeline for the selected note
//  - right: preview + diff of the selected version vs. another / current
//
// Consumes: HistoryContext (for undo after restore) + ToastContext.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useCallback } from "react";
import { useToast } from "../../context";
import type { LoadState } from "../../types";
import type {
  DiffRow,
  VersionMeta,
  NoteSummary,
} from "../../types";
import {
  versionsAll,
  versionsList,
  versionsGet,
  versionsRestore,
  versionsDuplicate,
  versionsDiff,
  snapshotCreate,
} from "../../ipc";
import { DiffView } from "./DiffView";
import { Icon } from "../layout/icons";

// ── Pure helper functions ────────────────────────────────────────────────────

/** Short relative timestamp for display ("5m ago", "3d ago"). */
export function relativeTime(rfc3339: string): string {
  const nowMs = Date.now();
  return relativeTimeAt(rfc3339, nowMs);
}

/** Pure version of relativeTime that accepts an explicit nowMs (epoch millis). */
export function relativeTimeAt(rfc3339: string, nowMs: number): string {
  const parsed = Date.parse(rfc3339);
  if (isNaN(parsed)) return "recently";
  const thenMs = parsed;
  const secs = Math.max(0, Math.floor((nowMs - thenMs) / 1000));
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

/** Absolute timestamp formatted directly. */
export function absoluteTime(rfc3339: string): string {
  const parsed = Date.parse(rfc3339);
  if (isNaN(parsed)) return rfc3339;
  const d = new Date(parsed);
  const month = d.toLocaleString("en-US", { month: "short" });
  const day = String(d.getDate()).padStart(2, " ");
  return `${month} ${day}, ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** Human-readable byte size. */
export function humanSize(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

// ── Component ────────────────────────────────────────────────────────────────

/** The Version History screen (ViewMode::History). */
export function VersionHistoryView() {
  const toasts = useToast();

  // ── State ──
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [notesState, setNotesState] = useState<LoadState>("idle");
  const [notesError, setNotesError] = useState("");

  const [selectedNote, setSelectedNote] = useState<string | null>(null);
  const [versions, setVersions] = useState<VersionMeta[]>([]);
  const [versionsState, setVersionsState] = useState<LoadState>("idle");
  const [versionsError, setVersionsError] = useState("");
  const [selectedVersion, setSelectedVersion] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<string | null>(null);
  const [diffRows, setDiffRows] = useState<DiffRow[] | null>(null);

  const [restoreOpen, setRestoreOpen] = useState(false);
  const [duplicateOpen, setDuplicateOpen] = useState(false);
  const [duplicateDest, setDuplicateDest] = useState("");

  // ── Async: fetch all notes with snapshots ──────────────────────────────────
  const fetchAllNotes = useCallback(async () => {
    setNotesState("loading");
    setNotesError("");
    try {
      const summaries = await versionsAll();
      setNotes(summaries);
      setNotesState("loaded");

      // Reconcile: if the selected note is no longer present, clear it.
      const sel = selectedNote;
      if (sel && !summaries.some((n) => n.path === sel)) {
        setSelectedNote(null);
        setVersions([]);
        setPreviewContent(null);
        setSelectedVersion(null);
        setDiffRows(null);
      }
    } catch (err) {
      setNotesError(
        err instanceof Error ? err.message : "versions_all returned no data.",
      );
      setNotesState("failed");
    }
  }, [selectedNote]);

  // ── Async: load versions for a note ────────────────────────────────────────
  const loadVersions = useCallback(
    async (path: string) => {
      setVersionsState("loading");
      setVersionsError("");
      setVersions([]);
      setPreviewContent(null);
      setDiffRows(null);

      try {
        const vs = await versionsList(path);
        setVersions(vs);
        const newest = vs.length > 0 ? vs[vs.length - 1].id : null;
        setSelectedVersion(newest);
        setVersionsState("loaded");

        if (newest) {
          // Load preview
          try {
            const content = await versionsGet(path, newest);
            setPreviewContent(content);
            setSelectedVersion(newest);
          } catch {
            // Preview failed — leave as null, version list is still loaded.
          }
        }
      } catch (err) {
        setVersionsError(
          err instanceof Error
            ? err.message
            : "versions_list returned no data.",
        );
        setVersionsState("failed");
      }
    },
    [setVersions, setSelectedVersion, setPreviewContent, setDiffRows],
  );

  // ── Async: preview a version ───────────────────────────────────────────────
  const previewVersion = useCallback(
    async (path: string, id: string) => {
      try {
        const content = await versionsGet(path, id);
        setSelectedVersion(id);
        setPreviewContent(content);
        setDiffRows(null);
      } catch (err) {
        toasts.toast(`Could not load version: ${String(err)}`, {
          variant: "error",
        });
      }
    },
    [toasts],
  );

  // ── Async: diff vs current ─────────────────────────────────────────────────
  const fetchDiff = useCallback(
    async (path: string, fromId: string) => {
      try {
        const rows = await versionsDiff(path, fromId, null);
        setDiffRows(rows);
      } catch {
        toasts.toast("Could not compute the diff", { variant: "error" });
      }
    },
    [toasts],
  );

  // ── Async: create snapshot ─────────────────────────────────────────────────
  const onSnapshot = useCallback(() => {
    const path = selectedNote;
    if (!path) {
      toasts.toast("Select a note first", { variant: "info" });
      return;
    }
    void (async () => {
      try {
        await snapshotCreate(path);
        toasts.toast(`Captured a snapshot of '${path}'`, {
          variant: "success",
        });
        loadVersions(path);
      } catch {
        toasts.toast("Could not create a snapshot", {
          variant: "error",
        });
      }
    })();
  }, [selectedNote, toasts, loadVersions]);

  // ── Async: restore version ─────────────────────────────────────────────────
  const onRestore = useCallback(() => {
    setRestoreOpen(false);
    const path = selectedNote;
    const id = selectedVersion;
    if (!path || !id) return;

    void (async () => {
      try {
        await versionsRestore(path, id);
        toasts.toast("The note was restored to this version.", {
          variant: "success",
        });
        loadVersions(path);
      } catch (err) {
        toasts.toast(`Restore failed: ${String(err)}`, { variant: "error" });
      }
    })();
  }, [selectedNote, selectedVersion, toasts, loadVersions]);

  // ── Async: duplicate version ───────────────────────────────────────────────
  const onDuplicate = useCallback(() => {
    setDuplicateOpen(false);
    const dest = duplicateDest.trim();
    if (!dest) return;

    const path = selectedNote;
    const id = selectedVersion;
    if (!path || !id) return;

    void (async () => {
      try {
        await versionsDuplicate(path, id, dest);
        toasts.toast("Created a new note from this version.", {
          variant: "success",
        });
      } catch (err) {
        toasts.toast(`Duplicate failed: ${String(err)}`, { variant: "error" });
      }
    })();
  }, [duplicateDest, selectedNote, selectedVersion, toasts]);

  // ── Initial load ──
  useEffect(() => {
    const doLoad = async () => {
      setNotesState("loading");
      setNotesError("");
      try {
        const summaries = await versionsAll();
        setNotes(summaries);
        setNotesState("loaded");
      } catch (err) {
        setNotesError(
          err instanceof Error ? err.message : "versions_all returned no data.",
        );
        setNotesState("failed");
      }
    };
    void doLoad();
  }, []);

  // ── Derived values ──
  const notesLen = notes.length;
  const selectedNoteName =
    selectedNote !== null
      ? (() => {
          const parts = selectedNote.split("/");
          return parts[parts.length - 1] || selectedNote;
        })()
      : "Select a note";
  const noNoteSelected = selectedNote === null;
  const versionsLen = versions.length;

  return (
    <div className="version-history flex h-screen bg-gray-950 text-gray-100 overflow-hidden">
      {/* ── Left: snapshot browser ── */}
      <div className="flex-none w-72 border-r border-gray-800 flex flex-col min-w-0">
        <div className="px-4 py-3 border-b border-gray-800">
          <h2 className="text-base font-semibold text-gray-500">
            Version History
          </h2>
          <p className="text-xs text-gray-500">
            {notesLen} notes with snapshots
          </p>
        </div>

        <div className="flex-1 overflow-y-auto">
          {notesState === "loading" || notesState === "idle" ? (
            <div className="p-6 flex items-center justify-center">
              <div className="w-5 h-5 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
              <span className="ml-2 text-sm text-gray-400">
                Loading snapshot browser…
              </span>
            </div>
          ) : notesState === "failed" ? (
            <div className="p-4">
              <div className="p-4 border border-red-900/50 bg-red-950/30 rounded">
                <h3 className="text-red-300 font-semibold mb-1">
                  Couldn't load version history
                </h3>
                <p className="text-sm text-red-300/80 mb-2">
                  Failed to load the list of notes with snapshots.
                </p>
                {notesError && (
                  <pre className="text-xs text-red-400/60 bg-red-950/50 p-2 rounded mb-3 overflow-auto">
                    {notesError}
                  </pre>
                )}
                <p className="text-xs text-red-300/70 mb-3">
                  Make sure your vault is accessible and the backend is running.
                </p>
                <button
                  type="button"
                  onClick={() => void fetchAllNotes()}
                  className="px-3 py-1 text-sm bg-blue-600 rounded hover:bg-blue-500"
                >
                  Retry
                </button>
              </div>
            </div>
          ) : (
            <>
              {notesLen === 0 ? (
                <div className="p-6 text-center text-gray-500">
                  <div className="text-3xl mb-2">
                    <Icon name="history" className="w-8 h-8 mx-auto opacity-30" />
                  </div>
                  <h3 className="text-lg font-medium text-gray-300 mb-1">
                    No snapshots yet
                  </h3>
                  <p className="text-sm text-gray-500">
                    Save a note and it will appear here with version history.
                  </p>
                </div>
              ) : (
                <div className="divide-y divide-gray-800">
                  {notes.map((note) => {
                    const path = note.path;
                    const parts = path.split("/");
                    const name = parts[parts.length - 1] || path;
                    const is_selected = selectedNote === path;
                    return (
                      <button
                        key={path}
                        type="button"
                        className={`w-full text-left px-3 py-2 hover:bg-gray-800/60 transition-colors ${
                          is_selected
                            ? "bg-gray-800 border-l-2 border-l-blue-500"
                            : "border-l-2 border-l-transparent"
                        }`}
                        onClick={() => {
                          setSelectedNote(path);
                          void loadVersions(path);
                        }}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-sm font-medium truncate">
                            {name}
                          </span>
                          <span className="text-xs px-1.5 py-0.5 rounded bg-gray-700 text-gray-300">
                            {note.version_count}
                          </span>
                        </div>
                        <div className="text-xs text-gray-500 mt-0.5 truncate">
                          {note.last_snapshot_at
                            ? relativeTime(note.last_snapshot_at)
                            : ""}
                        </div>
                      </button>
                    );
                  })}
                </div>
              )}
            </>
          )}
        </div>
      </div>

      {/* ── Middle: version timeline ── */}
      <div className="flex-none w-80 border-r border-gray-800 flex flex-col min-w-0">
        <div className="px-4 py-3 border-b border-gray-800 flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold text-gray-300 truncate">
            {selectedNoteName}
          </h3>
          <button
            type="button"
            onClick={onSnapshot}
            className="px-2 py-1 text-xs rounded bg-blue-700 hover:bg-blue-600 text-white transition-colors"
            title="Capture a manual snapshot of the current content"
          >
            Snapshot
          </button>
        </div>

        <div className="flex-1 overflow-y-auto">
          {versionsState === "loading" || versionsState === "idle" ? (
            <div className="p-6 flex items-center justify-center">
              <div className="w-5 h-5 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
              <span className="ml-2 text-sm text-gray-400">
                Loading versions…
              </span>
            </div>
          ) : versionsState === "failed" ? (
            <div className="p-4">
              <div className="p-4 border border-red-900/50 bg-red-950/30 rounded">
                <h3 className="text-red-300 font-semibold mb-1">
                  Couldn't load versions
                </h3>
                <p className="text-sm text-red-300/80 mb-2">
                  Failed to load the version timeline for this note.
                </p>
                {versionsError && (
                  <pre className="text-xs text-red-400/60 bg-red-950/50 p-2 rounded mb-3 overflow-auto">
                    {versionsError}
                  </pre>
                )}
                <button
                  type="button"
                  onClick={() => {
                    if (selectedNote) {
                      void loadVersions(selectedNote);
                    }
                  }}
                  className="px-3 py-1 text-sm bg-blue-600 rounded hover:bg-blue-500"
                >
                  Retry
                </button>
              </div>
            </div>
          ) : noNoteSelected ? (
            <div className="px-4 py-3 text-xs text-gray-500">
              Select a note to see its version timeline.
            </div>
          ) : versionsLen === 0 ? (
            <div className="px-4 py-3 text-xs text-gray-500">
              No versions recorded yet.
            </div>
          ) : (
            <div className="divide-y divide-gray-800">
              {[...versions].reverse().map((version) => {
                const id = version.id;
                const is_selected = selectedVersion === id;
                const summary = version.summary || "Untitled version";
                const createdAbs = absoluteTime(version.created_at);
                const createdRel = relativeTime(version.created_at);
                const sizeText = humanSize(version.size);
                const charCount = `${version.char_count} chars`;
                return (
                  <button
                    key={id}
                    type="button"
                    className={`w-full text-left px-3 py-2 hover:bg-gray-800/60 transition-colors ${
                      is_selected
                        ? "bg-gray-800 border-l-2 border-l-blue-500"
                        : "border-l-2 border-l-transparent"
                    }`}
                    onClick={() => {
                      if (selectedNote) {
                        void previewVersion(selectedNote, id);
                      }
                    }}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-medium">{summary}</span>
                      {version.manual && (
                        <span className="text-xs px-1.5 py-0.5 rounded bg-blue-900/50 text-blue-300">
                          Manual
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-gray-500 mt-0.5">
                      {createdAbs} <span className="mx-1">•</span> {createdRel}
                    </div>
                    <div className="text-xs text-gray-600">
                      {sizeText} <span className="mx-1">•</span> {charCount}
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </div>

      {/* ── Right: preview + diff ── */}
      <div className="flex-1 overflow-y-auto p-4 min-w-0">
        {diffRows ? (
          <div className="space-y-3">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-sm font-semibold text-gray-300">Diff</h3>
              <button
                type="button"
                onClick={() => setDiffRows(null)}
                className="px-2 py-1 text-xs rounded bg-gray-700 hover:bg-gray-600 text-gray-200 transition-colors"
              >
                <Icon name="x" className="w-3 h-3" /> Close
              </button>
            </div>
            <DiffView
              rows={diffRows}
              oldLabel="Version"
              newLabel="Current / Other"
            />
          </div>
        ) : previewContent ? (
          <div className="space-y-3">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <h3 className="text-sm font-semibold text-gray-300">Preview</h3>
              <div className="flex items-center gap-2 flex-wrap">
                <button
                  type="button"
                  onClick={() => {
                    if (selectedNote && selectedVersion) {
                      void fetchDiff(selectedNote, selectedVersion);
                    }
                  }}
                  className="px-2 py-1 text-xs rounded bg-gray-700 hover:bg-gray-600 text-gray-200 transition-colors"
                >
                  Diff vs current
                </button>
                <button
                  type="button"
                  onClick={() => setRestoreOpen(true)}
                  className="px-2 py-1 text-xs rounded bg-red-700 hover:bg-red-600 text-white transition-colors"
                >
                  Restore this version
                </button>
                <button
                  type="button"
                  onClick={() => setDuplicateOpen(true)}
                  className="px-2 py-1 text-xs rounded border border-gray-600 text-gray-300 hover:bg-gray-800 transition-colors"
                >
                  Duplicate…
                </button>
              </div>
            </div>
            <pre className="text-xs text-gray-300 bg-gray-900 p-3 rounded-lg overflow-auto max-h-96 whitespace-pre-wrap border border-gray-800">
              {previewContent}
            </pre>
          </div>
        ) : (
          <div className="h-full flex items-center justify-center text-center text-gray-500">
            <div>
              <Icon name="eye" className="w-8 h-8 mx-auto mb-2 opacity-30" />
              <h3 className="text-lg font-medium text-gray-300 mb-1">
                Select a note and a version
              </h3>
              <p className="text-sm text-gray-500">
                Preview the content, compare revisions, restore, or duplicate it.
              </p>
            </div>
          </div>
        )}
      </div>

      {/* ── Restore confirmation dialog ── */}
      {restoreOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-gray-800 border border-gray-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-gray-100 mb-2">
              Restore this version?
            </h3>
            <p className="text-sm text-gray-300 mb-4">
              The note will be replaced with this snapshot. The current content
              is snapshotted first, and you can undo the restore.
            </p>
            <div className="flex items-center gap-2 justify-end">
              <button
                type="button"
                onClick={() => setRestoreOpen(false)}
                className="px-3 py-1 text-sm rounded border border-gray-600 text-gray-300 hover:bg-gray-700"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={onRestore}
                className="px-3 py-1 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white"
              >
                Restore
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Duplicate prompt dialog ── */}
      {duplicateOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-gray-800 border border-gray-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-gray-100 mb-2">
              Duplicate version as new note
            </h3>
            <p className="text-sm text-gray-400 mb-2">
              Enter the new note path (e.g. copy-of-note.md):
            </p>
            <input
              type="text"
              value={duplicateDest}
              onChange={(e) => setDuplicateDest(e.target.value)}
              className="w-full bg-gray-900 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none mb-4"
              placeholder="copy-of-note.md"
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  void onDuplicate();
                }
              }}
            />
            <div className="flex items-center gap-2 justify-end">
              <button
                type="button"
                onClick={() => {
                  setDuplicateOpen(false);
                  setDuplicateDest("");
                }}
                className="px-3 py-1 text-sm rounded border border-gray-600 text-gray-300 hover:bg-gray-700"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void onDuplicate()}
                className="px-3 py-1 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white"
              >
                Duplicate
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
