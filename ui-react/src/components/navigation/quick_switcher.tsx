// ──────────────────────────────────────────────────────────────────────────────
// navigation/quick_switcher.tsx — overlay quick note switcher
//
// Mirrors: crates/nabu-ui/src/components/navigation/quick_switcher.rs
//
// Keyboard-first note navigation:
//   - fuzzy matching over note titles, folder paths and note names
//   - recent notes, pinned notes and the full vault index shown when the
//     query is empty
//   - keyboard-only workflow: ↑/↓/Enter/Escape, ⌘P toggles
//
// Focused purely on *navigation* — it opens notes and never executes
// commands. The Command Palette is the command surface.
//
// Controlled by NavContext.switcherOpen. Rendered once at the app root.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, type ReactNode } from "react";
import { useNav, useWorkspace } from "../../context";
import { Icon } from "../layout/icons";
import { fuzzyScore } from "./state";
import type { NoteIndexEntry } from "../../types";

// ── Row model ─────────────────────────────────────────────────────────

/** One row in the switcher list. */
type Row = { type: "header"; label: string } | { type: "note"; note: NoteIndexEntry };

// ── Row building ─────────────────────────────────────────────────────

function buildRows(
  notes: NoteIndexEntry[],
  recent: string[],
  pinned: string[],
  query: string
): Row[] {
  const q = query.trim();

  if (q.length === 0) return buildEmptyRows(notes, recent, pinned);

  // Fuzzy match on title, folder/title and the folder path.
  const scored: { score: number; note: NoteIndexEntry }[] = [];
  for (const note of notes) {
    let best: number | undefined;
    const consider = (text: string) => {
      const s = fuzzyScore(q, text);
      if (s !== undefined && (best === undefined || s > best)) best = s;
    };
    consider(note.title);
    const folderPath = note.folder
      ? `${note.folder}/${note.title}`
      : note.title;
    consider(folderPath);
    consider(note.folder);
    consider(note.path);
    if (best !== undefined) scored.push({ score: best, note });
  }
  scored.sort(
    (a, b) => b.score - a.score || a.note.title.localeCompare(b.note.title)
  );

  const rows: Row[] = [];
  if (scored.length > 0) {
    rows.push({ type: "header", label: `${scored.length} matches` });
    for (const { note } of scored) rows.push({ type: "note", note });
  }
  return rows;
}

function buildEmptyRows(
  notes: NoteIndexEntry[],
  recent: string[],
  pinned: string[]
): Row[] {
  const rows: Row[] = [];
  const seen = new Set<string>();

  const recentNotes = recent
    .map((p) => notes.find((n) => n.path === p))
    .filter((n): n is NoteIndexEntry => n !== undefined);
  if (recentNotes.length > 0) {
    rows.push({ type: "header", label: "Recent" });
    for (const note of recentNotes) {
      seen.add(note.path);
      rows.push({ type: "note", note });
    }
  }

  const pinnedNotes = pinned
    .map((p) => notes.find((n) => n.path === p))
    .filter(
      (n): n is NoteIndexEntry => n !== undefined && !seen.has(n.path)
    );
  if (pinnedNotes.length > 0) {
    rows.push({ type: "header", label: "Pinned" });
    for (const note of pinnedNotes) {
      seen.add(note.path);
      rows.push({ type: "note", note });
    }
  }

  const rest = notes
    .filter((n) => !seen.has(n.path))
    .sort((a, b) =>
      a.title.toLowerCase().localeCompare(b.title.toLowerCase())
    );
  if (rest.length > 0) {
    rows.push({ type: "header", label: "All notes" });
    for (const note of rest) rows.push({ type: "note", note });
  }
  return rows;
}

/** Counts the note rows (headers excluded). */
function noteCount(rows: Row[]): number {
  return rows.filter((r) => r.type === "note").length;
}

function folderOf(note: NoteIndexEntry): string {
  return note.folder || "/";
}

// ── Component ────────────────────────────────────────────────────────

/**
 * The Quick Switcher overlay. Rendered once at the app root.
 *
 * Opening: `nav.setSwitcherOpen(true)` (or ⌘P).
 */
export function QuickSwitcher(): ReactNode {
  const nav = useNav();
  const workspace = useWorkspace();

  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus the input + reset state whenever the switcher opens.
  useEffect(() => {
    if (nav.switcherOpen) {
      setQuery("");
      setActive(0);
      const t = setTimeout(() => inputRef.current?.focus(), 10);
      return () => clearTimeout(t);
    }
  }, [nav.switcherOpen]);

  if (!nav.switcherOpen) return null;

  // Pinned notes = pinned workspace tabs.
  const pinned = workspace.tabs.filter((t) => t.pinned).map((t) => t.path);

  const rows = buildRows(nav.notesIndex, nav.recentNotes, pinned, query);
  const count = noteCount(rows);

  // Pre-compute indexed rows for keyboard navigation.
  const indexed: { row: Row; idx: number | null }[] = (() => {
    let noteIdx = 0;
    return rows.map((row) => {
      if (row.type === "header") return { row, idx: null };
      const i = noteIdx;
      noteIdx += 1;
      return { row, idx: i };
    });
  })();

  const openNote = (path: string) => {
    workspace.openTab(path);
    nav.setRecentNotes([path, ...nav.recentNotes.filter((p) => p !== path)]);
    nav.setSwitcherOpen(false);
    nav.setViewMode("Editor");
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      nav.setSwitcherOpen(false);
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
      // Activate the note at the active index.
      let noteIdx = 0;
      for (const { row } of indexed) {
        if (row.type === "note") {
          if (noteIdx === active) {
            openNote(row.note.path);
            return;
          }
          noteIdx += 1;
        }
      }
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[10vh] bg-black/50 backdrop-blur-sm"
      onClick={() => {
        nav.setSwitcherOpen(false);
        setQuery("");
      }}
      role="dialog"
      aria-modal="true"
      aria-label="Quick switcher"
    >
      <div
        className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-xl shadow-2xl mx-4 palette panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gray-700">
          <input
            ref={inputRef}
            id="quick-switcher-input"
            type="text"
            className="palette-input w-full bg-transparent text-gray-100 placeholder-gray-500 outline-none text-sm"
            placeholder="Jump to a note…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
            aria-label="Quick switcher search"
          />
          <kbd className="palette-hint mt-1 text-xs text-gray-500">Esc</kbd>
        </div>

        <div className="palette-list max-h-80 overflow-y-auto py-1">
          {count === 0 ? (
            <div className="palette-empty px-3 py-4 text-xs text-gray-500">
              {query.trim().length === 0
                ? "No notes in the vault yet"
                : "No notes match"}
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

              const note = row.note;
              const thisIdx = idx ?? 0;
              const is_active = thisIdx === active;

              return (
                <button
                  key={note.path}
                  type="button"
                  role="option"
                  aria-selected={is_active}
                  className={`palette-item flex items-center gap-2 w-full px-3 py-1.5 text-left text-sm ${
                    is_active
                      ? "palette-item-active bg-gray-800 text-white"
                      : "text-gray-300 hover:bg-gray-800"
                  } transition-colors`}
                  onMouseEnter={() => setActive(thisIdx)}
                  onClick={() => openNote(note.path)}
                >
                  <span className="palette-item-icon w-4 flex-shrink-0 flex justify-center">
                    <Icon name="fileText" className="w-4 h-4" />
                  </span>
                  <span className="palette-item-body flex flex-col flex-1 min-w-0">
                    <span className="palette-item-label truncate">
                      {note.title}
                    </span>
                    <span className="palette-item-desc text-xs text-gray-500 truncate">
                      {folderOf(note)}
                    </span>
                  </span>
                </button>
              );
            })
          )}
        </div>

        <div className="palette-footer px-3 py-1.5 border-t border-gray-700 flex items-center gap-3 text-xs text-gray-500">
          <span>↑↓ navigate</span>
          <span>↵ open</span>
          <span>esc close</span>
        </div>
      </div>
    </div>
  );
}
