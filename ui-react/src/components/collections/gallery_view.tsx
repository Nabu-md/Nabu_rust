// ──────────────────────────────────────────────────────────────────────────────
// collections/gallery_view.tsx — card-based gallery view
//
// Mirrors: ui-react/src/components/collections/gallery_view.rs (GalleryView)
//
// Card-based gallery view with filtering and sorting.
// Views are projections of `CollectionItem[]` — views never own data.
// ──────────────────────────────────────────────────────────────────────────────

import type {
  CollectionItem,
  GalleryFilter,
} from "./shared/types";

export interface GalleryViewProps {
  objects: CollectionItem[];
  filter: GalleryFilter;
  onFilterChange: (filter: GalleryFilter) => void;
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

// ── Component ─────────────────────────────────────────────────────────────────

/**
 * Card-based gallery view with filtering and sorting.
 */
export function GalleryView({ objects, filter, onOpen }: GalleryViewProps) {
  // ── Pre-compute filtered + sorted items ──
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

  return (
    <div className="gallery-view p-4 overflow-y-auto h-full">
      {filtered.length === 0 ? (
        <div className="flex items-center justify-center h-64 text-gray-500">
          No items to display
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
          {filtered.map((obj) => (
            <div
              key={obj.path}
              className="bg-gray-800 rounded-lg border border-gray-700 p-4 hover:border-gray-600 transition-colors cursor-pointer"
              onClick={() => onOpen(obj.path)}
            >
              <div className="flex items-start justify-between mb-2">
                <span className="text-xs px-2 py-0.5 rounded-full bg-gray-700 text-gray-300">
                  {obj.folder || "root"}
                </span>
                <span className="text-xs text-gray-500">
                  {obj.modified_at}
                </span>
              </div>
              <h3 className="text-sm font-medium text-gray-200 mb-2 line-clamp-2">
                {obj.title}
              </h3>
              <div className="text-xs text-gray-500 truncate">
                {obj.path}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
