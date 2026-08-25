// ──────────────────────────────────────────────────────────────────────────────
// InboxMetadataSidebar.tsx — editable metadata for the previewed item
//
// Mirrors: crates/nabu-ui/src/components/inbox.rs (`InboxMetadataSidebar`)
//
// Wires `inbox_edit_metadata` + `inbox_move` IPC commands.  Collapsible so it
// only consumes vertical space when in use.
// ──────────────────────────────────────────────────────────────────────────────

import { useState } from "react";
import { useToast } from "../../context";
import { inboxEditMetadata, inboxMove } from "../../ipc";
import type { InboxItem } from "../../types";
import { InboxIcon } from "./icons";

export interface InboxMetadataSidebarProps {
  item: InboxItem;
  /** Called after a successful metadata apply so the queue reflects changes. */
  onRefresh: () => void;
  /** Existing vault folders to seed the destination autocomplete. */
  folderSuggestions: string[];
}

/** Collapsible metadata editor for the currently previewed inbox item. */
export function InboxMetadataSidebar({
  item,
  onRefresh,
  folderSuggestions,
}: InboxMetadataSidebarProps) {
  const { toast } = useToast();

  const [title, setTitle] = useState(item.metadata.title ?? "");
  const [author, setAuthor] = useState(item.metadata.author ?? "");
  const [language, setLanguage] = useState(item.metadata.language ?? "");
  const [tags, setTags] = useState(item.metadata.tags.join(", "));
  const [destination, setDestination] = useState(item.suggested_folder ?? "");
  const [expanded, setExpanded] = useState(false);

  const resetTitle = item.metadata.title ?? "";
  const resetAuthor = item.metadata.author ?? "";
  const resetLanguage = item.metadata.language ?? "";
  const resetTags = item.metadata.tags.join(", ");
  const resetDest = item.suggested_folder ?? "";

  const handleReset = () => {
    setTitle(resetTitle);
    setAuthor(resetAuthor);
    setLanguage(resetLanguage);
    setTags(resetTags);
    setDestination(resetDest);
  };

  const handleApply = async () => {
    const id = item.id;
    const parsedTags = tags
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);

    let ok = true;
    try {
      await inboxEditMetadata(id, {
        title: title || undefined,
        author: author || undefined,
        language: language || undefined,
        tags: parsedTags,
        custom: item.metadata.custom,
      });
    } catch {
      toast("Could not save the updated metadata", { variant: "error" });
      ok = false;
    }

    try {
      await inboxMove(id, destination);
    } catch {
      toast("Could not set the destination folder", { variant: "error" });
      ok = false;
    }

    if (ok) {
      toast("Metadata applied", { variant: "success" });
      void onRefresh();
    }
  };

  const labelCls = "text-xs text-gray-500 uppercase tracking-wide";
  const inputCls =
    "input w-full mt-1 bg-gray-800 text-gray-100 rounded px-2 py-1 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none";

  return (
    <div className="border-t border-gray-800 flex-none">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        className="flex items-center gap-1 w-full px-3 py-2 text-left text-xs text-gray-400 hover:text-gray-200 hover:bg-gray-800"
      >
        <InboxIcon name={expanded ? "chevron-down" : "chevron-right"} className="w-3 h-3" />
        Metadata editor
      </button>

      {expanded && (
        <div className="px-3 pb-3 space-y-3">
          <div>
            <label className={labelCls}>Title</label>
            <input
              type="text"
              className={inputCls}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>
          <div>
            <label className={labelCls}>Author</label>
            <input
              type="text"
              className={inputCls}
              value={author}
              onChange={(e) => setAuthor(e.target.value)}
            />
          </div>
          <div>
            <label className={labelCls}>Language</label>
            <input
              type="text"
              className={inputCls}
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
            />
          </div>
          <div>
            <label className={labelCls}>Tags</label>
            <input
              type="text"
              className={inputCls}
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              placeholder="comma, separated, tags"
            />
          </div>
          <div>
            <label className={labelCls}>Destination</label>
            <input
              type="text"
              className={inputCls}
              value={destination}
              onChange={(e) => setDestination(e.target.value)}
              placeholder={item.suggested_folder ?? "where to file this note"}
              list="inbox-dest-suggestions"
            />
            <datalist id="inbox-dest-suggestions">
              {folderSuggestions.map((f) => (
                <option key={f} value={f} />
              ))}
            </datalist>
          </div>

          {item.suggested_folder && (
            <div className="flex items-center gap-1 text-xs text-gray-500">
              <InboxIcon name="map-pin" className="w-3 h-3 text-blue-400" />
              <span>{item.suggested_folder}</span>
              <span>(suggested)</span>
            </div>
          )}

          <div className="flex gap-2 pt-2">
            <button
              type="button"
              onClick={handleApply}
              className="px-3 py-1.5 text-sm text-white bg-blue-700 rounded hover:bg-blue-600"
            >
              Apply Metadata
            </button>
            <button
              type="button"
              onClick={handleReset}
              className="px-3 py-1.5 text-sm text-gray-300 bg-gray-800 rounded hover:bg-gray-700"
            >
              Reset
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
