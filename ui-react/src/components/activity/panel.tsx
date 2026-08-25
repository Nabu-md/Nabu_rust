// ──────────────────────────────────────────────────────────────────────────────
// activity/panel — Activity Panel (React port)
//
// Mirrors: crates/nabu-ui/src/components/activity/panel.rs (ActivityPanel)
//
// Renders the chronological, real-time activity timeline backed by the
// ActivityManager from activity/mod.tsx. New events appear at the top of
// the list and the panel updates automatically (no polling).
//
// Features:
// - Chronological ordering (newest first — prepended on arrival).
// - Severity-based icon + color coding (info / warning / error).
// - Category grouping / labels.
// - Accessible: keyboard navigation, ARIA roles, screen-reader labels.
// - Graceful empty state with guidance text.
//
// Consumes: ActivityContext (via useActivity)
// Provides: <ActivityPanel />
// ──────────────────────────────────────────────────────────────────────────────

import { Icon } from "../layout/icons";
import { useActivity, formatRelativeTime, categoryLabel, categoryIcon, severityBadgeClass } from "./mod";
import type { ActivityCategory } from "../../types";

/** Severity-based dot color class. */
function severityDotClass(severity: "info" | "warning" | "error"): string {
  switch (severity) {
    case "info":
      return "bg-blue-400";
    case "warning":
      return "bg-yellow-400";
    case "error":
      return "bg-red-400";
    default:
      return "bg-gray-400";
  }
}

/** Renders a single activity entry in the timeline. */
function ActivityEntry({
  title,
  description,
  severity,
  category,
  subsystem,
  formattedTime,
}: {
  title: string;
  description: string | null;
  severity: "info" | "warning" | "error";
  category: ActivityCategory;
  subsystem: string;
  formattedTime: string;
}) {
  const iconName = categoryIcon(category);
  const dotClass = severityDotClass(severity);
  const badgeCls = severityBadgeClass(severity);

  return (
    <div
      className="activity-entry flex gap-3 px-4 py-2.5 border-b border-gray-800/50 hover:bg-gray-900/30"
      role="listitem"
    >
      <div
        className="activity-entry-icon flex-shrink-0 flex flex-col items-center gap-1 mt-0.5"
        aria-hidden="true"
      >
        <Icon name={iconName} className="w-4 h-4 text-gray-400" />
        <span className={`w-4 h-4 rounded-full ${dotClass}`} />
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-2">
          <span className="text-sm font-medium text-gray-100 truncate">{title}</span>
          <span className={badgeCls}>{categoryLabel(category)}</span>
        </div>

        {description && (
          <div className="text-sm text-gray-400 mt-0.5 truncate">
            {description}
          </div>
        )}

        <div className="activity-entry-meta flex items-center gap-2 mt-1">
          <span
            className="text-xs text-gray-500"
            aria-label="timestamp"
          >
            {formattedTime}
          </span>
          <span className="text-xs text-gray-600">{subsystem}</span>
        </div>
      </div>
    </div>
  );
}

/** Empty state shown when no activity has been recorded yet. */
function ActivityEmpty() {
  return (
    <div
      className="activity-empty flex flex-col items-center justify-center h-full py-8"
      role="status"
      aria-label="No activity yet"
    >
      <div
        className="flex h-8 w-8 items-center justify-center text-gray-500 mb-2"
        aria-hidden="true"
      >
        <Icon name="activity" className="w-5 h-5" />
      </div>
      <div className="text-sm font-medium text-gray-300">No activity yet</div>
      <div className="text-xs text-gray-500 mt-1 text-center max-w-xs">
        System events — capability changes, plugin lifecycle, synchronization
        status, and pipeline milestones — will appear here as they happen.
      </div>
    </div>
  );
}

/**
 * The Activity Panel component.
 *
 * Reads the shared ActivityManager from context and renders the
 * chronologically-ordered timeline (newest first).
 */
export function ActivityPanel() {
  const { activities } = useActivity();

  return (
    <div
      className="activity-panel flex h-full flex-col bg-gray-950 text-gray-100"
      aria-label="Activity timeline"
      role="region"
    >
      {/* Header */}
      <div className="activity-header flex items-center gap-3 px-4 py-3 border-b border-gray-800">
        <span className="text-lg font-semibold text-gray-100">Activity</span>
        <span className="text-xs text-gray-500">{activities.length} events</span>
      </div>

      {/* Timeline */}
      <div className="flex-1 overflow-y-auto activity-timeline">
        <div
          className="activity-timeline-inner space-y-px"
          role="list"
        >
          {activities.length === 0 ? (
            <ActivityEmpty />
          ) : (
            activities.map((item) => (
              <ActivityEntry
                key={item.id}
                title={item.title}
                description={item.description}
                severity={item.severity}
                category={item.category}
                subsystem={item.subsystem}
                formattedTime={formatRelativeTime(item.timestamp_ms)}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}
