// ─────────────────────────────────────────────────────────────────────────────
// components/pdf_viewer — PDF rendering via iframe
//
// Mirrors: ui-react/src/components/pdf_viewer.rs
// ─────────────────────────────────────────────────────────────────────────────

import { type HTMLAttributes } from "react";

export interface PdfViewerProps extends HTMLAttributes<HTMLDivElement> {
  pdfPath: string;
}

export function PdfViewer({ pdfPath, className = "", ...rest }: PdfViewerProps) {
  const src = pdfPath.startsWith("file://") ? pdfPath : `file://${pdfPath}`;
  return (
    <div className={["pdf-viewer-container h-screen w-full", className].filter(Boolean).join(" ")} {...rest}>
      <iframe src={src} title="PDF Viewer"
        className="pdf-viewer-frame w-full h-full border-0"
        style={{ height: "100%", width: "100%" }} />
    </div>
  );
}
