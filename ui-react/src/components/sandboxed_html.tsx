// ──────────────────────────────────────────────────────────────────────────────
// sandboxed_html.tsx — minimal sandboxed iframe for rendering HTML
//
// Mirrors: crates/nabu-ui/src/components/sandboxed_html.rs
//
// Renders an <iframe> with srcdoc set to the provided HTML and the
// `sandbox` attribute set to "allow-scripts". No message handling — purely
// a render surface for untrusted HTML content.
// ──────────────────────────────────────────────────────────────────────────────

import type { ReactNode } from "react";

export interface SandboxedHtmlProps {
  /** Raw HTML content to render inside the sandboxed iframe. */
  html: string;
}

/**
 * A minimal sandboxed iframe for rendering untrusted HTML content.
 */
export function SandboxedHtml({ html }: SandboxedHtmlProps): ReactNode {
  return (
    <iframe
      srcDoc={html}
      sandbox="allow-scripts"
      className="sandboxed-html"
      title="Sandboxed HTML"
    />
  );
}
