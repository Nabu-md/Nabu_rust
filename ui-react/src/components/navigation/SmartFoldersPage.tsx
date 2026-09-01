// ──────────────────────────────────────────────────────────────────────────────
// navigation/SmartFoldersPage.tsx — query-powered virtual collections
//
// Mirrors: ui-react/src/components/navigation/smart_folders.rs
//
// Virtual collections powered by saved queries. The folder list is read from
// the NavContext `smartFolders` signal (persisted in settings). Selecting a
// folder runs the backend `smart_folder_evaluate` command and renders the
// matching notes. New folders are created via the NavContext + backend
// `smart_folder_save`; deleted folders use `smart_folder_delete`.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useCallback, type ReactNode } from "react";
import { Icon } from "../layout/icons";
import { useNav, useWorkspace, useToast } from "../../context";
import {
  smartFoldersList,
  smartFolderSave,
  smartFolderDelete,
  smartFolderEvaluate,
} from "../../ipc";
import type { SmartFolder, NoteIndexEntry } from "../../types";
import type { ViewMode } from "./state";

// ── Load-state lifecycle ────────────────────────────────────────────────────

type LoadState = "loading" | "error" | "loaded";

// ── Helpers ─────────────────────────────────────────────────────────────────

function fmtDate(rfc: string): string {
  try {
    const dt = new Date(rfc);
    if (Number.isNaN(dt.getTime())) return rfc.slice(0, 10);
    return dt.toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return rfc.slice(0, 10);
  }
}

function openNote(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
  path: string,
): void {
  ws.openTab(path);
  nav.setRecentNotes([path, ...nav.recentNotes.filter((p) => p !== path)].slice(0, 20));
  nav.setViewMode("Editor" as ViewMode);
}

// ── SmartFoldersPage main component ─────────────────────────────────────────

/** Smart folders page — virtual collections powered by saved queries. */
export function SmartFoldersPage(): ReactNode {
  const nav = useNav();
  const ws = useWorkspace();
  const toasts = useToast();

  const [selected, setSelected] = useState<SmartFolder | null>(null);
  const [results, setResults] = useState<NoteIndexEntry[]>([]);
  const [resultsState, setResultsState] = useState<LoadState>("loading");
  const [showForm, setShowForm] = useState(false);
  const [newName, setNewName] = useState("");
  const [newQuery, setNewQuery] = useState("");
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmTarget, setConfirmTarget] = useState<SmartFolder | null>(null);

  // Load persisted smart folders from the backend on mount
  useEffect(() => {
    const load = async () => {
      try {
        const list = await smartFoldersList();
        nav.setSmartFolders(list);
      } catch {
        nav.setSmartFolders([]);
      }
    };
    load();
  }, []);

  // Evaluate the selected folder
  const evaluateFolder = useCallback(
    async (folder: SmartFolder) => {
      setResultsState("loading");
      try {
        const list = await smartFolderEvaluate(folder.query);
        setResults(list);
        setResultsState("loaded");
      } catch {
        setResults([]);
        setResultsState("error");
      }
    },
    [],
  );

  // Re-evaluate when selected folder changes
  useEffect(() => {
    if (selected) {
      evaluateFolder(selected);
    } else {
      setResults([]);
      setResultsState("loaded");
    }
  }, [selected, evaluateFolder]);

  // Derive display values
  const folders = nav.smartFolders;
  const heading = selected
    ? `"${selected.name}" — ${results.length} match(es)`
    : "Select a smart folder";

  // ── Handlers ──────────────────────────────────────────────────────────

  const handleCreate = async () => {
    if (!newName.trim() || !newQuery.trim()) return;

    const folder: SmartFolder = {
      id: `sf_${Date.now()}`,
      name: newName.trim(),
      icon: "folderTree",
      query: newQuery.trim(),
      pinned: false,
    };

    try {
      await smartFolderSave(folder);
      // Update NavContext (mirrors save_smart_folder)
      nav.setSmartFolders([...nav.smartFolders.filter((f) => f.id !== folder.id), folder]);
      nav.setRecentNotes(nav.recentNotes); // trigger re-render
      setNewName("");
      setNewQuery("");
      setShowForm(false);
      toasts.toast("Smart folder created", { variant: "success" });
    } catch {
      toasts.toast("Could not create smart folder", { variant: "error" });
    }
  };

  const handleDelete = (folder: SmartFolder) => {
    setConfirmTarget(folder);
    setConfirmOpen(true);
  };

  const handleConfirmDelete = async () => {
    if (!confirmTarget) return;
    try {
      await smartFolderDelete(confirmTarget.id);
      // Update NavContext (mirrors remove_smart_folder)
      nav.setSmartFolders(nav.smartFolders.filter((f) => f.id !== confirmTarget.id));
      if (selected?.id === confirmTarget.id) {
        setSelected(null);
      }
      toasts.toast("Smart folder deleted", { variant: "success" });
    } catch {
      toasts.toast("Could not delete smart folder", { variant: "error" });
    }
    setConfirmOpen(false);
    setConfirmTarget(null);
  };

  // ── Render ────────────────────────────────────────────────────────────

  return (
    <div className="smart-folders-page h-screen overflow-y-auto bg-gray-950 text-gray-100">
      {/* Toolbar */}
      <div className="sf-toolbar flex items-center justify-between px-6 py-4 border-b border-gray-700">
        <h1 className="text-lg font-semibold text-gray-100">Smart Folders</h1>
        <button
          type="button"
          className="sf-new inline-flex items-center gap-1 rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50 border border-gray-700"
          onClick={() => setShowForm(!showForm)}
        >
          <Icon name="plus" className="w-3 h-3" />
          New
        </button>
      </div>

      <div className="sf-body px-6 py-4">
        {/* Create form */}
        {showForm && (
          <div className="sf-form mb-4 flex items-end gap-3 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700">
            <input
              type="text"
              placeholder="Folder name"
              className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
            />
            <input
              type="text"
              placeholder="tag:work folder:projects after:2024-01-01"
              className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
              value={newQuery}
              onChange={(e) => setNewQuery(e.target.value)}
            />
            <button
              type="button"
              className="sf-submit rounded px-3 py-1.5 text-sm text-gray-900 bg-blue-500 hover:bg-blue-400 font-medium"
              onClick={handleCreate}
            >
              Create
            </button>
            <button
              type="button"
              className="sf-cancel rounded px-3 py-1.5 text-sm text-gray-300 hover:bg-gray-700/50 border border-gray-700"
              onClick={() => setShowForm(false)}
            >
              Cancel
            </button>
          </div>
        )}

        {/* Folder list */}
        {folders.length === 0 ? (
          <div className="text-center py-8 text-gray-500">
            <Icon name="folderTree" className="w-8 h-8 mx-auto mb-2 opacity-30" />
            <div className="text-lg font-medium text-gray-300">
              No smart folders yet
            </div>
            <div className="text-xs mt-1">
              Create one to filter your vault by tag, folder, date or text.
            </div>
          </div>
        ) : (
          <div className="sf-list space-y-1">
            {folders.map((f) => (
              <div
                key={f.id}
                className="sf-item flex items-center justify-between px-3 py-2 rounded-lg hover:bg-gray-800/50 border border-gray-700 text-sm group"
              >
                <div
                  className="flex items-center gap-2 flex-1 min-w-0 cursor-pointer"
                  onClick={() => setSelected(f)}
                >
                  <Icon name="folderTree" className="w-4 h-4 text-gray-400 shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-gray-200 truncate">{f.name}</div>
                    <div className="text-xs text-gray-500 truncate">
                      query: {f.query}
                    </div>
                  </div>
                  {f.pinned && (
                    <span className="text-xs text-yellow-400">★</span>
                  )}
                </div>
                <button
                  type="button"
                  className="sf-delete rounded px-1.5 py-0.5 text-xs text-gray-400 hover:text-red-400 opacity-0 group-hover:opacity-100"
                  title="Delete smart folder"
                  onClick={() => handleDelete(f)}
                >
                  <Icon name="x" className="w-3 h-3" />
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Results */}
        <div className="sf-results mt-6">
          <h2 className="text-sm font-semibold text-gray-200 mb-3">{heading}</h2>

          {selected === null ? (
            <div className="text-center py-8 text-gray-500">
              <Icon name="search" className="w-8 h-8 mx-auto mb-2 opacity-30" />
              <div className="text-lg font-medium text-gray-300">
                No folder selected
              </div>
              <div className="text-xs mt-1">
                Click a smart folder to evaluate its query.
              </div>
            </div>
          ) : resultsState === "loading" ? (
            <div className="space-y-1">
              {Array.from({ length: 3 }).map((_, i) => (
                <div
                  key={i}
                  className="h-4 bg-gray-800 rounded animate-pulse"
                />
              ))}
            </div>
          ) : resultsState === "error" ? (
            <div className="text-center py-8 text-gray-500 text-sm">
              <div>Couldn't evaluate the query.</div>
            </div>
          ) : results.length === 0 ? (
            <div className="text-center py-8 text-gray-500">
              <Icon name="search" className="w-8 h-8 mx-auto mb-2 opacity-30" />
              <div className="text-lg font-medium text-gray-300">
                No matches
              </div>
              <div className="text-xs mt-1">Try adjusting the query.</div>
            </div>
          ) : (
            <div className="space-y-1">
              {results.map((n) => {
                const path = n.path;
                return (
                  <div
                    key={path}
                    className="sf-result flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm"
                    onClick={() => openNote(nav, ws, path)}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="text-sm text-gray-200 truncate">{n.title}</div>
                      {n.folder && (
                        <span className="text-xs text-gray-500">{n.folder}/</span>
                      )}
                    </div>
                    <div className="ml-auto text-xs text-gray-500">
                      {fmtDate(n.modified_at)}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Delete confirmation dialog */}
        {confirmOpen && confirmTarget && (
          <div
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
            role="dialog"
            aria-modal="true"
            aria-label="Delete Smart Folder"
          >
            <div className="bg-gray-900 border border-gray-700 rounded-xl p-6 max-w-md w-full mx-4">
              <h3 className="text-lg font-semibold text-white mb-2">
                Delete Smart Folder
              </h3>
              <p className="text-sm text-gray-300 mb-4">
                Remove "{confirmTarget.name}"? This cannot be undone.
              </p>
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  className="px-3 py-1.5 text-sm text-gray-300 hover:bg-gray-800 rounded border border-gray-700"
                  onClick={() => {
                    setConfirmOpen(false);
                    setConfirmTarget(null);
                  }}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className="px-3 py-1.5 text-sm text-red-200 bg-red-900/30 hover:bg-red-900/50 rounded border border-red-700"
                  onClick={handleConfirmDelete}
                >
                  Delete
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
