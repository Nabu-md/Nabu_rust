// ──────────────────────────────────────────────────────────────────────────────
// components/graph_view.tsx — interactive knowledge graph canvas (React)
//
// Mirrors: ui-react/src/components/graph_view.rs
//
// Responsibilities (ported from the Dioxus spec):
// - fetch real graph data via the `graph_data` IPC command;
// - show loading / empty / error states;
// - render real nodes (DOM cards) and real edges (SVG connectors) from the
//   backend graph;
// - pan / zoom via CSS transforms (no external graph framework);
// - node selection with relationship highlighting;
// - open the selected note through WorkspaceContext.activePath;
// - refresh when the backend graph is updated (via event listener).
//
// Performance: the expensive layout computation is memoized and runs once
// per data load — never on every frame or trivial UI state change.
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useMemo, useCallback, useState, type ReactNode } from "react";
import { graphData } from "../ipc";
import { useWorkspace } from "../context";
import type { GraphData, GraphNode, GraphEdgeData } from "../types";
import { layoutGraph } from "./graph_layout";
import type { Layout, NodePos, Bounds } from "./graph_layout";

// ── State & classification ─────────────────────────────────────────────────

/** Graph data load lifecycle states (mirrors Rust `GraphLoadState`). */
export type GraphLoadState = "idle" | "loading" | "loaded" | "failed";

/** Which visual phase the view is in, derived purely from (state, data). */
export type ViewPhase = "loading" | "empty" | "error" | "ready";

/**
 * Classify the current view phase from load state + payload.
 *
 * - `idle`/`loading` → `loading` (idle is treated as loading on first mount).
 * - `failed` → `error` (never replaced by an empty canvas).
 * - `loaded` with no nodes → `empty`.
 * - `loaded` with nodes → `ready`.
 */
export function classifyView(data: GraphData | null, state: GraphLoadState): ViewPhase {
  switch (state) {
    case "idle":
    case "loading":
      return "loading";
    case "failed":
      return "error";
    case "loaded":
      return data && data.nodes.length > 0 ? "ready" : "empty";
  }
}

// ── Viewport state ───────────────────────────────────────────────────────────

interface Viewport {
  panX: number;
  panY: number;
  zoom: number;
  panning: boolean;
  panStartX: number;
  panStartY: number;
}

const DEFAULT_VIEWPORT: Viewport = {
  panX: 0,
  panY: 0,
  zoom: 0.6, // Start moderately zoomed out for a whole-graph overview.
  panning: false,
  panStartX: 0,
  panStartY: 0,
};

/** CSS transform string applied to the canvas container. */
function viewportTransform(vp: Viewport): string {
  return `translate(${vp.panX}px, ${vp.panY}px) scale(${vp.zoom})`;
}

// ── Edge segment (precomputed for SVG rendering) ────────────────────────────

interface EdgeSegment {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  stroke: string;
  width: number;
  dash: string;
}

// ── Helper: bounds accessors ─────────────────────────────────────────────────

function boundsWidth(b: Bounds): number {
  return b.maxX - b.minX;
}

function boundsHeight(b: Bounds): number {
  return b.maxY - b.minY;
}

// ── Component ────────────────────────────────────────────────────────────────

/**
 * The Graph view component.
 *
 * Shows loading / empty / error states around a real interactive graph canvas.
 * The canvas renders real nodes (DOM cards positioned from a memoized layout)
 * and real edges (SVG connectors) derived from the backend `graph_data`.
 */
export function GraphView(): ReactNode {
  const ws = useWorkspace();

  const [data, setData] = useState<GraphData | null>(null);
  const [loadState, setLoadState] = useState<GraphLoadState>("idle");
  const [errorMsg, setErrorMsg] = useState("");

  // Viewport state (pan / zoom) and node selection.
  const [viewport, setViewport] = useState<Viewport>(DEFAULT_VIEWPORT);
  const [selected, setSelected] = useState<string | null>(null);

  // Memoized layout: recomputed only when the graph payload changes.
  const layout: Layout | null = useMemo(() => {
    if (!data) return null;
    return layoutGraph(data);
  }, [data]);

  // --- Load graph data on mount + on GraphUpdated events ---

  const loadData = useCallback(async () => {
    setLoadState("loading");
    setErrorMsg("");
    try {
      const result = await graphData();
      setData(result);
      setLoadState("loaded");
      setErrorMsg("");
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setLoadState("failed");
    }
  }, []);

  useEffect(() => {
    let active = true;

    const load = async () => {
      await loadData();
      // If a race happened, the `active` flag prevents stale writes.
      // (loadData already sets state; this guard is a safety net.)
      void active;
    };

    load();

    // Refresh from backend truth when the graph is updated by the backend.
    const handler = () => {
      void loadData();
    };
    window.addEventListener("graph:updated", handler);
    return () => {
      window.removeEventListener("graph:updated", handler);
    };
  }, [loadData]);

  const currentPhase = classifyView(data, loadState);
  const selectedPath = selected;

  // ── Phase rendering: Loading ──

  if (currentPhase === "loading") {
    return (
      <div className="graph-view h-screen flex flex-col bg-gray-950 text-gray-100">
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-cyan-400 mx-auto mb-4" />
            <p className="text-gray-400">Building graph…</p>
          </div>
        </div>
      </div>
    );
  }

  // ── Phase rendering: Error ──

  if (currentPhase === "error") {
    return (
      <div className="graph-view h-screen flex flex-col bg-gray-950 text-gray-100">
        <div className="flex-1 flex items-center justify-center p-8">
          <div className="w-full max-w-md">
            <div className="bg-gray-900/90 border border-red-500/50 rounded-lg p-6">
              <h3 className="text-lg font-semibold text-red-300 mb-2">
                Couldn't load the graph
              </h3>
              <p className="text-sm text-gray-400 mb-4">
                The knowledge graph could not be built.
              </p>
              {errorMsg && (
                <pre className="text-xs text-gray-500 bg-gray-950/50 p-3 rounded mb-4 overflow-x-auto break-all">
                  {errorMsg}
                </pre>
              )}
              <p className="text-xs text-gray-500 mb-4">
                Make sure your vault is accessible and the backend is running.
              </p>
              <button className="btn btn-sm" onClick={() => void loadData()}>
                Retry
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Phase rendering: Empty ──

  if (currentPhase === "empty") {
    return (
      <div className="graph-view h-screen flex flex-col bg-gray-950 text-gray-100">
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center max-w-md">
            <div className="w-16 h-16 mx-auto mb-4 opacity-30">
              <svg
                className="w-full h-full"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M9 12h3m3 0h.01M21 12c0 4.418-4.484 8-10 8a10.017 10.017 0 01-5-1.343m0 0L8 10a4 4 0 017.83-1.117"
                />
              </svg>
            </div>
            <h3 className="text-lg font-medium text-gray-300 mb-2">
              No connections yet
            </h3>
            <p className="text-sm text-gray-500">
              Create a few notes with links and they will appear here as a
              knowledge graph.
            </p>
          </div>
        </div>
      </div>
    );
  }

  // ── Ready phase: render the interactive canvas ──

  const g: GraphData = data!;
  const layoutVal: Layout = layout!;

  // Precompute edge geometry + styling once per render. Every edge comes
  // from the backend graph + the memoized layout — no frontend-generated
  // edges are drawn.
  const edgeSegments: EdgeSegment[] = g.edges
    .filter((e): e is GraphEdgeData => !e.broken)
    .flatMap((e): EdgeSegment[] => {
      const src = layoutVal.positions.get(e.source);
      const tgt = layoutVal.positions.get(e.target);
      if (!src || !tgt) return [];
      const highlighted =
        selectedPath === e.source || selectedPath === e.target;
      return [
        {
          x1: src.x,
          y1: src.y,
          x2: tgt.x,
          y2: tgt.y,
          stroke: highlighted ? "#10b981" : "#4b5563",
          width: highlighted ? 2.5 : 1.5,
          dash: "0",
        },
      ];
    });

  // Relationships of the selected node, for the detail panel.
  const related: string[] = selectedPath
    ? g.edges
        .filter((e): e is GraphEdgeData => !e.broken)
        .flatMap((e) => {
          if (e.source === selectedPath) return [e.target];
          if (e.target === selectedPath) return [e.source];
          return [];
        })
    : [];

  // Pre-filter to nodes that have a layout position, because the map body
  // cannot use `continue`.
  const renderable: Array<{ node: GraphNode; pos: NodePos }> = g.nodes
    .map((node) => {
      const pos = layoutVal.positions.get(node.path);
      return pos ? { node, pos } : null;
    })
    .filter((item): item is { node: GraphNode; pos: NodePos } => item !== null);

  // ── Interaction handlers ──

  const handleWheel = (ev: React.WheelEvent<HTMLDivElement>) => {
    ev.preventDefault();
    const delta = ev.deltaY;
    const factor = delta > 0 ? 0.9 : 1.1;
    const rect = ev.currentTarget.getBoundingClientRect();
    const cx = ev.clientX - rect.left;
    const cy = ev.clientY - rect.top;
    setViewport((cur) => {
      const newZoom = Math.max(0.1, Math.min(4.0, cur.zoom * factor));
      // Keep the canvas point under the cursor stationary.
      const canvasX = (cx - cur.panX) / cur.zoom;
      const canvasY = (cy - cur.panY) / cur.zoom;
      return {
        panX: cx - canvasX * newZoom,
        panY: cy - canvasY * newZoom,
        zoom: newZoom,
        panning: false,
        panStartX: 0,
        panStartY: 0,
      };
    });
  };

  const handleMouseDown = (ev: React.MouseEvent<HTMLDivElement>) => {
    const rect = ev.currentTarget.getBoundingClientRect();
    setViewport((cur) => ({
      ...cur,
      panStartX: ev.clientX - rect.left,
      panStartY: ev.clientY - rect.top,
      panning: true,
    }));
  };

  const handleMouseMove = (ev: React.MouseEvent<HTMLDivElement>) => {
    const rect = ev.currentTarget.getBoundingClientRect();
    const cx = ev.clientX - rect.left;
    const cy = ev.clientY - rect.top;
    setViewport((cur) => {
      if (!cur.panning) return cur;
      const dx = cx - cur.panStartX;
      const dy = cy - cur.panStartY;
      return {
        panX: cur.panX + dx,
        panY: cur.panY + dy,
        panStartX: cx,
        panStartY: cy,
        zoom: cur.zoom,
        panning: true,
      };
    });
  };

  const endPan = () => {
    setViewport((cur) => ({ ...cur, panning: false }));
  };

  const handleFit = () => {
    const gw = Math.max(boundsWidth(layoutVal.bounds), 1);
    const gh = Math.max(boundsHeight(layoutVal.bounds), 1);
    // Fit within ~1200x720 canvas area.
    const fitZoom = Math.max(0.1, Math.min(2.0, Math.min(1200 / gw, 720 / gh) * 0.9));
    const centerX = (layoutVal.bounds.minX + layoutVal.bounds.maxX) / 2;
    const centerY = (layoutVal.bounds.minY + layoutVal.bounds.maxY) / 2;
    setViewport((cur) => ({
      ...cur,
      panX: 600 - centerX * fitZoom,
      panY: 360 - centerY * fitZoom,
      zoom: fitZoom,
    }));
  };

  const handleNodeClick = (path: string) => {
    setSelected(path);
  };

  const handleNodeDoubleClick = (path: string) => {
    ws.openTab(path);
  };

  // ── Render ──

  return (
    <div className="graph-view h-screen flex flex-col bg-gray-950 text-gray-100">
      {/* Main canvas */}
      <div className="flex-1 relative flex overflow-hidden">
        <div
          className="flex-1 relative bg-gray-950 overflow-hidden"
          style={{
            transform: viewportTransform(viewport),
            transformOrigin: "0 0",
          }}
          onWheel={handleWheel}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={endPan}
          onMouseLeave={endPan}
        >
          {/* Grid background (subtle for orientation) */}
          <div
            className="graph-canvas-bg absolute inset-0"
            style={{
              backgroundImage:
                "radial-gradient(circle, #374151 1px, transparent 1px)",
              backgroundSize: "40px 40px",
            }}
          />

          {/* Toolbar overlay on the canvas */}
          <div className="absolute top-3 left-3 z-10 flex gap-1.5">
            <div className="bg-gray-900/90 border border-gray-800 rounded-lg px-2 py-1 text-xs text-gray-400">
              {g.nodes.length} notes · {g.edges.length} links
            </div>
            <button
              className="bg-gray-800/90 border border-gray-700 rounded px-2 py-1 text-xs hover:bg-gray-700 text-gray-300"
              onClick={handleFit}
              aria-label="Fit graph to view"
              title="Fit to screen"
            >
              <svg
                className="inline w-3 h-3 mr-1"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 12m0 0l3-3m-3 3l-3 3m3-3v6m0-6h6"
                />
              </svg>
              Fit
            </button>
          </div>

          {/* Edges (SVG, behind nodes) */}
          <svg
            className="absolute inset-0"
            style={{ width: "100%", height: "100%", overflow: "visible" }}
          >
            {edgeSegments.map((seg, i) => (
              <line
                key={i}
                x1={seg.x1}
                y1={seg.y1}
                x2={seg.x2}
                y2={seg.y2}
                stroke={seg.stroke}
                strokeWidth={seg.width}
                strokeDasharray={seg.dash}
              />
            ))}
            <defs>
              <marker
                id="graph-arrow"
                markerWidth={8}
                markerHeight={8}
                refX={7}
                refY={3.5}
                orient="auto"
                markerUnits="strokeWidth"
              >
                <polygon points="0 0, 8 3.5, 0 7" fill="#6b7280" />
              </marker>
            </defs>
          </svg>

          {/* Nodes (DOM cards) */}
          {renderable.map(({ node, pos }) => {
            const isSelected = selectedPath === node.path;
            const cardClass = isSelected
              ? "absolute rounded-lg px-3 py-2 text-sm font-medium whitespace-nowrap bg-blue-600/20 border-2 border-blue-400 text-white shadow-lg"
              : "absolute rounded-lg px-3 py-2 text-sm font-medium whitespace-nowrap bg-gray-800/80 border border-gray-700 text-gray-300 hover:bg-gray-700/80";
            return (
              <div
                key={node.path}
                className={cardClass}
                style={{ left: `${pos.x}px`, top: `${pos.y}px` }}
                title={node.title}
                onClick={(ev) => {
                  ev.stopPropagation();
                  handleNodeClick(node.path);
                }}
                onDoubleClick={(ev) => {
                  ev.stopPropagation();
                  handleNodeDoubleClick(node.path);
                }}
              >
                <span
                  className={`inline-block w-2 h-2 rounded-full mr-1.5 align-middle ${
                    node.degree > 0 ? "bg-cyan-400" : "bg-gray-500"
                  }`}
                  aria-hidden="true"
                />
                {node.title}
                {node.folder && (
                  <span className="ml-1 text-xs text-gray-500 font-normal">
                    /{node.folder}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* Selection detail panel */}
      {selectedPath && (
        <GraphDetailPanel
          nodes={g.nodes}
          selected={selectedPath}
          related={related}
          onOpen={handleNodeDoubleClick}
        />
      )}
    </div>
  );
}

// ── Detail panel ─────────────────────────────────────────────────────────────

interface GraphDetailPanelProps {
  nodes: GraphNode[];
  selected: string;
  related: string[];
  onOpen: (path: string) => void;
}

/**
 * Right-hand detail panel shown when a graph node is selected.
 *
 * Mirrors: `ui-react/src/components/graph_view.rs` → `GraphDetailPanel`
 */
function GraphDetailPanel({
  nodes,
  selected,
  related,
  onOpen,
}: GraphDetailPanelProps): ReactNode {
  const node = nodes.find((n) => n.path === selected);

  return (
    <div className="flex-none w-72 border-l border-gray-800 bg-gray-900/60 p-4 overflow-y-auto">
      {node && (
        <>
          <div className="text-sm font-semibold text-gray-200 truncate">
            {node.title}
          </div>
          <div className="text-xs text-gray-500 mt-1 break-all">
            {node.path}
          </div>
        </>
      )}

      <div className="h-px bg-gray-800 my-3" />

      <div className="text-xs text-gray-400 mb-1">
        Links ({related.length})
      </div>
      {related.map((rpath) => {
        const rtitle = nodes.find((n) => n.path === rpath)?.title ?? rpath;
        return (
          <div
            key={rpath}
            className="text-xs text-cyan-400 hover:text-cyan-300 cursor-pointer py-0.5 break-all"
          >
            {rtitle}
          </div>
        );
      })}

      {node && (
        <>
          <div className="h-px bg-gray-800 my-3" />
          <div className="grid grid-cols-3 gap-2 text-center text-xs">
            <div />
            <div className="text-lg font-bold text-cyan-400">{node.degree}</div>
            <div className="text-gray-500">degree</div>
            <div />
            <div className="text-lg font-bold text-green-400">
              {node.backlink_count}
            </div>
            <div className="text-gray-500">backlinks</div>
            <div />
            <div className="text-lg font-bold text-orange-400">
              {node.outgoing_count}
            </div>
            <div className="text-gray-500">outgoing</div>
          </div>
        </>
      )}

      <button
        className="mt-4 w-full btn btn-sm"
        onClick={() => onOpen(selected)}
      >
        <svg
          className="inline w-3 h-3 mr-1"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M3 12h2.5a2 2 0 012 2v2.5a2 2 0 002 2H17a2 2 0 002-2V9a2 2 0 00-2-2h-2a2 2 0 00-2-2h-2a2 2 0 00-2 2v6a2 2 0 01-2 2H3"
          />
        </svg>
        Open note
      </button>
    </div>
  );
}
