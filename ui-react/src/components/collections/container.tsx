// ──────────────────────────────────────────────────────────────────────────────
// collections/container.tsx — collection container (view orchestration)
//
// Mirrors: crates/nabu-ui/src/components/collections/container.rs
// (CollectionContainer)
//
// Manages view state, filtering, and data projection for all four collection
// views. Data is loaded via the `notes_index` Tauri command and projected
// into the active view (Table, Board, Gallery, Calendar).
//
// Views are projections of existing notes — the container never owns data.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useCallback, useRef } from "react";
import { notesIndex } from "../../ipc";
import { useWorkspace, useToast } from "../../context";
import type { NoteIndexEntry } from "../../types";
import type { CollectionItem, CollectionView } from "./shared/types";
import { ViewSwitcher } from "./view_switcher";
import { TableView, type ColumnConfig } from "./table_view";
import { BoardView, type BoardColumn } from "./board_view";
import { GalleryView } from "./gallery_view";
import { CalendarView } from "./calendar_view";
import {
  defaultTableFilter,
  defaultBoardFilter,
  defaultGalleryFilter,
  defaultCalendarFilter,
} from "./shared/types";

// ── Load phase classification ──────────────────────────────────────────────────

type LoadPhase = "loading" | "error" | "empty" | "ready";

/**
 * Maps reactive state to a single phase. Error takes precedence.
 */
function classify(
  loaded: boolean,
  hadError: boolean,
  items: CollectionItem[],
): LoadPhase {
  if (!loaded) return "loading";
  if (hadError) return "error";
  if (items.length === 0) return "empty";
  return "ready";
}

// ── IPC loader ────────────────────────────────────────────────────────────────

/** Loads the note index from the backend via `notes_index`. */
function loadItems(
  setItems: (items: CollectionItem[]) => void,
  setLoaded: (loaded: boolean) => void,
  setHadError: (err: boolean) => void,
  toasts: ReturnType<typeof useToast>,
): void {
  void (async () => {
    try {
      const notes = await notesIndex();
      const items: CollectionItem[] = notes.map((n: NoteIndexEntry) => ({
        path: n.path,
        title: n.title,
        folder: n.folder,
        modified_at: n.modified_at,
        pinned: n.pinned,
      }));
      setItems(items);
    } catch {
      setHadError(true);
      toasts.toast("Couldn't load collections", {
        variant: "error",
      });
    } finally {
      setLoaded(true);
    }
  })();
}

// ── Component ─────────────────────────────────────────────────────────────────

/**
 * Collection container — manages view state, filtering, and data projection
 * for all four collection views (Table, Board, Gallery, Calendar).
 *
 * Data is loaded via the `notes_index` Tauri command and projected into the
 * active view. Views never own data — they are pure projections of
 * `CollectionItem[]`.
 */
export function CollectionContainer() {
  const workspace = useWorkspace();
  const toasts = useToast();

  // ── View state ──
  const [view, setView] = useState<CollectionView>("table");
  const [searchQuery, setSearchQuery] = useState("");
  const [items, setItems] = useState<CollectionItem[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [hadError, setHadError] = useState(false);

  // ── Initial load on mount (signal-guarded) ──
  const initializedRef = useRef(false);
  useEffect(() => {
    if (initializedRef.current) return;
    initializedRef.current = true;
    loadItems(setItems, setLoaded, setHadError, toasts);
  }, [toasts]);

  // ── Derived classification (precompute before JSX) ──
  const phase = classify(loaded, hadError, items);
  const currentItems = items;
  const q = searchQuery;

  // ── Handlers ──

  const onOpen = useCallback(
    (path: string) => {
      workspace.openTab(path);
    },
    [workspace],
  );

  const onRetry = useCallback(() => {
    loadItems(setItems, setLoaded, setHadError, toasts);
  }, [toasts]);

  // ── Pre-compute view-specific filter objects ──
  const tableFilter = defaultTableFilter();
  tableFilter.query = q;
  tableFilter.sort_by = "modified";
  tableFilter.sort_ascending = false;

  const boardColumns: BoardColumn[] = [
    {
      id: "root",
      title: "Root",
      items: currentItems.filter((i) => i.folder === ""),
    },
    {
      id: "folder",
      title: "Folders",
      items: currentItems.filter((i) => i.folder !== ""),
    },
    {
      id: "pinned",
      title: "Pinned",
      items: currentItems.filter((i) => i.pinned),
    },
  ];

  const boardFilter = defaultBoardFilter();
  boardFilter.query = q;
  boardFilter.group_by = "folder";

  const galleryFilter = defaultGalleryFilter();
  galleryFilter.query = q;
  galleryFilter.sort_by = "modified";
  galleryFilter.sort_ascending = false;

  const calendarFilter = defaultCalendarFilter();
  calendarFilter.query = q;
  calendarFilter.view_mode = "month";
  calendarFilter.group_by = "date";

  const tableColumns: ColumnConfig[] = [
    { key: "title", label: "Title", visible: true, sortable: true, width: "flex-1" },
    { key: "folder", label: "Folder", visible: true, sortable: true, width: "w-40" },
    { key: "modified", label: "Modified", visible: true, sortable: true, width: "w-32" },
    { key: "path", label: "Path", visible: false, sortable: true, width: null },
  ];

  // ── Pre-compute phase node ──
  let phaseNode: React.ReactNode;
  switch (phase) {
    case "loading":
      phaseNode = (
        <div className="p-4">
          <div className="animate-pulse space-y-2">
            {[...Array(6)].map((_, i) => (
              <div
                key={i}
                className="h-4 bg-gray-700 rounded"
                style={{ width: `${80 - i * 10}%` }}
              />
            ))}
          </div>
        </div>
      );
      break;
    case "error":
      phaseNode = (
        <div className="p-4">
          <div className="text-red-300">
            <div className="font-medium">Couldn't load collections</div>
            <div className="text-sm mt-1">
              Something went wrong while reading your knowledge objects.
            </div>
            <button
              type="button"
              className="mt-2 px-2 py-1 text-xs text-blue-400 hover:text-blue-300"
              onClick={onRetry}
            >
              Retry
            </button>
          </div>
          <div className="text-xs text-gray-500 mt-1">
            Check that your vault is accessible, then try again.
          </div>
        </div>
      );
      break;
    case "empty":
      phaseNode = (
        <div className="h-full flex items-center justify-center p-6">
          <div className="text-center">
            <div className="text-2xl mb-2">📁</div>
            <div className="text-gray-400">No knowledge objects yet</div>
            <div className="text-sm text-gray-600 mt-1">
              Collections show your structured knowledge once you start adding
              objects.
            </div>
          </div>
        </div>
      );
      break;
    case "ready":
      phaseNode = (() => {
        switch (view) {
          case "table":
            return (
              <TableView
                objects={currentItems}
                columns={tableColumns}
                filter={tableFilter}
                onFilterChange={() => {}}
                onSort={() => {}}
                onOpen={onOpen}
              />
            );
          case "board":
            return (
              <BoardView
                objects={currentItems}
                columns={boardColumns}
                filter={boardFilter}
                onFilterChange={() => {}}
                onMoveItem={() => {}}
                onOpen={onOpen}
              />
            );
          case "gallery":
            return (
              <GalleryView
                objects={currentItems}
                filter={galleryFilter}
                onFilterChange={() => {}}
                onOpen={onOpen}
              />
            );
          case "calendar":
            return (
              <CalendarView
                objects={currentItems}
                filter={calendarFilter}
                onFilterChange={() => {}}
                onOpen={onOpen}
              />
            );
          default:
            return null;
        }
      })();
      break;
  }

  return (
    <div className="collection-container flex h-screen flex-col bg-gray-950 text-gray-100">
      {/* View switcher */}
      <ViewSwitcher
        currentView={view}
        onChange={(v) => setView(v)}
      />

      {/* Search */}
      <div className="flex items-center gap-3 px-4 py-2 border-b border-gray-800">
        <span className="text-xs text-gray-500 ml-2">Search:</span>
        <input
          type="text"
          placeholder="Filter notes..."
          className="flex-1 bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
          value={q}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-hidden">{phaseNode}</div>
    </div>
  );
}
