// ──────────────────────────────────────────────────────────────────────────────
// collections/shared/types.ts — shared types for the Collections module
//
// Mirrors: ui-react/src/components/collections/shared/types.rs
//
// `CollectionItem` mirrors the backend `NoteIndexEntry` (defined in
// `src-tauri/src/commands.rs`) — it is a flat, deserialisable projection
// that the four collection views share. Views never own data; they are
// pure projections of `Vec<CollectionItem>`.
// ──────────────────────────────────────────────────────────────────────────────

/**
 * One note in the vault index — mirrors the backend `NoteIndexEntry`.
 *
 * The backend `notes_index` command returns this shape; the UI deserialises
 * directly into this interface and projects it into whichever view
 * (Table, Board, Gallery, Calendar) is active.
 */
export interface CollectionItem {
  /** Vault-relative path (forward slashes). Used as the stable key/id. */
  path: string;
  /** Display title (file name without `.md`). */
  title: string;
  /** Parent folder ("" for the vault root). */
  folder: string;
  /** Last modification time as an RFC 3339 string. */
  modified_at: string;
  /** Whether the note is pinned. */
  pinned: boolean;
}

/** The four supported collection views. */
export type CollectionView = "table" | "board" | "gallery" | "calendar";

/** Display label for each view, used by the view switcher. */
export const CollectionViewLabels: Record<CollectionView, string> = {
  table: "Table",
  board: "Board",
  gallery: "Gallery",
  calendar: "Calendar",
};

/** All variants in display order. */
export const CollectionViewAll: CollectionView[] = [
  "table",
  "board",
  "gallery",
  "calendar",
];

/** Filter state for the Table view. */
export interface TableFilter {
  query: string;
  object_type: string | null;
  sort_by: string;
  sort_ascending: boolean;
}

/** Default TableFilter (matches Rust `TableFilter::default()`). */
export function defaultTableFilter(): TableFilter {
  return {
    query: "",
    object_type: null,
    sort_by: "",
    sort_ascending: false,
  };
}

/** Filter state for the Board view. */
export interface BoardFilter {
  query: string;
  object_type: string | null;
  group_by: string;
}

/** Default BoardFilter (matches Rust `BoardFilter::default()`). */
export function defaultBoardFilter(): BoardFilter {
  return {
    query: "",
    object_type: null,
    group_by: "",
  };
}

/** Filter state for the Gallery view. */
export interface GalleryFilter {
  query: string;
  object_type: string | null;
  sort_by: string;
  sort_ascending: boolean;
}

/** Default GalleryFilter (matches Rust `GalleryFilter::default()`). */
export function defaultGalleryFilter(): GalleryFilter {
  return {
    query: "",
    object_type: null,
    sort_by: "",
    sort_ascending: false,
  };
}

/** Which calendar sub-view is active. */
export type CalendarViewMode = "month" | "week" | "day";

/** Default CalendarViewMode (matches Rust `CalendarViewMode::default()`). */
export function defaultCalendarViewMode(): CalendarViewMode {
  return "month";
}

/** Filter state for the Calendar view. */
export interface CalendarFilter {
  query: string;
  object_type: string | null;
  view_mode: CalendarViewMode;
  group_by: string;
}

/** Default CalendarFilter (matches Rust `CalendarFilter::default()`). */
export function defaultCalendarFilter(): CalendarFilter {
  return {
    query: "",
    object_type: null,
    view_mode: "month",
    group_by: "",
  };
}

/** Lightweight search + grouping state projected from a single source. */
export interface SearchState {
  query: string;
}

/** Default SearchState (matches Rust `SearchState::default()`). */
export function defaultSearchState(): SearchState {
  return { query: "" };
}
