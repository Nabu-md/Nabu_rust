// ──────────────────────────────────────────────────────────────────────────────
// activity/index — barrel export for the Activity module
//
// Mirrors: ui-react/src/components/activity/mod.rs
// ──────────────────────────────────────────────────────────────────────────────

export {
  ActivityProvider,
  useActivity,
  formatRelativeTime,
  severityLabel,
  severityBadgeClass,
  categoryLabel,
  categoryIcon,
  extractActivity,
  DEFAULT_MAX_ACTIVITIES,
  FRONTEND_EVENT_CHANNEL,
} from "./mod";
export type { ActivityManagerRef, ActivityContextValue } from "./mod";
export { ActivityPanel } from "./panel";
