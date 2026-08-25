// ──────────────────────────────────────────────────────────────────────────────
// InboxOcrReview.tsx — "OCR" tab
//
// Mirrors: crates/nabu-ui/src/components/inbox.rs (`InboxOcrReview`)
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

/** Format an optional number as "Nms" or "N/A". */
function fmtNum(value: number | null, suffix: string): string {
  return value !== null ? `${value}${suffix}` : "N/A";
}

/** The OCR tab. */
export function InboxOcrReview({ item }: { item: InboxItem }) {
  const ocr = item.ocr_info;

  if (!ocr) {
    return (
      <div className="text-gray-500 text-sm">
        No OCR information available for this item.
      </div>
    );
  }

  const confText = fmtNum(ocr.confidence !== null ? ocr.confidence * 100 : null, "%");
  const pagesText = ocr.page_count !== null ? String(ocr.page_count) : "N/A";
  const durText = fmtNum(ocr.processing_duration_ms, "ms");
  const scannedText = ocr.is_scanned !== null ? String(ocr.is_scanned) : "N/A";

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-3 text-sm">
        <Field label="Confidence">{confText}</Field>
        <Field label="Language">{ocr.recognition_language ?? ""}</Field>
        <Field label="Pages Processed">{pagesText}</Field>
        <Field label="Duration">{durText}</Field>
        <Field label="Scanned Document">{scannedText}</Field>
      </div>

      {ocr.extracted_text && (
        <div>
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Extracted Text
          </label>
          <pre className="mt-1 text-xs text-gray-300 bg-gray-900 p-3 rounded-lg overflow-auto max-h-48 whitespace-pre-wrap">
            {ocr.extracted_text}
          </pre>
        </div>
      )}

      {ocr.warning && (
        <div className="text-sm text-orange-400 flex items-center gap-1">
          <InboxIcon name="alert-triangle" className="w-4 h-4" />
          {ocr.warning}
        </div>
      )}
    </div>
  );
}
