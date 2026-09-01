// ──────────────────────────────────────────────────────────────────────────────
// InboxDuplicateReview.tsx — "Duplicate" tab
//
// Mirrors: ui-react/src/components/inbox.rs (`InboxDuplicateReview`)
// ──────────────────────────────────────────────────────────────────────────────

import type { InboxItem } from "../../types";
import { InboxIcon } from "./icons";

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

/** The Duplicate tab. */
export function InboxDuplicateReview({ item }: { item: InboxItem }) {
  const dup = item.duplicate_info;

  if (!dup) {
    return (
      <div className="text-gray-500 text-sm">
        No duplicate detected for this item.
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="p-3 bg-yellow-900/20 border border-yellow-700/30 rounded-lg">
        <div className="flex items-center gap-2">
          <InboxIcon name="alert-triangle" className="w-4 h-4 text-yellow-400" />
          <span className="text-sm font-medium text-yellow-300">
            Potential Duplicate Detected
          </span>
        </div>
        <p className="text-xs text-yellow-400/70 mt-1">{dup.reason ?? ""}</p>
      </div>

      <div className="grid grid-cols-2 gap-3 text-sm">
        <Field label="Confidence">{dup.confidence}</Field>
        <Field label="Content Hash">
          <span className="font-mono text-xs break-all">
            {dup.content_hash ?? ""}
          </span>
        </Field>
        <Field label="Duplicate Source">{dup.duplicate_source ?? ""}</Field>
        <Field label="Candidates">{dup.candidate_ids.length} found</Field>
      </div>

      <div className="flex gap-2">
        <button className="px-3 py-1.5 text-sm text-white bg-green-700 rounded hover:bg-green-600">
          Keep Both
        </button>
        <button className="px-3 py-1.5 text-sm text-white bg-blue-700 rounded hover:bg-blue-600">
          Replace
        </button>
        <button className="px-3 py-1.5 text-sm text-gray-300 bg-gray-700 rounded hover:bg-gray-600">
          Ignore
        </button>
      </div>
    </div>
  );
}
