// ──────────────────────────────────────────────────────────────────────────────
// navigation/Dashboard.tsx — configurable home view
//
// Mirrors: ui-react/src/components/navigation/dashboard.rs
//
// A configurable home view built from the enabled `dashboardSections` widget
// set. Widgets pull from NavContext discovery data — favourites, recent
// notes, recent searches, pinned notes — plus two IPC calls loaded once and
// refreshed on event:
//
//   - statistics_get → summary cards + the Recently Modified widget.
//   - inbox_get_queue → the Inbox widget (pending captures).
//
// All IPC goes through ipc.ts wrappers so a rejected promise becomes a
// graceful error state instead of a crash.
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useState, type ReactNode } from "react";
import { Icon } from "../layout/icons";
import { useNav, useWorkspace } from "../../context";
import { statisticsGet, inboxGetQueue, dailyNoteFor, noteCreateFile } from "../../ipc";
import type { NoteIndexEntry } from "../../types";
import { dashboardSectionLabel } from "./state";
import type { ViewMode } from "./state";

// ── Local projection types (subset of VaultStatistics) ─────────────────────

interface DashboardStats {
  note_count: number;
  folder_count: number;
  tag_count: number;
  total_tags: number;
  graph_nodes: number;
  graph_edges: number;
  storage_bytes: number;
  writing_streak_days: number;
  active_days_last_30: number;
  recently_modified: NoteStat[];
}

interface NoteStat {
  path: string;
  title: string;
  folder: string;
  modified_at: string;
  created_at: string | null;
  size: number;
}

interface InboxRow {
  id: string;
  title: string;
  object_type: string;
  suggested_folder: string | null;
  confidence: number | null;
  status: string;
}

// ── Load-state lifecycle ────────────────────────────────────────────────────

type LoadState = "loading" | "error" | "loaded";

// ── Helpers ─────────────────────────────────────────────────────────────────

function fmtBytes(n: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  if (n === 0) return "0 B";
  let b = n;
  let i = 0;
  while (b >= 1024 && i < units.length - 1) {
    b /= 1024;
    i++;
  }
  return i === 0 ? `${n} ${units[0]}` : `${b.toFixed(1)} ${units[i]}`;
}

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

function basename(path: string): string {
  return path
    .split("/")
    .pop()
    ?.replace(/\.md$/, "") ?? path;
}

function noteTitle(index: NoteIndexEntry[], path: string): string {
  const match = index.find((n) => n.path === path);
  return match?.title ?? basename(path);
}

// ── Data loading ────────────────────────────────────────────────────────────

interface LoadedData {
  stats: DashboardStats | null;
  statsState: LoadState;
  inboxItems: InboxRow[];
  inboxState: LoadState;
}

async function loadDashboardData(): Promise<LoadedData> {
  // Load stats
  let stats: DashboardStats | null = null;
  let statsState: LoadState = "loading";
  try {
    const result = await statisticsGet();
    stats = {
      note_count: result.note_count,
      folder_count: result.folder_count,
      tag_count: result.tag_count,
      total_tags: result.total_tags,
      graph_nodes: result.graph_nodes,
      graph_edges: result.graph_edges,
      storage_bytes: result.storage_bytes,
      writing_streak_days: result.writing_streak_days,
      active_days_last_30: result.active_days_last_30,
      recently_modified: (result.recently_modified ?? []).map((n) => ({
        path: n.path,
        title: n.title,
        folder: n.folder,
        modified_at: n.modified_at,
        created_at: n.created_at ?? null,
        size: n.size,
      })),
    };
    statsState = "loaded";
  } catch {
    stats = null;
    statsState = "error";
  }

  // Load inbox
  let inboxItems: InboxRow[] = [];
  let inboxState: LoadState = "loading";
  try {
    const items = await inboxGetQueue();
    inboxItems = items.map((item) => ({
      id: item.id,
      title: item.title,
      object_type: item.object_type,
      suggested_folder: item.suggested_folder ?? null,
      confidence: item.confidence ?? null,
      status: item.status,
    }));
    inboxState = "loaded";
  } catch {
    inboxItems = [];
    inboxState = "error";
  }

  return { stats, statsState, inboxItems, inboxState };
}

// ── Open-note helper ────────────────────────────────────────────────────────

function openNote(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
  path: string,
) {
  ws.openTab(path);
  nav.setRecentNotes([path, ...nav.recentNotes.filter((p) => p !== path)].slice(0, 20));
  // Persist to settings
  // (NavProvider in the Dioxus spec persists via settings_persist; in React
  // we mirror this by updating the NavContext state which is the source of truth)
  nav.setViewMode("Editor" as ViewMode);
}

// ── Sub-components ──────────────────────────────────────────────────────────

interface StatCardProps {
  icon: string;
  label: string;
  value: string;
}

function StatCard({ icon, label, value }: StatCardProps): ReactNode {
  return (
    <div className="stat-card flex items-center gap-3 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700">
      <Icon name={icon} className="w-4 h-4 text-gray-400 shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="text-xs text-gray-400">{label}</div>
        <div className="text-lg font-semibold text-gray-100 truncate">{value}</div>
      </div>
    </div>
  );
}

function renderSummary(
  statsState: LoadState,
  stats: DashboardStats | null,
): ReactNode {
  switch (statsState) {
    case "loading":
      return (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div
              key={i}
              className="h-10 bg-gray-800 rounded animate-pulse"
            />
          ))}
        </div>
      );

    case "error":
      return (
        <div className="text-center py-8 text-gray-500 text-sm">
          <div className="mb-2">Couldn't load vault statistics.</div>
        </div>
      );

    case "loaded":
      if (!stats) return null;
      return (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <StatCard icon="fileText" label="Notes" value={stats.note_count.toString()} />
          <StatCard icon="folder" label="Folders" value={stats.folder_count.toString()} />
          <StatCard icon="tag" label="Tags" value={stats.total_tags.toString()} />
          <StatCard icon="database" label="Graph" value={`${stats.graph_nodes} → ${stats.graph_edges}`} />
          <StatCard icon="hardDrive" label="Storage" value={fmtBytes(stats.storage_bytes)} />
          <StatCard icon="activity" label="Streak" value={`${stats.writing_streak_days} days`} />
        </div>
      );

    default:
      return null;
  }
}

// ── Note list helpers ───────────────────────────────────────────────────────

function renderNoteList(
  notes: NoteStat[],
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
  index: NoteIndexEntry[],
): ReactNode {
  if (notes.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500 text-sm">
        <Icon name="fileText" className="w-8 h-8 mx-auto mb-2" />
        <div>No recent notes</div>
        <div className="text-xs mt-1">No activity yet.</div>
      </div>
    );
  }

  return (
    <div className="space-y-1">
      {notes.map((n) => {
        const path = n.path;
        const title = noteTitle(index, path);
        const folder = index.find((entry) => entry.path === path)?.folder ?? n.folder;
        const modified = n.modified_at;
        return (
          <div
            key={path}
            className="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm"
            onClick={() => openNote(nav, ws, path)}
          >
            <div className="flex-1 min-w-0">
              <div className="text-sm text-gray-200 truncate">{title}</div>
              {folder && (
                <span className="text-xs text-gray-500">{folder}/</span>
              )}
            </div>
            <div className="ml-auto text-xs text-gray-500">{fmtDate(modified)}</div>
          </div>
        );
      })}
    </div>
  );
}

function renderPathList(
  paths: string[],
  index: NoteIndexEntry[],
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  if (paths.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500 text-sm">
        <Icon name="fileText" className="w-8 h-8 mx-auto mb-2" />
        <div>Empty</div>
        <div className="text-xs mt-1">No items yet.</div>
      </div>
    );
  }

  return (
    <div className="space-y-1">
      {paths.map((p) => {
        const title = noteTitle(index, p);
        const folder = index.find((n) => n.path === p)?.folder ?? "";
        return (
          <div
            key={p}
            className="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-800/50 cursor-pointer text-sm"
            onClick={() => openNote(nav, ws, p)}
          >
            <div className="flex-1 min-w-0">
              <div className="text-sm text-gray-200 truncate">{title}</div>
              {folder && (
                <span className="text-xs text-gray-500">{folder}/</span>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function renderPinned(
  index: NoteIndexEntry[],
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  const pinned = index.filter((n) => n.pinned);
  if (pinned.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500 text-sm">
        <Icon name="bookMarked" className="w-8 h-8 mx-auto mb-2" />
        <div>No pinned notes</div>
        <div className="text-xs mt-1">Pin a note to see it here.</div>
      </div>
    );
  }

  return (
    <div className="flex flex-wrap gap-2">
      {pinned.map((n) => {
        const path = n.path;
        return (
          <span
            key={n.path}
            className="inline-flex items-center gap-1 rounded bg-gray-800/50 px-2 py-1 text-xs text-gray-200 border border-gray-700 cursor-pointer"
            onClick={() => openNote(nav, ws, path)}
          >
            <Icon name="bookMarked" className="w-3 h-3" />
            {n.title}
          </span>
        );
      })}
    </div>
  );
}

function renderInbox(
  inboxState: LoadState,
  items: InboxRow[],
): ReactNode {
  switch (inboxState) {
    case "loading":
      return (
        <div className="space-y-1">
          {Array.from({ length: 3 }).map((_, i) => (
            <div
              key={i}
              className="h-4 bg-gray-800 rounded animate-pulse"
            />
          ))}
        </div>
      );

    case "error":
      return (
        <div className="text-center py-8 text-gray-500 text-sm">
          <div className="mb-2">Couldn't load pending captures.</div>
        </div>
      );

    case "loaded":
      if (items.length === 0) {
        return (
          <div className="text-center py-8 text-gray-500 text-sm">
            <Icon name="inbox" className="w-8 h-8 mx-auto mb-2" />
            <div>Inbox is clean</div>
            <div className="text-xs mt-1">No pending captures.</div>
          </div>
        );
      }
      return (
        <div className="space-y-1">
          {items.map((item) => (
            <div
              key={item.id}
              className="flex items-center gap-3 px-3 py-2 rounded-lg bg-gray-800/30 border border-gray-700 text-sm"
            >
              <Icon name="fileText" className="w-4 h-4 text-gray-400 shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="text-sm text-gray-200 truncate">{item.title}</div>
              </div>
              <span className="text-xs text-gray-500">{item.object_type}</span>
              {item.suggested_folder && (
                <span className="text-xs text-blue-400">→ {item.suggested_folder}</span>
              )}
            </div>
          ))}
        </div>
      );

    default:
      return null;
  }
}

function renderSearches(
  searches: string[],
  nav: ReturnType<typeof useNav>,
): ReactNode {
  if (searches.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500 text-sm">
        <Icon name="search" className="w-8 h-8 mx-auto mb-2" />
        <div>No recent searches</div>
        <div className="text-xs mt-1">Searches you run will appear here.</div>
      </div>
    );
  }

  return (
    <div className="flex flex-wrap gap-2">
      {searches.map((q) => (
        <button
          key={q}
          type="button"
          className="search-chip inline-flex items-center gap-1 rounded bg-gray-800/50 px-2 py-1 text-xs text-gray-300 border border-gray-700 hover:bg-gray-700/50"
          onClick={() => {
            nav.setSearchQuery(q);
            nav.setViewMode("Search" as ViewMode);
          }}
        >
          <Icon name="search" className="w-3 h-3" />
          {q}
        </button>
      ))}
    </div>
  );
}

// ── Quick actions ───────────────────────────────────────────────────────────

function renderQuickActions(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  const handleTodayNote = async () => {
    try {
      const path = await dailyNoteFor(todayStr());
      openNote(nav, ws, path);
    } catch {
      // Non-fatal
    }
  };

  const handleNewNote = async () => {
    const name = "untitled.md";
    try {
      await noteCreateFile(name, "# Untitled");
      openNote(nav, ws, name);
    } catch {
      // Non-fatal
    }
  };

  const handleSearch = () => nav.setViewMode("Search" as ViewMode);
  const handleInbox = () => nav.setViewMode("Inbox" as ViewMode);
  const handleStats = () => nav.setViewMode("Statistics" as ViewMode);

  const actions = [
    { icon: "clock", label: "Today's Note", onClick: handleTodayNote },
    { icon: "filePlus", label: "New Note", onClick: handleNewNote },
    { icon: "search", label: "Search", onClick: handleSearch },
    { icon: "inbox", label: "Inbox", onClick: handleInbox },
    { icon: "database", label: "Statistics", onClick: handleStats },
  ];

  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
      {actions.map((a) => (
        <button
          key={a.label}
          type="button"
          className="quick-action flex flex-col items-center gap-2 rounded-lg bg-gray-800/50 px-4 py-3 border border-gray-700 hover:bg-gray-700/50 transition-colors text-center"
          onClick={a.onClick}
        >
          <Icon name={a.icon} className="w-5 h-5 text-gray-300" />
          <span className="text-xs text-gray-300">{a.label}</span>
        </button>
      ))}
    </div>
  );
}

// ── Section renderer ────────────────────────────────────────────────────────

function renderSection(
  section: string,
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
  index: NoteIndexEntry[],
  statsState: LoadState,
  stats: DashboardStats | null,
  inboxState: LoadState,
  inboxItems: InboxRow[],
): ReactNode {
  const label = dashboardSectionLabel(section);

  const widget = (() => {
    switch (section) {
      case "quick_actions":
        return renderQuickActions(nav, ws);
      case "recently_modified":
        if (statsState === "loading" && !stats) {
          return (
            <div className="space-y-1">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="h-4 bg-gray-800 rounded animate-pulse" />
              ))}
            </div>
          );
        }
        if (statsState === "error") {
          return (
            <div className="text-center py-4 text-gray-500 text-sm">
              Couldn't load statistics.
            </div>
          );
        }
        return renderNoteList(
          stats?.recently_modified ?? [],
          nav,
          ws,
          index,
        );
      case "favourites":
        return renderPathList(
          nav.favourites,
          index,
          nav,
          ws,
        );
      case "recently_opened":
        return renderPathList(
          nav.recentNotes,
          index,
          nav,
          ws,
        );
      case "pinned":
        return renderPinned(index, nav, ws);
      case "inbox":
        return renderInbox(inboxState, inboxItems);
      case "recent_searches":
        return renderSearches(nav.recentSearches, nav);
      case "summary":
        return null;
      default:
        return (
          <div className="text-xs text-gray-500">
            Unknown section: {section}
          </div>
        );
    }
  })();

  return (
    <section className="dashboard-section">
      <div className="section-header flex items-center gap-2 mb-3">
        <Icon name="info" className="w-4 h-4 text-gray-400" />
        <h2 className="text-sm font-semibold text-gray-200">{label}</h2>
      </div>
      {widget}
    </section>
  );
}

// ── Dashboard main component ────────────────────────────────────────────────

/** The Dashboard view — configurable home with summary cards + widgets. */
export function Dashboard(): ReactNode {
  const nav = useNav();
  const ws = useWorkspace();

  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [statsState, setStatsState] = useState<LoadState>("loading");
  const [inboxItems, setInboxItems] = useState<InboxRow[]>([]);
  const [inboxState, setInboxState] = useState<LoadState>("loading");

  // Initial load (runs once on mount)
  useEffect(() => {
    let cancelled = false;
    loadDashboardData().then((data) => {
      if (cancelled) return;
      setStats(data.stats);
      setStatsState(data.statsState);
      setInboxItems(data.inboxItems);
      setInboxState(data.inboxState);
    });
    return () => { cancelled = true; };
  }, []);

  // Re-load on ItemStored events (simulated — would hook into event bus)
  // For React, we rely on the notes index being updated via NavContext.
  // The Dioxus spec re-loads on ItemStored; in React we'd need a global
  // event listener, but that's outside this view's scope.

  // Pre-compute snapshot values for rendering
  const vaultName = nav.vaultName || "Vault";
  const sections = nav.dashboardSections;
  const index = nav.notesIndex;

  return (
    <div className="dashboard h-screen overflow-y-auto bg-gray-950 text-gray-100">
      <div className="dashboard-header border-b border-gray-700 px-6 py-4">
        <h1 className="text-2xl font-bold text-gray-100">Dashboard</h1>
        <p className="text-sm text-gray-400 mt-1">Vault • {vaultName}</p>
      </div>

      <div className="dashboard-content p-6 space-y-6">
        {renderSummary(statsState, stats)}

        {sections.map((section) =>
          renderSection(
            section,
            nav,
            ws,
            index,
            statsState,
            stats,
            inboxState,
            inboxItems,
          ),
        )}
      </div>
    </div>
  );
}
