// ──────────────────────────────────────────────────────────────────────────────
// InboxDetails.tsx — "Details" tab: read-only item metadata
//
// Mirrors: crates/nabu-ui/src/components/inbox.rs (`InboxDetails`)
// ──────────────────────────────────────────────────────────────────────────────

import type { InboxItem } from "../../types";
import { InboxIcon } from "./icons";
import { confidenceBarClass, confidenceColor, confidencePct } from "./utils";

/** Reads a string field from the unstructured `custom` metadata map. */
function customString(custom: Record<string, unknown>, key: string): string {
  const v = custom[key];
  return typeof v === "string" ? v : "";
}

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

/** The Details tab. */
export function InboxDetails({ item }: { item: InboxItem }) {
  const classification = customString(item.metadata.custom, "classification");
  const hasClassification = classification.length > 0;

  return (
    <div className="space-y-3">
      <div>
        <label className="text-xs text-gray-500 uppercase tracking-wide">
          Title
        </label>
        <p className="text-lg font-medium text-gray-200 break-all">
          {item.title}
        </p>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <Field label="Type">{item.object_type}</Field>
        <Field label="Source">{item.source}</Field>
        <Field label="MIME Type">{item.mime_type ?? ""}</Field>
        <Field label="Source File">{item.source_file ?? ""}</Field>
      </div>

      {hasClassification && item.confidence !== null && (
        <div className="mt-3">
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Classification
          </label>
          <div className="flex items-center gap-2 mt-1">
            <span className="text-sm font-medium text-blue-300">
              {classification}
            </span>
            <span className={`text-xs ${confidenceColor(item.confidence)}`}>
              {confidencePct(item.confidence)}% confidence
            </span>
            <div className="w-full max-w-[120px] h-1.5 bg-gray-700 rounded overflow-hidden">
              <div
                className={`h-1.5 rounded transition-all ${confidenceBarClass(item.confidence)}`}
                style={{ width: `${confidencePct(item.confidence)}%` }}
              />
            </div>
          </div>
        </div>
      )}

      {item.suggested_folder && (
        <div className="mt-3">
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Suggested Destination
          </label>
          <div className="flex items-center gap-2 mt-1 p-2 bg-blue-900/20 border border-blue-700/30 rounded-lg">
            <InboxIcon name="map-pin" className="w-4 h-4 text-blue-400" />
            <span className="text-sm text-blue-300 break-all">
              {item.suggested_folder}
            </span>
            <span className="text-xs text-gray-500">(suggested)</span>
          </div>
        </div>
      )}

      {item.warnings.length > 0 && (
        <div className="mt-3">
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Warnings
          </label>
          <ul className="mt-1 space-y-1">
            {item.warnings.map((w) => (
              <li key={w} className="text-sm text-yellow-400">
                {w}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
