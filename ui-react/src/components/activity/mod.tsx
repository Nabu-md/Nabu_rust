// ──────────────────────────────────────────────────────────────────────────────
// activity/mod — Activity Manager & context (React port)
//
// Mirrors: crates/nabu-ui/src/components/activity/mod.rs
//
// The ActivityManager is the central coordinator for the Activity Panel.
// It receives platform events (from the Tauri backend via the "nabu-event"
// channel when available, or via the imperative `record` API for testing),
// converts each meaningful event into a display-ready ActivityItem, stores
// it in a bounded, chronologically-ordered history, and exposes the result
// to descendants via ActivityContext.
//
// Data flow:
//   Backend EventBus → EventBusBridge (emit_str "nabu-event")
//     → [bindings] → ActivityManager (this module)
//     → ActivityContext (React state)
//     → ActivityPanel → User Timeline
//
// If the Tauri event system is unavailable (e.g. running in a browser
// during development without the Tauri bridge), the manager remains usable
// with an empty timeline and the imperative `record` API.
// ──────────────────────────────────────────────────────────────────────────────

import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  useRef,
  type ReactNode,
} from "react";
import type {
  ActivityItem,
  ActivitySeverity,
  ActivityCategory,
} from "../../types";

// ── Constants ───────────────────────────────────────────────────────────────

/** Maximum number of activities retained in memory. */
export const DEFAULT_MAX_ACTIVITIES = 500;

/** The Tauri event channel name that the backend EventBusBridge emits on. */
export const FRONTEND_EVENT_CHANNEL = "nabu-event";

// ── Label / badge helpers ───────────────────────────────────────────────────

/** Human-readable label for a severity level. */
export function severityLabel(severity: ActivitySeverity): string {
  return severity; // "info" | "warning" | "error"
}

/** CSS badge class for a severity level. */
export function severityBadgeClass(severity: ActivitySeverity): string {
  switch (severity) {
    case "info":
      return "badge badge-info";
    case "warning":
      return "badge badge-warning";
    case "error":
      return "badge badge-error";
    default:
      return "badge badge-info";
  }
}

/** Human-readable label for a category. */
export function categoryLabel(category: ActivityCategory): string {
  const labels: Record<ActivityCategory, string> = {
    capture: "Capture",
    processing: "Processing",
    index: "Index",
    storage: "Storage",
    capability: "Capability",
    plugin: "Plugin",
    sync: "Synchronization",
    agent: "Agent",
    process: "Process",
    conversation: "Conversation",
    stream: "Stream",
    lifecycle: "Lifecycle",
    other: "Other",
  };
  return labels[category] ?? "Other";
}

/** Returns the icon name (key into the ICONS map) for a given category. */
export function categoryIcon(category: ActivityCategory): string {
  const icons: Record<ActivityCategory, string> = {
    capture: "upload",
    processing: "cog",
    index: "database",
    storage: "save",
    capability: "settings",
    plugin: "package",
    sync: "cloudUpload",
    agent: "user",
    process: "monitor",
    conversation: "messageCircle",
    stream: "activity",
    lifecycle: "lifeBuoy",
    other: "info",
  };
  return icons[category] ?? "info";
}

// ── Time helpers ────────────────────────────────────────────────────────────

/** Formats a millisecond timestamp as a relative duration (e.g. "2m ago"). */
export function formatRelativeTime(timestampMs: number): string {
  const now = Date.now();
  const elapsedMs = Math.max(now - timestampMs, 0);

  if (elapsedMs < 1000) {
    return "just now";
  }

  const secs = Math.floor(elapsedMs / 1000);
  const mins = Math.floor(secs / 60);
  const hours = Math.floor(mins / 60);
  const days = Math.floor(hours / 24);

  if (days > 0) {
    return `${days}d ago`;
  }
  if (hours > 0) {
    return `${hours}h ago`;
  }
  if (mins > 0) {
    return `${mins}m ago`;
  }
  return "just now";
}

// ── ActivityManager ─────────────────────────────────────────────────────────

export interface ActivityManagerRef {
  /** Append a new activity item (prepended, bounded history). */
  record: (item: ActivityItem) => void;
  /** Clear all activities. */
  clear: () => void;
  /** Current count of activities. */
  len: () => number;
  /** Whether the store is empty. */
  isEmpty: () => boolean;
  /** Maximum retained activities. */
  maxActivities: number;
}

export interface ActivityContextValue {
  activities: ActivityItem[];
  manager: ActivityManagerRef;
}

const ActivityContext = createContext<ActivityContextValue | null>(null);

/** Hook to access the activity context. */
export function useActivity(): ActivityContextValue {
  const ctx = useContext(ActivityContext);
  if (!ctx) {
    throw new Error("useActivity must be used within an ActivityProvider");
  }
  return ctx;
}

interface ActivityProviderProps {
  children: ReactNode;
  /** Maximum activities retained (defaults to DEFAULT_MAX_ACTIVITIES). */
  maxActivities?: number;
}

/**
 * Provider component that owns the ActivityManager lifetime.
 *
 * On mount, it attempts to subscribe to the Tauri "nabu-event" channel
 * (when running inside Tauri). If the bridge is unavailable, the timeline
 * remains empty and items can be added via the imperative `record` API.
 */
export function ActivityProvider({
  children,
  maxActivities = DEFAULT_MAX_ACTIVITIES,
}: ActivityProviderProps) {
  const [activities, setActivities] = useState<ActivityItem[]>([]);
  const maxRef = useRef(maxActivities);

  // ── Imperative record (used by event listener + testing) ──
  const record = useCallback((item: ActivityItem) => {
    setActivities((prev) => {
      // Deduplicate: if same event_kind + id already exists, replace in-place.
      const pos = prev.findIndex(
        (existing) => existing.event_kind === item.event_kind && existing.id === item.id
      );
      if (pos >= 0) {
        const next = prev.slice();
        next[pos] = item;
        return next;
      }
      // New item — prepend, prune to cap.
      const next = [item, ...prev];
      if (next.length > maxRef.current) {
        next.length = maxRef.current;
      }
      return next;
    });
  }, []);

  const clear = useCallback(() => {
    setActivities([]);
  }, []);

  const len = useCallback(() => activities.length, [activities]);
  const isEmpty = useCallback(() => activities.length === 0, [activities]);

  const managerRef = useRef<ActivityManagerRef>({
    record,
    clear,
    len,
    isEmpty,
    maxActivities,
  });

  // Keep the ref current with the latest callbacks.
  useEffect(() => {
    managerRef.current = {
      record,
      clear,
      len,
      isEmpty,
      maxActivities,
    };
  }, [record, clear, len, isEmpty, maxActivities]);

  // ── Tauri event subscription ──
  // When running inside Tauri, the backend EventBusBridge emits every
  // platform event on the "nabu-event" channel. We install a single
  // listener that converts each envelope into an ActivityItem.
  useEffect(() => {
    let unsubscribe: (() => void) | undefined;

    // Try to access the Tauri event API. In a browser-only dev build this
    // will be undefined and we silently skip subscription.
    const tryListen = () => {
      const g = (window as unknown) as {
        __TAURI__?: {
          event?: {
            listen: (
              event: string,
              handler: (event: { payload: unknown }) => void
            ) => Promise<() => void>;
          };
        };
      };
      if (typeof g !== "undefined" && g.__TAURI__?.event?.listen) {
        g.__TAURI__.event.listen(FRONTEND_EVENT_CHANNEL, (event) => {
          const item = extractActivity(event.payload);
          if (item) {
            record(item);
          }
        }).then((unsub) => {
          unsubscribe = unsub;
        });
      }
    };

    tryListen();

    return () => {
      if (unsubscribe) unsubscribe();
    };
  }, [record]);

  const value: ActivityContextValue = {
    activities,
    manager: managerRef.current,
  };

  return (
    <ActivityContext.Provider value={value}>
      {children}
    </ActivityContext.Provider>
  );
}

// ── Event extraction ────────────────────────────────────────────────────────

/**
 * Converts a raw frontend event payload into an ActivityItem, if the event
 * kind is user-facing and should appear in the activity timeline.
 *
 * Returns null for low-level or internal events (streaming tokens,
 * processing progress, etc.).
 *
 * Mirrors: crates/nabu-ui/src/components/activity/mod.rs::extract_activity
 */
export function extractActivity(payload: unknown): ActivityItem | null {
  if (!payload || typeof payload !== "object") return null;
  const ev = payload as Record<string, unknown>;
  const kind = String(ev.kind ?? ev.event_type ?? "");
  const timestampMs =
    typeof ev.timestamp === "number"
      ? ev.timestamp
      : typeof ev.timestamp_ms === "number"
      ? ev.timestamp_ms
      : Date.now();

  // Extract the PipelineEvent variant from the payload.
  const pipelineEvent = ev.payload as Record<string, unknown> | undefined;

  if (!pipelineEvent) {
    // The event itself may BE the pipeline event (flattened envelope).
    return extractFromPipeline(ev, kind, timestampMs) ?? null;
  }

  return extractFromPipeline(pipelineEvent, kind, timestampMs) ?? null;
}

/**
 * Maps a pipeline event payload (already extracted from the envelope) to
 * an ActivityItem based on its `event_type` discriminant field.
 */
function extractFromPipeline(
  event: Record<string, unknown>,
  kind: string,
  timestampMs: number
): ActivityItem | null {
  const event_type = String(event.event_type ?? event.type ?? "");

  const makeItem = (
    id: string,
    title: string,
    description: string | null,
    severity: ActivitySeverity,
    category: ActivityCategory,
    subsystem: string,
    metadata: Record<string, unknown> = {}
  ): ActivityItem => ({
    id,
    title,
    description,
    severity,
    category,
    subsystem,
    event_kind: kind,
    timestamp_ms: timestampMs,
    metadata,
  });

  // ── Item captured ──
  if (event_type === "item.captured" || event_type === "ItemCaptured") {
    const objectId = String(event.object_id ?? "");
    return makeItem(
      `item.captured:${objectId}`,
      "Item captured",
      typeof event.title === "string" ? event.title : null,
      "info",
      "capture",
      "pipeline",
      {
        object_id: event.object_id,
        object_type: event.object_type,
        capture_source: event.capture_source,
      }
    );
  }

  // ── Item processing started ──
  if (event_type === "item.processing.started" || event_type === "ItemProcessingStarted") {
    const jobId = String(event.job_id ?? "");
    return makeItem(
      `item.processing.started:${jobId}`,
      "Processing started",
      typeof event.processor_name === "string" ? event.processor_name : null,
      "info",
      "processing",
      "pipeline",
      { processor: event.processor_name }
    );
  }

  // ── Item processing completed ──
  if (event_type === "item.processing.completed" || event_type === "ItemProcessingCompleted") {
    const jobId = String(event.job_id ?? "");
    return makeItem(
      `item.processing.completed:${jobId}`,
      "Processing completed",
      typeof event.processor_name === "string" ? event.processor_name : null,
      "info",
      "processing",
      "pipeline",
      { processor: event.processor_name }
    );
  }

  // ── Item processing failed ──
  if (event_type === "item.processing.failed" || event_type === "ItemProcessingFailed") {
    const jobId = String(event.job_id ?? "");
    return makeItem(
      `item.processing.failed:${jobId}`,
      "Processing failed",
      typeof event.error === "string" ? event.error : null,
      "error",
      "processing",
      "pipeline",
      {
        processor: event.processor_name,
        retry_count: event.retry_count,
      }
    );
  }

  // ── Item stored ──
  if (event_type === "item.stored" || event_type === "ItemStored") {
    const vaultPath = String(event.vault_path ?? "");
    return makeItem(
      `item.stored:${vaultPath}`,
      "Item stored",
      vaultPath,
      "info",
      "storage",
      "storage",
      { vault_path: event.vault_path }
    );
  }

  // ── Index updated ──
  if (event_type === "index.updated" || event_type === "IndexUpdated") {
    const objectId = String(event.object_id ?? "");
    return makeItem(
      `index.updated:${objectId}`,
      "Index updated",
      typeof event.operation === "string" ? event.operation : String(event.operation ?? ""),
      "info",
      "index",
      "indexer",
      { operation: event.operation }
    );
  }

  // ── Graph updated ──
  if (event_type === "graph.updated" || event_type === "GraphUpdated") {
    const objectId = String(event.object_id ?? "");
    return makeItem(
      `graph.updated:${objectId}`,
      "Graph updated",
      typeof event.operation === "string" ? event.operation : String(event.operation ?? ""),
      "info",
      "index",
      "graph",
      { operation: event.operation }
    );
  }

  // ── Item cancelled ──
  if (event_type === "item.cancelled" || event_type === "ItemCancelled") {
    const jobId = String(event.job_id ?? "");
    return makeItem(
      `item.cancelled:${jobId}`,
      "Item cancelled",
      null,
      "warning",
      "processing",
      "pipeline"
    );
  }

  // ── Item retried ──
  if (event_type === "item.retried" || event_type === "ItemRetried") {
    const jobId = String(event.job_id ?? "");
    return makeItem(
      `item.retried:${jobId}`,
      "Processing retried",
      `retry ${event.retry_count}/${event.max_retries}`,
      "warning",
      "processing",
      "pipeline"
    );
  }

  // ── Capability state changed ──
  if (event_type === "capability.state_changed" || event_type === "CapabilityStateChanged") {
    const capabilityId = String(event.capability_id ?? "");
    const enabled = event.enabled === true;
    return makeItem(
      `capability:${capabilityId}`,
      `Capability ${enabled ? "enabled" : "disabled"}`,
      typeof event.name === "string" ? event.name : null,
      "info",
      "capability",
      "capability",
      { capability_id: event.capability_id, enabled }
    );
  }

  // ── Sync status changed ──
  if (event_type === "sync.status_changed" || event_type === "SyncStatusChanged") {
    return makeItem(
      `sync:${event.status ?? "unknown"}`,
      "Sync status changed",
      typeof event.status === "string" ? event.status : null,
      "info",
      "sync",
      "sync"
    );
  }

  // ── Plugin lifecycle ──
  if (event_type === "plugin.loaded" || event_type === "PluginLoaded") {
    const id = String(event.plugin_id ?? "");
    return makeItem(
      `plugin.loaded:${id}`,
      "Plugin loaded",
      typeof event.name === "string" ? event.name : null,
      "info",
      "plugin",
      "plugin"
    );
  }

  if (event_type === "plugin.error" || event_type === "PluginError") {
    const id = String(event.plugin_id ?? "");
    return makeItem(
      `plugin.error:${id}`,
      "Plugin error",
      typeof event.error === "string" ? event.error : null,
      "error",
      "plugin",
      "plugin"
    );
  }

  // ── Conversation persisted ──
  if (event_type === "conversation.saved" || event_type === "ConversationSaved") {
    const id = String(event.thread_id ?? "");
    return makeItem(
      `conversation:${id}`,
      "Conversation saved",
      null,
      "info",
      "conversation",
      "conversation"
    );
  }

  // ── Stream lifecycle ──
  if (event_type === "stream.started" || event_type === "StreamStarted") {
    const id = String(event.stream_id ?? "");
    return makeItem(
      `stream.started:${id}`,
      "Stream started",
      null,
      "info",
      "stream",
      "streaming"
    );
  }

  if (event_type === "stream.failed" || event_type === "StreamFailed") {
    const id = String(event.stream_id ?? "");
    return makeItem(
      `stream.failed:${id}`,
      "Stream failed",
      typeof event.error === "string" ? event.error : null,
      "error",
      "stream",
      "streaming"
    );
  }

  // Unknown / internal events — skip.
  return null;
}
