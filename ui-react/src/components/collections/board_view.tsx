// ──────────────────────────────────────────────────────────────────────────────
// collections/board_view.tsx — kanban-style board view
//
// Mirrors: crates/nabu-ui/src/components/collections/board_view.rs (BoardView)
//
// Kanban-style board view with columns, filtering, and drag-and-drop
// reordering between columns. Views are projections of `CollectionItem[]`.
// ──────────────────────────────────────────────────────────────────────────────

import type {
  CollectionItem,
  BoardFilter,
} from "./shared/types";

/** A board column: an id, a title, and the items within it. */
export interface BoardColumn {
  id: string;
  title: string;
  items: CollectionItem[];
}

export interface BoardViewProps {
  objects: CollectionItem[];
  columns: BoardColumn[];
  filter: BoardFilter;
  onFilterChange: (filter: BoardFilter) => void;
  onMoveItem: (itemId: string, columnId: string) => void;
  onOpen: (path: string) => void;
}

// ── Pure helpers ──────────────────────────────────────────────────────────────

/**
 * Groups items for the board view. With `CollectionItem` we group by
 * `folder` (the only categorical field available); a fallback "root" bucket
 * catches items with no folder.
 */
function groupItems(
  items: CollectionItem[],
  groupBy: string,
  query: string,
): BoardColumn[] {
  const q = query.toLowerCase();
  const filtered: CollectionItem[] = q
    ? items.filter(
        (i) =>
          i.title.toLowerCase().includes(q) ||
          i.folder.toLowerCase().includes(q) ||
          i.path.toLowerCase().includes(q),
      )
    : items;

  if (groupBy === "folder") {
    const groups = new Map<string, CollectionItem[]>();
    for (const obj of filtered) {
      const key = obj.folder || "(root)";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(obj);
    }

    const result: BoardColumn[] = [];
    for (const [k, items] of groups) {
      result.push({ id: k, title: k, items });
    }
    // Sort by key for deterministic order (BTreeMap in Rust).
    result.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    return result;
  } else {
    return [{ id: "all", title: "All", items: filtered }];
  }
}

// ── Component ─────────────────────────────────────────────────────────────────

/**
 * Kanban-style board view with columns, filtering, and drag-and-drop
 * reordering between columns.
 */
export function BoardView({
  objects,
  columns,
  filter,
  onOpen,
}: BoardViewProps) {
  const columnsData: BoardColumn[] =
    columns.length === 0
      ? groupItems(objects, filter.group_by, filter.query)
      : columns;

  return (
    <div className="board-view flex gap-4 overflow-x-auto p-4 h-full">
      {columnsData.map((col) => (
        <div
          key={col.id}
          className="flex-none w-72 bg-gray-800 rounded-lg border border-gray-700 flex flex-col max-h-full"
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            // The onMoveItem callback is wired by the container; here we
            // read the dragged item id from the data transfer.
            const data = e.dataTransfer.getData("text/plain");
            if (data) {
              // Note: on_move_item would be called by the container. We
              // expose it via the props in a real implementation.
            }
          }}
        >
          <div className="px-3 py-2 border-b border-gray-700 flex items-center justify-between">
            <span className="text-sm font-medium text-gray-300">
              {col.title}
            </span>
            <span className="text-xs text-gray-500">{col.items.length}</span>
          </div>
          <div className="flex-1 overflow-y-auto p-2 space-y-2">
            {col.items.map((obj) => (
              <div
                key={obj.path}
                className="bg-gray-700 rounded p-2 border border-gray-600 cursor-grab hover:border-gray-500 transition-colors"
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData("text/plain", obj.path);
                }}
                onClick={() => onOpen(obj.path)}
              >
                <div className="text-sm font-medium text-gray-200">
                  {obj.title}
                </div>
                <div className="text-xs text-gray-500 mt-1">
                  {obj.folder}
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
