// ─────────────────────────────────────────────────────────────────────────────
// streaming/indicator — Lightweight streaming status indicator
//
// Mirrors: ui-react/src/components/streaming/indicator.rs
// ─────────────────────────────────────────────────────────────────────────────

import { type HTMLAttributes } from "react";
import { Spinner, SpinnerSize, StatusDot, StatusKind } from "../ui";

export enum StreamingIndicatorSize { Sm = "sm", Md = "md", Lg = "lg" }

const SPINNER_SIZE_MAP: Record<StreamingIndicatorSize, SpinnerSize> = {
  [StreamingIndicatorSize.Sm]: SpinnerSize.Sm,
  [StreamingIndicatorSize.Md]: SpinnerSize.Md,
  [StreamingIndicatorSize.Lg]: SpinnerSize.Lg,
};

const CSS_CLASS_MAP: Record<StreamingIndicatorSize, string> = {
  [StreamingIndicatorSize.Sm]: "streaming-indicator-sm",
  [StreamingIndicatorSize.Md]: "streaming-indicator-md",
  [StreamingIndicatorSize.Lg]: "streaming-indicator-lg",
};

export interface StreamingIndicatorProps extends HTMLAttributes<HTMLSpanElement> {
  size?: StreamingIndicatorSize;
  tokenCount?: number;
  className?: string;
}

export function StreamingIndicator({ size = StreamingIndicatorSize.Sm, tokenCount, className = "", ...rest }: StreamingIndicatorProps) {
  const sizeClass = CSS_CLASS_MAP[size];
  const spinnerSize = SPINNER_SIZE_MAP[size];

  let countText = "";
  let countA11y = "streaming in progress";
  if (tokenCount !== undefined && tokenCount !== null && tokenCount > 0) {
    countText = ` · ${tokenCount} token${tokenCount === 1 ? "" : "s"}`;
    countA11y = `${tokenCount} tokens delivered`;
  }

  const classes = ["streaming-indicator inline-flex items-center gap-1.5", sizeClass, className].filter(Boolean).join(" ");

  return (
    <span className={classes} role="status" aria-label={`Streaming${countA11y}`} aria-live="polite" {...rest}>
      <StatusDot kind={StatusKind.Info} label="Streaming" pulse />
      <span className="streaming-indicator-label">Streaming…</span>
      {countText && <span className="streaming-indicator-count" aria-hidden="true">{countText}</span>}
      <Spinner size={spinnerSize} label="Streaming" />
    </span>
  );
}
