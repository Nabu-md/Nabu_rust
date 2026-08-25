// ──────────────────────────────────────────────────────────────────────────────
// InboxPreview.tsx — right panel: tabbed preview + per-item action buttons
//
// Mirrors: crates/nabu-ui/src/components/inbox.rs (`InboxPreview` component)
//
// Tabs: Details | Duplicate | Timeline | OCR | History.
// Action buttons: Approve / Reject / Retry / Delete (wired by the parent).
// ──────────────────────────────────────────────────────────────────────────────

import { useState, type ReactNode } from "react";
import type { InboxItem } from "../../types";
import { InboxIcon } from "./icons";
import { InboxDetails } from "./InboxDetails";
import { InboxDuplicateReview } from "./InboxDuplicateReview";
import { InboxTimelineReview } from "./InboxTimelineReview";
import { InboxOcrReview } from "./InboxOcrReview";
import { InboxHistory } from "./InboxHistory";
import type { InboxPreviewTab } from "./types";

export interface InboxPreviewProps {
  item: InboxItem;
  onApprove: () => void;
  onReject: () => void;
  onRetry: () => void;
  onDelete: () => void;
}

interface TabDef {
  label: string;
  value: InboxPreviewTab;
}

const TABS: TabDef[] = [
  { label: "Details", value: "details" },
  { label: "Duplicate", value: "duplicate" },
  { label: "Timeline", value: "timeline" },
  { label: "OCR", value: "ocr" },
  { label: "History", value: "history" },
];

/** Right-panel preview: tabs + tab content + action buttons. */
export function InboxPreview({
  item,
  onApprove,
  onReject,
  onRetry,
  onDelete,
}: InboxPreviewProps) {
  const [activeTab, setActiveTab] = useState<InboxPreviewTab>("details");

  let content: ReactNode;
  switch (activeTab) {
    case "duplicate":
      content = <InboxDuplicateReview item={item} />;
      break;
    case "timeline":
      content = <InboxTimelineReview item={item} />;
      break;
    case "ocr":
      content = <InboxOcrReview item={item} />;
      break;
    case "history":
      content = <InboxHistory item={item} />;
      break;
    case "details":
    default:
      content = <InboxDetails item={item} />;
  }

  return (
    <div className="inbox-preview flex flex-col flex-1 overflow-hidden p-4">
      {/* Tab header */}
      <div className="flex items-center gap-1 mb-4 border-b border-gray-800 pb-2 overflow-x-auto">
        {TABS.map((tab) => {
          const isActive = activeTab === tab.value;
          const cls = isActive
            ? "bg-blue-600 text-white"
            : "text-gray-400 hover:text-gray-200";
          return (
            <button
              key={tab.value}
              type="button"
              onClick={() => setActiveTab(tab.value)}
              className={`px-3 py-1 text-sm rounded ${cls}`}
            >
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto min-h-0">{content}</div>

      {/* Action buttons */}
      <div className="flex items-center gap-2 mt-4 pt-4 border-t border-gray-800 flex-none">
        <button
          type="button"
          onClick={onApprove}
          className="px-3 py-1.5 text-sm text-white bg-green-700 rounded hover:bg-green-600 flex items-center gap-1"
        >
          <InboxIcon name="check" className="w-4 h-4" />
          Approve
        </button>
        <button
          type="button"
          onClick={onReject}
          className="px-3 py-1.5 text-sm text-white bg-red-700 rounded hover:bg-red-600 flex items-center gap-1"
        >
          <InboxIcon name="x" className="w-4 h-4" />
          Reject
        </button>
        <button
          type="button"
          onClick={onRetry}
          className="px-3 py-1.5 text-sm text-white bg-yellow-700 rounded hover:bg-yellow-600 flex items-center gap-1"
        >
          <InboxIcon name="refresh-cw" className="w-4 h-4" />
          Retry
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="px-3 py-1.5 text-sm text-white bg-gray-700 rounded hover:bg-gray-600 flex items-center gap-1"
        >
          <InboxIcon name="trash-2" className="w-4 h-4" />
          Delete
        </button>
      </div>
    </div>
  );
}
