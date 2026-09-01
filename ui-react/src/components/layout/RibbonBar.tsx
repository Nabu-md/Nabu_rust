// ──────────────────────────────────────────────────────────────────────────────
// RibbonBar — left vertical icon bar
//
// Mirrors: ui-react/src/components/layout/ribbon_bar.rs
//
// A narrow vertical bar of icon buttons (vault explorer, search, graph,
// daily note, dictation, canvas, activity, streaming, settings).
// ──────────────────────────────────────────────────────────────────────────────

import { Icon } from "./icons";
import { useNav, useWorkspace } from "../../context";
import { toggleDictationPill, openSettings, noteDaily } from "../../ipc";

/** Props for a ribbon icon button. */
interface RibbonButtonProps {
  title: string;
  "aria-label": string;
  onClick: () => void;
  icon: string;
}

/** A simple icon button used in the ribbon bar. */
function RibbonButton({ title, "aria-label": ariaLabel, onClick, icon }: RibbonButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-label={ariaLabel}
      className="w-12 h-10 mx-auto rounded-lg flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500/50"
    >
      <Icon name={icon} className="w-5 h-5" />
    </button>
  );
}

/** The ribbon bar — a narrow vertical column of icon buttons. */
export function RibbonBar() {
  const nav = useNav();
  const ws = useWorkspace();

  const handleToggleSidebar = () => {
    nav.setShowLeftSidebar(true);
  };

  const handleSearch = () => {
    nav.setSearchQuery("");
    nav.setViewMode("Search");
  };

  const handleGraph = () => {
    nav.setViewMode("Graph");
  };

  const handleDailyNote = async () => {
    try {
      const name = await noteDaily();
      ws.openTab(name);
    } catch {
      // Non-fatal: daily note failed to load
    }
  };

  const handleDictation = async () => {
    try {
      await toggleDictationPill();
    } catch {
      // Non-fatal
    }
  };

  const handleCanvas = () => {
    nav.setViewMode("Canvas");
  };

  const handleActivity = () => {
    nav.setViewMode("Activity");
  };

  const handleStreaming = () => {
    nav.setViewMode("Streaming");
  };

  const handleSettings = async () => {
    try {
      await openSettings();
    } catch {
      // Non-fatal
    }
  };

  return (
    <aside className="w-12 h-screen border-r border-gray-700 bg-gray-900 flex flex-col items-center py-4 space-y-2">
      <RibbonButton
        title="Vault Explorer"
        aria-label="Vault Explorer"
        icon="folderOpen"
        onClick={handleToggleSidebar}
      />
      <RibbonButton
        title="Global Search"
        aria-label="Global Search"
        icon="search"
        onClick={handleSearch}
      />
      <RibbonButton
        title="Graph View"
        aria-label="Graph View"
        icon="graph"
        onClick={handleGraph}
      />
      <RibbonButton
        title="Daily Note"
        aria-label="Daily Note"
        icon="calendar"
        onClick={handleDailyNote}
      />
      <RibbonButton
        title="Dictation"
        aria-label="Dictation"
        icon="mic"
        onClick={handleDictation}
      />
      <RibbonButton
        title="Canvas"
        aria-label="Canvas"
        icon="palette"
        onClick={handleCanvas}
      />
      <RibbonButton
        title="Activity"
        aria-label="Activity"
        icon="activity"
        onClick={handleActivity}
      />
      <RibbonButton
        title="Streaming"
        aria-label="Streaming"
        icon="sparkles"
        onClick={handleStreaming}
      />

      <div className="flex-grow" />

      <RibbonButton
        title="Settings"
        aria-label="Settings"
        icon="settings"
        onClick={handleSettings}
      />
    </aside>
  );
}
