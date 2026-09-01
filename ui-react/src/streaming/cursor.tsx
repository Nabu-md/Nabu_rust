// ─────────────────────────────────────────────────────────────────────────────
// streaming/cursor — Animated cursor indicator for active streams
//
// Mirrors: crates/nabu-ui/src/components/streaming/cursor.rs
// ─────────────────────────────────────────────────────────────────────────────

import { type HTMLAttributes } from "react";

export interface StreamingCursorProps extends HTMLAttributes<HTMLSpanElement> {
  className?: string;
}

export function StreamingCursor({ className = "", ...rest }: StreamingCursorProps) {
  return (
    <span className={["stream-cursor animate-pulse", className].filter(Boolean).join(" ")}
      aria-hidden="true" style={{ height: "1.1em", display: "inline-block", width: "1ch", backgroundColor: "currentColor" }} {...rest} />
  );
}
