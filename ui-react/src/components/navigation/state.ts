// ──────────────────────────────────────────────────────────────────────────────
// navigation/state.ts — view-mode enum + helper functions
//
// Mirrors: crates/nabu-ui/src/components/navigation::state
// Exports: ViewMode union, DASHBOARD_SECTIONS, dashboardSectionLabel,
//          viewModeKey, viewModeLabel, viewModeIcon, fuzzyScore.
// ──────────────────────────────────────────────────────────────────────────────



/**
 * Top-level view modes — drives the ribbon bar and view switcher.
 *
 * Mirrors: crates/nabu-ui/src/components/navigation::state::ViewMode
 */
export type ViewMode =
  | "Dashboard"
  | "Editor"
  | "Graph"
  | "Inbox"
  | "ReadingQueue"
  | "Templates"
  | "Settings"
  | "Trash"
  | "History"
  | "Recovery"
  | "Search"
  | "Calendar"
  | "Archive"
  | "SmartFolders"
  | "Canvas"
  | "Reader"
  | "Comparison"
  | "Statistics"
  | "Activity"
  | "Streaming"
  | "Chat";

/** Every dashboard section id, in display order. */
export const DASHBOARD_SECTIONS = [
  "quick_actions",
  "recently_modified",
  "favourites",
  "recently_opened",
  "pinned",
  "inbox",
  "recent_searches",
  "summary",
] as const;

/** Maps a section id to its human-readable label. */
export function dashboardSectionLabel(id: string): string {
  const labels: Record<string, string> = {
    quick_actions: "Quick Actions",
    recently_modified: "Recently Modified",
    favourites: "Favourites",
    recently_opened: "Recently Opened",
    pinned: "Pinned",
    inbox: "Inbox",
    recent_searches: "Recent Searches",
    summary: "Workspace Summary",
  };
  return labels[id] ?? "Section";
}

/** Canonical string key for a view mode (persisted to settings). */
export function viewModeKey(mode: ViewMode): string {
  const keys: Record<ViewMode, string> = {
    Dashboard: "dashboard",
    Editor: "editor",
    Graph: "graph",
    Inbox: "inbox",
    ReadingQueue: "reading_queue",
    Templates: "templates",
    Settings: "settings",
    Trash: "trash",
    History: "history",
    Recovery: "recovery",
    Search: "search",
    Calendar: "calendar",
    Archive: "archive",
    SmartFolders: "smart_folders",
    Canvas: "canvas",
    Reader: "reader",
    Comparison: "comparison",
    Statistics: "statistics",
    Activity: "activity",
    Streaming: "streaming",
    Chat: "chat",
  };
  return keys[mode] ?? "editor";
}

/** Human-readable label for a view mode. */
export function viewModeLabel(mode: ViewMode): string {
  const labels: Record<ViewMode, string> = {
    Dashboard: "Dashboard",
    Editor: "Editor",
    Graph: "Graph",
    Inbox: "Inbox",
    ReadingQueue: "Reading Queue",
    Templates: "Templates",
    Settings: "Settings",
    Trash: "Trash",
    History: "History",
    Recovery: "Recovery",
    Search: "Search",
    Calendar: "Calendar",
    Archive: "Archive",
    SmartFolders: "Smart Folders",
    Canvas: "Canvas",
    Reader: "Reader",
    Comparison: "Comparison",
    Statistics: "Statistics",
    Activity: "Activity",
    Streaming: "Streaming",
    Chat: "Chat",
  };
  return labels[mode] ?? "Editor";
}

/** Icon name for a view mode (key into the ICONS map). */
export function viewModeIcon(mode: ViewMode): string {
  const icons: Record<ViewMode, string> = {
    Dashboard: "dashboard",
    Editor: "filePen",
    Graph: "network",
    Inbox: "inbox",
    ReadingQueue: "bookOpen",
    Templates: "clipboardList",
    Settings: "settings",
    Trash: "trash2",
    History: "history",
    Recovery: "lifeBuoy",
    Search: "search",
    Calendar: "calendar",
    Archive: "archive",
    SmartFolders: "folderTree",
    Canvas: "palette",
    Reader: "bookText",
    Comparison: "comparison",
    Statistics: "trendingUp",
    Activity: "activity",
    Streaming: "sparkles",
    Chat: "messageCircle",
  };
  return icons[mode] ?? "info";
}

/**
 * Scores a fuzzy subsequence match of `query` in `candidate`.
 * Returns the score if every character of `query` was matched (in order),
 * or `undefined` when the query is not a subsequence.
 *
 * Mirrors: crates/nabu-ui/src/components/navigation::state::fuzzy_score
 */
export function fuzzyScore(query: string, candidate: string): number | undefined {
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
      if (prev !== null && ci === prev + 1) {
        score += 6;
      }
      prev = ci;
      qi += 1;
    }
  }

  return qi === q.length ? score : undefined;
}
