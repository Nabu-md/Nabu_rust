// ──────────────────────────────────────────────────────────────────────────────
// navigation/commands.ts — command catalog (source of truth for palette)
//
// Mirrors: ui-react/src/components/navigation/commands.rs
//
// Each entry carries a stable id, label, aliases, category, description,
// optional keyboard hint, icon, and a run() callback. The catalog is built
// with shared contexts (nav, workspace, toasts) captured at call time.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav, useWorkspace } from "../../context";
import type { ToastContextValue } from "../../context";
import {
  noteCreateFile,
  noteDaily,
  inboxQuickCapture,
  revealInFileManager,
} from "../../ipc";
import type { ViewMode } from "./state";

/** Context bundle passed into command builders. */
export interface CommandContext {
  nav: ReturnType<typeof useNav>;
  workspace: ReturnType<typeof useWorkspace>;
  toasts: ToastContextValue;
}

/** One command the palette can execute. */
export interface AppCommand {
  id: string;
  label: string;
  aliases: string[];
  category: string;
  description: string;
  shortcut: string | null;
  icon: string;
  run: () => void;
}

/** Resolves commands by id from a catalog (for recent / favourites sections). */
export function resolveCommandsById(
  catalog: AppCommand[],
  ids: string[]
): AppCommand[] {
  return ids
    .map((id) => catalog.find((c) => c.id === id))
    .filter((c): c is AppCommand => c !== undefined);
}

// ── Command builders ─────────────────────────────────────────────────

function setView(nav: CommandContext["nav"], mode: ViewMode): () => void {
  return () => nav.setViewMode(mode);
}

function toggleSidebar(nav: CommandContext["nav"]): () => void {
  return () => nav.setShowLeftSidebar(!nav.showLeftSidebar);
}

function toggleInspector(nav: CommandContext["nav"]): () => void {
  return () => nav.setShowRightInspector(!nav.showRightInspector);
}

function openPalette(nav: CommandContext["nav"]): () => void {
  return () => {
    nav.setPaletteOpen(false);
    nav.setSwitcherOpen(false);
    nav.setShortcutsOpen(false);
    nav.setPaletteOpen(true);
  };
}

function openSwitcher(nav: CommandContext["nav"]): () => void {
  return () => {
    nav.setPaletteOpen(false);
    nav.setSwitcherOpen(false);
    nav.setShortcutsOpen(false);
    nav.setSwitcherOpen(true);
  };
}

function openShortcuts(nav: CommandContext["nav"]): () => void {
  return () => {
    nav.setPaletteOpen(false);
    nav.setSwitcherOpen(false);
    nav.setShortcutsOpen(false);
    nav.setShortcutsOpen(true);
  };
}

function openSearch(nav: CommandContext["nav"]): () => void {
  return () => {
    nav.setSearchQuery("");
    nav.setViewMode("Search");
  };
}

/** Shared "create a note in the vault root and open it". */
export function createNewNote(
  workspace: CommandContext["workspace"],
  toasts: ToastContextValue
): () => void {
  return () => {
    void (async () => {
      const stamp = new Date();
      const name = `note-${stamp.getTime()}.md`;
      try {
        await noteCreateFile(name);
        workspace.openTab(name);
        workspace.refreshTreeBump();
        toasts.toast(`Created ${name}`, { variant: "success" });
      } catch {
        toasts.toast("Could not create that note", { variant: "error" });
      }
    })();
  };
}

/** Shared "quick capture into the Inbox". */
function quickCapture(
  nav: CommandContext["nav"],
  toasts: ToastContextValue
): () => void {
  return () => {
    void (async () => {
      const stamp = new Date();
      const month = stamp.getMonth() + 1;
      const day = stamp.getDate();
      const hours = stamp.getHours();
      const hour12 = hours % 12 === 0 ? 12 : hours % 12;
      const meridiem = hours < 12 ? "AM" : "PM";
      const title = `Capture — ${month} ${day} ${hour12}:${String(
        stamp.getMinutes()
      ).padStart(2, "0")} ${meridiem}`;
      try {
        await inboxQuickCapture(title, "");
        nav.setViewMode("Inbox");
        toasts.toast("Added to your Inbox for review.", { variant: "success" });
      } catch {
        toasts.toast("Could not capture that note", { variant: "error" });
      }
    })();
  };
}

/** Shared "reveal the vault folder in the OS file manager". */
function openVaultFolder(): () => void {
  return () => {
    void (async () => {
      try {
        await revealInFileManager("");
      } catch {
        // Non-fatal
      }
    })();
  };
}

/** Shared "open (or create) today's dated note". */
export function openDailyNote(
  workspace: CommandContext["workspace"],
  toasts: ToastContextValue
): () => void {
  return () => {
    void (async () => {
      try {
        const path = await noteDaily();
        workspace.openTab(path);
        toasts.toast(`Opened ${path}`, { variant: "success" });
      } catch {
        toasts.toast("Could not open the daily note", { variant: "error" });
      }
    })();
  };
}

// ── Catalog ─────────────────────────────────────────────────────────

/**
 * Builds the full command catalog. Call at render time with the shared
 * contexts so closures capture them by value.
 */
export function allCommands(ctx: CommandContext): AppCommand[] {
  const nav = ctx.nav;
  const ws = ctx.workspace;
  const toasts = ctx.toasts;

  return [
    // ── Navigation ──
    { id: "nav.dashboard", label: "Go to Dashboard", aliases: ["home", "start"], category: "Navigation", description: "Open the home dashboard", shortcut: "⌘1", icon: "dashboard", run: setView(nav, "Dashboard") },
    { id: "nav.editor", label: "Go to Editor", aliases: ["note", "write"], category: "Navigation", description: "Open the note editor", shortcut: "⌘2", icon: "filePen", run: setView(nav, "Editor") },
    { id: "nav.graph", label: "Go to Graph", aliases: ["canvas", "links", "network"], category: "Navigation", description: "Open the knowledge graph view", shortcut: "⌘3", icon: "network", run: setView(nav, "Graph") },
    { id: "nav.inbox", label: "Go to Inbox", aliases: ["capture"], category: "Navigation", description: "Review captured knowledge", shortcut: null, icon: "inbox", run: setView(nav, "Inbox") },
    { id: "nav.reading_queue", label: "Go to Reading Queue", aliases: ["read later", "queue"], category: "Navigation", description: "Open the reading queue", shortcut: null, icon: "bookOpen", run: setView(nav, "ReadingQueue") },
    { id: "nav.templates", label: "Go to Templates", aliases: ["template manager"], category: "Navigation", description: "Manage note templates", shortcut: null, icon: "clipboardList", run: setView(nav, "Templates") },
    { id: "nav.trash", label: "Go to Trash", aliases: ["deleted", "recycle bin"], category: "Navigation", description: "Restore or permanently delete notes", shortcut: null, icon: "trash2", run: setView(nav, "Trash") },
    { id: "nav.history", label: "Go to Version History", aliases: ["versions", "snapshots"], category: "Navigation", description: "Browse note snapshots", shortcut: null, icon: "history", run: setView(nav, "History") },
    { id: "nav.recovery", label: "Go to Recovery Manager", aliases: ["restore", "session"], category: "Navigation", description: "Inspect and restore saved sessions", shortcut: null, icon: "lifeBuoy", run: setView(nav, "Recovery") },
    { id: "nav.calendar", label: "Go to Calendar", aliases: ["dates", "journal", "daily notes"], category: "Navigation", description: "Browse notes by date", shortcut: null, icon: "calendar", run: setView(nav, "Calendar") },
    { id: "nav.archive", label: "Go to Archive", aliases: ["archived", "stored"], category: "Navigation", description: "Restore archived notes", shortcut: null, icon: "archive", run: setView(nav, "Archive") },
    { id: "nav.smart_folders", label: "Go to Smart Folders", aliases: ["virtual folders", "collections", "queries"], category: "Navigation", description: "Manage query-powered folders", shortcut: null, icon: "folderTree", run: setView(nav, "SmartFolders") },
    { id: "nav.settings", label: "Open Settings", aliases: ["preferences", "options"], category: "Navigation", description: "Configure Nabu", shortcut: "⌘,", icon: "settings", run: setView(nav, "Settings") },
    { id: "nav.canvas", label: "Open Canvas", aliases: ["whiteboard", "visual workspace", "spatial"], category: "Navigation", description: "Open the infinite visual canvas", shortcut: "⌘⇧C", icon: "palette", run: setView(nav, "Canvas") },
    { id: "nav.reader", label: "Open Reader Mode", aliases: ["read", "reading mode", "distraction-free"], category: "Navigation", description: "Open the distraction-free reader", shortcut: "⌘⇧1", icon: "bookText", run: setView(nav, "Reader") },
    { id: "nav.comparison", label: "Open Comparison View", aliases: ["diff", "compare notes", "side-by-side"], category: "Navigation", description: "Compare two notes or revisions", shortcut: "⌘⇧M", icon: "comparison", run: setView(nav, "Comparison") },
    { id: "nav.statistics", label: "Open Statistics", aliases: ["insights", "metrics", "dashboard stats"], category: "Navigation", description: "View vault-wide metrics and insights", shortcut: "⌘⇧S", icon: "trendingUp", run: setView(nav, "Statistics") },
    { id: "capture.quick", label: "Quick Capture to Inbox", aliases: ["capture", "clip", "inbox note"], category: "Capture", description: "Add a pending note to the Inbox for review", shortcut: null, icon: "zap", run: quickCapture(nav, toasts) },
    { id: "nav.search", label: "Search all notes", aliases: ["find", "full-text", "search"], category: "Navigation", description: "Open the full-text search page", shortcut: "⌘⇧F", icon: "search", run: openSearch(nav) },
    { id: "nav.palette", label: "Open Command Palette", aliases: ["commands", "⌘k", "palette"], category: "Navigation", description: "Run any command by name", shortcut: "⌘K", icon: "command", run: openPalette(nav) },
    { id: "nav.quick_switcher", label: "Open Quick Switcher", aliases: ["goto note", "switch", "⌘p"], category: "Navigation", description: "Jump to any note by name", shortcut: "⌘P", icon: "zap", run: openSwitcher(nav) },
    { id: "nav.shortcuts", label: "Keyboard Shortcuts Reference", aliases: ["hotkeys", "keybindings", "help"], category: "Navigation", description: "Browse every keyboard shortcut", shortcut: null, icon: "keyboard", run: openShortcuts(nav) },
    // ── Notes ──
    { id: "note.new", label: "Create New Note", aliases: ["add note", "new note"], category: "Notes", description: "Create a note in the vault root and open it", shortcut: "⌘N", icon: "plus", run: createNewNote(ws, toasts) },
    { id: "note.daily", label: "Open Daily Note", aliases: ["today", "journal"], category: "Notes", description: "Open (or create) today's dated note", shortcut: null, icon: "calendar", run: openDailyNote(ws, toasts) },
    // ── View ──
    { id: "view.sidebar", label: "Toggle Left Sidebar", aliases: ["explorer", "file tree", "panel"], category: "View", description: "Show or hide the file explorer", shortcut: "⌘\\", icon: "folder", run: toggleSidebar(nav) },
    { id: "view.inspector", label: "Toggle Right Inspector", aliases: ["properties", "backlinks", "panel"], category: "View", description: "Show or hide the inspector sidebar", shortcut: "⌘⇧\\", icon: "clipboardList", run: toggleInspector(nav) },
    { id: "nav.vault_folder", label: "Reveal Vault in Folder", aliases: ["show in finder", "reveal folder"], category: "Navigation", description: "Open the vault folder in the OS file manager", shortcut: null, icon: "folderOpen", run: openVaultFolder() },
  ];
}
