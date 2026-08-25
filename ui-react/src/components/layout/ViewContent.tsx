// ──────────────────────────────────────────────────────────────────────────────
// ViewContent — placeholder for the main content area
//
// This component is a SHELL placeholder. Wave 3 owns the actual view content
// children (Dashboard, Editor, Graph, Search, etc.). This simply renders
// a placeholder so the shell is visible and not zero-height.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav } from "../../context";

/** Placeholder view content — renders the current view name. */
export function ViewContent() {
  const nav = useNav();

  return (
    <div className="flex-1 overflow-auto p-6 bg-gray-950">
      <div className="text-center py-16 text-gray-500">
        <div className="text-2xl font-medium mb-2">{nav.viewMode}</div>
        <div className="text-sm">
          View content — Wave 3 will populate this area.
        </div>
      </div>
    </div>
  );
}
