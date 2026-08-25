// ──────────────────────────────────────────────────────────────────────────────
// navigation/view_switcher.tsx — view mode router
//
// Mirrors: crates/nabu-ui/src/components/app.rs → ViewContent
//
// Switching views based on NavContext.viewMode. This component renders the
// appropriate navigation view for the active ViewMode. Views not owned by
// Wave 3 (Graph, Settings, Inbox, Templates, Trash, History, Recovery,
// Canvas, Reader, Comparison, Activity, Streaming, Chat, ReadingQueue,
// Editor, etc.) fall back to a placeholder so the shell remains functional.
//
// The Editor view is special: when no active tab exists it shows the
// HomeScreen (the vault front door); when a note is open it would show the
// NoteEditor (owned by another wave).
// ──────────────────────────────────────────────────────────────────────────────

import { useNav, useWorkspace } from "../../context";
import type { ReactNode } from "react";

import { Dashboard } from "./Dashboard";
import { HomeScreen } from "./HomeScreen";
import { SearchPage } from "./SearchPage";
import { CalendarPage } from "./CalendarPage";
import { SmartFoldersPage } from "./SmartFoldersPage";
import { BreadcrumbBar } from "./breadcrumb";
import { viewModeLabel, viewModeIcon } from "./state";
import type { ViewMode } from "./state";
import { Icon } from "../layout/icons";

/**
 * Switches views based on NavContext.viewMode.
 *
 * Owned views (Wave 3): Dashboard, HomeScreen, SearchPage, CalendarPage,
 * SmartFoldersPage, BreadcrumbBar.
 *
 * Other view modes render a placeholder with the view label and icon.
 */
export function ViewSwitcher(): ReactNode {
  const nav = useNav();
  const ws = useWorkspace();
  const mode: ViewMode = nav.viewMode;

  // The Editor view: show HomeScreen when no note is open,
  // otherwise placeholder for the NoteEditor (owned by another wave).
  if (mode === "Editor") {
    if (ws.activePath) {
      return (
        <div className="max-w-4xl mx-auto h-screen flex flex-col items-center justify-center text-gray-500">
          <Icon name="filePen" className="w-12 h-12 mb-4 opacity-30" />
          <div className="text-lg font-medium text-gray-300">
            Note Editor — placeholder
          </div>
          <div className="text-xs text-gray-500">
            Wave: editor will render here.
          </div>
        </div>
      );
    }
    return <HomeScreen />;
  }

  // Map view modes to their owned navigation views
  const renderView = (viewMode: ViewMode): ReactNode => {
    switch (viewMode) {
      case "Dashboard":
        return <Dashboard />;

      case "Search":
        return <SearchPage />;

      case "Calendar":
        return <CalendarPage />;

      case "SmartFolders":
        return <SmartFoldersPage />;

      // Views not owned by Wave 3 — placeholder with label + icon
      case "Graph":
      case "Settings":
      case "Inbox":
      case "ReadingQueue":
      case "Templates":
      case "Trash":
      case "History":
      case "Recovery":
      case "Archive":
      case "Canvas":
      case "Reader":
      case "Comparison":
      case "Statistics":
      case "Activity":
      case "Streaming":
      case "Chat":
        return (
          <div className="h-screen flex flex-col items-center justify-center text-gray-500 bg-gray-950">
            <Icon
              name={viewModeIcon(viewMode)}
              className="w-12 h-12 mb-4 opacity-30"
            />
            <div className="text-lg font-medium text-gray-300">
              {viewModeLabel(viewMode)} — placeholder
            </div>
            <div className="text-xs text-gray-500">
              Wave: will populate this view.
            </div>
          </div>
        );

      default:
        return (
          <div className="h-screen flex items-center justify-center text-gray-500 bg-gray-950">
            Unknown view: {viewMode}
          </div>
        );
    }
  };

  return (
    <>
      {/* Breadcrumb bar sits above the view content */}
      <div className="border-b border-gray-700 px-4 py-2 bg-gray-900">
        <BreadcrumbBar />
      </div>
      {renderView(mode)}
    </>
  );
}
