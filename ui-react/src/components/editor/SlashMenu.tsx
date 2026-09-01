// ──────────────────────────────────────────────────────────────────────────────
// SlashMenu.tsx — block-type picker invoked with `/` inside the editor
//
// Mirrors: ui-react/src/components/editor/slash_menu.rs (SlashMenu)
//
// Phase 0.4a: same item set as the Dioxus implementation; no redesign.
// Selecting an item calls `onSelect` and closes the menu. The editor is
// responsible for inserting the corresponding markdown at the cursor.
// ──────────────────────────────────────────────────────────────────────────────

/** Callback invoked when the user picks a slash-menu item. */
export type SlashMenuSelect = (label: string) => void;

/** The fixed set of block types offered by the slash menu (Dioxus parity). */
export const SLASH_MENU_ITEMS: readonly string[] = [
  "# Heading 1",
  "## Heading 2",
  "### Heading 3",
  "📋 Kanban Board",
  "📷 Vision OCR Scan",
  "📦 Code Block / Sandbox",
  "💡 Callout Box",
];

export interface SlashMenuProps {
  /** Called with the chosen label; the editor inserts the matching markdown. */
  onSelect: SlashMenuSelect;
}

/**
 * Floating slash-menu popup. Renders an absolutely-positioned list of block
 * types with `role="menu"`. Selecting an item calls `onSelect` and the menu
 * is expected to be unmounted by the parent (controlled via `showMenu` signal).
 */
export function SlashMenu({ onSelect }: SlashMenuProps) {
  return (
    <div
      className="absolute bg-gray-800 border border-gray-700 rounded shadow-lg z-10 w-48 py-1"
      role="menu"
      aria-label="Slash menu"
    >
      {SLASH_MENU_ITEMS.map((item) => (
        <button
          key={item}
          type="button"
          role="menuitem"
          onClick={() => onSelect(item)}
          className="w-full text-left px-3 py-1.5 text-sm text-gray-200 hover:bg-gray-700 focus:outline-none focus:bg-gray-700"
        >
          {item}
        </button>
      ))}
    </div>
  );
}
