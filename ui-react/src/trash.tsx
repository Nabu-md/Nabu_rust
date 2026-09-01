// ──────────────────────────────────────────────────────────────────────────────
// trash.tsx — Trash / Recycle Bin screen
//
// Mirrors: ui-react/src/components/trash.rs (Trash)
//
// A full-screen view for reviewing and recovering deleted vault items.
// Deleted notes and folders are never destroyed immediately — they live in
// the vault trash (``.nabu/trash``) until the user restores them, the
// retention period elapses, or the user explicitly empties the trash.
//
// Features (ported from spec):
//  - list with per-item preview, deletion date and original location
//  - search, sorting and filtering (All / Notes / Folders / Attachments)
//  - single and multi-select restore / permanent delete
//  - confirmation dialogs for every irreversible action
//  - "undo" toast after restore so an accidental restore can be reversed
//  - keyboard shortcuts: Delete / Backspace (delete selection, confirmed),
//    Cmd/Ctrl+Shift+R (restore selection), Cmd/Ctrl+Shift+Backspace (empty)
//  - listens for the global "nabu:history-changed" event so an undo of a
//    restore or delete is reflected immediately
//
// Consumes: HistoryContext + ToastContext.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useCallback, useRef } from "react";
import type { Dispatch, SetStateAction, MutableRefObject } from "react";
import { useToast, useHistory } from "./context";
import type { TrashRecord } from "./types";
import {
  trashList,
  trashRestoreMany,
  trashDelete,
  trashEmpty,
} from "./ipc";
import { Icon } from "./components/layout/icons";

// ── Types ─────────────────────────────────────────────────────────────────────

/** Load lifecycle of the trash list. */
type TrashLoadState = "loading" | "loaded" | "error";

/** Filter bucket for the trash list. */
type TrashFilter = "all" | "notes" | "folders" | "attachments";

/** Sort key for the trash list. */
type TrashSort = "name" | "deleted_at" | "original" | "size";

// ── Pure helper functions ────────────────────────────────────────────────────

function displayName(record: TrashRecord): string {
  const parts = record.original_path.split("/");
  const name = parts[parts.length - 1];
  return name || "item";
}

function recordIcon(record: TrashRecord): string {
  if (record.is_folder) return "folder";
  if (record.original_path.endsWith(".md")) return "fileText";
  return "fileText";
}

/** Short relative timestamp for display ("5m ago", "3d ago"). */
function relativeTime(rfc3339: string): string {
  const nowMs = Date.now();
  return relativeTimeAt(rfc3339, nowMs);
}

/** Pure version of relativeTime with explicit nowMs. */
function relativeTimeAt(rfc3339: string, nowMs: number): string {
  const parsed = Date.parse(rfc3339);
  if (isNaN(parsed)) return "recently";
  const secs = Math.max(0, Math.floor((nowMs - parsed) / 1000));
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

/** Returns true when keyboard focus is inside an editable element. */
function focusIsEditable(): boolean {
  if (typeof document === "undefined") return false;
  const active = document.activeElement;
  if (!active) return false;
  const tag = active.tagName.toLowerCase();
  if (tag === "textarea" || tag === "input" || tag === "select") {
    return true;
  }
  return (active as HTMLElement).isContentEditable;
}

/** Computes the sorted + filtered + searched list of trash records. */
function sortedView(
  items: TrashRecord[],
  filter: TrashFilter,
  sort: TrashSort,
  ascending: boolean,
  query: string,
): TrashRecord[] {
  const queryLower = query.toLowerCase();
  const result = items.filter((r) => {
    // Filter
    const filterMatch = (() => {
      switch (filter) {
        case "all":
          return true;
        case "notes":
          return !r.is_folder && r.original_path.endsWith(".md");
        case "folders":
          return r.is_folder;
        case "attachments":
          return !r.is_folder && !r.original_path.endsWith(".md");
      }
    })();
    if (!filterMatch) return false;

    // Search
    if (queryLower.length > 0) {
      const name = displayName(r).toLowerCase();
      const orig = r.original_path.toLowerCase();
      const preview = (r.preview ?? "").toLowerCase();
      if (
        !name.includes(queryLower) &&
        !orig.includes(queryLower) &&
        !preview.includes(queryLower)
      ) {
        return false;
      }
    }
    return true;
  });

  result.sort((a, b) => {
    const ord = (() => {
      switch (sort) {
        case "name":
          return displayName(a).localeCompare(displayName(b));
        case "deleted_at":
          return (a.deleted_at ?? "").localeCompare(b.deleted_at ?? "");
        case "original":
          return a.original_path.localeCompare(b.original_path);
        case "size":
          return a.file_count - b.file_count;
      }
    })();
    return ascending ? ord : -ord;
  });

  return result;
}

// ── Async operations (port of fetch_trash, restore_selected, etc.) ─────────

/**
 * Loads the current trash contents from the backend via trash_list.
 * Reconciles selections and preview against the new list.
 */
function fetchTrash(
  setItems: (items: TrashRecord[]) => void,
  setLoadState: (s: TrashLoadState) => void,
  setErrorMsg: (msg: string) => void,
  setSelected: Dispatch<SetStateAction<string[]>>,
  selectedRef: MutableRefObject<string[]>,
  setPreview: (p: string | null) => void,
  previewRef: MutableRefObject<string | null>,
): Promise<void> {
  setLoadState("loading");
  setErrorMsg("");
  return (async () => {
    try {
      const records = await trashList();
      setItems(records);

      // Reconcile selection
      const current = records;
      setSelected((prev) =>
        prev.filter((tp) => current.some((r) => r.trash_path === tp)),
      );
      selectedRef.current = selectedRef.current.filter((tp) =>
        current.some((r) => r.trash_path === tp),
      );

      // Reconcile preview
      const tp = previewRef.current;
      if (tp) {
        if (!current.some((r) => r.trash_path === tp)) {
          setPreview(null);
          previewRef.current = null;
        }
      }

      setLoadState("loaded");
    } catch (err) {
      const msg =
        err instanceof Error
          ? err.message
          : "trash_list returned no data.";
      setErrorMsg(msg);
      setLoadState("error");
    }
  })();
}

// ── Component ────────────────────────────────────────────────────────────────

/** The Trash screen (ViewMode::Trash). */
export function TrashScreen() {
  const toasts = useToast();
  const history = useHistory();

  // ── State ──
  const [items, setItems] = useState<TrashRecord[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [loadState, setLoadState] = useState<TrashLoadState>("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const [filter, setFilter] = useState<TrashFilter>("all");
  const [sort, setSort] = useState<TrashSort>("name");
  const [sortAscending, setSortAscending] = useState(true);
  const [query, setQuery] = useState("");
  const [preview, setPreview] = useState<string | null>(null);

  // Refs for async reconciliation (avoids stale closures)
  const selectedRef = useRef<string[]>([]);
  const previewRef = useRef<string | null>(null);

  // Keep refs in sync
  useEffect(() => {
    selectedRef.current = selected;
  }, [selected]);
  useEffect(() => {
    previewRef.current = preview;
  }, [preview, previewRef]);

  // ── Dialog state ──
  const [deleteSingleOpen, setDeleteSingleOpen] = useState(false);
  const [deleteSelectedOpen, setDeleteSelectedOpen] = useState(false);
  const [emptyOpen, setEmptyOpen] = useState(false);
  const [pendingSingleDelete, setPendingSingleDelete] = useState<string | null>(
    null,
  );

  // ── Initial load ──
  const initialLoadRef = useRef(false);
  useEffect(() => {
    if (initialLoadRef.current) return;
    initialLoadRef.current = true;
    void fetchTrash(
      setItems,
      setLoadState,
      setErrorMsg,
      setSelected,
      selectedRef,
      setPreview,
      previewRef,
    );
  }, []);

  // ── Global keydown listener (Delete, Cmd+Shift+R, Cmd+Shift+Backspace) ──
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (focusIsEditable()) return;

      const meta = e.metaKey || e.ctrlKey;
      const shift = e.shiftKey;
      const key = e.key;

      // Cmd/Ctrl+Shift+R → restore selected
      if (meta && shift && key.toLowerCase() === "r") {
        e.preventDefault();
        if (selectedRef.current.length > 0) {
          void restoreSelectedImpl(selectedRef.current);
        }
        return;
      }

      // Cmd/Ctrl+Shift+Backspace → empty trash
      if (meta && shift && key === "Backspace") {
        e.preventDefault();
        if (items.length > 0) setEmptyOpen(true);
        return;
      }

      // Delete / Backspace → delete selected (no modifier)
      if (!meta && (key === "Delete" || key === "Backspace")) {
        e.preventDefault();
        if (selectedRef.current.length > 0) {
          setDeleteSelectedOpen(true);
        }
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [items]);

  // ── History-change listener: refresh on undo/redo ──
  useEffect(() => {
    const listener = () => {
      void fetchTrash(
        setItems,
        setLoadState,
        setErrorMsg,
        setSelected,
        selectedRef,
        setPreview,
        previewRef,
      );
    };
    window.addEventListener("nabu:history-changed", listener);
    return () =>
      window.removeEventListener("nabu:history-changed", listener);
  }, []);

  // ── Async: restore selected ──
  const restoreSelectedImpl = useCallback(
    async (paths: string[]): Promise<void> => {
      if (paths.length === 0) return;
      const count = paths.length;
      toasts.toast(`Restoring… Restoring ${count} item(s) from Trash`, {
        variant: "info",
      });

      try {
        const restored = await trashRestoreMany(paths);
        if (restored.length > 0) {
          const restoredCount = restored.length;
          toasts.toast(
            `Restored ${restoredCount} item(s)`,
            { variant: "success" },
          );
          // Undo toast: the HistoryContext.undo() call fires its own toast
          // ("Undid: …" or "Nothing to undo") and dispatches the
          // nabu:history-changed event that triggers a refresh below.
          void history.undo();
          // Clear selection and refresh
          setSelected([]);
          selectedRef.current = [];
          await fetchTrash(
            setItems,
            setLoadState,
            setErrorMsg,
            setSelected,
            selectedRef,
            setPreview,
            previewRef,
          );
        } else {
          toasts.toast("No items were restored.", { variant: "error" });
        }
      } catch (err) {
        toasts.toast(
          `Restore failed: ${String(err)}`,
          { variant: "error" },
        );
        await fetchTrash(
          setItems,
          setLoadState,
          setErrorMsg,
          setSelected,
          selectedRef,
          setPreview,
          previewRef,
        );
      }
    },
    [toasts, history],
  );

  // ── Async: restore single ──
  const restoreOneImpl = useCallback(
    async (trashPath: string): Promise<void> => {
      toasts.toast(`Restoring… Restoring 1 item(s) from Trash`, {
        variant: "info",
      });
      try {
        const restored = await trashRestoreMany([trashPath]);
        if (restored.length > 0) {
          toasts.toast(
            `Restored ${restored.length} item(s)`,
            { variant: "success" },
          );
          // Remove from selection
          const tp = trashPath;
          setSelected((prev) => prev.filter((p) => p !== tp));
          selectedRef.current = selectedRef.current.filter((p) => p !== tp);
          await fetchTrash(
            setItems,
            setLoadState,
            setErrorMsg,
            setSelected,
            selectedRef,
            setPreview,
            previewRef,
          );
        } else {
          toasts.toast("Could not restore this item", { variant: "error" });
        }
      } catch (err) {
        toasts.toast(
          `Restore failed: ${String(err)}`,
          { variant: "error" },
        );
        await fetchTrash(
          setItems,
          setLoadState,
          setErrorMsg,
          setSelected,
          selectedRef,
          setPreview,
          previewRef,
        );
      }
    },
    [toasts],
  );

  // ── Async: delete selected ──
  const deleteSelectedImpl = useCallback(async (): Promise<void> => {
    const paths = [...selectedRef.current];
    if (paths.length === 0) return;
    const count = paths.length;
    toasts.toast(`Deleting… Permanently deleting ${count} item(s)`, {
      variant: "info",
    });
    setDeleteSelectedOpen(false);

    try {
      const n = await trashDelete(paths);
      const message =
        n === 1
          ? "Permanently deleted 1 item"
          : `Permanently deleted ${n} items`;
      toasts.toast(message, { variant: "warning" });
      setSelected([]);
      selectedRef.current = [];
      await fetchTrash(
        setItems,
        setLoadState,
        setErrorMsg,
        setSelected,
        selectedRef,
        setPreview,
        previewRef,
      );
    } catch (err) {
      toasts.toast(`Delete failed: ${String(err)}`, { variant: "error" });
      await fetchTrash(
        setItems,
        setLoadState,
        setErrorMsg,
        setSelected,
        selectedRef,
        setPreview,
        previewRef,
      );
    }
  }, [toasts]);

  // ── Async: delete single ──
  const deleteOneImpl = useCallback(
    async (trashPath: string): Promise<void> => {
      try {
        const n = await trashDelete([trashPath]);
        const message =
          n === 1
            ? "Permanently deleted 1 item"
            : `Permanently deleted ${n} items`;
        toasts.toast(message, { variant: "warning" });
        await fetchTrash(
          setItems,
          setLoadState,
          setErrorMsg,
          setSelected,
          selectedRef,
          setPreview,
          previewRef,
        );
      } catch (err) {
        toasts.toast(`Delete failed: ${String(err)}`, { variant: "error" });
        await fetchTrash(
          setItems,
          setLoadState,
          setErrorMsg,
          setSelected,
          selectedRef,
          setPreview,
          previewRef,
        );
      }
    },
    [toasts],
  );

  // ── Async: empty trash ──
  const emptyTrashImpl = useCallback(async (): Promise<void> => {
    toasts.toast("Emptying… Permanently deleting all trashed items", {
      variant: "info",
    });
    setEmptyOpen(false);

    try {
      const n = await trashEmpty();
      const message =
        n === 1
          ? "Emptied trash — 1 item permanently deleted"
          : `Emptied trash — ${n} items permanently deleted`;
      toasts.toast(message, { variant: "warning" });
      setSelected([]);
      selectedRef.current = [];
      await fetchTrash(
        setItems,
        setLoadState,
        setErrorMsg,
        setSelected,
        selectedRef,
        setPreview,
        previewRef,
      );
    } catch (err) {
      toasts.toast(`Empty failed: ${String(err)}`, { variant: "error" });
      await fetchTrash(
        setItems,
        setLoadState,
        setErrorMsg,
        setSelected,
        selectedRef,
        setPreview,
        previewRef,
      );
    }
  }, [toasts]);

  // ── Computed values ──
  const itemsLen = items.length;
  const totalFiles = items.reduce((sum, r) => sum + r.file_count, 0);
  const selectedCount = selected.length;
  const headerCount = `${itemsLen} item(s) · ${totalFiles} file(s)`;

  const sortedItems = sortedView(
    items,
    filter,
    sort,
    sortAscending,
    query,
  );

  const selectAllLabel =
    sortedItems.length > 0 && selectedCount === sortedItems.length
      ? "Deselect all"
      : "Select all";
  const selectedCountText = `${selectedCount} selected`;

  const deleteSelectedMsg =
    selectedCount === 0
      ? "No items selected."
      : `Permanently delete ${selectedCount} selected item(s)? This cannot be undone.`;

  const emptyMsgText =
    itemsLen === 0
      ? "Trash is already empty."
      : `Permanently delete all ${itemsLen} items in Trash? This cannot be undone.`;

  // ── Render ──
  return (
    <div className="trash-screen flex h-screen bg-gray-950 text-gray-100 overflow-hidden">
      {/* ── Left: item list ── */}
      <div className="flex-none w-96 border-r border-gray-800 flex flex-col min-w-0">
        {/* Header */}
        <div className="px-4 py-3 border-b border-gray-800 flex items-center gap-2">
          <Icon name="trash2" className="w-5 h-5 text-gray-400" aria-hidden="true" />
          <h2 className="text-base font-semibold text-gray-50">Trash</h2>
          <span className="text-xs text-gray-500 ml-auto">{headerCount}</span>
        </div>

        {/* Toolbar: Empty Trash */}
        <div className="px-3 py-2 border-b border-gray-800 flex items-center gap-2">
          <button
            type="button"
            onClick={() => setEmptyOpen(true)}
            disabled={itemsLen === 0}
            className="px-2 py-1 text-xs rounded bg-red-700/80 hover:bg-red-700 text-white disabled:opacity-30 disabled:cursor-not-allowed transition-colors flex items-center gap-1"
          >
            <Icon name="trash2" className="w-3 h-3" /> Empty Trash
          </button>
        </div>

        {/* Search */}
        <div className="px-3 py-2 border-b border-gray-800">
          <input
            type="text"
            placeholder="Search trash…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
          />
        </div>

        {/* Filter segmented control */}
        <div className="flex gap-1 px-3 pb-2">
          <button
            type="button"
            onClick={() => setFilter("all")}
            className={
              filter === "all"
                ? "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                : "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
            }
          >
            All
          </button>
          <button
            type="button"
            onClick={() => setFilter("notes")}
            className={
              filter === "notes"
                ? "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                : "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
            }
          >
            Notes
          </button>
          <button
            type="button"
            onClick={() => setFilter("folders")}
            className={
              filter === "folders"
                ? "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                : "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
            }
          >
            Folders
          </button>
          <button
            type="button"
            onClick={() => setFilter("attachments")}
            className={
              filter === "attachments"
                ? "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                : "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200"
            }
          >
            Files
          </button>
        </div>

        {/* Sort dropdown */}
        <div className="px-3 pb-2">
          <select
            value={sort}
            onChange={(e) => {
              const newSort = e.target.value as TrashSort;
              if (sort === newSort) {
                setSortAscending(!sortAscending);
              } else {
                setSort(newSort);
                setSortAscending(true);
              }
            }}
            className="w-full bg-gray-800 text-gray-100 rounded px-2 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
          >
            <option value="name">Sort: Name</option>
            <option value="deleted_at">Sort: Deleted</option>
            <option value="original">Sort: Location</option>
            <option value="size">Sort: Files</option>
          </select>
        </div>

        {/* Batch actions bar */}
        {selectedCount > 0 && (
          <div className="flex items-center gap-2 px-3 py-2 bg-blue-900/30 border-b border-blue-800/30">
            <span className="text-xs text-blue-400">
              {selectedCount} selected
            </span>
            <div className="flex-1" />
            <button
              type="button"
              onClick={() => {
                void restoreSelectedImpl([...selectedRef.current]);
              }}
              className="px-2 py-0.5 text-xs rounded bg-green-700/80 hover:bg-green-600 text-white flex items-center gap-1 transition-colors"
            >
              <Icon name="undo" className="w-3 h-3" /> Restore
            </button>
            <button
              type="button"
              onClick={() => setDeleteSelectedOpen(true)}
              className="px-2 py-0.5 text-xs rounded bg-gray-700 hover:bg-red-700 text-gray-200 hover:text-white transition-colors flex items-center gap-1"
            >
              <Icon name="trash2" className="w-3 h-3" /> Delete
            </button>
          </div>
        )}

        {/* Main list area */}
        <div className="flex-1 overflow-y-auto">
          {loadState === "loading" ? (
            <div className="p-6">
              <div className="space-y-2">
                {Array.from({ length: 6 }).map((_, i) => (
                  <div
                    key={i}
                    className="h-12 bg-gray-800 rounded animate-pulse"
                    style={{ width: `${80 - i * 8}%` }}
                  />
                ))}
              </div>
            </div>
          ) : loadState === "error" ? (
            <div className="p-4">
              <div className="p-4 border border-red-900/50 bg-red-950/30 rounded">
                <h3 className="text-red-300 font-semibold mb-1">
                  Couldn't load Trash
                </h3>
                <p className="text-sm text-red-300/80 mb-2">
                  The backend refused to return the trash manifest.
                </p>
                {errorMsg && (
                  <pre className="text-xs text-red-400/60 bg-red-950/50 p-2 rounded mb-3 overflow-auto">
                    {errorMsg}
                  </pre>
                )}
                <p className="text-xs text-red-300/70 mb-3">
                  Make sure your vault is accessible, then try again.
                </p>
                <button
                  type="button"
                  onClick={() => {
                    void fetchTrash(
                      setItems,
                      setLoadState,
                      setErrorMsg,
                      setSelected,
                      selectedRef,
                      setPreview,
                      previewRef,
                    );
                  }}
                  className="px-3 py-1 text-sm bg-blue-600 rounded hover:bg-blue-500"
                >
                  Retry
                </button>
              </div>
            </div>
          ) : itemsLen === 0 ? (
            <div className="p-6 text-center text-gray-500">
              <Icon name="trash2" className="w-10 h-10 mx-auto mb-3 opacity-30" />
              <h3 className="text-lg font-medium text-gray-300 mb-1">
                Trash is empty
              </h3>
              <p className="text-sm text-gray-500">
                Deleted notes and folders appear here and can be restored
                before they are permanently removed.
              </p>
            </div>
          ) : (
            <div className="divide-y divide-gray-800">
              {sortedItems.map((record) => {
                const trashPath = record.trash_path;
                const is_selected_only = selected.includes(trashPath);
                const is_preview = preview === trashPath;
                const is_folder = record.is_folder;

                let rowClass =
                  "w-full text-left px-3 py-2 cursor-pointer border-l-2 ";
                if (is_selected_only) {
                  rowClass +=
                    "bg-gray-800 border-l-blue-500";
                } else if (is_preview) {
                  rowClass += "bg-gray-900 border-l-gray-500";
                } else {
                  rowClass +=
                    "border-l-transparent hover:bg-gray-800/60";
                }
                rowClass += " transition-colors";

                const icon = recordIcon(record);
                const name = displayName(record);
                const original = record.original_path;
                const deletedAt = record.deleted_at ?? "";
                const deletedAtFull = deletedAt;
                const relative = deletedAt
                  ? relativeTime(deletedAt)
                  : "";
                const fileCount = record.file_count;
                let fileCountText = "";
                if (fileCount > 1) {
                  fileCountText = `${fileCount} files`;
                } else if (fileCount === 1) {
                  fileCountText = `${fileCount} file`;
                }

                return (
                  <div
                    key={trashPath}
                    className={rowClass}
                    onClick={() => setPreview(trashPath)}
                  >
                    <div className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        className="check-field"
                        checked={is_selected_only}
                        onClick={(e) => {
                          e.stopPropagation();
                          const tp = trashPath;
                          setSelected((prev) => {
                            if (prev.includes(tp)) {
                              return prev.filter((t) => t !== tp);
                            }
                            return [...prev, tp];
                          });
                        }}
                      />
                      <Icon
                        name={icon}
                        className="w-4 h-4 text-gray-400"
                        aria-hidden="true"
                      />
                      <span className="text-sm font-medium truncate flex-1">
                        {name}
                      </span>
                      {is_folder && (
                        <span className="text-xs px-1.5 py-0.5 rounded bg-gray-700 text-gray-300">
                          Folder
                        </span>
                      )}
                      {fileCountText && fileCountText.length > 0 && (
                        <span className="text-xs text-gray-500">
                          {fileCountText}
                        </span>
                      )}
                    </div>

                    <div className="flex items-center gap-2 mt-1 pl-7">
                      <span
                        className="text-xs text-gray-500 truncate max-w-52"
                        title={original}
                      >
                        {original}
                      </span>
                      <span className="text-xs text-gray-600">•</span>
                      <span
                        className="text-xs text-gray-500"
                        title={deletedAtFull}
                      >
                        {relative}
                      </span>
                    </div>

                    <div className="flex-1" />
                    <div className="flex gap-1 mt-1">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          void restoreOneImpl(trashPath);
                        }}
                        className="px-2 py-0.5 text-xs rounded bg-green-700/80 hover:bg-green-600 text-white transition-colors"
                      >
                        Restore
                      </button>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setPendingSingleDelete(trashPath);
                          setDeleteSingleOpen(true);
                        }}
                        className="px-2 py-0.5 text-xs rounded bg-gray-700 hover:bg-red-700 text-gray-200 hover:text-white transition-colors"
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Footer / select-all */}
        <div className="flex items-center justify-between px-3 py-2 border-t border-gray-800 text-xs text-gray-500">
          <button
            type="button"
            onClick={() => {
              if (
                sortedItems.length > 0 &&
                selected.length === sortedItems.length
              ) {
                setSelected([]);
              } else {
                setSelected(sortedItems.map((r) => r.trash_path));
              }
            }}
            className="hover:text-gray-300"
          >
            {selectAllLabel}
          </button>
          <span>{selectedCountText}</span>
        </div>
      </div>

      {/* ── Right: preview ── */}
      <div className="flex-1 overflow-y-auto p-4 min-w-0">
        {preview ? (
          (() => {
            const record = items.find((r) => r.trash_path === preview);
            if (!record) {
              return (
                <div className="h-full flex items-center justify-center text-center text-gray-500">
                  <div>
                    <Icon name="eye" className="w-8 h-8 mx-auto mb-2 opacity-30" />
                    <h3 className="text-lg font-medium text-gray-300 mb-1">
                      Item not found
                    </h3>
                    <p className="text-sm text-gray-500">
                      The selected item could not be located.
                    </p>
                  </div>
                </div>
              );
            }

            const name = displayName(record);
            const original = record.original_path;
            const is_folder = record.is_folder;
            const fileCount = record.file_count;
            const previewText = record.preview ?? "";
            const deletePath = record.trash_path;
            const restorePath = record.trash_path;
            const deletedAtFull = record.deleted_at ?? "Unknown";
            const deletedAtRelative = record.deleted_at
              ? relativeTime(record.deleted_at)
              : "Unknown";

            return (
              <div className="trash-preview space-y-4">
                <div className="flex items-start gap-3">
                  <Icon
                    name={recordIcon(record)}
                    className="w-8 h-8 text-gray-400"
                    aria-hidden="true"
                  />
                  <div className="flex-1 min-w-0">
                    <h3 className="text-lg font-semibold text-gray-500 truncate">
                      {name}
                    </h3>
                    <p
                      className="text-xs text-gray-500 truncate"
                      title={original}
                    >
                      {original}
                    </p>
                  </div>
                </div>

                {/* Meta grid */}
                <div className="grid grid-cols-2 gap-3 text-sm">
                  <div>
                    <span className="text-xs text-gray-500 uppercase tracking-wide">
                      Kind
                    </span>
                    <p className="text-gray-300">
                      {is_folder ? "Folder" : "File"}
                    </p>
                  </div>
                  <div>
                    <span className="text-xs text-gray-500 uppercase tracking-wide">
                      Files
                    </span>
                    <p className="text-gray-300">{fileCount}</p>
                  </div>
                  <div>
                    <span className="text-xs text-gray-500 uppercase tracking-wide">
                      Deleted
                    </span>
                    <p
                      className="text-gray-300"
                      title={deletedAtFull}
                    >
                      {deletedAtRelative}
                    </p>
                  </div>
                  <div>
                    <span className="text-xs text-gray-500 uppercase tracking-wide">
                      Original location
                    </span>
                    <p
                      className="text-gray-300 text-xs truncate"
                      title={original}
                    >
                      {original}
                    </p>
                  </div>
                </div>

                {/* Preview text */}
                {!!previewText && previewText.length > 0 ? (
                  <div>
                    <span className="text-xs text-gray-500 uppercase tracking-wide">
                      Preview
                    </span>
                    <div className="mt-1">
                      <pre className="text-xs text-gray-300 bg-gray-900 p-3 rounded-lg overflow-auto max-h-64 whitespace-pre-wrap border border-gray-800">
                        {previewText}
                      </pre>
                    </div>
                  </div>
                ) : is_folder ? (
                  <div className="callout callout-info">
                    This folder and its contents were moved to trash together.
                  </div>
                ) : null}

                {/* Actions */}
                <div className="flex items-center gap-2 pt-3 border-t border-gray-800">
                  <button
                    type="button"
                    onClick={() => void restoreOneImpl(restorePath)}
                    className="px-2 py-1 text-xs rounded bg-green-700/80 hover:bg-green-600 text-white flex items-center gap-1 transition-colors"
                  >
                    <Icon name="undo" className="w-3 h-3" /> Restore
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setPendingSingleDelete(deletePath);
                      setDeleteSingleOpen(true);
                    }}
                    className="px-2 py-1 text-xs rounded bg-gray-700 hover:bg-red-700 text-gray-200 hover:text-white flex items-center gap-1 transition-colors"
                  >
                    <Icon name="trash2" className="w-3 h-3" /> Delete Forever
                  </button>
                </div>
              </div>
            );
          })()
        ) : (
          <div className="h-full flex items-center justify-center text-center text-gray-500">
            <div>
              <Icon name="eye" className="w-8 h-8 mx-auto mb-2 opacity-30" />
              <h3 className="text-lg font-medium text-gray-300 mb-1">
                Select an item
              </h3>
              <p className="text-sm text-gray-500">
                Preview items, see where they came from, restore, or delete
                permanently.
              </p>
            </div>
          </div>
        )}
      </div>

      {/* ── Confirmation dialogs ── */}

      {/* Delete single */}
      {deleteSingleOpen && pendingSingleDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-gray-800 border border-gray-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-gray-100 mb-2">
              Permanently delete this item?
            </h3>
            <p className="text-sm text-gray-300 mb-4">
              {(() => {
                const tp = pendingSingleDelete;
                const rec = items.find((r) => r.trash_path === tp);
                if (rec) {
                  const dn = displayName(rec);
                  return `'${dn}' will be permanently deleted (${rec.file_count} file(s)). This cannot be undone.`;
                }
                return "This item will be permanently deleted. This cannot be undone.";
              })()}
            </p>
            <div className="flex items-center gap-2 justify-end">
              <button
                type="button"
                onClick={() => {
                  setDeleteSingleOpen(false);
                  setPendingSingleDelete(null);
                }}
                className="px-3 py-1 text-sm rounded border border-gray-600 text-gray-300 hover:bg-gray-700"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => {
                  setDeleteSingleOpen(false);
                  const tp = pendingSingleDelete;
                  setPendingSingleDelete(null);
                  if (tp) {
                    void deleteOneImpl(tp);
                  }
                }}
                className="px-3 py-1 text-sm rounded bg-red-700 hover:bg-red-600 text-white"
              >
                Delete Forever
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Delete selected */}
      {deleteSelectedOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-gray-800 border border-gray-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-gray-100 mb-2">
              Delete selected items?
            </h3>
            <p className="text-sm text-gray-300 mb-4">{deleteSelectedMsg}</p>
            <div className="flex items-center gap-2 justify-end">
              <button
                type="button"
                onClick={() => setDeleteSelectedOpen(false)}
                className="px-3 py-1 text-sm rounded border border-gray-600 text-gray-300 hover:bg-gray-700"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void deleteSelectedImpl()}
                className="px-3 py-1 text-sm rounded bg-red-700 hover:bg-red-600 text-white"
              >
                Delete Forever
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Empty trash */}
      {emptyOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-gray-800 border border-gray-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-gray-100 mb-2">
              Empty Trash?
            </h3>
            <p className="text-sm text-gray-300 mb-4">{emptyMsgText}</p>
            <div className="flex items-center gap-2 justify-end">
              <button
                type="button"
                onClick={() => setEmptyOpen(false)}
                className="px-3 py-1 text-sm rounded border border-gray-600 text-gray-300 hover:bg-gray-700"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void emptyTrashImpl()}
                className="px-3 py-1 text-sm rounded bg-red-700 hover:bg-red-600 text-white"
              >
                Empty Trash
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
