// ──────────────────────────────────────────────────────────────────────────────
// navigation/index — barrel export for all navigation components
//
// Mirrors: ui-react/src/components/navigation/mod.rs
// ──────────────────────────────────────────────────────────────────────────────

export { ViewSwitcher } from "./view_switcher";
export { BreadcrumbBar } from "./breadcrumb";
export { Dashboard } from "./Dashboard";
export { HomeScreen } from "./HomeScreen";
export { SearchPage } from "./SearchPage";
export { CalendarPage } from "./CalendarPage";
export { SmartFoldersPage } from "./SmartFoldersPage";

export {
  DASHBOARD_SECTIONS,
  dashboardSectionLabel,
  viewModeKey,
  viewModeLabel,
  viewModeIcon,
  fuzzyScore,
} from "./state";

export type { ViewMode } from "./state";
