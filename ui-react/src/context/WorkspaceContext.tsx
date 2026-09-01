// ──────────────────────────────────────────────────────────────────────────────
// WorkspaceContext — open tabs + active note path
//
// Mirrors: ui-react/src/components/contexts::WorkspaceContext
// Contract: read-only spec consumed by Wave 3 view components.
// ──────────────────────────────────────────────────────────────────────────────

import {
  createContext,
  useContext,
  useState,
  useCallback,
  type ReactNode,
} from "react";

/** One open tab. */
export interface OpenTab {
  /** Vault-relative path of the note. */
  path: string;
  /** Display title (derived from the file name). */
  title: string;
  /** Pinned tabs survive "Close Others" / "Close All". */
  pinned: boolean;
}

/** Derives a tab title from a vault-relative path. */
export function titleFromPath(path: string): string {
  return path
    .split("/")
    .pop()
    ?.replace(/\.md$/, "") ?? path;
}

export interface WorkspaceContextValue {
  /** Open tabs in display order. */
  tabs: OpenTab[];
  /** The active note (vault-relative path), or null. */
  activePath: string | null;
  /** Counter bumped by file tree after structural mutations. */
  refreshTree: number;
  /** Tracks external content changes (path, version). */
  contentVersion: [string, number];

  // ── Mutations ──────────────────────────────────────────────────────────

  /** Open a note in a tab: adds if missing and makes active. */
  openTab: (path: string) => void;
  /** Make an already-open tab active. */
  activateTab: (path: string) => void;
  /** Close a tab. When active tab closes, activates neighbouring tab. */
  closeTab: (path: string) => void;
  /** Close every tab except `keep` (and pinned tabs). */
  closeOthers: (keep: string) => void;
  /** Close all unpinned tabs. */
  closeAll: () => void;
  /** Toggle the pinned flag on a tab. */
  pinTab: (path: string) => void;
  /** Move a tab from one index to another (drag-reorder). */
  reorderTab: (from: number, to: number) => void;
  /** Rewrite tabs whose path lives under old_prefix to new_prefix. */
  renameTabPrefix: (oldPrefix: string, newPrefix: string) => void;
  /** Bump the refresh tree counter (after create / rename / delete). */
  refreshTreeBump: () => void;
  /** Mark a note's content as changed outside the editor. */
  bumpContentVersion: (path: string) => void;
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null);

/** Hook to access the workspace context. */
export function useWorkspace(): WorkspaceContextValue {
  const ctx = useContext(WorkspaceContext);
  if (!ctx) {
    throw new Error("useWorkspace must be used within a WorkspaceProvider");
  }
  return ctx;
}

interface WorkspaceProviderProps {
  children: ReactNode;
}

/** Provider component for workspace tab management. */
export function WorkspaceProvider({ children }: WorkspaceProviderProps) {
  const [tabs, setTabs] = useState<OpenTab[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [refreshTree, setRefreshTree] = useState(0);
  const [contentVersion, setContentVersion] = useState<[string, number]>([
    "",
    0,
  ]);

  const openTab = useCallback((path: string) => {
    const title = titleFromPath(path);
    setTabs((prev) => {
      if (prev.some((t) => t.path === path)) return prev;
      return [...prev, { path, title, pinned: false }];
    });
    setActivePath(path);
  }, []);

  const activateTab = useCallback((path: string) => {
    setActivePath(path);
  }, []);

  const closeTab = useCallback(
    (path: string) => {
      const wasActive = activePath === path;
      let closedIndex = -1;
      setTabs((prev) => {
        closedIndex = prev.findIndex((t) => t.path === path);
        return prev.filter((t) => t.path !== path);
      });
      if (wasActive) {
        // We need to read the updated tabs. Use a functional updater approach.
        setTabs((remaining) => {
          const next =
            closedIndex >= 0
              ? remaining[closedIndex] ??
                remaining[Math.max(0, closedIndex - 1)]
              : remaining[0];
          setActivePath(next?.path ?? null);
          return remaining;
        });
      }
    },
    [activePath]
  );

  const closeOthers = useCallback((keep: string) => {
    setTabs((prev) => prev.filter((t) => t.path === keep || t.pinned));
    setActivePath(keep);
  }, []);

  const closeAll = useCallback(() => {
    setTabs((prev) => prev.filter((t) => t.pinned));
    setActivePath(null);
  }, []);

  const pinTab = useCallback((path: string) => {
    setTabs((prev) =>
      prev.map((t) => (t.path === path ? { ...t, pinned: !t.pinned } : t))
    );
  }, []);

  const reorderTab = useCallback((from: number, to: number) => {
    if (from === to) return;
    setTabs((prev) => {
      if (from >= prev.length || to >= prev.length) return prev;
      const next = [...prev];
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      return next;
    });
  }, []);

  const renameTabPrefix = useCallback(
    (oldPrefix: string, newPrefix: string) => {
      if (!oldPrefix) return;
      const old = `${oldPrefix}/`;
      const newPfx = newPrefix ? `${newPrefix}/` : "";
      setTabs((prev) =>
        prev.map((t) => {
          if (t.path === oldPrefix) {
            return { ...t, path: newPrefix, title: titleFromPath(newPrefix) };
          }
          if (t.path.startsWith(old)) {
            const rest = t.path.slice(old.length);
            const newPath = `${newPfx}${rest}`;
            return { ...t, path: newPath, title: titleFromPath(newPath) };
          }
          return t;
        })
      );
      setActivePath((prev) => {
        if (prev === oldPrefix) return newPrefix;
        if (prev?.startsWith(old)) {
          const rest = prev.slice(old.length);
          return `${newPfx}${rest}`;
        }
        return prev;
      });
    },
    []
  );

  const refreshTreeBump = useCallback(() => {
    setRefreshTree((v) => v + 1);
  }, []);

  const bumpContentVersion = useCallback((path: string) => {
    setContentVersion(([_, v]) => [path, v + 1]);
  }, []);

  const value: WorkspaceContextValue = {
    tabs,
    activePath,
    refreshTree,
    contentVersion,
    openTab,
    activateTab,
    closeTab,
    closeOthers,
    closeAll,
    pinTab,
    reorderTab,
    renameTabPrefix,
    refreshTreeBump,
    bumpContentVersion,
  };

  return (
    <WorkspaceContext.Provider value={value}>
      {children}
    </WorkspaceContext.Provider>
  );
}
