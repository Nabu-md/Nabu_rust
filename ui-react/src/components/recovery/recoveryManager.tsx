// ──────────────────────────────────────────────────────────────────────────────
// recovery/recoveryManager.tsx — Snapshot Browser / Recovery Manager
//
// Mirrors: crates/nabu-ui/src/components/recovery/recovery_manager.rs
//          (RecoveryManager)
//
// A dedicated screen for inspecting every recoverable snapshot across the
// vault. Lists all notes that have version history, shows retention
// summaries, and provides a per-note timeline with restore / duplicate /
// diff actions (reusing the DiffView from the recovery module).
//
// Consumes: HistoryContext + ToastContext.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useCallback } from "react";
import { useToast } from "../../context";
import type { LoadState, NoteSummary, VersionMeta } from "../../types";
import { versionsAll, versionsList, versionsRestore, versionsDuplicate } from "../../ipc";
import { Icon } from "../layout/icons";

/** Trims a path to its basename for display. */
function basename(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

/** The Recovery Manager screen (ViewMode::Recovery). */
export function RecoveryManagerView() {
  const toasts = useToast();

  // ── State ──
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [notesState, setNotesState] = useState<LoadState>("idle");
  const [notesError, setNotesError] = useState("");
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [versions, setVersions] = useState<VersionMeta[]>([]);
  const [selectedVersion, setSelectedVersion] = useState<string | null>(null);
  const [restoreOpen, setRestoreOpen] = useState(false);

  // ── Async: fetch all notes with snapshots ──
  const fetchNotes = useCallback(async () => {
    setNotesState("loading");
    setNotesError("");
    try {
      const result = await versionsAll();
      setNotes(result);
      setNotesState("loaded");
    } catch (err) {
      setNotesError(
        err instanceof Error ? err.message : "versions_all returned no data.",
      );
      setNotesState("failed");
    }
  }, []);

  // ── Async: expand a note to show its version timeline ──
  const expandNote = useCallback(
    async (path: string) => {
      setNotesState("loading");
      setNotesError("");
      try {
        const vs = await versionsList(path);
        const newest = vs.length > 0 ? vs[vs.length - 1].id : null;
        setExpanded(path);
        setVersions(vs);
        setSelectedVersion(newest);
        setNotesState("loaded");
      } catch (err) {
        setNotesError(
          err instanceof Error ? err.message : "versions_list returned no data.",
        );
        setNotesState("failed");
        setExpanded(null);
        setVersions([]);
      }
    },
    [setExpanded, setVersions, setSelectedVersion],
  );

  // ── Async: restore ──
  const onRestore = useCallback(() => {
    setRestoreOpen(false);
    const path = expanded;
    const id = selectedVersion;
    if (!path || !id) return;

    void (async () => {
      try {
        await versionsRestore(path, id);
        toasts.toast("The note was restored to this version.", {
          variant: "success",
        });
      } catch (err) {
        toasts.toast(`Restore failed: ${String(err)}`, { variant: "error" });
      }
    })();
  }, [expanded, selectedVersion, toasts]);

  // ── Async: duplicate ──
  const onDuplicate = useCallback(
    (path: string, id: string) => {
      const base = basename(path);
      const stem = base.replace(/\.md$/, "");
      const destPath = `${stem}-copy.md`;

      void (async () => {
        try {
          await versionsDuplicate(path, id, destPath);
          toasts.toast("Created a new note from this version.", {
            variant: "success",
          });
        } catch (err) {
          toasts.toast(`Duplicate failed: ${String(err)}`, {
            variant: "error",
          });
        }
      })();
    },
    [toasts],
  );

  // ── Initial load ──
  useEffect(() => {
    void fetchNotes();
  }, [fetchNotes]);

  // ── Derived: filtered notes ──
  const q = search.toLowerCase();
  const filtered: NoteSummary[] =
    search.length === 0
      ? notes
      : notes.filter((n) => n.path.toLowerCase().includes(q));

  return (
    <div className="recovery-manager flex h-screen flex-col bg-gray-950 text-gray-100 overflow-hidden">
      {/* ── Header ── */}
      <div className="px-4 py-3 border-b border-gray-800">
        <h2 className="text-lg font-semibold text-gray-500">Recovery Manager</h2>
        <p className="text-xs text-gray-500">
          Every note with version history, snapshotted automatically on save.
          Restore or duplicate any version — nothing is lost.
        </p>
      </div>

      {/* ── Search ── */}
      <div className="px-4 py-3 border-b border-gray-800">
        <input
          type="text"
          placeholder="Search notes with snapshots…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
        />
      </div>

      {/* ── Content ── */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-4xl mx-auto">
          {notesState === "loading" || notesState === "idle" ? (
            <div className="py-8 flex items-center justify-center">
              <div className="w-6 h-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
              <span className="ml-2 text-sm text-gray-400">
                Scanning for recoverable snapshots…
              </span>
            </div>
          ) : notesState === "failed" ? (
            <div className="p-4">
              <div className="p-4 border border-red-900/50 bg-red-950/30 rounded">
                <h3 className="text-red-300 font-semibold mb-1">
                  Couldn't load recovery data
                </h3>
                <p className="text-sm text-red-300/80 mb-2">
                  Failed to scan for notes with snapshots.
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
                  onClick={() => void fetchNotes()}
                  className="px-3 py-1 text-sm bg-blue-600 rounded hover:bg-blue-500"
                >
                  Retry
                </button>
              </div>
            </div>
          ) : filtered.length === 0 ? (
            <div className="p-6 text-center text-gray-500">
              <Icon name="lifeBuoy" className="w-10 h-10 mx-auto mb-3 opacity-30" />
              <h3 className="text-lg font-medium text-gray-300 mb-1">
                Nothing to recover yet
              </h3>
              <p className="text-sm text-gray-500">
                Notes appear here once they have been saved at least once.
              </p>
            </div>
          ) : (
            <div className="divide-y divide-gray-800 rounded-lg border border-gray-800">
              {filtered.map((note) => {
                const path = note.path;
                const name = basename(path);
                const isExpanded = expanded === path;
                const versionsForNote = isExpanded ? versions : [];
                const selectedVer = isExpanded ? selectedVersion : null;

                return (
                  <div key={path}>
                    {/* Note row */}
                    <div className="flex items-center gap-2 px-3 py-2 hover:bg-gray-800/40 transition-colors">
                      <button
                        type="button"
                        className="flex-1 text-left"
                        onClick={() => {
                          if (isExpanded) {
                            setExpanded(null);
                            setVersions([]);
                            setSelectedVersion(null);
                          } else {
                            void expandNote(path);
                          }
                        }}
                      >
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium truncate">
                            {name}
                          </span>
                          <span className="text-xs px-1.5 py-0.5 rounded bg-gray-700 text-gray-300">
                            {note.version_count} versions
                          </span>
                        </div>
                        <div className="text-xs text-gray-500 mt-0.5 truncate">
                          {path}
                          {note.last_snapshot_at
                            ? ` — last saved ${note.last_snapshot_at}`
                            : ""}
                        </div>
                      </button>
                      {isExpanded && (
                        <Icon
                          name="chevronDown"
                          className="w-4 h-4 text-gray-500"
                        />
                      )}
                    </div>

                    {/* Expanded version timeline */}
                    {isExpanded && (
                      <div className="border-t border-gray-800 bg-gray-900/40">
                        <div className="px-3 py-2 text-xs text-gray-400">
                          Versions (newest first):
                        </div>
                        <div className="divide-y divide-gray-800">
                          {[...versionsForNote].reverse().map((version) => {
                            const id = version.id;
                            const isSel = selectedVer === id;
                            const summary =
                              version.summary || "Untitled version";
                            return (
                              <div
                                key={id}
                                className={`px-3 py-2 flex items-center gap-2 ${
                                  isSel ? "bg-blue-900/20" : ""
                                }`}
                              >
                                <button
                                  type="button"
                                  className="flex-1 text-left"
                                  onClick={() => setSelectedVersion(id)}
                                >
                                  <div className="text-sm">
                                    {summary}
                                  </div>
                                  <div className="text-xs text-gray-500">
                                    {new Date(
                                      version.created_at,
                                    ).toLocaleString()}
                                  </div>
                                </button>
                                <button
                                  type="button"
                                  onClick={() => {
                                    setSelectedVersion(id);
                                    setRestoreOpen(true);
                                  }}
                                  className="px-2 py-0.5 text-xs rounded bg-blue-700/80 hover:bg-blue-600 text-white transition-colors"
                                >
                                  Restore
                                </button>
                                <button
                                  type="button"
                                  onClick={() =>
                                    onDuplicate(path, id)
                                  }
                                  className="px-2 py-0.5 text-xs rounded border border-gray-600 text-gray-300 hover:bg-gray-800 transition-colors"
                                >
                                  Duplicate
                                </button>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>

      {/* ── Restore confirmation dialog ── */}
      {restoreOpen && expanded && selectedVersion && (
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
    </div>
  );
}
