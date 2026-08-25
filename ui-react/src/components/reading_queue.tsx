// ──────────────────────────────────────────────────────────────────────────────
// reading_queue.tsx — Reading Queue view (React port)
//
// Mirrors: crates/nabu-ui/src/components/reading_queue.rs (ReadingQueue)
//
// Real reading queue backed by the `queue_*` Tauri commands. The queue is a
// projection of persisted KnowledgeObjects — the view never owns data.
//
// Data flow:
//   - load:  queue_get_all → QueueItem[]
//   - mutate: queue_set_status / queue_set_priority / queue_set_progress /
//             queue_batch_set_status
//
// Mutations route through the real backend and the queue is reconciled from
// backend truth after every successful change. A failed mutation is surfaced
// (toast) and the UI is NOT advanced as though persistence succeeded.
//
// Consumes: ToastContext (useToast), NavContext (useNav)
// Provides: <ReadingQueue />
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useState, useCallback } from "react";
import { Icon } from "./layout/icons";
import { useToast } from "../context";
import {
  queueGetAll,
  queueSetStatus,
  queueSetPriority,
  queueSetProgress,
  queueBatchSetStatus,
} from "../ipc";
import type { QueueItem, QueueStatus, QueuePriority } from "../types";

// ── Types (mirror backend QueueStatus / QueuePriority wire format) ───────────

export type { QueueStatus, QueuePriority };

/** Sort field for the queue filter. Mirrors QueueSortField in the Dioxus spec. */
export type QueueSortField = "modified_at" | "title" | "priority" | "progress" | "status";

export interface QueueFilter {
  status: QueueStatus | null;
  priority: QueuePriority | null;
  search: string;
  sort_by: QueueSortField;
  sort_ascending: boolean;
}

/** Lifecycle phase of the queue view, derived from load state + payload. */
export type QueueViewPhase = "loading" | "empty" | "error" | "ready";

// ── Label helpers ───────────────────────────────────────────────────────────

/** Wire-format label consumed by the backend `QueueStatus::from_label`. */
export function statusLabel(s: QueueStatus): string {
  return s; // "unread" | "reading" | "completed" | "archived"
}

/** Human-readable display label for a status. */
export function statusDisplayLabel(s: QueueStatus): string {
  switch (s) {
    case "reading":
      return "Reading";
    case "completed":
      return "Done";
    case "archived":
      return "Archived";
    default:
      return "Unread";
  }
}

/** Inverse of `label`, mirroring the backend parser. */
export function statusFromLabel(label: string): QueueStatus {
  switch (label) {
    case "reading":
      return "reading";
    case "completed":
      return "completed";
    case "archived":
      return "archived";
    default:
      return "unread";
  }
}

/** Wire-format label consumed by the backend `QueuePriority::from_label`. */
export function priorityLabel(p: QueuePriority): string {
  return p; // "low" | "normal" | "high"
}

/** Inverse of `label`, mirroring the backend parser. */
export function priorityFromLabel(label: string): QueuePriority {
  switch (label) {
    case "low":
      return "low";
    case "high":
      return "high";
    default:
      return "normal";
  }
}

/** Human-readable display label for a priority. */
export function priorityDisplayLabel(p: QueuePriority): "Low" | "Normal" | "High" {
  switch (p) {
    case "low":
      return "Low";
    case "high":
      return "High";
    default:
      return "Normal";
  }
}

// ── Pure projection helpers (tested independently of the DOM) ──────────────

/**
 * Lifecycle phase of the queue view, derived purely from load state + payload.
 *
 * A failed load is NEVER collapsed into an empty queue: the error is
 * surfaced so the user can distinguish "no items" from "could not load."
 */
export function classifyQueue(
  loaded: boolean,
  loadError: string | null,
  items: QueueItem[]
): QueueViewPhase {
  if (!loaded) return "loading";
  if (loadError !== null) return "error";
  if (items.length === 0) return "empty";
  return "ready";
}

/** Apply the active filter + sort to an array of queue items. */
export function applyFilter(items: QueueItem[], filter: QueueFilter): QueueItem[] {
  let result = items.slice();

  if (filter.status !== null) {
    result = result.filter((i) => i.status === filter.status);
  }
  if (filter.priority !== null) {
    result = result.filter((i) => i.priority === filter.priority);
  }
  if (filter.search.length > 0) {
    const q = filter.search.toLowerCase();
    result = result.filter(
      (i) =>
        i.title.toLowerCase().includes(q) ||
        i.object_type.toLowerCase().includes(q) ||
        i.tags.some((t) => t.toLowerCase().includes(q))
    );
  }

  result.sort((a, b) => cmpBy(a, b, filter.sort_by, filter.sort_ascending));
  return result;
}

/** Comparison function for sorting. */
function cmpBy(
  a: QueueItem,
  b: QueueItem,
  field: QueueSortField,
  ascending: boolean
): number {
  let ord: number;
  switch (field) {
    case "modified_at":
      ord = a.modified_at < b.modified_at ? -1 : a.modified_at > b.modified_at ? 1 : 0;
      break;
    case "title":
      ord = a.title < b.title ? -1 : a.title > b.title ? 1 : 0;
      break;
    case "priority":
      ord = priorityRank(a.priority) - priorityRank(b.priority);
      break;
    case "progress":
      ord = a.progress < b.progress ? -1 : a.progress > b.progress ? 1 : 0;
      break;
    case "status":
      ord = statusRank(a.status) - statusRank(b.status);
      break;
    default:
      ord = 0;
  }
  return ascending ? ord : -ord;
}

const STATUS_RANK: Record<QueueStatus, number> = {
  unread: 0,
  reading: 1,
  completed: 2,
  archived: 3,
};

const PRIORITY_RANK: Record<QueuePriority, number> = {
  low: 0,
  normal: 1,
  high: 2,
};

function statusRank(s: QueueStatus): number {
  return STATUS_RANK[s] ?? 0;
}

function priorityRank(p: QueuePriority): number {
  return PRIORITY_RANK[p] ?? 1;
}

/** Aggregate counts of items per status (unread, reading, completed, archived). */
export function statusCounts(items: QueueItem[]): [number, number, number, number] {
  let unread = 0;
  let reading = 0;
  let completed = 0;
  let archived = 0;
  for (const i of items) {
    switch (i.status) {
      case "unread":
        unread++;
        break;
      case "reading":
        reading++;
        break;
      case "completed":
        completed++;
        break;
      case "archived":
        archived++;
        break;
    }
  }
  return [unread, reading, completed, archived];
}

/** IDs of all currently-selected queue items (drives batch actions). */
export function selectedIds(items: QueueItem[]): string[] {
  return items.filter((i) => i.selected).map((i) => i.id);
}

/**
 * Post-mutation reconciliation, isolated for testability.
 *
 * `Ok` means the backend accepted the change and the caller must reload the
 * queue so the visible state reflects backend truth. `Err(msg)` means the
 * backend rejected the change: the local items are returned UNTOUCHED
 * (no false update) together with the message to surface.
 */
export function reconcileMutation(
  items: QueueItem[],
  outcome: { ok: true } | { ok: false; error: string }
): { items: QueueItem[]; error: string | null; reload: boolean } {
  if (outcome.ok) {
    return { items, error: null, reload: true };
  }
  return { items, error: outcome.error, reload: false };
}

// ── Row + display helpers ───────────────────────────────────────────────────

/** CSS class for a queue item row based on selection + priority. */
export function itemRowClass(selected: boolean, priority: QueuePriority): string {
  const base = "queue-item px-3 py-2 cursor-pointer hover:bg-gray-800 border-l-2";
  const sel = selected
    ? "bg-gray-800 border-l-blue-500"
    : "border-l-transparent";
  let pri = "";
  switch (priority) {
    case "high":
      pri = "border-l-red-500";
      break;
    case "low":
      pri = "border-l-blue-500";
      break;
    default:
      pri = "";
  }
  return `${base} ${sel} ${pri}`;
}

/** Progress percentage (0–100) from a 0.0–1.0 progress value. */
export function progressPct(progress: number): number {
  return Math.round(progress * 100);
}

// ── Sub-components ──────────────────────────────────────────────────────────

/** A status filter pill. */
function StatusPill({
  label,
  count,
  active,
  status,
  onToggle,
}: {
  label: string;
  count: number;
  active: boolean;
  status: QueueStatus;
  onToggle: (status: QueueStatus) => void;
}) {
  const cls = active
    ? "px-2 py-0.5 text-xs rounded-full border bg-blue-900/50 border-blue-600 text-blue-300"
    : "px-2 py-0.5 text-xs rounded-full border border-gray-700 text-gray-400 hover:text-gray-200";
  return (
    <button type="button" className={cls} onClick={() => onToggle(status)}>
      {label} {count}
    </button>
  );
}

/** A priority filter pill. */
function PriorityPill({
  label,
  priority,
  active,
  onToggle,
}: {
  label: string;
  priority: QueuePriority;
  active: boolean;
  onToggle: (priority: QueuePriority) => void;
}) {
  const cls = active
    ? "px-2 py-0.5 text-xs rounded border bg-gray-700 border-gray-500 text-gray-300"
    : "px-2 py-0.5 text-xs rounded border border-gray-700 text-gray-400";
  return (
    <button type="button" className={cls} onClick={() => onToggle(priority)}>
      {label}
    </button>
  );
}

/** Skeleton row for loading state. */
function SkeletonRow({ rows = 6 }: { rows?: number }) {
  return (
    <div className="space-y-1">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="h-4 bg-gray-800 rounded animate-pulse" />
      ))}
    </div>
  );
}

// ── Main component ──────────────────────────────────────────────────────────

/** Reading Queue component — full queue list + detail view. */
export function ReadingQueue() {
  const { toast } = useToast();

  // Queue data
  const [items, setItems] = useState<QueueItem[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  // Filter state
  const [filter, setFilter] = useState<QueueFilter>({
    status: null,
    priority: null,
    search: "",
    sort_by: "modified_at",
    sort_ascending: false,
  });

  // ── Load queue ──
  const loadQueue = useCallback(async () => {
    setLoaded(false);
    setLoadError(null);
    try {
      const result = await queueGetAll();
      setItems(result);
      setLoaded(true);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setLoadError(msg);
      setLoaded(true);
      toast("Could not load the reading queue", { variant: "error" });
    }
  }, [toast]);

  // ── Initial load ──
  useEffect(() => {
    loadQueue();
  }, [loadQueue]);

  // ── Filter handlers ──
  const onFilterChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setFilter((prev) => ({ ...prev, search: e.target.value }));
  };

  const onStatusFilter = (status: QueueStatus) => {
    setFilter((prev) => ({
      ...prev,
      status: prev.status === status ? null : status,
    }));
  };

  const onPriorityFilter = (priority: QueuePriority) => {
    setFilter((prev) => ({
      ...prev,
      priority: prev.priority === priority ? null : priority,
    }));
  };

  const onSortChange = (field: QueueSortField) => {
    setFilter((prev) => ({
      ...prev,
      sort_by: field,
      sort_ascending: prev.sort_by === field ? !prev.sort_ascending : false,
    }));
  };

  // ── Item actions ──
  const toggleSelect = (id: string) => {
    setItems((prev) =>
      prev.map((i) => (i.id === id ? { ...i, selected: !i.selected } : i))
    );
  };

  const setStatus = async (id: string, status: string) => {
    try {
      await queueSetStatus(id, status);
      await loadQueue();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast(`Could not update that item's status: ${msg}`, {
        variant: "error",
      });
    }
  };

  const setPriority = async (id: string, priority: string) => {
    try {
      await queueSetPriority(id, priority);
      await loadQueue();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast(`Could not update that item's priority: ${msg}`, {
        variant: "error",
      });
    }
  };

  const setProgress = async (id: string, progress: number) => {
    try {
      await queueSetProgress(id, progress);
      await loadQueue();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast(`Could not update reading progress: ${msg}`, {
        variant: "error",
      });
    }
  };

  // ── Batch actions ──
  const batchSetStatus = async (status: QueueStatus) => {
    const ids = selectedIds(items);
    if (ids.length === 0) return;
    try {
      await queueBatchSetStatus(ids, statusLabel(status));
      await loadQueue();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast(`Could not update the selected items: ${msg}`, {
        variant: "error",
      });
    }
  };

  // ── Derived state ──
  const phase = classifyQueue(loaded, loadError, items);
  const visible = applyFilter(items, filter);
  const selectedCount = items.filter((i) => i.selected).length;

  return (
    <div className="reading-queue flex h-full bg-gray-950 text-gray-100 overflow-hidden">
      {/* ── Left panel: queue list ── */}
      <div className="flex-none w-96 border-r border-gray-800 flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-800">
          <h2 className="text-sm font-semibold text-gray-300">Reading Queue</h2>
          <span className="text-xs text-gray-500">{items.length} items</span>
        </div>

        {/* Search */}
        <div className="px-3 py-2 border-b border-gray-800">
          <input
            type="text"
            placeholder="Search queue..."
            className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
            value={filter.search}
            onChange={onFilterChange}
          />
        </div>

        {/* Status filter pills */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-800">
          {(() => {
            const [unread, reading, completed, archived] = statusCounts(items);
            return (
              <>
                <StatusPill
                  label="Unread"
                  count={unread}
                  active={filter.status === "unread"}
                  status="unread"
                  onToggle={onStatusFilter}
                />
                <StatusPill
                  label="Reading"
                  count={reading}
                  active={filter.status === "reading"}
                  status="reading"
                  onToggle={onStatusFilter}
                />
                <StatusPill
                  label="Done"
                  count={completed}
                  active={filter.status === "completed"}
                  status="completed"
                  onToggle={onStatusFilter}
                />
                <StatusPill
                  label="Archived"
                  count={archived}
                  active={filter.status === "archived"}
                  status="archived"
                  onToggle={onStatusFilter}
                />
              </>
            );
          })()}
        </div>

        {/* Priority filter pills */}
        <div className="flex items-center gap-2 px-3 py-1.5 border-b border-gray-800">
          <span className="text-xs text-gray-500">Priority:</span>
          <PriorityPill
            label="High"
            priority="high"
            active={filter.priority === "high"}
            onToggle={onPriorityFilter}
          />
          <PriorityPill
            label="Normal"
            priority="normal"
            active={filter.priority === "normal"}
            onToggle={onPriorityFilter}
          />
          <PriorityPill
            label="Low"
            priority="low"
            active={filter.priority === "low"}
            onToggle={onPriorityFilter}
          />
        </div>

        {/* Selection bar */}
        {selectedCount > 0 && (
          <div className="flex items-center gap-2 px-3 py-2 bg-blue-900/30 border-b border-blue-800/30">
            <span className="text-xs text-blue-400">{selectedCount} selected</span>
            <button
              type="button"
              className="px-2 py-1 text-xs bg-green-700 rounded hover:bg-green-600"
              onClick={() => batchSetStatus("reading")}
            >
              Mark Reading
            </button>
            <button
              type="button"
              className="px-2 py-1 text-xs bg-blue-700 rounded hover:bg-blue-600"
              onClick={() => batchSetStatus("completed")}
            >
              Mark Done
            </button>
            <button
              type="button"
              className="px-2 py-1 text-xs bg-gray-700 rounded hover:bg-gray-600"
              onClick={() => batchSetStatus("archived")}
            >
              Archive
            </button>
          </div>
        )}

        {/* Queue list body */}
        <div className="flex-1 overflow-y-auto">
          {phase === "loading" && (
            <div className="p-4">
              <SkeletonRow rows={6} />
            </div>
          )}

          {phase === "error" && loadError && (
            <div className="p-4">
              <div className="bg-red-900/20 border border-red-800/50 rounded-lg p-4 text-sm">
                <div className="font-semibold text-red-300">
                  Couldn't load the reading queue
                </div>
                <div className="text-gray-400 mt-1">
                  Something went wrong while reading your queue.
                </div>
                <div className="text-xs text-gray-500 mt-2 break-words">
                  {loadError}
                </div>
                <div className="text-xs text-gray-500 mt-2">
                  Check that your vault is accessible, then try again.
                </div>
                <button
                  type="button"
                  onClick={loadQueue}
                  className="mt-2 px-3 py-1 text-xs bg-gray-800 rounded hover:bg-gray-700 border border-gray-700"
                >
                  Retry
                </button>
              </div>
            </div>
          )}

          {phase === "empty" && (
            <div className="h-full flex items-center justify-center p-6">
              <div className="flex flex-col items-center text-center">
                <Icon name="bookOpen" className="w-10 h-10 text-gray-500 mb-3" />
                <div className="text-sm font-medium text-gray-300">
                  Nothing in the queue
                </div>
                <div className="text-xs text-gray-500 mt-1 max-w-xs">
                  Add documents or web articles to your reading queue and
                  track progress here.
                </div>
              </div>
            </div>
          )}

          {phase === "ready" && visible.length === 0 && (
            <div className="p-6 text-center text-gray-500 text-sm">
              No items match your filters.
            </div>
          )}

          {phase === "ready" && visible.length > 0 && (
            <div className="divide-y divide-gray-800">
              {visible.map((item) => {
                const pct = progressPct(item.progress);
                return (
                  <div
                    key={item.id}
                    className={itemRowClass(item.selected, item.priority)}
                    onClick={() => toggleSelect(item.id)}
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium truncate max-w-48">
                        {item.title}
                      </span>
                      <span className="text-xs px-1.5 py-0.5 rounded bg-gray-700 text-gray-300">
                        {statusDisplayLabel(item.status)}
                      </span>
                    </div>
                    <div className="flex items-center gap-2 mt-1">
                      <span className="text-xs text-gray-500">{item.object_type}</span>
                      <div className="flex-1 h-1 bg-gray-700 rounded-full overflow-hidden">
                        <div
                          className="h-full bg-blue-500 rounded-full transition-all"
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                      <span className="text-xs text-gray-500">{pct}%</span>
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          {/* Footer with sort controls */}
          <div className="flex items-center justify-between px-3 py-2 border-t border-gray-800 text-xs text-gray-500">
            <span>{visible.length} items</span>
            <div className="flex gap-2">
              <button
                type="button"
                className="hover:text-gray-300"
                onClick={() => onSortChange("modified_at")}
              >
                Sort by Date
              </button>
              <button
                type="button"
                className="hover:text-gray-300"
                onClick={() => onSortChange("title")}
              >
                Sort by Title
              </button>
              <button
                type="button"
                className="hover:text-gray-300"
                onClick={() => onSortChange("priority")}
              >
                Sort by Priority
              </button>
              <button
                type="button"
                className="hover:text-gray-300"
                onClick={() => onSortChange("progress")}
              >
                Sort by Progress
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* ── Right panel: detail view ── */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {(() => {
          const selected = items.find((i) => i.selected);
          if (!selected) {
            return (
              <div className="flex items-center justify-center h-full text-gray-500">
                Select an item to view details
              </div>
            );
          }

          const item = selected;
          const pct = progressPct(item.progress);

          return (
            <div className="flex h-full">
              <div className="flex-1 overflow-y-auto p-4">
                <div className="space-y-4">
                  <h2 className="text-xl font-semibold">{item.title}</h2>

                  <div className="grid grid-cols-2 gap-4">
                    {/* Status */}
                    <div>
                      <label className="text-xs text-gray-500 uppercase tracking-wide">
                        Status
                      </label>
                      <select
                        className="mt-1 block w-full bg-gray-800 text-gray-100 rounded px-2 py-1 text-sm border border-gray-700"
                        value={statusLabel(item.status)}
                        onChange={(e) =>
                          setStatus(item.id, e.target.value)
                        }
                      >
                        <option value="unread">Unread</option>
                        <option value="reading">Reading</option>
                        <option value="completed">Completed</option>
                        <option value="archived">Archived</option>
                      </select>
                    </div>

                    {/* Priority */}
                    <div>
                      <label className="text-xs text-gray-500 uppercase tracking-wide">
                        Priority
                      </label>
                      <select
                        className="mt-1 block w-full bg-gray-800 text-gray-100 rounded px-2 py-1 text-sm border border-gray-700"
                        value={priorityLabel(item.priority)}
                        onChange={(e) =>
                          setPriority(item.id, e.target.value)
                        }
                      >
                        <option value="normal">Normal</option>
                        <option value="high">High</option>
                        <option value="low">Low</option>
                      </select>
                    </div>

                    {/* Progress */}
                    <div>
                      <label className="text-xs text-gray-500 uppercase tracking-wide">
                        Progress
                      </label>
                      <div className="mt-1 flex items-center gap-3">
                        <input
                          type="range"
                          min="0"
                          max="100"
                          value={pct}
                          className="flex-1"
                          onChange={(e) =>
                            setProgress(
                              item.id,
                              parseInt(e.target.value, 10) / 100
                            )
                          }
                        />
                        <span className="text-sm text-gray-400">{pct}%</span>
                      </div>
                    </div>

                    {/* Type */}
                    <div>
                      <label className="text-xs text-gray-500 uppercase tracking-wide">
                        Type
                      </label>
                      <p className="text-sm text-gray-300 mt-1">
                        {item.object_type}
                      </p>
                    </div>

                    {/* Source */}
                    <div>
                      <label className="text-xs text-gray-500 uppercase tracking-wide">
                        Source
                      </label>
                      <p className="text-sm text-gray-300 mt-1">
                        {item.source}
                      </p>
                    </div>

                    {/* Tags */}
                    <div>
                      <label className="text-xs text-gray-500 uppercase tracking-wide">
                        Tags
                      </label>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {item.tags.map((t) => (
                          <span
                            key={t}
                            className="px-2 py-0.5 text-xs bg-gray-700 rounded text-gray-300"
                          >
                            {t}
                          </span>
                        ))}
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          );
        })()}
      </div>
    </div>
  );
}
