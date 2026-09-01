// ──────────────────────────────────────────────────────────────────────────────
// inbox/utils.ts — pure helpers (extracted from inbox.rs for testability)
//
// Mirrors the pure functions in ui-react/src/components/inbox.rs:
//   filter_and_sort, confidence_color, confidence_bar (color + pct).
// ──────────────────────────────────────────────────────────────────────────────

import type { InboxItem, InboxStatus } from "../../types";
import type { SortField } from "./types";

/** Pre-computes the confidence bar width percentage (0–100). */
export function confidencePct(score: number): number {
  return Math.max(0, Math.min(100, Math.round(score * 100)));
}

/** Tailwind colour class for the confidence figure text. */
export function confidenceColor(score: number): string {
  if (score >= 0.8) return "text-green-400";
  if (score >= 0.5) return "text-amber-400";
  if (score >= 0.3) return "text-orange-400";
  return "text-red-400";
}

/** Tailwind colour class for the confidence bar fill. */
export function confidenceBarClass(score: number): string {
  if (score >= 0.8) return "bg-green-400";
  if (score >= 0.5) return "bg-amber-400";
  if (score >= 0.3) return "bg-orange-400";
  return "bg-red-400";
}

/** Whether an inbox status is terminal (no further processing expected). */
export function isTerminalStatus(status: InboxStatus): boolean {
  return status === "approved" || status === "rejected" || status === "failed";
}

/** Tailwind colour class for an inbox status badge. */
export function statusColor(status: InboxStatus): string {
  switch (status) {
    case "approved":
      return "text-green-400";
    case "rejected":
      return "text-orange-400";
    case "failed":
      return "text-red-400";
    case "ready":
      return "text-blue-400";
    case "processing":
      return "text-yellow-400";
    default:
      return "text-gray-400";
  }
}

/** Human-readable label for an inbox status (capitalised). */
export function statusLabel(status: InboxStatus): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

/** Sort field labels for the sort dropdown. */
export const SORT_FIELD_LABELS: Record<SortField, string> = {
  timestamp: "Timestamp",
  title: "Title",
  source: "Source",
  status: "Status",
  object_type: "Type",
};

/**
 * Pure filter + sort of inbox items by a search query, sort field, and
 * direction.  Extracted from the `Inbox` component so it can be unit-tested
 * on native without a DOM or IPC boundary (mirrors inbox.rs `filter_and_sort`).
 */
export function filterAndSort(
  items: InboxItem[],
  filter: string,
  sortBy: SortField,
  sortAscending: boolean,
): InboxItem[] {
  let result = items.slice();
  if (filter.trim().length > 0) {
    const f = filter.toLowerCase();
    result = result.filter(
      (i) =>
        i.title.toLowerCase().includes(f) ||
        i.source.toLowerCase().includes(f) ||
        i.object_type.toLowerCase().includes(f),
    );
  }

  let ord = 0;
  result.sort((a, b) => {
    switch (sortBy) {
      case "timestamp":
        ord = a.id.localeCompare(b.id);
        break;
      case "title":
        ord = a.title.localeCompare(b.title);
        break;
      case "source":
        ord = a.source.localeCompare(b.source);
        break;
      case "status":
        ord = a.status.localeCompare(b.status);
        break;
      case "object_type":
        ord = a.object_type.localeCompare(b.object_type);
        break;
      default:
        ord = 0;
    }
    return sortAscending ? ord : -ord;
  });

  return result;
}

/** True when the event target is a text-entry element (skip inbox shortcuts). */
export function isTextInputTarget(target: EventTarget | null): boolean {
  if (!target || !("tagName" in target)) {
    return false;
  }
  const tag = (target as HTMLElement).tagName.toLowerCase();
  return (
    tag === "input" ||
    tag === "textarea" ||
    (target as HTMLElement).isContentEditable === true
  );
}
