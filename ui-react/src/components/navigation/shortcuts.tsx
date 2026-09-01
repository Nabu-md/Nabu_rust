// ──────────────────────────────────────────────────────────────────────────────
// navigation/shortcuts.tsx — shortcut registry, global listener, reference dialog
//
// Mirrors: ui-react/src/components/navigation/shortcuts.rs
//
// A single registry of every shortcut the app binds, used both to *install*
// the global window listener and to render the searchable shortcuts
// reference dialog (single source of truth).
//
// The global listener is installed once per app mount and removed on cleanup.
// Shortcuts that type inside editors/inputs are deliberately skipped so
// native editing behaviour is preserved.
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useRef, useState, type ReactNode } from "react";
import { useNav, useWorkspace, useToast } from "../../context";
import { Icon } from "../layout/icons";
import { createNewNote, openDailyNote } from "./commands";
import type { ViewMode } from "./state";

// ── Shortcut catalog ─────────────────────────────────────────────────

/** One registered shortcut. */
export interface Shortcut {
  category: string;
  keys: string;
  description: string;
}

/**
 * The complete shortcut catalog — the source of truth for the reference
 * dialog. Mirrors SHORTCUTS in the Dioxus spec.
 */
export const SHORTCUTS: Shortcut[] = [
  // ── Command palette / navigation ──
  { category: "Navigation", keys: "⌘K", description: "Open the command palette" },
  { category: "Navigation", keys: "⌘P", description: "Open the quick switcher" },
  { category: "Navigation", keys: "⌘⇧F", description: "Open full-text search" },
  { category: "Navigation", keys: "⌘1", description: "Go to Dashboard" },
  { category: "Navigation", keys: "⌘2", description: "Go to Editor" },
  { category: "Navigation", keys: "⌘3", description: "Go to Graph" },
  { category: "Navigation", keys: "⌘,", description: "Open Settings" },
  { category: "Navigation", keys: "⌘⇧C", description: "Open Canvas" },
  { category: "Navigation", keys: "⌘⇧1", description: "Open Reader Mode" },
  { category: "Navigation", keys: "⌘⇧M", description: "Open Comparison View" },
  { category: "Navigation", keys: "⌘⇧S", description: "Open Statistics" },
  { category: "Navigation", keys: "⌘⇧?", description: "Open shortcuts reference" },
  { category: "Navigation", keys: "Esc", description: "Close any overlay / palette" },
  // ── Note management ──
  { category: "Notes", keys: "⌘N", description: "Create a new note" },
  { category: "Notes", keys: "⌘⇧D", description: "Open the daily note" },
  // ── Workspace / panels ──
  { category: "Workspace", keys: "⌘\\", description: "Toggle the left sidebar" },
  { category: "Workspace", keys: "⌘⇧\\", description: "Toggle the right inspector" },
  { category: "Workspace", keys: "⌘Z", description: "Undo" },
  { category: "Workspace", keys: "⌘⇧Z / Ctrl+Y", description: "Redo" },
  // ── Editor ──
  { category: "Editor", keys: "⌘B", description: "Bold selection (markdown ** **)" },
  { category: "Editor", keys: "⌘I", description: "Italic selection (markdown * *)" },
  { category: "Editor", keys: "/", description: "Open the slash (block) menu" },
];

// ── Helpers ──────────────────────────────────────────────────────────

/**
 * Returns `true` when keyboard focus is inside an editable element so
 * global shortcuts never hijack typing.
 */
function focusIsEditable(): boolean {
  const win = typeof window !== "undefined" ? window : null;
  if (!win) return false;
  const document = win.document;
  const active = document.activeElement;
  if (!active) return false;
  const tag = active.tagName.toLowerCase();
  if (tag === "textarea" || tag === "input" || tag === "select") return true;
  const editable = active.getAttribute("contenteditable");
  return !!editable && editable.toLowerCase() !== "false";
}

// ── Global listener installation ─────────────────────────────────────

type NavLike = ReturnType<typeof useNav>;
type WorkspaceLike = ReturnType<typeof useWorkspace>;
type ToastLike = ReturnType<typeof useToast>;

/**
 * Installs the global keyboard shortcuts for the app shell. Called once on
 * mount; the returned cleanup removes the listener.
 */
export function installGlobalShortcuts(
  nav: NavLike,
  workspace: WorkspaceLike,
  toasts: ToastLike
): (() => void) | undefined {
  if (typeof window === "undefined") return undefined;

  const handler = (ev: KeyboardEvent) => {
    const meta = ev.metaKey || ev.ctrlKey;
    const shift = ev.shiftKey;
    const key = ev.key;

    // Overlay-open: only palette/quick-switcher toggles still apply.
    if (nav.paletteOpen || nav.switcherOpen || nav.shortcutsOpen) {
      if (meta && !shift && key.toLowerCase() === "k") {
        ev.preventDefault();
        nav.setPaletteOpen(!nav.paletteOpen);
        return;
      }
      if (meta && !shift && key.toLowerCase() === "p") {
        ev.preventDefault();
        nav.setSwitcherOpen(!nav.switcherOpen);
        return;
      }
      if (!meta && key === "Escape") {
        ev.preventDefault();
        nav.setPaletteOpen(false);
        nav.setSwitcherOpen(false);
        nav.setShortcutsOpen(false);
      }
      return;
    }

    // When typing in an input/textarea, only overlay toggles apply.
    if (
      focusIsEditable() &&
      !(meta && (key.toLowerCase() === "k" || key.toLowerCase() === "p"))
    ) {
      return;
    }

    if (meta && shift && key.toLowerCase() === "?") {
      ev.preventDefault();
      nav.setShortcutsOpen(true);
    } else if (meta && shift && key.toLowerCase() === "f") {
      ev.preventDefault();
      nav.setSearchQuery("");
      nav.setViewMode("Search");
    } else if (meta && !shift && key.toLowerCase() === "k") {
      ev.preventDefault();
      nav.setPaletteOpen(true);
    } else if (meta && !shift && key.toLowerCase() === "p") {
      ev.preventDefault();
      nav.setSwitcherOpen(true);
    } else if (meta && !shift && key.toLowerCase() === "n") {
      ev.preventDefault();
      createNewNote(workspace, toasts)();
    } else if (meta && shift && key.toLowerCase() === "d") {
      ev.preventDefault();
      openDailyNote(workspace, toasts)();
    } else if (meta && !shift && key === "\\") {
      ev.preventDefault();
      nav.setShowLeftSidebar(!nav.showLeftSidebar);
    } else if (meta && shift && key === "\\") {
      ev.preventDefault();
      nav.setShowRightInspector(!nav.showRightInspector);
    } else if (meta && !shift && key === ",") {
      ev.preventDefault();
      nav.setViewMode("Settings");
    } else if (meta && !shift) {
      const mode: ViewMode | null =
        key === "1"
          ? "Dashboard"
          : key === "2"
          ? "Editor"
          : key === "3"
          ? "Graph"
          : key === "9"
          ? "Settings"
          : null;
      if (mode) {
        ev.preventDefault();
        nav.setViewMode(mode);
      }
    } else if (meta && shift && key.toLowerCase() === "c") {
      ev.preventDefault();
      nav.setViewMode("Canvas");
    } else if (meta && shift && key === "1") {
      ev.preventDefault();
      nav.setViewMode("Reader");
    } else if (meta && shift && key.toLowerCase() === "m") {
      ev.preventDefault();
      nav.setViewMode("Comparison");
    } else if (meta && shift && key.toLowerCase() === "s") {
      ev.preventDefault();
      nav.setViewMode("Statistics");
    }
  };

  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}

/**
 * Component that installs global keyboard shortcuts on mount. Renders no
 * output. Must be placed inside all context providers.
 */
export function KeyboardShortcuts(): ReactNode {
  const nav = useNav();
  const workspace = useWorkspace();
  const toasts = useToast();

  useEffect(() => {
    const cleanup = installGlobalShortcuts(nav, workspace, toasts);
    return cleanup;
  }, [nav, workspace, toasts]);

  return null;
}

// ── Shortcuts reference dialog ───────────────────────────────────────

/**
 * The searchable shortcuts reference dialog. Rendered once at the app root.
 * Controlled by NavContext.shortcutsOpen.
 */
export function ShortcutReference(): ReactNode {
  const nav = useNav();
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus the input + reset state whenever the dialog opens.
  useEffect(() => {
    if (nav.shortcutsOpen) {
      setQuery("");
      const t = setTimeout(() => {
        if (nav.shortcutsOpen) inputRef.current?.focus();
      }, 10);
      return () => clearTimeout(t);
    }
  }, [nav.shortcutsOpen]);

  if (!nav.shortcutsOpen) return null;

  // Group shortcuts by category, filtered by the query.
  const q = query.trim().toLowerCase();
  const groups: { category: string; items: Shortcut[] }[] = [];
  for (const shortcut of SHORTCUTS) {
    if (
      q.length > 0 &&
      !shortcut.keys.toLowerCase().includes(q) &&
      !shortcut.description.toLowerCase().includes(q) &&
      !shortcut.category.toLowerCase().includes(q)
    ) {
      continue;
    }
    const bucket = groups.find((g) => g.category === shortcut.category);
    if (bucket) {
      bucket.items.push(shortcut);
    } else {
      groups.push({ category: shortcut.category, items: [shortcut] });
    }
  }

  const closeHandler = () => {
    nav.setPaletteOpen(false);
    nav.setSwitcherOpen(false);
    nav.setShortcutsOpen(false);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onClick={closeHandler}
      role="dialog"
      aria-modal="true"
      aria-label="Keyboard shortcuts"
    >
      <div
        className="bg-gray-900 border border-gray-700 rounded-xl shadow-2xl max-w-lg w-full mx-4 shortcut-dialog panel"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") closeHandler();
        }}
      >
        <div className="shortcut-dialog-header flex items-center justify-between p-4 border-b border-gray-700">
          <h2 className="shortcut-dialog-title text-lg font-semibold text-white flex items-center gap-2">
            <Icon name="keyboard" className="w-5 h-5" />
            Keyboard Shortcuts
          </h2>
          <button
            type="button"
            className="dialog-close p-1 rounded text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
            aria-label="Close"
            onClick={closeHandler}
          >
            <Icon name="x" className="w-4 h-4" />
          </button>
        </div>

        <div className="shortcut-dialog-search p-3 border-b border-gray-700">
          <input
            ref={inputRef}
            id="shortcuts-input"
            type="text"
            className="input w-full bg-transparent text-gray-100 placeholder-gray-500 outline-none text-sm"
            placeholder="Search shortcuts…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") closeHandler();
            }}
            aria-label="Search shortcuts"
          />
        </div>

        <div className="shortcut-dialog-body p-2 overflow-y-auto max-h-80">
          {groups.length === 0 ? (
            <div className="palette-empty px-3 py-4 text-xs text-gray-500">
              No shortcuts match
            </div>
          ) : (
            groups.map(({ category, items }) => (
              <div key={category} className="shortcut-group mb-3">
                <div className="palette-category px-2 py-1 text-xs font-semibold text-gray-500 uppercase">
                  {category}
                </div>
                {items.map((s) => (
                  <div
                    key={`${category}-${s.keys}`}
                    className="shortcut-row flex items-center justify-between px-3 py-1"
                  >
                    <span className="shortcut-desc text-sm text-gray-300">
                      {s.description}
                    </span>
                    <kbd className="shortcut-keys px-2 py-1 text-xs bg-gray-800 border border-gray-700 rounded text-gray-300">
                      {s.keys}
                    </kbd>
                  </div>
                ))}
              </div>
            ))
          )}
        </div>

        <div className="p-3 border-t border-gray-700 text-center">
          <button
            type="button"
            className="text-xs text-gray-400 hover:text-gray-200"
            onClick={closeHandler}
          >
            Press <kbd className="px-1.5 py-0.5 bg-gray-800 rounded">Esc</kbd> to close
          </button>
        </div>
      </div>
    </div>
  );
}
