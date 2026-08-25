// ──────────────────────────────────────────────────────────────────────────────
// Inbox.tsx — split-pane inbox view (Dioxus → React port)
//
// Mirrors: crates/nabu-ui/src/components/inbox.rs (`Inbox` component)
//
// Layout: left = queue (search + batch bar + sortable list), right = preview
//  (tabbed details/duplicate/timeline/ocr/history + per-item actions + metadata).
//
// Behaviour ported from the spec:
//   • mounts by fetching the queue via `inbox_subscribe`
//   • click = toggle selection, double-click = open preview
//   • batch approve/reject/retry/delete for selected items
//   • keyboard shortcuts (Cmd+A select-all, A/R/D batch, Space toggle, Enter
//     approve, Cmd+Shift+arrows reverse sort)
//   • native file drag-and-drop capture (capture_file_drop) on the root pane
//   • dnd-kit sortable queue (drag to reorder, PointerSensor)
//
// Consumes: NavContext (vault name for the empty state) + ToastProvider.
// ──────────────────────────────────────────────────────────────────────────────

import {
  useCallback,
  useEffect,
  useState,
  type KeyboardEvent,
  type DragEvent as ReactDragEvent,
} from "react";
import { useNav, useToast } from "../../context";
import {
  inboxSubscribe,
  inboxApprove,
  inboxReject,
  inboxRetry,
  inboxDelete,
  inboxBatchApprove,
  inboxBatchReject,
  inboxBatchDelete,
  inboxBatchRetry,
  captureFileDrop,
} from "../../ipc";
import type { InboxItem } from "../../types";
import type { SortField } from "./types";
import { InboxQueue } from "./InboxQueue";
import { InboxPreview } from "./InboxPreview";
import { InboxMetadataSidebar } from "./InboxMetadataSidebar";
import { QuickCapture } from "./QuickCapture";
import { InboxIcon } from "./icons";
import { filterAndSort, isTextInputTarget, SORT_FIELD_LABELS } from "./utils";

// ── Shared action helper ─────────────────────────────────────────────────────

/**
 * Runs a single-item IPC action, refreshes the queue on success, and toasts an
 * error (with a label) on failure.  Mirrors the per-item helpers in inbox.rs.
 */
async function runAction(
  label: string,
  fn: () => Promise<unknown>,
  toast: (
    msg: string,
    opts?: { variant?: "error" | "success"; duration?: number | null },
  ) => void,
  onRefresh: () => void,
): Promise<void> {
  try {
    await fn();
    onRefresh();
  } catch {
    toast(`Could not ${label.toLowerCase()} that capture`, { variant: "error" });
  }
}

// ── Component ─────────────────────────────────────────────────────────────────

/** Main inbox split-pane component. */
export function Inbox() {
  const { toast } = useToast();
  const nav = useNav();

  // ── State ──────────────────────────────────────────────────────────────
  const [items, setItems] = useState<InboxItem[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [previewId, setPreviewId] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [sortBy, setSortBy] = useState<SortField>("timestamp");
  const [sortAscending, setSortAscending] = useState(true);
  const [dragOver, setDragOver] = useState(false);

  // ── Refresh (re-fetch queue) ───────────────────────────────────────────
  const reload = useCallback(async (): Promise<void> => {
    try {
      const data = await inboxSubscribe();
      setItems(data);
      setSelectedIds(new Set());
      setPreviewId(null);
    } catch {
      toast("Could not load the inbox queue", { variant: "error" });
    }
  }, [toast]);

  // ── Selection helpers ──────────────────────────────────────────────────
  const toggleSelect = useCallback((id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const selectAll = useCallback(
    (checked: boolean) => {
      setSelectedIds(checked ? new Set(items.map((i) => i.id)) : new Set());
    },
    [items],
  );

  // ── Sort change ────────────────────────────────────────────────────────
  const onSortChange = useCallback((field: SortField) => {
    setSortBy((prev) => {
      if (prev === field) {
        setSortAscending((a) => !a);
        return prev;
      }
      setSortAscending(true);
      return field;
    });
  }, []);

  // ── Single-item actions ────────────────────────────────────────────────
  const approveItem = useCallback(
    (id: string) => runAction("Approve", () => inboxApprove(id), toast, reload),
    [reload, toast],
  );
  const rejectItem = useCallback(
    (id: string) =>
      runAction("Reject", () => inboxReject(id, "User rejected"), toast, reload),
    [reload, toast],
  );
  const retryItem = useCallback(
    (id: string) => runAction("Retry", () => inboxRetry(id), toast, reload),
    [reload, toast],
  );
  const deleteItem = useCallback(
    (id: string) => runAction("Delete", () => inboxDelete(id), toast, reload),
    [reload, toast],
  );

  // ── Batch actions ──────────────────────────────────────────────────────
  const batchApprove = useCallback(
    async (ids: string[]) => {
      try {
        await inboxBatchApprove(ids);
        await reload();
      } catch {
        toast("Could not approve the selected captures", { variant: "error" });
      }
    },
    [reload, toast],
  );
  const batchReject = useCallback(
    async (ids: string[]) => {
      try {
        await inboxBatchReject(ids, "User rejected");
        await reload();
      } catch {
        toast("Could not reject the selected captures", { variant: "error" });
      }
    },
    [reload, toast],
  );
  const batchDelete = useCallback(
    async (ids: string[]) => {
      try {
        await inboxBatchDelete(ids);
        await reload();
      } catch {
        toast("Could not delete the selected captures", { variant: "error" });
      }
    },
    [reload, toast],
  );
  const batchRetry = useCallback(
    async (ids: string[]) => {
      try {
        await inboxBatchRetry(ids);
        await reload();
      } catch {
        toast("Could not retry the selected captures", { variant: "error" });
      }
    },
    [reload, toast],
  );

  // ── File-drop capture (native HTML5 drag & drop) ──────────────────────
  const captureFile = useCallback(
    async (file: File) => {
      try {
        const buf = await file.arrayBuffer();
        const data = Array.from(new Uint8Array(buf));
        const mime = file.type || "application/octet-stream";
        const id = await captureFileDrop(file.name, mime, data);
        toast(`Captured "${file.name}" to inbox (#${id})`, { variant: "success" });
        void reload();
      } catch {
        toast("Could not capture the dropped file", { variant: "error" });
      }
    },
    [reload, toast],
  );

  const handleDragOver = useCallback((e: ReactDragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: ReactDragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
  }, []);

  const handleDrop = useCallback(
    (e: ReactDragEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.stopPropagation();
      setDragOver(false);
      const files = e.dataTransfer.files;
      for (let i = 0; i < files.length; i++) {
        const file = files.item(i);
        if (file) {
          void captureFile(file);
        }
      }
    },
    [captureFile],
  );

  // ── Reorder (dnd-kit sortable — operates on the full items array) ─────
  const handleReorder = useCallback(
    (activeId: string, overId: string) => {
      if (activeId === overId) {
        return;
      }
      setItems((prev) => {
        const oldIndex = prev.findIndex((i) => i.id === activeId);
        const newIndex = prev.findIndex((i) => i.id === overId);
        if (oldIndex === -1 || newIndex === -1) {
          return prev;
        }
        return arrayMoveLocal(prev, oldIndex, newIndex);
      });
    },
    [],
  );

  // ── Keyboard shortcuts (mirrors inbox.rs `on_keydown`) ─────────────────
  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (isTextInputTarget(e.target)) {
      return;
    }
    const meta = e.metaKey || e.ctrlKey;
    const k = e.key.toLowerCase();

    // Cmd/Ctrl+A → toggle select all
    if (meta && k === "a") {
      e.preventDefault();
      const all = items.every((i) => selectedIds.has(i.id));
      selectAll(!all);
      return;
    }

    // A → batch approve
    if (!meta && k === "a") {
      e.preventDefault();
      const ids = Array.from(selectedIds);
      if (ids.length > 0) {
        void batchApprove(ids);
      }
      return;
    }

    // R → batch reject
    if (!meta && k === "r") {
      e.preventDefault();
      const ids = Array.from(selectedIds);
      if (ids.length > 0) {
        void batchReject(ids);
      }
      return;
    }

    // D → batch delete
    if (!meta && k === "d") {
      e.preventDefault();
      const ids = Array.from(selectedIds);
      if (ids.length > 0) {
        void batchDelete(ids);
      }
      return;
    }

    // Space → toggle selection of the previewed item
    if (!meta && (e.key === " " || e.key === "Spacebar")) {
      e.preventDefault();
      if (previewId) {
        toggleSelect(previewId);
      }
      return;
    }

    // Enter / → ArrowRight → approve previewed item
    if (!meta && (e.key === "Enter" || e.key === "ArrowRight")) {
      e.preventDefault();
      if (previewId) {
        void approveItem(previewId);
      }
      return;
    }

    // Cmd/Ctrl+Shift+ArrowLeft/Right → reverse sort direction
    if (meta && e.shiftKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      setSortAscending((a) => !a);
    }
  };

  // Refresh once on mount.
  useEffect(() => {
    void reload();
  }, [reload]);

  // ── Derived ────────────────────────────────────────────────────────────
  const filteredItems = filterAndSort(items, filter, sortBy, sortAscending);
  const selectedCount = selectedIds.size;
  const previewItem = previewId
    ? items.find((i) => i.id === previewId) ?? null
    : null;
  const dragOverClass = dragOver ? "drag-over" : "";
  const vaultName = nav.vaultName || "vault";
  const folderSuggestions = uniqueFolders(items);

  return (
    <div
      className={`inbox flex h-screen flex-col bg-gray-950 text-gray-100 overflow-hidden relative ${dragOverClass}`}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {/* ── Top bar: search + sort ── */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-800 flex-none">
        <div className="relative flex-1">
          <InboxIcon
            name="search"
            className="absolute left-2 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-500"
          />
          <input
            type="text"
            placeholder="Search inbox…"
            className="w-full bg-gray-800 text-gray-100 rounded pl-8 pr-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <SortButton sortBy={sortBy} ascending={sortAscending} onSortChange={onSortChange} />
      </div>

      {/* ── Batch action bar ── */}
      {selectedCount > 0 && (
        <div className="flex items-center gap-2 px-3 py-2 bg-blue-900/30 border-b border-blue-800/30 flex-none">
          <span className="text-xs text-blue-400">{selectedCount} selected</span>
          <button
            type="button"
            onClick={() => void batchApprove(Array.from(selectedIds))}
            className="px-2 py-1 text-xs text-white bg-green-700 rounded hover:bg-green-600"
          >
            Approve
          </button>
          <button
            type="button"
            onClick={() => void batchReject(Array.from(selectedIds))}
            className="px-2 py-1 text-xs text-white bg-red-700 rounded hover:bg-red-600"
          >
            Reject
          </button>
          <button
            type="button"
            onClick={() => void batchRetry(Array.from(selectedIds))}
            className="px-2 py-1 text-xs text-white bg-yellow-700 rounded hover:bg-yellow-600"
          >
            Retry
          </button>
          <button
            type="button"
            onClick={() => void batchDelete(Array.from(selectedIds))}
            className="px-2 py-1 text-xs text-white bg-gray-700 rounded hover:bg-gray-600"
          >
            Delete
          </button>
        </div>
      )}

      {/* ── Split pane: queue + preview ── */}
      <div className="flex-1 flex overflow-hidden">
        <InboxQueue
          items={filteredItems}
          selectedIds={selectedIds}
          onSelect={toggleSelect}
          onPreview={setPreviewId}
          onReorder={handleReorder}
        />

        <div className="flex-1 flex flex-col overflow-hidden border-l border-gray-800">
          {previewItem ? (
            <>
              <InboxPreview
                key={previewItem.id}
                item={previewItem}
                onApprove={() => void approveItem(previewItem.id)}
                onReject={() => void rejectItem(previewItem.id)}
                onRetry={() => void retryItem(previewItem.id)}
                onDelete={() => void deleteItem(previewItem.id)}
              />
              <InboxMetadataSidebar
                key={previewItem.id}
                item={previewItem}
                onRefresh={() => void reload()}
                folderSuggestions={folderSuggestions}
              />
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center p-6">
              <div className="text-center text-gray-500">
                <InboxIcon name="inbox" className="w-10 h-10 mx-auto mb-2" />
                <p className="text-sm">
                  Select or double-click a capture to preview it.
                </p>
                <p className="text-xs mt-1">
                  Drop files anywhere in the inbox to capture them into{" "}
                  <span className="text-gray-400">{vaultName}</span>.
                </p>
              </div>
            </div>
          )}
        </div>
      </div>

      <QuickCapture onRefresh={() => void reload()} />
    </div>
  );
}

// ── Inline helpers ───────────────────────────────────────────────────────────

/** Derive unique destination folders from inbox items for autocomplete. */
function uniqueFolders(items: InboxItem[]): string[] {
  const set = new Set<string>();
  for (const item of items) {
    if (item.suggested_folder) {
      set.add(item.suggested_folder);
    }
  }
  return Array.from(set);
}

/** Stable array-move (dnd-kit `arrayMove` is used inside the queue list). */
function arrayMoveLocal<T>(arr: T[], from: number, to: number): T[] {
  const result = arr.slice();
  const [removed] = result.splice(from, 1);
  result.splice(to, 0, removed!);
  return result;
}

/** Sort-field dropdown button wired to the keyboard-friendly sort cycle. */
interface SortButtonProps {
  sortBy: SortField;
  ascending: boolean;
  onSortChange: (field: SortField) => void;
}

function SortButton({ sortBy, ascending, onSortChange }: SortButtonProps) {
  const arrow = ascending ? "chevron-up" : "chevron-down";
  return (
    <div className="relative">
      <select
        value={sortBy}
        onChange={(e) => onSortChange(e.target.value as SortField)}
        className="appearance-none bg-gray-800 text-gray-200 text-xs rounded px-2 py-1 border border-gray-700 focus:border-blue-500 focus:outline-none pr-6"
      >
        {Object.entries(SORT_FIELD_LABELS).map(([value, label]) => (
          <option key={value} value={value}>
            {label}
          </option>
        ))}
      </select>
      <InboxIcon
        name={arrow}
        className="absolute right-1 top-1/2 -translate-y-1/2 w-3 h-3 text-gray-500 pointer-events-none"
      />
    </div>
  );
}
