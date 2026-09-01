// ──────────────────────────────────────────────────────────────────────────────
// InboxHistory.tsx — "History" tab: per-processor processing history
//
// Mirrors: ui-react/src/components/inbox.rs (`InboxHistory`)
// ──────────────────────────────────────────────────────────────────────────────

import type { InboxItem } from "../../types";
import { InboxIcon } from "./icons";

/** The History tab. */
export function InboxHistory({ item }: { item: InboxItem }) {
  const history = item.processing_history;

  if (history.length === 0) {
    return (
      <div className="text-gray-500 text-sm">
        No processing history available.
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {history.map((entry, idx) => {
        const statusClass = entry.success
          ? "text-green-400"
          : "text-red-400";
        const key = `${entry.processor_name}-${idx}`;
        return (
          <div
            key={key}
            className="flex items-start gap-3 p-2 rounded-lg bg-gray-900/50 text-sm"
          >
            <span className={`mt-0.5 ${statusClass}`}>
              {entry.success ? (
                <InboxIcon name="check" className="w-4 h-4" />
              ) : (
                <InboxIcon name="x" className="w-4 h-4" />
              )}
            </span>
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between">
                <span className="font-medium text-gray-200">
                  {entry.processor_name}
                </span>
                <span className="text-xs text-gray-500">
                  {entry.duration_ms}ms
                </span>
              </div>
              <div className="text-xs text-gray-500 mt-0.5">{entry.timestamp}</div>
              {entry.warnings.length > 0 && (
                <ul className="mt-1 space-y-0.5">
                  {entry.warnings.map((w) => (
                    <li key={w} className="text-xs text-yellow-400">
                      {w}
                    </li>
                  ))}
                </ul>
              )}
              {entry.error && (
                <div className="text-xs text-red-400 mt-1">{entry.error}</div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
