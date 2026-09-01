// ──────────────────────────────────────────────────────────────────────────────
// InboxTimelineReview.tsx — "Timeline" tab
//
// Mirrors: ui-react/src/components/inbox.rs (`InboxTimelineReview`)
// ──────────────────────────────────────────────────────────────────────────────

import type { InboxItem } from "../../types";

/** Field label + value pair. */
function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="text-xs text-gray-500 uppercase tracking-wide">
        {label}
      </label>
      <p className="text-sm text-gray-300 break-all">{children}</p>
    </div>
  );
}

/** The Timeline tab. */
export function InboxTimelineReview({ item }: { item: InboxItem }) {
  const tl = item.timeline_info;

  if (!tl) {
    return (
      <div className="text-gray-500 text-sm">
        No timeline information extracted for this item.
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-3 text-sm">
        <Field label="Document Date">{tl.document_date ?? ""}</Field>
        <Field label="Created Date">{tl.created_date ?? ""}</Field>
        <Field label="Modified Date">{tl.modified_date ?? ""}</Field>
        <Field label="Detected Event Date">{tl.detected_event_date ?? ""}</Field>
        <Field label="Extraction Confidence">
          {tl.extraction_confidence ?? ""}
        </Field>
      </div>
    </div>
  );
}
