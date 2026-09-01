// ─────────────────────────────────────────────────────────────────────────────
// streaming/container — Scrollable container with auto-scroll
//
// Mirrors: crates/nabu-ui/src/components/streaming/container.rs
// ─────────────────────────────────────────────────────────────────────────────

import { type HTMLAttributes, useEffect, useRef, useState } from "react";
import { useStreaming, isLifecycleActive } from "./mod";

const DRIFT_THRESHOLD_PX = 40;

export interface StreamingContainerProps extends HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
}

export function StreamingContainer({ children, className = "", ...rest }: StreamingContainerProps) {
  const ctx = useStreaming();
  const containerRef = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);
  const activeIdsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const sessions = Array.from(ctx.sessions.values());
    const newActive = new Set(sessions.filter((s) => isLifecycleActive(s.state)).map((s) => s.stream_id));
    const newStreamStarted = Array.from(newActive).some((id) => !activeIdsRef.current.has(id));
    if (newStreamStarted) { setFollow(true); scrollToBottom(); }
    activeIdsRef.current = newActive;
    if (follow) scrollToBottom();
  }, [ctx.sessions, follow]);

  const handleScroll = () => {
    const el = containerRef.current;
    if (!el) return;
    const scrollHeight = el.scrollHeight;
    const clientHeight = el.clientHeight;
    const scrollTop = el.scrollTop;
    const bottom = scrollHeight - clientHeight - scrollTop;
    if (bottom > DRIFT_THRESHOLD_PX) setFollow(false);
    else setFollow(true);
  };

  const classes = ["streaming-container overflow-y-auto overflow-x-hidden", className].filter(Boolean).join(" ");

  return (
    <div ref={containerRef} className={classes} role="log" aria-label="Streaming output"
      aria-live="polite" aria-relevant="additions text" onScroll={handleScroll} {...rest}>
      {children}
    </div>
  );
}

export function useAutoScroll(): () => void {
  return scrollToBottom;
}

function scrollToBottom(): void {
  const el = document.querySelector(".streaming-container");
  if (el) { el.scrollTop = el.scrollHeight; }
}
