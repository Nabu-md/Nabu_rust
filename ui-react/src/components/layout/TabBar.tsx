// ──────────────────────────────────────────────────────────────────────────────
// TabBar — the workspace tab bar
//
// Mirrors: ui-react/src/components/layout/tab_bar.rs
//
// Renders open tabs from WorkspaceContext: click-to-activate, close (X),
// pin indicator. The trailing "+" creates a new note via `noteCreateFile`.
// ──────────────────────────────────────────────────────────────────────────────

import { Icon } from "./icons";
import { useWorkspace } from "../../context";
import { noteCreateFile } from "../../ipc";

/** The tab bar component. */
export function TabBar() {
  const ws = useWorkspace();
  const { tabs, activePath, openTab, activateTab, closeTab, pinTab, refreshTreeBump } = ws;

  const handleNewNote = async () => {
    const name = `note-${Date.now()}.md`;
    try {
      await noteCreateFile(name);
      openTab(name);
      refreshTreeBump();
    } catch {
      // Non-fatal
    }
  };

  return (
    <div
      className="tab-bar flex items-stretch h-9 bg-gray-900 border-b border-gray-700 overflow-x-auto"
      role="tablist"
      aria-label="Open notes"
    >
      {tabs.map((tab) => {
        const isActive = activePath === tab.path;
        return (
          <div
            key={tab.path}
            onClick={() => activateTab(tab.path)}
            className={`tab relative flex items-center gap-1.5 px-3 text-xs whitespace-nowrap cursor-pointer border-r border-gray-800 transition-colors ${
              isActive
                ? "bg-gray-800 text-white"
                : "text-gray-400 hover:bg-gray-800 hover:text-gray-200"
            }`}
            role="tab"
            aria-selected={isActive}
            title={tab.path}
          >
            {/* Pin indicator */}
            {tab.pinned && (
              <Icon
                name="pin"
                className="w-3 h-3 text-yellow-500 shrink-0"
                aria-hidden="true"
              />
            )}

            {/* Tab title */}
            <span className="truncate max-w-32">{tab.title}</span>

            {/* Pin toggle */}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                pinTab(tab.path);
              }}
              className="p-0.5 rounded hover:bg-gray-700 text-gray-500 hover:text-gray-300 focus:outline-none"
              title={tab.pinned ? "Unpin" : "Pin"}
              aria-label={tab.pinned ? "Unpin" : "Pin"}
            >
              <Icon name="pin" className="w-3 h-3" />
            </button>

            {/* Close button */}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                closeTab(tab.path);
              }}
              className="p-0.5 rounded hover:bg-gray-700 hover:text-red-300 text-gray-500 focus:outline-none focus:ring-1 focus:ring-red-500/50"
              title="Close"
              aria-label={`Close ${tab.title}`}
            >
              <Icon name="x" className="w-3 h-3" />
            </button>
          </div>
        );
      })}

      {/* New note button */}
      <button
        type="button"
        onClick={handleNewNote}
        className="tab-new px-3 text-gray-400 text-sm shrink-0 hover:text-gray-200 hover:bg-gray-800 border-r border-gray-800 transition-colors"
        title="New note"
        aria-label="New note"
      >
        <Icon name="plus" className="w-4 h-4" />
      </button>
    </div>
  );
}
