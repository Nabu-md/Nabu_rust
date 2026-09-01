// ──────────────────────────────────────────────────────────────────────────────
// CommandPalette — overlay command palette (stub)
//
// Mirrors: ui-react/src/components/navigation/command_palette.rs
//
// Controlled by NavContext.paletteOpen. Renders an overlay with a search
// input. Wave 3 will populate commands; for now this is a visible shell.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav } from "../../context";

/** Command palette overlay (stub — command population is Wave 3). */
export function CommandPalette() {
  const nav = useNav();
  if (!nav.paletteOpen) return null;

  const handleEscape = () => nav.setPaletteOpen(false);
  const handleBackdrop = (e: React.MouseEvent) => {
    e.stopPropagation();
    handleEscape();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[10vh] bg-black/50 backdrop-blur-sm"
      onClick={handleBackdrop}
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-xl shadow-2xl mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gray-700">
          <input
            type="text"
            placeholder="Type a command…"
            className="w-full bg-transparent text-gray-100 placeholder-gray-500 outline-none text-sm"
            autoFocus
          />
        </div>
        <div className="p-3 text-xs text-gray-500">
          Command palette — Wave 3 will populate commands.
        </div>
      </div>
    </div>
  );
}
