// ──────────────────────────────────────────────────────────────────────────────
// collections/view_switcher.tsx — view toggle buttons
//
// Mirrors: crates/nabu-ui/src/components/collections/view_switcher.rs (ViewSwitcher)
//
// A row of buttons that lets the user toggle between Table, Board, Gallery,
// and Calendar views.
// ──────────────────────────────────────────────────────────────────────────────

import type { CollectionView } from "./shared/types";
import { CollectionViewLabels, CollectionViewAll } from "./shared/types";

export interface ViewSwitcherProps {
  /** The currently active view. */
  currentView: CollectionView;
  /** Called when the user selects a different view. */
  onChange: (view: CollectionView) => void;
}

/**
 * A row of buttons that lets the user toggle between Table, Board, Gallery,
 * and Calendar views.
 */
export function ViewSwitcher({ currentView, onChange }: ViewSwitcherProps) {
  return (
    <div className="view-switcher flex items-center gap-1 p-2 bg-gray-800 border-b border-gray-700">
      {CollectionViewAll.map((view) => {
        const active = currentView === view;
        const label = CollectionViewLabels[view];
        const className = active
          ? "px-3 py-1.5 text-xs rounded-md bg-blue-600 text-white border border-blue-500"
          : "px-3 py-1.5 text-xs rounded-md border border-gray-700 text-gray-400 hover:text-gray-200 hover:border-gray-600";
        return (
          <button
            key={view}
            type="button"
            className={className}
            onClick={() => onChange(view)}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}
