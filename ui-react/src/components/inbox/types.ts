// ──────────────────────────────────────────────────────────────────────────────
// inbox/types.ts — Inbox-local view types (mirroring inbox.rs type enums)
//
// These are the *view* enums only.  The persisted data shapes (InboxItem,
// InboxStatus, etc.) live in src/types.ts and are imported from there so the
// types always match the IPC wire format.
// ──────────────────────────────────────────────────────────────────────────────

/** Sort fields for the inbox queue (mirrors `SortField` in inbox.rs). */
export type SortField = "timestamp" | "title" | "source" | "status" | "object_type";

/** Tabs shown in the inbox preview pane (mirrors `InboxPreviewTab` in inbox.rs). */
export type InboxPreviewTab = "details" | "duplicate" | "timeline" | "ocr" | "history";
