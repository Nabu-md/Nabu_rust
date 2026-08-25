// ──────────────────────────────────────────────────────────────────────────────
// InboxQueue.tsx — left panel: the sortable capture queue list
//
// Mirrors the queue list portion of `Inbox` in inbox.rs.  Adds dnd-kit sortable
// reordering (PointerSensor) with a grip handle so click-to-select and
// double-click-to-preview are not disturbed by drag gestures.
//
// The search bar + batch action bar are rendered by the parent `Inbox` so they
// stay pinned above the split pane (faithful to the left-panel header in the
// spec, lifted out for a stable top toolbar).
// ──────────────────────────────────────────────────────────────────────────────

import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
  type UniqueIdentifier,
} from "@dnd-kit/core";
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { useState, type CSSProperties } from "react";
import type { InboxItem } from "../../types";
import { InboxIcon, FileThumbnailIcon } from "./icons";
import {
  confidenceBarClass,
  confidenceColor,
  confidencePct,
  isTerminalStatus,
  statusColor,
} from "./utils";

export interface InboxQueueProps {
  /** Already filtered + sorted items for display. */
  items: InboxItem[];
  /** Currently selected item ids. */
  selectedIds: Set<string>;
  /** Toggle selection of a single item (click). */
  onSelect: (id: string) => void;
  /** Open an item in the preview pane (double-click). */
  onPreview: (id: string) => void;
  /** Reorder handler called with (activeId, overId). */
  onReorder: (activeId: string, overId: string) => void;
}

/** A single draggable queue row. */
function InboxQueueItem({
  item,
  selected,
  onSelect,
  onPreview,
}: {
  item: InboxItem;
  selected: boolean;
  onSelect: (id: string) => void;
  onPreview: (id: string) => void;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
    isSorting,
  } = useSortable({ id: item.id });

  const style: CSSProperties = {
    transform: transform
      ? `translate3d(${transform.x}px, ${transform.y}px, 0)`
      : undefined,
    transition,
    opacity: isDragging ? 0.4 : 1,
    zIndex: isSorting ? 10 : undefined,
  };

  const hasWarnings = item.warnings.length > 0;
  const hasDup = item.duplicate_info !== null;
  const hasOcrWarning = item.ocr_info?.warning !== null;
  const isTerminal = isTerminalStatus(item.status);

  let borderClass = "border-l-transparent";
  if (!selected) {
    if (hasWarnings && hasDup) {
      borderClass = "border-l-yellow-500";
    } else if (hasWarnings && hasOcrWarning) {
      borderClass = "border-l-orange-500";
    }
  }
  const itemClass = selected ? "bg-gray-800 border-l-blue-500" : borderClass;
  const terminalClass = isTerminal ? "opacity-50" : "";

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...attributes}
      onClick={() => onSelect(item.id)}
      onDoubleClick={() => onPreview(item.id)}
      className={`inbox-item flex items-start gap-2 px-3 py-2 cursor-pointer hover:bg-gray-800 border-l-2 transition-colors ${itemClass} ${terminalClass}`}
    >
      {/* Drag handle (only this element initiates a reorder) */}
      <div
        ref={setActivatorNodeRef}
        {...listeners}
        className="mt-0.5 flex-shrink-0 cursor-grab rounded p-0.5 text-gray-500 hover:text-gray-400 active:cursor-grabbing"
        aria-hidden="true"
        onClick={(e) => e.stopPropagation()}
        onDoubleClick={(e) => e.stopPropagation()}
      >
        <InboxIcon name="grip-vertical" className="w-4 h-4" />
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <FileThumbnailIcon
            thumbnail={item.thumbnail}
            className="flex-shrink-0 w-5 h-5 text-gray-400"
          />
          <span className="text-sm font-medium truncate max-w-36" title={item.title}>
            {item.title}
          </span>
          {hasWarnings && (
            <span
              className="text-xs text-yellow-400"
              title={item.warnings.join("; ")}
              aria-label={`${item.warnings.length} warning(s)`}
            >
              <InboxIcon name="alert-triangle" className="w-3 h-3 inline" />
              {item.warnings.length}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2 mt-1 ml-7">
          <span className="text-xs text-gray-500 truncate max-w-32">{item.source}</span>
          <span className="text-xs text-gray-600">•</span>
          <span className="text-xs text-gray-500">{item.object_type}</span>
        </div>

        {item.suggested_folder && (
          <div className="flex items-center gap-1 mt-1 ml-7">
            <InboxIcon name="map-pin" className="w-3 h-3 text-blue-400" />
            <span className="text-xs text-blue-400 truncate max-w-32">
              {item.suggested_folder}
            </span>
            <span className="text-xs text-gray-500">(suggested)</span>
          </div>
        )}

        {item.confidence !== null && (
          <div className="ml-7 mt-1 w-full max-w-[120px]">
            <div className="w-full bg-gray-700 rounded h-1.5 overflow-hidden">
              <div
                className={`h-1.5 rounded transition-all ${confidenceBarClass(item.confidence)}`}
                style={{ width: `${confidencePct(item.confidence)}%` }}
              />
            </div>
            <span className={`text-xs ${confidenceColor(item.confidence)}`}>
              {confidencePct(item.confidence)}% confidence
            </span>
          </div>
        )}

        <div className="ml-7 mt-1">
          <span
            className={`text-xs ${statusColor(item.status)}`}
            title={`Status: ${item.status}`}
          >
            [{item.status}]
          </span>
        </div>
      </div>
    </div>
  );
}

/** Left panel: the empty state or the sortable queue list with a drag overlay. */
export function InboxQueue({
  items,
  selectedIds,
  onSelect,
  onPreview,
  onReorder,
}: InboxQueueProps) {
  const [activeId, setActiveId] = useState<UniqueIdentifier | null>(null);
  const sensors = useSensors(useSensor(PointerSensor));

  const handleDragStart = (event: DragStartEvent) => {
    setActiveId(event.active.id);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    setActiveId(null);
    if (over && active.id !== over.id) {
      onReorder(String(active.id), String(over.id));
    }
  };

  const activeItem = activeId
    ? items.find((i) => i.id === String(activeId)) ?? null
    : null;

  return (
    <div className="flex-none w-96 border-r border-gray-800 flex flex-col overflow-hidden">
      {items.length === 0 ? (
        <div className="flex-1 flex items-center justify-center p-6">
          <div className="text-center text-gray-500">
            <InboxIcon name="inbox" className="w-8 h-8 mx-auto mb-2" />
            <p className="text-sm">Inbox is empty.</p>
            <p className="text-xs mt-1">
              Captured knowledge appears here, ready to review and file into
              your vault.
            </p>
          </div>
        </div>
      ) : (
        <DndContext
          sensors={sensors}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
        >
          <SortableContext
            items={items.map((i) => i.id)}
            strategy={verticalListSortingStrategy}
          >
            <div className="flex-1 overflow-y-auto divide-y divide-gray-800">
              {items.map((item) => (
                <InboxQueueItem
                  key={item.id}
                  item={item}
                  selected={selectedIds.has(item.id)}
                  onSelect={onSelect}
                  onPreview={onPreview}
                />
              ))}
            </div>
          </SortableContext>
          <DragOverlay>
            {activeItem ? (
              <div className="flex items-start gap-2 px-3 py-2 bg-gray-800 rounded-lg shadow-xl border border-blue-500/50">
                <FileThumbnailIcon
                  thumbnail={activeItem.thumbnail}
                  className="flex-shrink-0 w-5 h-5 text-gray-400"
                />
                <span className="text-sm font-medium text-gray-200 truncate">
                  {activeItem.title}
                </span>
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      )}
    </div>
  );
}
