// ──────────────────────────────────────────────────────────────────────────────
// QuickSwitcher — overlay quick note switcher (stub)
//
// Mirrors: ui-react/src/components/navigation/quick_switcher.rs
//
// Controlled by NavContext.switcherOpen. Renders an overlay with a search
// input. Wave 3 will populate results; for now this is a visible shell.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav } from "../../context";

/** Quick switcher overlay (stub — result population is Wave 3). */
export function QuickSwitcher() {
  const nav = useNav();
  if (!nav.switcherOpen) return null;

  const handleBackdrop = (e: React.MouseEvent) => {
    e.stopPropagation();
    nav.setSwitcherOpen(false);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[10vh] bg-black/50 backdrop-blur-sm"
      onClick={handleBackdrop}
      role="dialog"
      aria-modal="true"
      aria-label="Quick switcher"
    >
      <div
        className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-xl shadow-2xl mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gray-700">
          <input
            type="text"
            placeholder="Jump to note…"
            className="w-full bg-transparent text-gray-100 placeholder-gray-500 outline-none text-sm"
            autoFocus
          />
        </div>
        <div className="p-3 text-xs text-gray-500">
          Quick switcher — Wave 3 will populate results.
        </div>
      </div>
    </div>
  );
}
