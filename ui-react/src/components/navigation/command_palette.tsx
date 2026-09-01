// ──────────────────────────────────────────────────────────────────────────────
// navigation/command_palette.tsx — overlay command palette
//
// Mirrors: ui-react/src/components/navigation/command_palette.rs
//
// A complete command centre overlay:
//   - fuzzy search across label, aliases, category and description
//   - categories as grouped headers
//   - recent commands and favourite commands pinned to the top when the
//     query is empty
//   - keyboard-only navigation (↑/↓/Enter/Escape, ⌘K toggles)
//   - star a command to favourite it (persisted via NavContext)
//
// Controlled by NavContext.paletteOpen. Rendered once at the app root.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, type ReactNode } from "react";
import { useNav, useWorkspace, useToast } from "../../context";
import { Icon } from "../layout/icons";
import { allCommands, resolveCommandsById } from "./commands";
import type { AppCommand } from "./commands";

// ── Row model ─────────────────────────────────────────────────────────

/** One row in the palette list — a category header or a command entry. */
type Row = { type: "header"; label: string } | { type: "command"; cmd: AppCommand };

// ── Row building ─────────────────────────────────────────────────────

/** Fuzzy subsequence scorer — returns score if query is a subsequence of
 * candidate, else undefined. Local copy so this file stays self-contained. */
function fuzzyScoreLocal(query: string, candidate: string): number | undefined {
  const q = query.toLowerCase();
  if (q.length === 0) return 0;
  const cand = candidate.toLowerCase();
  let qi = 0;
  let score = 0;
  let prev: number | null = null;
  for (let ci = 0; ci < cand.length; ci++) {
    if (qi < q.length && cand[ci] === q[qi]) {
      score += 10;
      if (ci === 0) score += 15;
      if (ci > 0 && (cand[ci - 1] === " " || cand[ci - 1] === "-" || cand[ci - 1] === "/")) {
        score += 8;
      }
      if (prev !== null && ci === prev + 1) score += 6;
      prev = ci;
      qi += 1;
    }
  }
  return qi === q.length ? score : undefined;
}

function buildRows(
  catalog: AppCommand[],
  query: string,
  recentIds: string[],
  favIds: string[]
): Row[] {
  const q = query.trim();

  if (q.length === 0) return buildEmptyRows(catalog, recentIds, favIds);

  // Fuzzy filtering: score each command on label + aliases + category + description.
  const scored: { score: number; cmd: AppCommand }[] = [];
  for (const cmd of catalog) {
    let best: number | undefined;
    const texts: string[] = [
      cmd.label,
      ...cmd.aliases,
      cmd.category,
      cmd.description,
    ];
    for (const text of texts) {
      const s = fuzzyScoreLocal(q, text);
      if (s !== undefined && (best === undefined || s > best)) best = s;
    }
    if (best !== undefined) scored.push({ score: best, cmd });
  }
  scored.sort((a, b) => b.score - a.score);

  const rows: Row[] = [];
  const groups: Map<string, AppCommand[]> = new Map();
  for (const { cmd } of scored) {
    const list = groups.get(cmd.category) ?? [];
    list.push(cmd);
    groups.set(cmd.category, list);
  }
  for (const [category, list] of groups) {
    rows.push({ type: "header", label: category });
    for (const cmd of list) rows.push({ type: "command", cmd });
  }
  return rows;
}

function buildEmptyRows(
  catalog: AppCommand[],
  recentIds: string[],
  favIds: string[]
): Row[] {
  const rows: Row[] = [];
  const seen = new Set<string>();

  const recents = resolveCommandsById(catalog, recentIds);
  if (recents.length > 0) {
    rows.push({ type: "header", label: "Recent" });
    for (const cmd of recents) {
      seen.add(cmd.id);
      rows.push({ type: "command", cmd });
    }
  }

  const favs = resolveCommandsById(catalog, favIds).filter(
    (c) => !seen.has(c.id)
  );
  if (favs.length > 0) {
    rows.push({ type: "header", label: "Favourites" });
    for (const cmd of favs) {
      seen.add(cmd.id);
      rows.push({ type: "command", cmd });
    }
  }

  // Remaining commands grouped by category.
  const groups: Map<string, AppCommand[]> = new Map();
  for (const cmd of catalog) {
    if (seen.has(cmd.id)) continue;
    const list = groups.get(cmd.category) ?? [];
    list.push(cmd);
    groups.set(cmd.category, list);
  }
  for (const [category, list] of groups) {
    rows.push({ type: "header", label: category });
    for (const cmd of list) rows.push({ type: "command", cmd });
  }
  return rows;
}

/** Counts the command rows (headers excluded). */
function commandCount(rows: Row[]): number {
  return rows.filter((r) => r.type === "command").length;
}

// ── Component ────────────────────────────────────────────────────────

/**
 * The Command Palette overlay. Rendered once at the app root.
 *
 * Opening: `nav.setPaletteOpen(true)` (or ⌘K).
 */
export function CommandPalette(): ReactNode {
  const nav = useNav();
  const workspace = useWorkspace();
  const toasts = useToast();

  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Build the catalog once per render with shared contexts.
  const catalog = allCommands({ nav, workspace, toasts });

  // Focus the input whenever the palette opens.
  useEffect(() => {
    if (nav.paletteOpen) {
      setQuery("");
      setActive(0);
      const t = setTimeout(() => {
        inputRef.current?.focus();
      }, 10);
      return () => clearTimeout(t);
    }
  }, [nav.paletteOpen]);

  if (!nav.paletteOpen) return null;

  const rows = buildRows(
    catalog,
    query,
    nav.recentCommands,
    nav.favouriteCommands
  );
  const count = commandCount(rows);

  // Pre-compute indexed rows for keyboard navigation.
  const indexed: { row: Row; idx: number | null }[] = (() => {
    let cmdIdx = 0;
    return rows.map((row) => {
      if (row.type === "header") return { row, idx: null };
      const i = cmdIdx;
      cmdIdx += 1;
      return { row, idx: i };
    });
  })();

  const runAtIndex = (targetIdx: number) => {
    let cmdIdx = 0;
    for (const { row } of indexed) {
      if (row.type === "command") {
        if (cmdIdx === targetIdx) {
          const cmd = row.cmd;
          nav.setRecentCommands([
            cmd.id,
            ...nav.recentCommands.filter((id) => id !== cmd.id),
          ]);
          nav.setPaletteOpen(false);
          setQuery("");
          cmd.run();
          return;
        }
        cmdIdx += 1;
      }
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      nav.setPaletteOpen(false);
      setQuery("");
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (count === 0) {
        setActive(0);
      } else {
        setActive((a) => (a + 1) % count);
      }
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (count === 0) {
        setActive(0);
      } else {
        setActive((a) => (a === 0 ? count - 1 : a - 1));
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      runAtIndex(active);
    }
  };

  const toggleFavourite = (cmdId: string) => {
    const isFav = nav.favouriteCommands.includes(cmdId);
    nav.setFavouriteCommands(
      isFav
        ? nav.favouriteCommands.filter((id) => id !== cmdId)
        : [...nav.favouriteCommands, cmdId]
    );
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[10vh] bg-black/50 backdrop-blur-sm"
      onClick={() => {
        nav.setPaletteOpen(false);
        setQuery("");
      }}
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-xl shadow-2xl mx-4 palette panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gray-700">
          <input
            ref={inputRef}
            id="command-palette-input"
            type="text"
            className="palette-input w-full bg-transparent text-gray-100 placeholder-gray-500 outline-none text-sm"
            placeholder="Type a command or search…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
            aria-label="Command palette search"
          />
          <kbd className="palette-hint mt-1 text-xs text-gray-500">Esc</kbd>
        </div>

        <div className="palette-list max-h-80 overflow-y-auto py-1">
          {count === 0 ? (
            <div className="palette-empty px-3 py-4 text-xs text-gray-500">
              {query.trim().length === 0
                ? "No commands yet"
                : "No commands match"}
            </div>
          ) : (
            indexed.map(({ row, idx }) => {
              if (row.type === "header") {
                return (
                  <div
                    key={`header-${row.label}`}
                    className="palette-category px-3 py-1.5 text-xs font-semibold text-gray-500 uppercase"
                  >
                    {row.label}
                  </div>
                );
              }

              const cmd = row.cmd;
              const thisIdx = idx ?? 0;
              const isFav = nav.favouriteCommands.includes(cmd.id);
              const is_active = thisIdx === active;

              return (
                <button
                  key={cmd.id}
                  type="button"
                  role="option"
                  aria-selected={is_active}
                  className={`palette-item flex items-center gap-2 w-full px-3 py-1.5 text-left text-sm ${
                    is_active
                      ? "palette-item-active bg-gray-800 text-white"
                      : "text-gray-300 hover:bg-gray-800"
                  } transition-colors`}
                  onMouseEnter={() => setActive(thisIdx)}
                  onClick={() => {
                    nav.setRecentCommands([
                      cmd.id,
                      ...nav.recentCommands.filter((id) => id !== cmd.id),
                    ]);
                    nav.setPaletteOpen(false);
                    setQuery("");
                    cmd.run();
                  }}
                >
                  <span className="palette-item-icon w-4 flex-shrink-0 flex justify-center">
                    <Icon name={cmd.icon} className="w-4 h-4" />
                  </span>
                  <span className="palette-item-body flex flex-col flex-1 min-w-0">
                    <span className="palette-item-label">{cmd.label}</span>
                    <span className="palette-item-desc text-xs text-gray-500 truncate">
                      {cmd.description}
                    </span>
                  </span>
                  {cmd.shortcut && (
                    <kbd className="palette-shortcut text-xs px-1.5 py-0.5 bg-gray-800 border border-gray-700 rounded text-gray-400">
                      {cmd.shortcut}
                    </kbd>
                  )}
                  <span
                    className="palette-star flex-shrink-0 cursor-pointer p-0.5 rounded hover:bg-gray-700"
                    title={isFav ? "Remove from favourites" : "Add to favourites"}
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleFavourite(cmd.id);
                    }}
                  >
                    <Icon
                      name={isFav ? "star" : "starHalf"}
                      className="w-3.5 h-3.5 text-yellow-400"
                    />
                  </span>
                </button>
              );
            })
          )}
        </div>

        <div className="palette-footer px-3 py-1.5 border-t border-gray-700 flex items-center gap-3 text-xs text-gray-500">
          <span>↑↓ navigate</span>
          <span>↵ run</span>
          <span className="flex items-center gap-0.5">
            <Icon name="star" className="w-3 h-3 text-yellow-400" />
            <span>favourite</span>
          </span>
          <span>esc close</span>
        </div>
      </div>
    </div>
  );
}
