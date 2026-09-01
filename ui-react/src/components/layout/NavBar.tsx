// ──────────────────────────────────────────────────────────────────────────────
// NavBar — top navigation bar
//
// Mirrors: ui-react/src/components/navigation/navbar.rs
//
// Shows:
//  - breadcrumb (vault name)
//  - quick-search + Command Palette / Quick Switcher / Shortcuts buttons
//  - undo / redo (from HistoryContext)
//  - sidebar / inspector toggles
// ──────────────────────────────────────────────────────────────────────────────

import { Icon } from "./icons";
import { useNav, useHistory, useSaveStatus } from "../../context";

/** The top navigation bar. */
export function NavBar() {
  const nav = useNav();
  const history = useHistory();
  const saveStatus = useSaveStatus();

  // ── Handlers ──────────────────────────────────────────────────────────
  const handleSearch = () => {
    nav.setSearchQuery("");
    nav.setViewMode("Search");
  };

  const handleCommandPalette = () => nav.setPaletteOpen(true);
  const handleQuickSwitcher = () => nav.setSwitcherOpen(true);
  const handleShortcuts = () => nav.setShortcutsOpen(true);

  const handleUndo = () => {
    if (history.canUndo) history.undo();
  };

  const handleRedo = () => {
    if (history.canRedo) history.redo();
  };

  const handleToggleLeftSidebar = () => {
    nav.setShowLeftSidebar(!nav.showLeftSidebar);
  };

  const handleToggleRightInspector = () => {
    nav.setShowRightInspector(!nav.showRightInspector);
  };

  // ── Save status indicator ─────────────────────────────────────────────
  const saveStatusClass = {
    saved: "text-green-400",
    saving: "text-blue-400",
    unsaved: "text-yellow-400",
    error: "text-red-400",
  }[saveStatus.status];

  return (
    <nav className="navbar flex items-center justify-between h-9 px-3 border-b border-gray-700 bg-gray-900 text-gray-300 text-xs">
      {/* Left: Breadcrumb / vault name */}
      <div className="flex items-center gap-2 text-gray-400 truncate">
        <Icon name="folder" className="w-3 h-3" />
        <span className="truncate max-w-48" title={nav.vaultName || "Vault"}>
          {nav.vaultName || "Vault"}
        </span>
      </div>

      {/* Center: view mode indicator */}
      <div className="text-gray-500 text-xs">
        {nav.viewMode}
      </div>

      {/* Right: actions */}
      <div className="flex items-center gap-1">
        {/* Search */}
        <button
          type="button"
          onClick={handleSearch}
          className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          title="Search all notes (⌘⇧F)"
          aria-label="Search"
        >
          <Icon name="search" className="w-4 h-4" />
        </button>

        {/* Command Palette */}
        <button
          type="button"
          onClick={handleCommandPalette}
          className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          title="Command palette (⌘K)"
          aria-label="Command palette"
        >
          <Icon name="command" className="w-4 h-4" />
        </button>

        {/* Quick Switcher */}
        <button
          type="button"
          onClick={handleQuickSwitcher}
          className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          title="Quick switcher (⌘P)"
          aria-label="Quick switcher"
        >
          <Icon name="zap" className="w-4 h-4" />
        </button>

        {/* Shortcuts */}
        <button
          type="button"
          onClick={handleShortcuts}
          className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          title="Keyboard shortcuts (⌘⇧?)"
          aria-label="Shortcuts reference"
        >
          <Icon name="keyboard" className="w-4 h-4" />
        </button>

        <div className="w-px h-5 bg-gray-700 mx-1" />

        {/* Undo */}
        <button
          type="button"
          onClick={handleUndo}
          disabled={!history.canUndo}
          className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 disabled:opacity-30 disabled:cursor-not-allowed transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          title="Undo (⌘Z)"
          aria-label="Undo"
        >
          <Icon name="undo" className="w-4 h-4" />
        </button>

        {/* Redo */}
        <button
          type="button"
          onClick={handleRedo}
          disabled={!history.canRedo}
          className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 disabled:opacity-30 disabled:cursor-not-allowed transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          title="Redo (⌘⇧Z)"
          aria-label="Redo"
        >
          <Icon name="redo" className="w-4 h-4" />
        </button>

        <div className="w-px h-5 bg-gray-700 mx-1" />

        {/* Save status */}
        <div
          className={`w-7 h-7 rounded flex items-center justify-center ${saveStatusClass}`}
          title={`Save status: ${saveStatus.status}`}
          aria-label={`Save status: ${saveStatus.status}`}
        >
          {saveStatus.status === "saving" ? (
            <div className="w-3 h-3 animate-spin rounded-full border border-current border-t-transparent" />
          ) : (
            <Icon name="check" className="w-4 h-4" />
          )}
        </div>

        {/* Notifications */}
        <button
          type="button"
          className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          title="Notifications"
          aria-label="Notifications"
        >
          <Icon name="bell" className="w-4 h-4" />
        </button>

        {/* Toggle left sidebar */}
        <button
          type="button"
          onClick={handleToggleLeftSidebar}
          className={`navbar-action w-7 h-7 rounded flex items-center justify-center transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50 ${
            nav.showLeftSidebar
              ? "text-white bg-gray-800"
              : "text-gray-400 hover:text-white hover:bg-gray-800"
          }`}
          title="Toggle left sidebar (⌘\\)"
          aria-label="Toggle left sidebar"
          aria-pressed={nav.showLeftSidebar}
        >
          <Icon name="folder" className="w-4 h-4" />
        </button>

        {/* Toggle right inspector */}
        <button
          type="button"
          onClick={handleToggleRightInspector}
          className={`navbar-action w-7 h-7 rounded flex items-center justify-center transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50 ${
            nav.showRightInspector
              ? "text-white bg-gray-800"
              : "text-gray-400 hover:text-white hover:bg-gray-800"
          }`}
          title="Toggle right inspector (⌘⇧\\)"
          aria-label="Toggle right inspector"
          aria-pressed={nav.showRightInspector}
        >
          <Icon name="tag" className="w-4 h-4" />
        </button>
      </div>
    </nav>
  );
}
