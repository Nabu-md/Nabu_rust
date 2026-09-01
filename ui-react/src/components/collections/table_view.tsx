// ──────────────────────────────────────────────────────────────────────────────
// collections/table_view.tsx — column-based table view
//
// Mirrors: ui-react/src/components/collections/table_view.rs (TableView)
//
// Column-based table view with filtering, sorting, and configurable columns.
// Projections operate over `CollectionItem[]` — views never own data.
// ──────────────────────────────────────────────────────────────────────────────

import type {
  CollectionItem,
  TableFilter,
} from "./shared/types";

// ── Types ─────────────────────────────────────────────────────────────────────

/** Column configuration for the table view. */
export interface ColumnConfig {
  key: string;
  label: string;
  visible: boolean;
  sortable: boolean;
  width: string | null;
}

export interface TableViewProps {
  /** The note index entries to display (never owned by the view). */
  objects: CollectionItem[];
  /** Column definitions for the table. */
  columns: ColumnConfig[];
  /** Current filter/sort state. */
  filter: TableFilter;
  /** Called when the filter changes. */
  onFilterChange: (filter: TableFilter) => void;
  /** Called when the user clicks a sortable column header. */
  onSort: (sortBy: string, ascending: boolean) => void;
  /** Called when a row is clicked. */
  onOpen: (path: string) => void;
}

// ── Pure helpers ──────────────────────────────────────────────────────────────

/** Projects a sort value from a CollectionItem by column key. */
function getSortValue(obj: CollectionItem, key: string): string {
  switch (key) {
    case "title":
      return obj.title;
    case "folder":
      return obj.folder;
    case "modified":
      return obj.modified_at;
    case "path":
      return obj.path;
    default:
      return obj.title;
  }
}

/** Projects a display value from a CollectionItem by column key. */
function getColumnValue(obj: CollectionItem, key: string): string {
  return getSortValue(obj, key);
}

// ── Component ─────────────────────────────────────────────────────────────────

/**
 * Column-based table view with filtering, sorting, and configurable columns.
 */
export function TableView({
  objects,
  columns,
  filter,
  onSort,
  onOpen,
}: TableViewProps) {
  // ── Pre-compute filtered + sorted rows ──
  const filtered: CollectionItem[] = (() => {
    let result = objects.slice();

    if (filter.query) {
      const q = filter.query.toLowerCase();
      result = result.filter(
        (obj) =>
          obj.title.toLowerCase().includes(q) ||
          obj.folder.toLowerCase().includes(q) ||
          obj.path.toLowerCase().includes(q),
      );
    }

    if (filter.object_type) {
      const ot = filter.object_type;
      result = result.filter((obj) => obj.folder === ot);
    }

    if (filter.sort_by) {
      const sortKey = filter.sort_by;
      const asc = filter.sort_ascending;
      result.sort((a, b) => {
        const aVal = getSortValue(a, sortKey);
        const bVal = getSortValue(b, sortKey);
        const ord = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
        return asc ? ord : -ord;
      });
    }

    return result;
  })();

  const visibleColumns = columns.filter((c) => c.visible);
  const sortBy = filter.sort_by;
  const sortAscending = filter.sort_ascending;
  const colSpan = visibleColumns.length;

  return (
    <div className="table-view w-full overflow-auto h-full">
      <table className="w-full text-sm text-left text-gray-300">
        <thead className="text-xs text-gray-400 uppercase bg-gray-800 border-b border-gray-700">
          <tr>
            {visibleColumns.map((col) => {
              const sortable = col.sortable;
              const sortKey = col.key;
              const isSorted = sortBy === sortKey;
              const classStr = sortable
                ? "cursor-pointer hover:text-gray-200"
                : "";
              const widthStr = col.width ?? "";
              const fullClass = `px-4 py-3 ${classStr} ${widthStr}`;

              // Sort indicator icon
              let sortIcon: React.ReactNode = null;
              if (sortable && isSorted) {
                sortIcon = (
                  <span className="ml-1 inline-block w-3 h-3">
                    {sortAscending ? "↑" : "↓"}
                  </span>
                );
              }

              return (
                <th
                  key={col.key}
                  className={fullClass}
                  onClick={() => {
                    if (sortable) {
                      onSort(sortKey, !isSorted || !sortAscending);
                    }
                  }}
                >
                  <div className="flex items-center gap-1">
                    {col.label}
                    {sortIcon}
                  </div>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-800">
          {filtered.length === 0 ? (
            <tr>
              <td
                className="px-4 py-8 text-center text-gray-500"
                colSpan={colSpan}
              >
                No items to display
              </td>
            </tr>
          ) : (
            filtered.map((obj) => (
              <tr
                key={obj.path}
                className="hover:bg-gray-800/50 transition-colors"
                onClick={() => onOpen(obj.path)}
              >
                {visibleColumns.map((col) => {
                  const value = getColumnValue(obj, col.key);
                  const cellClass =
                    col.key === "title"
                      ? "px-4 py-2 text-gray-300 font-medium truncate max-w-xs"
                      : "px-4 py-2 text-gray-300";
                  return (
                    <td key={col.key} className={cellClass}>
                      {value}
                    </td>
                  );
                })}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}
