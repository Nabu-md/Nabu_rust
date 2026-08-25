// ──────────────────────────────────────────────────────────────────────────────
// collections/calendar_view.tsx — date-based calendar view
//
// Mirrors: crates/nabu-ui/src/components/collections/calendar_view.rs (CalendarView)
//
// Date-based calendar view with month/week/day modes and filtering.
// Views are projections of `CollectionItem[]` — views never own data.
// ──────────────────────────────────────────────────────────────────────────────

import type {
  CollectionItem,
  CalendarFilter,
} from "./shared/types";

export interface CalendarViewProps {
  objects: CollectionItem[];
  filter: CalendarFilter;
  onFilterChange: (filter: CalendarFilter) => void;
  onOpen: (path: string) => void;
}

// ── Pure helpers ──────────────────────────────────────────────────────────────

/** Extracts a date key (YYYY-MM-DD) from an RFC 3339 timestamp string. */
function dateKey(modified_at: string): string {
  return modified_at.slice(0, 10) || "no-date";
}

/** Groups items by their modification date key. */
function groupByDate(
  items: CollectionItem[],
  query: string,
): Array<[string, CollectionItem[]]> {
  const q = query.toLowerCase();
  const filtered: CollectionItem[] = q
    ? items.filter(
        (i) =>
          i.title.toLowerCase().includes(q) ||
          i.folder.toLowerCase().includes(q) ||
          i.path.toLowerCase().includes(q),
      )
    : items;

  const groups = new Map<string, CollectionItem[]>();
  for (const obj of filtered) {
    const key = dateKey(obj.modified_at);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(obj);
  }

  // Sort keys ascending (BTreeMap in Rust).
  const result: Array<[string, CollectionItem[]]> = [];
  const sortedKeys = Array.from(groups.keys()).sort();
  for (const k of sortedKeys) {
    result.push([k, groups.get(k)!]);
  }
  return result;
}

// ── Component ─────────────────────────────────────────────────────────────────

/**
 * Date-based calendar view with month/week/day modes and filtering.
 */
export function CalendarView({ objects, filter, onFilterChange, onOpen }: CalendarViewProps) {
  const viewMode = filter.view_mode;
  const query = filter.query;

  const dateGroups = groupByDate(objects, query);

  // ── View mode button classes (precompute before JSX) ──
  const monthClass =
    viewMode === "month"
      ? "px-3 py-1 text-xs rounded border bg-blue-700 border-blue-500 text-blue-100"
      : "px-3 py-1 text-xs rounded border border-gray-600 text-gray-400";
  const weekClass =
    viewMode === "week"
      ? "px-3 py-1 text-xs rounded border bg-blue-700 border-blue-500 text-blue-100"
      : "px-3 py-1 text-xs rounded border border-gray-600 text-gray-400";
  const dayClass =
    viewMode === "day"
      ? "px-3 py-1 text-xs rounded border bg-blue-700 border-blue-500 text-blue-100"
      : "px-3 py-1 text-xs rounded border border-gray-600 text-gray-400";

  return (
    <div className="calendar-view p-4 overflow-y-auto h-full">
      {/* Header: title + view mode toggle */}
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-gray-200">Calendar</h2>
        <div className="flex gap-2">
          <button
            type="button"
            className={monthClass}
            onClick={() => onFilterChange({ ...filter, view_mode: "month" })}
          >
            Month
          </button>
          <button
            type="button"
            className={weekClass}
            onClick={() => onFilterChange({ ...filter, view_mode: "week" })}
          >
            Week
          </button>
          <button
            type="button"
            className={dayClass}
            onClick={() => onFilterChange({ ...filter, view_mode: "day" })}
          >
            Day
          </button>
        </div>
      </div>

      {/* Search */}
      <div className="flex gap-3 mb-4">
        <input
          type="text"
          placeholder="Search..."
          className="flex-1 bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
          value={query}
          onChange={(e) => onFilterChange({ ...filter, query: e.target.value })}
        />
      </div>

      {/* Calendar grid header */}
      <div className="grid grid-cols-7 gap-1">
        {["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].map((day) => (
          <div
            key={day}
            className="text-center text-xs text-gray-500 py-2 font-medium"
          >
            {day}
          </div>
        ))}
      </div>

      {/* Date groups */}
      {dateGroups.length === 0 ? (
        <div className="col-span-full flex items-center justify-center h-64 text-gray-500">
          No items to display
        </div>
      ) : (
        <div className="grid grid-cols-7 gap-1">
          {dateGroups.map(([dateKeyStr, dayItems]) => (
            <div
              key={dateKeyStr}
              className="col-span-1 bg-gray-800 rounded border border-gray-700 p-2 min-h-[80px]"
            >
              <div className="text-xs text-gray-400 mb-1">{dateKeyStr}</div>
              <div className="text-xs text-gray-500 mb-1">
                {dayItems.length} items
              </div>
              <div className="mt-1 space-y-1">
                {dayItems.slice(0, 3).map((obj) => (
                  <div
                    key={obj.path}
                    className="text-xs text-blue-400 truncate"
                    title={obj.title}
                    onClick={() => onOpen(obj.path)}
                  >
                    {obj.title}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
