// ──────────────────────────────────────────────────────────────────────────────
// statistics.tsx — Vault statistics & insights dashboard
//
// Mirrors: crates/nabu-ui/src/components/statistics.rs (StatisticsView)
//
// Displays comprehensive vault statistics: note count, folder count, tag
// count, graph connections, orphan notes, writing streaks, recently
// created/modified notes, vault growth (30-day histogram), and storage
// usage. Data is computed on demand by the backend `statistics_get`
// command — no persistent index to maintain.
//
// Runtime metrics (timers, counters, gauges) and worker-pool health are
// fetched via `metrics` and `pool_health` IPC commands, mirroring the
// MetricsContext + PoolHealthSnapshot in the Dioxus spec.
//
// Consumes NavContext (via useNav) and WorkspaceContext (via useWorkspace)
// for opening notes from the "Recently Modified / Created" lists.
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useState, useCallback } from "react";
import { Icon } from "./layout/icons";
import { useNav, useWorkspace, useToast } from "../context";
import { statisticsGet, metrics, poolHealth } from "../ipc";
import type {
  VaultStatistics,
  PoolHealth,
  RuntimeMetrics,
  MetricTimer,
  MetricCounter,
  MetricGauge,
} from "../types";

// ── Load-state lifecycle ────────────────────────────────────────────────────

type LoadState = "idle" | "loading" | "loaded" | "failed";

// ── Helpers ─────────────────────────────────────────────────────────────────

/** Human-readable byte size. Mirrors `format_bytes` in statistics.rs. */
function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/**
 * Opens a note in a new tab and navigates to the Editor view.
 * Mirrors the `open_tab` + view-switch pattern from the Dioxus spec.
 */
function openNote(
  ws: ReturnType<typeof useWorkspace>,
  nav: ReturnType<typeof useNav>,
  path: string,
) {
  ws.openTab(path);
  nav.setViewMode("Editor");
}

// ── Skeleton ────────────────────────────────────────────────────────────────

function Skeleton({ width = "100%", height = "80px" }: { width?: string; height?: string }) {
  return (
    <div
      className="animate-pulse bg-gray-800 rounded"
      style={{ width, height }}
    />
  );
}

function SkeletonGrid({ rows = 8 }: { rows?: number }) {
  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} height="80px" />
      ))}
    </div>
  );
}

// ── Error panel ─────────────────────────────────────────────────────────────

interface ErrorPanelProps {
  title: string;
  message: string;
  details: string;
  recovery: string;
  onRetry: () => void;
}

function ErrorPanel({ title, message, details, recovery, onRetry }: ErrorPanelProps) {
  return (
    <div className="bg-red-900/20 border border-red-800/50 rounded-lg p-4 text-sm">
      <div className="font-semibold text-red-300">{title}</div>
      <div className="text-gray-400 mt-1">{message}</div>
      {details && <div className="text-xs text-gray-500 mt-2 break-words">{details}</div>}
      <div className="text-xs text-gray-500 mt-2">{recovery}</div>
      <button
        type="button"
        onClick={onRetry}
        className="mt-2 px-3 py-1 text-xs bg-gray-800 rounded hover:bg-gray-700 border border-gray-700"
      >
        Retry
      </button>
    </div>
  );
}

// ── Empty state ─────────────────────────────────────────────────────────────

interface EmptyStateProps {
  icon: string;
  title: string;
  description: string;
}

function EmptyState({ icon, title, description }: EmptyStateProps) {
  return (
    <div className="py-12 flex flex-col items-center justify-center text-center">
      <Icon name={icon} className="w-10 h-10 text-gray-500 mb-3" />
      <div className="text-lg font-medium text-gray-300">{title}</div>
      <div className="text-sm text-gray-500 mt-1 max-w-md">{description}</div>
    </div>
  );
}

// ── Sub-components ──────────────────────────────────────────────────────────

/** Metric stat card for the key-metrics grid. */
function StatCard({
  icon,
  label,
  value,
  color = "text-gray-400",
}: {
  icon: string;
  label: string;
  value: string;
  color?: string;
}) {
  return (
    <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
      <div className="text-2xl font-bold" style={{ color }}>{value}</div>
      <div className="text-xs text-gray-500 mt-1 flex items-center gap-1">
        <Icon name={icon} className="w-3 h-3" />
        {label}
      </div>
    </div>
  );
}

/** Streak / active-days / storage card with icon + value. */
function IconStatCard({
  icon,
  label,
  value,
  color = "text-gray-400",
}: {
  icon: string;
  label: string;
  value: string;
  color?: string;
}) {
  return (
    <div className="bg-gray-900 border border-gray-800 rounded-lg p-4 flex items-center gap-3">
      <span className="text-3xl">
        <Icon name={icon} className="w-6 h-6" />
      </span>
      <div>
        <div className="text-2xl font-bold" style={{ color }}>{value}</div>
        <div className="text-xs text-gray-500">{label}</div>
      </div>
    </div>
  );
}

// ── Main component ──────────────────────────────────────────────────────────

/** Statistics & Insights dashboard view. */
export function StatisticsView() {
  const nav = useNav();
  const ws = useWorkspace();
  const { toast } = useToast();

  // Vault statistics (owned by this view — computed on demand).
  const [stats, setStats] = useState<VaultStatistics | null>(null);
  const [statsState, setStatsState] = useState<LoadState>("idle");
  const [statsError, setStatsError] = useState<string | null>(null);

  // Worker pool health (owned by this view).
  const [pool, setPool] = useState<PoolHealth | null>(null);
  const [poolState, setPoolState] = useState<LoadState>("idle");
  const [poolError, setPoolError] = useState<string | null>(null);

  // Runtime metrics (timers, counters, gauges).
  const [runtimeMetrics, setRuntimeMetrics] = useState<RuntimeMetrics | null>(null);
  const [metricsState, setMetricsState] = useState<LoadState>("idle");
  const [metricsError, setMetricsError] = useState<string | null>(null);

  // Tag filter input.
  const [tagFilter, setTagFilter] = useState("");

  // ── Load: statistics ──
  const loadStats = useCallback(async () => {
    setStatsState("loading");
    setStatsError(null);
    try {
      const result = await statisticsGet();
      setStats(result);
      setStatsState("loaded");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setStatsError(msg);
      setStatsState("failed");
      toast(`Failed to load statistics: ${msg}`, { variant: "error" });
    }
  }, [toast]);

  // ── Load: pool health ──
  const loadPool = useCallback(async () => {
    setPoolState("loading");
    setPoolError(null);
    try {
      const result = await poolHealth();
      setPool(result);
      setPoolState("loaded");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setPoolError(msg);
      setPoolState("failed");
    }
  }, [toast]);

  // ── Load: runtime metrics ──
  const loadMetrics = useCallback(async () => {
    setMetricsState("loading");
    setMetricsError(null);
    try {
      const result = await metrics();
      setRuntimeMetrics(result);
      setMetricsState("loaded");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setMetricsError(msg);
      setMetricsState("failed");
    }
  }, []);

  // ── Initial load on mount ──
  useEffect(() => {
    loadStats();
    loadPool();
    loadMetrics();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Refresh all ──
  const refreshAll = () => {
    loadStats();
    loadPool();
    loadMetrics();
  };

  // ── Tag filtering ──
  const filteredTags = (() => {
    if (!stats) return [];
    const query = tagFilter.toLowerCase();
    if (!query) return stats.tags;
    return stats.tags.filter((t) => t.tag.toLowerCase().includes(query));
  })();

  // ── Growth histogram ──
  const maxGrowth = stats?.growth?.length
    ? Math.max(...stats.growth.map((g) => g.count))
    : 1;

  return (
    <div className="h-screen overflow-y-auto bg-gray-950 text-gray-100">
      <div className="max-w-6xl mx-auto p-6 space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold text-gray-200">Statistics &amp; Insights</h1>
            <p className="text-sm text-gray-500">Vault-wide metrics and writing insights</p>
          </div>
          <button
            type="button"
            onClick={refreshAll}
            className="px-3 py-1.5 text-sm bg-gray-800 rounded hover:bg-gray-700 border border-gray-700 flex items-center gap-1"
          >
            <Icon name="database" className="w-3 h-3" />
            Refresh
          </button>
        </div>

        {/* ── Vault statistics: error / loading / content ── */}
        {statsState === "failed" && statsError ? (
          <ErrorPanel
            title="Couldn't load statistics"
            message="Failed to compute vault statistics."
            details={statsError}
            recovery="Make sure your vault is accessible, then try again."
            onRetry={loadStats}
          />
        ) : statsState !== "loaded" ? (
          <SkeletonGrid rows={8} />
        ) : !stats ? null : stats.note_count === 0 ? (
          // Empty vault (loaded successfully but no notes)
          <EmptyState
            icon="database"
            title="Your vault is empty"
            description="Create your first note to see statistics, writing streaks, and insights here."
          />
        ) : (
          (() => {
            const s = stats;
            return (
              <>
                {/* Key metrics grid */}
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <StatCard icon="fileText" label="Notes" value={String(s.note_count)} color="text-blue-400" />
                  <StatCard icon="folder" label="Folders" value={String(s.folder_count)} color="text-green-400" />
                  <StatCard icon="tag" label="Unique Tags" value={String(s.tag_count)} color="text-purple-400" />
                  <StatCard icon="tag" label="Total Tag Uses" value={String(s.total_tags)} color="text-yellow-400" />
                </div>

                {/* Graph metrics */}
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <StatCard icon="network" label="Graph Nodes" value={String(s.graph_nodes)} color="text-cyan-400" />
                  <StatCard icon="link" label="Graph Connections" value={String(s.graph_edges)} color="text-indigo-400" />
                  <StatCard icon="fileText" label="Orphan Notes" value={String(s.graph_orphans)} color="text-orange-400" />
                  <StatCard icon="folderTree" label="Clusters" value={String(s.graph_clusters)} color="text-pink-400" />
                </div>

                {/* Writing streak & storage */}
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                  <IconStatCard icon="sparkles" label="Day writing streak" value={String(s.writing_streak_days)} color="text-red-400" />
                  <IconStatCard icon="calendar" label="Active days (last 30)" value={String(s.active_days_last_30)} color="text-green-400" />
                  <IconStatCard icon="hardDrive" label="Storage usage" value={formatBytes(s.storage_bytes)} color="text-blue-400" />
                </div>

                {/* Vault growth histogram */}
                <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-gray-300 mb-3">Vault Growth (30 days)</h3>
                  <div className="flex items-end gap-1 h-32">
                    {s.growth.length === 0 ? (
                      <div className="text-xs text-gray-500 py-4">No growth data yet.</div>
                    ) : (
                      s.growth.map((point) => {
                        const pct = maxGrowth > 0
                          ? (point.count / maxGrowth) * 100
                          : 0;
                        const height = Math.max(pct, 2);
                        const label = `${point.date}: ${point.count}`;
                        return (
                          <div key={point.date} className="flex-1 flex flex-col items-center justify-end group relative">
                            <div
                              className="w-full bg-blue-600 rounded-t hover:bg-blue-500 transition-colors"
                              style={{ height: `${height}%` }}
                            />
                            <div className="absolute -top-6 opacity-0 group-hover:opacity-100 transition-opacity text-xs text-gray-400 whitespace-nowrap">
                              {label}
                            </div>
                          </div>
                        );
                      })
                    )}
                  </div>
                  {s.growth.length > 0 && (
                    <div className="flex justify-between mt-2 text-xs text-gray-600">
                      <span>{s.growth[0]?.date ?? ""}</span>
                      <span>{s.growth[s.growth.length - 1]?.date ?? ""}</span>
                    </div>
                  )}
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  {/* Tags */}
                  <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
                    <div className="flex items-center justify-between mb-3">
                      <h3 className="text-sm font-semibold text-gray-300">Tags</h3>
                      <input
                        type="text"
                        placeholder="Filter tags…"
                        className="bg-gray-800 text-gray-100 rounded px-2 py-1 text-xs border border-gray-700"
                        value={tagFilter}
                        onChange={(e) => setTagFilter(e.target.value)}
                      />
                    </div>
                    {filteredTags.length === 0 ? (
                      <div className="text-sm text-gray-500">No tags found</div>
                    ) : (
                      <div className="flex flex-wrap gap-2">
                        {filteredTags.map((t) => (
                          <span
                            key={t.tag}
                            className="px-2 py-1 text-xs bg-gray-800 rounded text-gray-300 border border-gray-700"
                          >
                            {t.tag} ({t.count})
                          </span>
                        ))}
                      </div>
                    )}
                  </div>

                  {/* Recently Modified */}
                  <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
                    <h3 className="text-sm font-semibold text-gray-300 mb-3">Recently Modified</h3>
                    {s.recently_modified.length === 0 ? (
                      <div className="text-sm text-gray-500">No recently modified notes.</div>
                    ) : (
                      <div className="space-y-1">
                        {s.recently_modified.map((n) => (
                          <div
                            key={n.path}
                            className="flex items-center justify-between py-1 cursor-pointer hover:bg-gray-800 rounded px-2"
                            onClick={() => openNote(ws, nav, n.path)}
                          >
                            <span className="text-sm text-gray-300 truncate">{n.title}</span>
                            <span className="text-xs text-gray-500">{formatBytes(n.size)}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>

                  {/* Recently Created */}
                  <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
                    <h3 className="text-sm font-semibold text-gray-300 mb-3">Recently Created</h3>
                    {s.recently_created.length === 0 ? (
                      <div className="text-sm text-gray-500">No recently created notes.</div>
                    ) : (
                      <div className="space-y-1">
                        {s.recently_created.map((n) => (
                          <div
                            key={n.path}
                            className="flex items-center justify-between py-1 cursor-pointer hover:bg-gray-800 rounded px-2"
                            onClick={() => openNote(ws, nav, n.path)}
                          >
                            <span className="text-sm text-gray-300 truncate">{n.title}</span>
                            <span className="text-xs text-gray-500">{formatBytes(n.size)}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                {/* Worker Pool Health */}
                <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-gray-300 mb-3">Worker Pool Health</h3>
                  {poolState === "loading" ? (
                    <div className="space-y-1">
                      {Array.from({ length: 4 }).map((_, i) => (
                        <Skeleton key={i} height="24px" />
                      ))}
                    </div>
                  ) : poolState === "failed" && poolError ? (
                    <ErrorPanel
                      title="Worker pool unavailable"
                      message="Could not load worker pool health."
                      details={poolError}
                      recovery="The pool may still be starting up. Try again in a moment."
                      onRetry={loadPool}
                    />
                  ) : !pool ? null : (
                    <>
                      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
                        <div className="flex flex-col">
                          <div className="text-xl font-bold text-blue-400">{pool.worker_count}</div>
                          <div className="text-xs text-gray-500">Workers</div>
                        </div>
                        <div className="flex flex-col">
                          <div className="text-xl font-bold text-green-400">{pool.active_workers}</div>
                          <div className="text-xs text-gray-500">Active</div>
                        </div>
                        <div className="flex flex-col">
                          <div className="text-xl font-bold text-yellow-400">{pool.pending_jobs}</div>
                          <div className="text-xs text-gray-500">Pending</div>
                        </div>
                        <div className="flex flex-col">
                          <div className="text-xl font-bold text-purple-400">{pool.running_jobs}</div>
                          <div className="text-xs text-gray-500">Running</div>
                        </div>
                      </div>
                      <div className="mt-3 text-xs text-gray-500">
                        Lifecycle: {pool.lifecycle_stage ?? "(unknown)"}
                      </div>
                      {pool.shutting_down ? (
                        <span className="text-xs text-orange-400">Shutting down</span>
                      ) : pool.is_throttled ? (
                        <span className="text-xs text-orange-400">Throttled</span>
                      ) : (
                        <span className="text-xs text-gray-500">Normal capacity</span>
                      )}
                    </>
                  )}
                </div>

                {/* Performance Metrics */}
                <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-gray-300 mb-3">Performance Metrics</h3>
                  {metricsState === "loading" ? (
                    <div className="space-y-1">
                      {Array.from({ length: 6 }).map((_, i) => (
                        <Skeleton key={i} height="24px" />
                      ))}
                    </div>
                  ) : metricsState === "failed" && metricsError ? (
                    <div className="text-sm text-yellow-400">
                      Metrics unavailable ({metricsError})
                    </div>
                  ) : !runtimeMetrics ? null : (
                    (() => {
                      const m = runtimeMetrics;
                      return (
                        <>
                          {/* Timers */}
                          {m.timers.length > 0 && (
                            <div className="mb-3">
                              <h4 className="text-xs font-semibold text-gray-500 uppercase mb-2">Timers</h4>
                              <table className="w-full text-xs">
                                <thead>
                                  <tr>
                                    <th className="text-left text-gray-600 pb-1">Operation</th>
                                    <th className="text-right text-gray-600 pb-1">Count</th>
                                    <th className="text-right text-gray-600 pb-1">Avg (ms)</th>
                                    <th className="text-right text-gray-600 pb-1">p50</th>
                                    <th className="text-right text-gray-600 pb-1">p90</th>
                                    <th className="text-right text-gray-600 pb-1">Max</th>
                                  </tr>
                                </thead>
                                <tbody>
                                  {m.timers.map((t: MetricTimer) => (
                                    <tr key={t.name}>
                                      <td className="text-gray-400 py-1">{t.name}</td>
                                      <td className="text-right text-gray-500">{t.count}</td>
                                      <td className="text-right text-gray-500">{t.avg_ms.toFixed(1)}</td>
                                      <td className="text-right text-gray-500">{t.p50_ms.toFixed(1)}</td>
                                      <td className="text-right text-gray-500">{t.p90_ms.toFixed(1)}</td>
                                      <td className="text-right text-gray-500">{t.max_ms.toFixed(1)}</td>
                                    </tr>
                                  ))}
                                </tbody>
                              </table>
                            </div>
                          )}

                          {/* Counters */}
                          {m.counters.length > 0 && (
                            <div className="mt-3">
                              <h4 className="text-xs font-semibold text-gray-500 uppercase mb-2">Counters</h4>
                              <table className="w-full text-xs">
                                <tbody>
                                  {m.counters.map((c: MetricCounter) => (
                                    <tr key={c.name}>
                                      <td className="text-gray-400 py-1">{c.name}</td>
                                      <td className="text-right text-gray-500">{c.value}</td>
                                    </tr>
                                  ))}
                                </tbody>
                              </table>
                            </div>
                          )}

                          {/* Gauges */}
                          {m.gauges.length > 0 && (
                            <div className="mt-3">
                              <h4 className="text-xs font-semibold text-gray-500 uppercase mb-2">Gauges</h4>
                              <table className="w-full text-xs">
                                <tbody>
                                  {m.gauges.map((g: MetricGauge) => (
                                    <tr key={g.name}>
                                      <td className="text-gray-400 py-1">{g.name}</td>
                                      <td className="text-right text-gray-500">{g.value}</td>
                                    </tr>
                                  ))}
                                </tbody>
                              </table>
                            </div>
                          )}
                        </>
                      );
                    })()
                  )}
                </div>
              </>
            );
          })()
        )}
      </div>
    </div>
  );
}
