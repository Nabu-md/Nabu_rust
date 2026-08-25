// ──────────────────────────────────────────────────────────────────────────────
// navigation/HomeScreen.tsx — vault front door
//
// Mirrors: crates/nabu-ui/src/components/navigation/home_screen.rs
//
// Shown when no note is open in the editor. It is the vault's front door — a
// welcome banner, a row of quick actions (today's note, new note, search,
// statistics, inbox), the recently modified list (sourced from the
// NavContext note index, no extra IPC) and the user's favourites.
//
// Opening a note here switches the view to the editor and records the visit
// in recent-notes history, mirroring the Quick Switcher behaviour.
// ──────────────────────────────────────────────────────────────────────────────

import { type ReactNode } from "react";
import { Icon } from "../layout/icons";
import { useNav, useWorkspace } from "../../context";
import { dailyNoteFor, noteCreateFile } from "../../ipc";
import type { NoteIndexEntry } from "../../types";
import type { ViewMode } from "./state";

// ── Helpers (mirrors Dioxus home_screen.rs) ─────────────────────────────────

function fmtDate(rfc: string): string {
  try {
    const dt = new Date(rfc);
    if (Number.isNaN(dt.getTime())) return rfc.slice(0, 10);
    return dt.toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return rfc.slice(0, 10);
  }
}

function todayStr(): string {
  return new Date().toISOString().slice(0, 10);
}

/// Top-N notes by modification time from the vault index.
function recentNotes(index: NoteIndexEntry[], limit: number): NoteIndexEntry[] {
  return [...index]
    .sort((a, b) => b.modified_at.localeCompare(a.modified_at))
    .slice(0, limit);
}

// ── Note opener ─────────────────────────────────────────────────────────────

function openNote(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
  path: string,
): void {
  ws.openTab(path);
  nav.setRecentNotes([path, ...nav.recentNotes.filter((p) => p !== path)].slice(0, 20));
  nav.setViewMode("Editor" as ViewMode);
}

function openToday(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): void {
  const date = todayStr();
  dailyNoteFor(date)
    .then((path) => openNote(nav, ws, path))
    .catch(() => {
      /* Non-fatal: daily note failed to open */
    });
}

function openNewNote(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): void {
  const path = "untitled.md";
  noteCreateFile(path, "# Untitled")
    .then(() => openNote(nav, ws, path))
    .catch(() => {
      /* Non-fatal */
    });
}

// ── Sub-renderers ───────────────────────────────────────────────────────────

function renderNoteRow(
  n: NoteIndexEntry,
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  const path = n.path;
  const title = n.title;
  const folder = n.folder;
  const modified = n.modified_at;
  return (
    <div
      key={path}
      className="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm"
      onClick={() => openNote(nav, ws, path)}
    >
      <div className="flex-1 min-w-0">
        <div className="text-sm text-gray-200 truncate">{title}</div>
        {folder && <span className="text-xs text-gray-500">{folder}/</span>}
      </div>
      <div className="ml-auto text-xs text-gray-500">{fmtDate(modified)}</div>
    </div>
  );
}

function renderNoteList(
  notes: NoteIndexEntry[],
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  if (notes.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500 text-sm">
        <Icon name="fileText" className="w-8 h-8 mx-auto mb-2" />
        <div>No notes yet</div>
        <div className="text-xs mt-1">Create a note to get started.</div>
      </div>
    );
  }
  return (
    <div className="space-y-1">
      {notes.map((n) => renderNoteRow(n, nav, ws))}
    </div>
  );
}

function renderFavouriteTag(
  n: NoteIndexEntry,
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  const path = n.path;
  return (
    <span
      key={path}
      className="inline-flex items-center gap-1 rounded bg-gray-800/50 px-2 py-1 text-xs text-gray-200 border border-gray-700 cursor-pointer"
      onClick={() => openNote(nav, ws, path)}
    >
      <Icon name="bookMarked" className="w-3 h-3" />
      {n.title}
    </span>
  );
}

function renderFavourites(
  favs: NoteIndexEntry[],
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  if (favs.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500 text-sm">
        <Icon name="bookMarked" className="w-8 h-8 mx-auto mb-2" />
        <div>No favourites</div>
        <div className="text-xs mt-1">Star a note to pin it here.</div>
      </div>
    );
  }
  return (
    <div className="flex flex-wrap gap-2">
      {favs.map((n) => renderFavouriteTag(n, nav, ws))}
    </div>
  );
}

// ── HomeScreen main component ───────────────────────────────────────────────

/** The HomeScreen view — welcome banner + quick actions + recent activity. */
export function HomeScreen(): ReactNode {
  const nav = useNav();
  const ws = useWorkspace();

  // Derive data from NavContext (no extra IPC — mirrors the Dioxus spec)
  const vaultName = nav.vaultName || "Vault";
  const index = nav.notesIndex;
  const favourites = nav.favourites;
  const recent = recentNotes(index, 10);
  const favRows: NoteIndexEntry[] = favourites
    .map((p) => index.find((n) => n.path === p))
    .filter((n): n is NoteIndexEntry => n !== undefined);

  // Quick action handlers
  const handleTodayNote = () => openToday(nav, ws);
  const handleNewNote = () => openNewNote(nav, ws);
  const handleSearch = () => nav.setViewMode("Search" as ViewMode);
  const handleInbox = () => nav.setViewMode("Inbox" as ViewMode);
  const handleStats = () => nav.setViewMode("Statistics" as ViewMode);

  const quickActions = [
    { icon: "clock", label: "Today's Note", onClick: handleTodayNote },
    { icon: "filePlus", label: "New Note", onClick: handleNewNote },
    { icon: "search", label: "Search", onClick: handleSearch },
    { icon: "inbox", label: "Inbox", onClick: handleInbox },
    { icon: "database", label: "Statistics", onClick: handleStats },
  ];

  return (
    <div className="home-screen h-screen overflow-y-auto bg-gray-950 text-gray-100">
      {/* Hero banner */}
      <div className="home-hero border-b border-gray-700 px-6 py-8">
        <h1 className="home-title text-3xl font-bold text-gray-100">
          Welcome to {vaultName}
        </h1>
        <p className="text-sm text-gray-400 mt-2 max-w-lg">
          Your knowledge base is ready. Start a note, search everything, or
          open today's entry.
        </p>
      </div>

      {/* Quick actions */}
      <div className="home-actions px-6 py-4">
        <div className="flex flex-wrap gap-3">
          {quickActions.map((a) => (
            <button
              key={a.label}
              type="button"
              className="home-action inline-flex items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-2 border border-gray-700 hover:bg-gray-700/50 text-sm text-gray-200"
              onClick={a.onClick}
            >
              <Icon name={a.icon} className="w-4 h-4" />
              {a.label}
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
      <div className="home-content px-6 py-6 space-y-8">
        <section className="home-section">
          <h2 className="text-sm font-semibold text-gray-200 mb-3">
            Recently Modified
          </h2>
          {renderNoteList(recent, nav, ws)}
        </section>

        <section className="home-section mt-8">
          <h2 className="text-sm font-semibold text-gray-200 mb-3">Favourites</h2>
          {renderFavourites(favRows, nav, ws)}
        </section>
      </div>
    </div>
  );
}
