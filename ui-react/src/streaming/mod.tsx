// ─────────────────────────────────────────────────────────────────────────────
// streaming/mod — Frontend streaming state provider
//
// Mirrors: crates/nabu-ui/src/components/streaming/mod.rs
// ─────────────────────────────────────────────────────────────────────────────

import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { streamCancel } from "../ipc";
import { useToast } from "../context";
import type {
  StreamId,
  StreamLifeCycle,
  StreamSession,
  StreamingContextValue,
} from "../types";

export { StreamingContent } from "./content";
export { StreamingContainer, useAutoScroll } from "./container";
export { StreamingCursor } from "./cursor";
export { StreamingIndicator, StreamingIndicatorSize } from "./indicator";

const FRONTEND_EVENT_CHANNEL = "nabu-event";

export function statusKind(state: StreamLifeCycle): string {
  switch (state) {
    case "created": case "active": return "status-dot-info";
    case "completed": return "status-dot-success";
    case "cancelled": return "status-dot-warning";
    case "failed": return "status-dot-error";
    default: return "text-gray-400";
  }
}

export function lifecycleLabel(state: StreamLifeCycle): string {
  switch (state) {
    case "created": return "Created";
    case "active": return "Streaming";
    case "completed": return "Completed";
    case "cancelled": return "Cancelled";
    case "failed": return "Failed";
    default: return "Unknown";
  }
}

export function isLifecycleActive(state: StreamLifeCycle): boolean {
  return state === "created" || state === "active";
}

export function isLifecycleTerminal(state: StreamLifeCycle): boolean {
  return state === "completed" || state === "cancelled" || state === "failed";
}

function createInitialSession(streamId: StreamId): StreamSession {
  return {
    stream_id: streamId,
    thread_id: null,
    agent_name: null,
    state: "created" as StreamLifeCycle,
    content: "",
    token_count: 0,
    total_tokens: null,
    error: null,
    cancel_reason: null,
    last_event: null,
    metadata: {},
  };
}

export interface StreamingProviderContextValue extends StreamingContextValue {
  sessions: Map<StreamId, StreamSession>;
}

const StreamingContext = createContext<StreamingProviderContextValue | null>(null);

export function useStreaming(): StreamingProviderContextValue {
  const ctx = useContext(StreamingContext);
  if (!ctx) throw new Error("useStreaming must be used within a StreamingProvider");
  return ctx;
}

interface NabuEvent {
  event_type: string;
  payload: unknown;
}

interface FrontendEventEnvelope {
  event_type: string;
  payload: NabuEvent;
}

export interface StreamingProviderProps {
  children: ReactNode;
}

export function StreamingProvider({ children }: StreamingProviderProps) {
  const toasts = useToast();
  const [sessions, setSessions] = useState<Map<StreamId, StreamSession>>(() => new Map());

  const processEvent = (ev: NabuEvent) => {
    setSessions((prev) => {
      const next = new Map(prev);
      const handle = (streamId: StreamId) => next.get(streamId) ?? createInitialSession(streamId);

      switch (ev.event_type) {
        case "SessionCreated": {
          const p = ev.payload as Record<string, unknown>;
          const sid = String(p.stream_id ?? "");
          if (sid && !next.has(sid)) {
            const s = handle(sid as StreamId);
            s.state = "created";
            s.last_event = typeof p.timestamp === "string" ? p.timestamp : null;
            if (p.thread_id) s.thread_id = String(p.thread_id);
            next.set(sid as StreamId, s);
          }
          break;
        }
        case "SessionStarted": {
          const p = ev.payload as Record<string, unknown>;
          const sid = String(p.stream_id ?? "");
          if (sid && next.has(sid)) {
            const s = next.get(sid)!;
            s.state = "active";
            s.last_event = typeof p.timestamp === "string" ? p.timestamp : s.last_event;
          }
          break;
        }
        case "SessionCancelled": {
          const p = ev.payload as Record<string, unknown>;
          const sid = String(p.stream_id ?? "");
          if (sid && next.has(sid)) {
            const s = next.get(sid)!;
            s.state = "cancelled";
            s.cancel_reason = typeof p.reason === "string" ? p.reason : null;
            s.last_event = typeof p.timestamp === "string" ? p.timestamp : s.last_event;
          }
          break;
        }
        case "SessionCleanedUp": {
          const p = ev.payload as Record<string, unknown>;
          const sid = String(p.stream_id ?? "");
          if (sid) next.delete(sid as StreamId);
          break;
        }
        case "StreamStarted": {
          const e = ev.payload as Record<string, unknown>;
          const sid = String(e.stream_id ?? "");
          if (sid) {
            const s = handle(sid as StreamId);
            s.thread_id = e.thread_id ? String(e.thread_id) : null;
            s.agent_name = e.agent_name ? String(e.agent_name) : null;
            s.metadata = (e.metadata as Record<string, unknown>) ?? {};
            if (s.state === "created") s.state = "active";
            s.last_event = typeof e.timestamp === "string" ? e.timestamp : null;
            next.set(sid as StreamId, s);
          }
          break;
        }
        case "Token": {
          const e = ev.payload as Record<string, unknown>;
          const sid = String(e.stream_id ?? "");
          if (sid && next.has(sid)) {
            const s = next.get(sid)!;
            const token = typeof e.token === "string" ? e.token : "";
            if (token) s.content += token;
            s.token_count = typeof e.sequence === "number" ? e.sequence + 1 : s.token_count;
            s.last_event = typeof e.timestamp === "string" ? e.timestamp : s.last_event;
          }
          break;
        }
        case "PartialUpdate": {
          const e = ev.payload as Record<string, unknown>;
          const sid = String(e.stream_id ?? "");
          if (sid && next.has(sid)) {
            const s = next.get(sid)!;
            if (typeof e.content === "string") s.content = e.content;
            if (typeof e.token_count === "number") s.token_count = e.token_count;
            s.last_event = typeof e.timestamp === "string" ? e.timestamp : s.last_event;
          }
          break;
        }
        case "Completed": {
          const e = ev.payload as Record<string, unknown>;
          const sid = String(e.stream_id ?? "");
          if (sid && next.has(sid)) {
            const s = next.get(sid)!;
            if (typeof e.full_content === "string") s.content = e.full_content;
            if (typeof e.total_tokens === "number") { s.token_count = e.total_tokens; s.total_tokens = e.total_tokens; }
            s.state = "completed";
            s.last_event = typeof e.timestamp === "string" ? e.timestamp : s.last_event;
          }
          break;
        }
        case "Cancelled": {
          const e = ev.payload as Record<string, unknown>;
          const sid = String(e.stream_id ?? "");
          if (sid && next.has(sid)) {
            const s = next.get(sid)!;
            if (typeof e.tokens_delivered === "number") s.token_count = e.tokens_delivered;
            s.state = "cancelled";
            s.cancel_reason = typeof e.reason === "string" ? e.reason : null;
            s.last_event = typeof e.timestamp === "string" ? e.timestamp : s.last_event;
          }
          break;
        }
        case "Failed": {
          const e = ev.payload as Record<string, unknown>;
          const sid = String(e.stream_id ?? "");
          if (sid && next.has(sid)) {
            const s = next.get(sid)!;
            if (typeof e.tokens_delivered === "number") s.token_count = e.tokens_delivered;
            s.state = "failed";
            s.error = typeof e.error === "string" ? e.error : null;
            s.last_event = typeof e.timestamp === "string" ? e.timestamp : s.last_event;
          }
          break;
        }
        default:
          return prev;
      }
      return next;
    });
  };

  useEffect(() => {
    const g = window as unknown as {
      __TAURI__?: { event?: { listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void> } };
    };
    let unsubscribe: (() => void) | undefined;
    if (typeof g !== "undefined" && g.__TAURI__?.event?.listen) {
      g.__TAURI__.event.listen(FRONTEND_EVENT_CHANNEL, (event: { payload: unknown }) => {
        const payload = event.payload as FrontendEventEnvelope;
        if (payload && typeof payload === "object" && "event_type" in payload) {
          processEvent(payload as NabuEvent);
        }
      }).then((unsub) => { unsubscribe = unsub; });
    }
    return () => { if (unsubscribe) unsubscribe(); };
  }, [toasts]);

  const cancelStream = async (streamId: StreamId, reason: string) => {
    try { await streamCancel(streamId, reason); }
    catch { toasts.toast("Could not cancel stream", { variant: "error" }); }
  };

  const pruneTerminal = () => {
    setSessions((prev) => {
      const next = new Map(prev);
      for (const [id, s] of next) {
        if (isLifecycleTerminal(s.state)) next.delete(id);
      }
      return next;
    });
  };

  const clearTerminal = pruneTerminal;
  const clear = () => setSessions(new Map());

  const len = sessions.size;
  const isEmpty = sessions.size === 0;
  const activeCount = Array.from(sessions.values()).filter((s) => isLifecycleActive(s.state)).length;
  const hasActive = activeCount > 0;

  const ctx: StreamingProviderContextValue = {
    sessions, cancelStream, pruneTerminal, clearTerminal, clear,
    len, is_empty: isEmpty, active_count: activeCount, has_active: hasActive,
  };

  return <StreamingContext.Provider value={ctx}>{children}</StreamingContext.Provider>;
}
