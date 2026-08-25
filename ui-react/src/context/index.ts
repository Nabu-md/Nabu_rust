// ──────────────────────────────────────────────────────────────────────────────
// context/index — barrel export for all context providers and hooks
// ──────────────────────────────────────────────────────────────────────────────

export { NavProvider, useNav, type NavContextValue } from "./NavContext";
export { ThemeProvider, useTheme, type Theme, type ThemeContextValue } from "./ThemeContext";
export { WorkspaceProvider, useWorkspace, type WorkspaceContextValue, type OpenTab, titleFromPath } from "./WorkspaceContext";
export { ToastProvider, useToast, type ToastContextValue, type Toast, type ToastVariant } from "./ToastContext";
export { HistoryProvider, useHistory, type HistoryContextValue } from "./HistoryContext";
export { SaveStatusProvider, useSaveStatus, type SaveStatusContextValue, type SaveStatusType } from "./SaveStatusContext";
export { ActivityProvider, useActivity, type ActivityContextValue, type ActivityManagerRef } from "../components/activity";
