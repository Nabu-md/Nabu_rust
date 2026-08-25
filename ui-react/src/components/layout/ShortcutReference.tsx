// ──────────────────────────────────────────────────────────────────────────────
// ShortcutReference — overlay keyboard shortcuts reference (stub)
//
// Mirrors: crates/nabu-ui/src/components/navigation/shortcuts.rs
//
// Controlled by NavContext.shortcutsOpen. Renders an overlay with a few
// common shortcuts. Wave 3 will expand the list.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav } from "../../context";

/** Keyboard shortcuts reference overlay (stub). */
export function ShortcutReference() {
  const nav = useNav();
  if (!nav.shortcutsOpen) return null;

  const handleBackdrop = (e: React.MouseEvent) => {
    e.stopPropagation();
    nav.setShortcutsOpen(false);
  };

  const shortcuts = [
    { key: "⌘K", label: "Command Palette" },
    { key: "⌘P", label: "Quick Switcher" },
    { key: "⌘⇧F", label: "Global Search" },
    { key: "⌘Z", label: "Undo" },
    { key: "⌘⇧Z", label: "Redo" },
    { key: "⌘\\", label: "Toggle Left Sidebar" },
    { key: "⌘⇧\\", label: "Toggle Right Inspector" },
    { key: "⌘⇧?", label: "Keyboard Shortcuts" },
  ];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onClick={handleBackdrop}
      role="dialog"
      aria-modal="true"
      aria-label="Keyboard shortcuts"
    >
      <div
        className="bg-gray-900 border border-gray-700 rounded-xl shadow-2xl max-w-md w-full mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-4 border-b border-gray-700">
          <h2 className="text-lg font-semibold text-white">
            Keyboard Shortcuts
          </h2>
        </div>
        <div className="p-4 space-y-2">
          {shortcuts.map((s) => (
            <div
              key={s.key}
              className="flex items-center justify-between"
            >
              <kbd className="px-2 py-1 text-xs bg-gray-800 border border-gray-700 rounded text-gray-300">
                {s.key}
              </kbd>
              <span className="text-sm text-gray-400">{s.label}</span>
            </div>
          ))}
        </div>
        <div className="p-3 border-t border-gray-700 text-center">
          <button
            type="button"
            onClick={() => nav.setShortcutsOpen(false)}
            className="text-xs text-gray-400 hover:text-gray-200"
          >
            Press <kbd className="px-1.5 py-0.5 bg-gray-800 rounded">Esc</kbd> to close
          </button>
        </div>
      </div>
    </div>
  );
}
