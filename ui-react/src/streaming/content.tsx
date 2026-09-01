// ─────────────────────────────────────────────────────────────────────────────
// streaming/content — Live token rendering for streaming sessions
//
// Mirrors: ui-react/src/components/streaming/content.rs
// ─────────────────────────────────────────────────────────────────────────────

import { type HTMLAttributes } from "react";
import { useStreaming, statusKind, lifecycleLabel, isLifecycleActive } from "./mod";
import { StreamingCursor } from "./cursor";
import { StreamingIndicator } from "./indicator";
import { Button, ButtonVariant, ButtonSize } from "../ui";
import type { StreamSession, StreamId } from "../types";

export interface StreamingContentProps extends HTMLAttributes<HTMLDivElement> {
  className?: string;
}

export function StreamingContent({ className = "", ...rest }: StreamingContentProps) {
  const ctx = useStreaming();

  const sorted: StreamSession[] = Array.from(ctx.sessions.values()).sort((a, b) => {
    const aTerm = isLifecycleActive(a.state) ? 0 : 1;
    const bTerm = isLifecycleActive(b.state) ? 0 : 1;
    if (aTerm !== bTerm) return aTerm - bTerm;
    return b.token_count - a.token_count;
  });

  const classes = ["streaming-content", className].filter(Boolean).join(" ");

  return (
    <div className={classes} role="region" aria-label="Streaming responses" {...rest}>
      {sorted.length === 0 ? (
        <div className="streaming-empty text-xs text-gray-500">No active streams</div>
      ) : (
        sorted.map((session) => <StreamMessage key={String(session.stream_id)} session={session} />)
      )}
    </div>
  );
}

export function StreamMessage({ session }: { session: StreamSession }) {
  const ctx = useStreaming();
  const { stream_id, agent_name, state, content, token_count, total_tokens, error, cancel_reason } = session;
  const agentLabel = agent_name ?? "agent";
  const stateClass = statusKind(state);
  const stateLabel = lifecycleLabel(state);
  const isStreaming = isLifecycleActive(state);

  let subtitle: string;
  switch (state) {
    case "active":
      subtitle = token_count > 0 ? `${token_count} token${token_count === 1 ? "" : "s"}` : "Starting…";
      break;
    case "completed":
      { const total = total_tokens ?? token_count; subtitle = `Completed · ${total} token${total === 1 ? "" : "s"}`; }
      break;
    case "cancelled":
      subtitle = `Cancelled: ${cancel_reason ?? "unknown"}`;
      break;
    case "failed":
      subtitle = `Failed: ${error ?? "unknown error"}`;
      break;
    case "created":
      subtitle = "Created…";
      break;
    default:
      subtitle = "";
  }

  const hasContent = content.length > 0;

  return (
    <div className="stream-message group">
      <div className="stream-message-inner flex items-start gap-2">
        <span className={`status-dot ${stateClass} shrink-0 mt-0.5`} role="status" aria-label={stateLabel} />
        <div className="stream-message-content flex-1 min-w-0">
          <div className="stream-message-text text-sm text-gray-200" style={{ whiteSpace: "pre-wrap" }}
            aria-live={isStreaming ? "polite" : "off"}
            aria-label={isStreaming ? "Streaming response" : "Complete response"}>
            {content}
          </div>
          {isStreaming && <StreamingCursor />}
          {isStreaming && <StreamingIndicator tokenCount={token_count > 0 ? token_count : undefined} />}
        </div>
      </div>

      <div className="stream-message-meta mt-1 flex items-center gap-2 text-xs text-gray-500">
        <span className="stream-agent-name text-gray-400">{agentLabel}</span>
        <span className="stream-subtitle">{subtitle}</span>
      </div>

      {error && (
        <div className="stream-error mt-1" role="alert" aria-label="Stream error">
          <span className="text-red-400 text-xs">{error}</span>
        </div>
      )}

      {isStreaming && (
        <Button variant={ButtonVariant.Destructive} size={ButtonSize.Sm} aria-label="Cancel stream"
          onClick={() => ctx.cancelStream(stream_id as StreamId, "user requested")}>
          Stop
        </Button>
      )}

      {!hasContent && isStreaming && <span className="stream-placeholder" aria-hidden="true"> </span>}
    </div>
  );
}
