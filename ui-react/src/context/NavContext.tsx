// ──────────────────────────────────────────────────────────────────────────────
// NavContext — navigation state (view mode, sidebars, overlays, discovery data)
//
// Mirrors: crates/nabu-ui/src/components/navigation::state::NavContext
// Contract: read-only spec consumed by Wave 3 view components.
// ──────────────────────────────────────────────────────────────────────────────

import { createContext, useContext, useState, type ReactNode } from "react";
import type { ViewMode, SavedSearch, SmartFolder, NoteIndexEntry } from "../types";

/** Shape of the navigation context (React equivalent of Dioxus NavContext). */
export interface NavContextValue {
  // Core view state
  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;

  // Overlay visibility
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  switcherOpen: boolean;
  setSwitcherOpen: (open: boolean) => void;
  shortcutsOpen: boolean;
  setShortcutsOpen: (open: boolean) => void;

  // Search
  searchQuery: string;
  setSearchQuery: (query: string) => void;

  // Sidebar / Inspector visibility
  showLeftSidebar: boolean;
  setShowLeftSidebar: (show: boolean) => void;
  showRightInspector: boolean;
  setShowRightInspector: (show: boolean) => void;

  // Discovery data (persisted in settings)
  recentNotes: string[];
  setRecentNotes: (notes: string[]) => void;
  favourites: string[];
  setFavourites: (favs: string[]) => void;
  recentSearches: string[];
  setRecentSearches: (searches: string[]) => void;
  savedSearches: SavedSearch[];
  setSavedSearches: (searches: SavedSearch[]) => void;
  smartFolders: SmartFolder[];
  setSmartFolders: (folders: SmartFolder[]) => void;
  recentCommands: string[];
  setRecentCommands: (cmds: string[]) => void;
  favouriteCommands: string[];
  setFavouriteCommands: (cmds: string[]) => void;

  // Vault note index
  notesIndex: NoteIndexEntry[];
  setNotesIndex: (index: NoteIndexEntry[]) => void;

  // Dashboard
  dashboardSections: string[];
  setDashboardSections: (sections: string[]) => void;

  // Vault identity
  vaultName: string;
  setVaultName: (name: string) => void;
}

const NavContext = createContext<NavContextValue | null>(null);

/** Hook to access the navigation context. */
export function useNav(): NavContextValue {
  const ctx = useContext(NavContext);
  if (!ctx) {
    throw new Error("useNav must be used within a NavProvider");
  }
  return ctx;
}

const DEFAULT_DASHBOARD_SECTIONS = [
  "quick_actions",
  "recently_modified",
  "favourites",
  "recently_opened",
  "pinned",
  "inbox",
  "recent_searches",
  "summary",
];

interface NavProviderProps {
  children: ReactNode;
}

/** Provider component for navigation state. */
export function NavProvider({ children }: NavProviderProps) {
  const [viewMode, setViewMode] = useState<ViewMode>("Dashboard");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [switcherOpen, setSwitcherOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [showLeftSidebar, setShowLeftSidebar] = useState(true);
  const [showRightInspector, setShowRightInspector] = useState(true);
  const [recentNotes, setRecentNotes] = useState<string[]>([]);
  const [favourites, setFavourites] = useState<string[]>([]);
  const [recentSearches, setRecentSearches] = useState<string[]>([]);
  const [savedSearches, setSavedSearches] = useState<SavedSearch[]>([]);
  const [smartFolders, setSmartFolders] = useState<SmartFolder[]>([]);
  const [recentCommands, setRecentCommands] = useState<string[]>([]);
  const [favouriteCommands, setFavouriteCommands] = useState<string[]>([]);
  const [notesIndex, setNotesIndex] = useState<NoteIndexEntry[]>([]);
  const [dashboardSections, setDashboardSections] = useState<string[]>(
    DEFAULT_DASHBOARD_SECTIONS
  );
  const [vaultName, setVaultName] = useState("");

  const value: NavContextValue = {
    viewMode,
    setViewMode,
    paletteOpen,
    setPaletteOpen,
    switcherOpen,
    setSwitcherOpen,
    shortcutsOpen,
    setShortcutsOpen,
    searchQuery,
    setSearchQuery,
    showLeftSidebar,
    setShowLeftSidebar,
    showRightInspector,
    setShowRightInspector,
    recentNotes,
    setRecentNotes,
    favourites,
    setFavourites,
    recentSearches,
    setRecentSearches,
    savedSearches,
    setSavedSearches,
    smartFolders,
    setSmartFolders,
    recentCommands,
    setRecentCommands,
    favouriteCommands,
    setFavouriteCommands,
    notesIndex,
    setNotesIndex,
    dashboardSections,
    setDashboardSections,
    vaultName,
    setVaultName,
  };

  return <NavContext.Provider value={value}>{children}</NavContext.Provider>;
}
