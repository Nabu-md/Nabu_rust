// ──────────────────────────────────────────────────────────────────────────────
// collections/index — barrel export for the Collections module
//
// Mirrors: crates/nabu-ui/src/components/collections/mod.rs
// ──────────────────────────────────────────────────────────────────────────────

export { CollectionContainer } from "./container";
export { ViewSwitcher } from "./view_switcher";
export type { ViewSwitcherProps } from "./view_switcher";

export { TableView } from "./table_view";
export type { ColumnConfig, TableViewProps } from "./table_view";

export { BoardView } from "./board_view";
export type { BoardColumn, BoardViewProps } from "./board_view";

export { GalleryView } from "./gallery_view";
export type { GalleryViewProps } from "./gallery_view";

export { CalendarView } from "./calendar_view";
export type { CalendarViewProps } from "./calendar_view";

export {
  CollectionViewLabels,
  CollectionViewAll,
  defaultTableFilter,
  defaultBoardFilter,
  defaultGalleryFilter,
  defaultCalendarFilter,
  defaultSearchState,
} from "./shared/types";

export type {
  CollectionItem,
  CollectionView,
  TableFilter,
  BoardFilter,
  GalleryFilter,
  CalendarFilter,
  CalendarViewMode,
  SearchState,
} from "./shared/types";
