// ──────────────────────────────────────────────────────────────────────────────
// inbox/index.ts — barrel export for the Inbox view
//
// Public surface for the Inbox port: the main split-pane component plus the
// quick-capture entry point.  All sub-components live alongside it in inbox/.
// ──────────────────────────────────────────────────────────────────────────────

export { Inbox } from "./Inbox";
export { QuickCapture } from "./QuickCapture";
export { InboxQueue } from "./InboxQueue";
export { InboxPreview } from "./InboxPreview";
export { InboxDetails } from "./InboxDetails";
export { InboxDuplicateReview } from "./InboxDuplicateReview";
export { InboxTimelineReview } from "./InboxTimelineReview";
export { InboxOcrReview } from "./InboxOcrReview";
export { InboxHistory } from "./InboxHistory";
export { InboxMetadataSidebar } from "./InboxMetadataSidebar";

export { InboxIcon, FileThumbnailIcon, thumbnailToName } from "./icons";
export type { InboxIconProps, FileThumbnailIconProps } from "./icons";

export type { SortField, InboxPreviewTab } from "./types";
export {
  filterAndSort,
  confidencePct,
  confidenceColor,
  confidenceBarClass,
  isTerminalStatus,
  statusColor,
  statusLabel,
  SORT_FIELD_LABELS,
  isTextInputTarget,
} from "./utils";
