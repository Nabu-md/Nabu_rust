// ──────────────────────────────────────────────────────────────────────────────
// canvas.tsx — infinite pannable / zoomable workspace
//
// Mirrors: crates/nabu-ui/src/components/canvas.rs (CanvasView + CanvasSurface)
//
// An infinite canvas that *references* existing notes rather than duplicating
// their content. Nodes are positioned cards pointing at vault-relative note
// paths; edges are visual connectors; groups are labelled bounding boxes.
//
// Canvas definitions are persisted as JSON in the settings store
// (`nabu.canvases`) via the `canvas_list` / `canvas_get` / `canvas_save` /
// `canvas_delete` IPC commands.
//
// React patterns: useState + useEffect for lifecycle, useRef for mutable
// interaction state (dragging/panning) and for latest-value tracking so async
// IPC callbacks always read current state, useCallback for handlers,
// pre-computed derived values before JSX.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback, Dispatch, SetStateAction } from "react";
import {
  canvasList,
  canvasGet,
  canvasSave,
  canvasDelete,
} from "../ipc";
import { useNav, useWorkspace, useToast } from "../context";
import type { CanvasDef, CanvasNode, CanvasEdge, NoteIndexEntry } from "../types";
import { Icon } from "./layout/icons";

// ── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_NODE_W = 240;
const DEFAULT_NODE_H = 120;

// ── State enums & classification ─────────────────────────────────────────────

type ListState = "idle" | "loading" | "loaded" | "failed";
type CanvasLoadState = "idle" | "loading" | "loaded" | "failed";
type SaveState = "idle" | "saving" | "saved" | "error";

type ListPhase = "loading" | "empty" | "error" | "loaded";
type CanvasPhase = "select" | "loading" | "error" | "empty" | "ready";

/** Which visual phase the canvas list is in, derived from (state, count). */
function classifyList(state: ListState, count: number): ListPhase {
  switch (state) {
    case "idle":
    case "loading":
      return "loading";
    case "failed":
      return "error";
    case "loaded":
      return count === 0 ? "empty" : "loaded";
  }
}

/** Which visual phase the canvas workspace is in, derived from the load state,
 * the currently loaded canvas, and the active selection. */
function classifyCanvas(
  state: CanvasLoadState,
  canvas: CanvasDef | null,
  activeId: string | null,
): CanvasPhase {
  switch (state) {
    case "idle":
    case "loading":
      return activeId ? "loading" : "select";
    case "failed":
      return "error";
    case "loaded":
      if (canvas && canvas.nodes.length > 0) return "ready";
      if (canvas) return "empty";
      return "select";
  }
}

// ── Pure model helpers ───────────────────────────────────────────────────────

/** Builds a new (unsaved) canvas definition. */
function newCanvasWithId(name: string, id: string): CanvasDef {
  return {
    id,
    name,
    nodes: [],
    edges: [],
    groups: [],
    pan_x: 0,
    pan_y: 0,
    zoom: 1.0,
  };
}

/** A short human-readable summary of a canvas, for the list row subtitle. */
function canvasSummary(c: CanvasDef): string {
  if (c.nodes.length === 0) {
    return "blank canvas";
  }
  return `${c.nodes.length} nodes · ${c.edges.length} connections`;
}

/** Cascading placement for a new node near the viewport centre (canvas coords). */
function nextNodePosition(canvas: CanvasDef, index: number): [number, number] {
  const offset = index * 30;
  const cx = -canvas.pan_x / canvas.zoom + offset;
  const cy = -canvas.pan_y / canvas.zoom + offset;
  return [cx, cy];
}

/** Appends a new note node to the canvas at a cascaded position. */
function createNode(canvas: CanvasDef, entry: NoteIndexEntry): CanvasDef {
  const count = canvas.nodes.length;
  const [x, y] = nextNodePosition(canvas, count);
  const id = `n${count + 1}`;
  return {
    ...canvas,
    nodes: [
      ...canvas.nodes,
      {
        id,
        note_path: entry.path,
        title: entry.title,
        x,
        y,
        width: null,
        height: null,
        kind: "note",
        source: "",
        text: "",
      },
    ],
  };
}

/** Removes a node and every edge attached to it. */
function removeNode(canvas: CanvasDef, nodeId: string): CanvasDef {
  return {
    ...canvas,
    nodes: canvas.nodes.filter((n) => n.id !== nodeId),
    edges: canvas.edges.filter(
      (e) => e.source !== nodeId && e.target !== nodeId,
    ),
  };
}

/** Rendered bounding box of a node (x, y, width, height). */
function nodeViewRect(
  node: CanvasNode,
): [number, number, number, number] {
  const w = node.width ?? DEFAULT_NODE_W;
  const h = node.height ?? DEFAULT_NODE_H;
  return [node.x, node.y, w, h];
}

/** Compute the SVG line endpoints (centres) for an edge's two nodes. */
function edgeEndpoints(
  canvas: CanvasDef,
  edge: CanvasEdge,
): [number, number, number, number] | null {
  const source = canvas.nodes.find((n) => n.id === edge.source);
  const target = canvas.nodes.find((n) => n.id === edge.target);
  if (!source || !target) return null;
  const [sx, sy, sw, sh] = nodeViewRect(source);
  const [tx, ty, tw, th] = nodeViewRect(target);
  return [sx + sw / 2, sy + sh / 2, tx + tw / 2, ty + th / 2];
}

/** Generates a client-side id for a new canvas. */
function generateCanvasId(): string {
  return `canvas-${crypto.randomUUID()}`;
}

// ── State bundle ─────────────────────────────────────────────────────────────

/**
 * All reactive state owned by [`CanvasView`], bundled so the IPC helper
 * functions stay concise. The `Ref` fields hold the latest values so async
 * closures don't capture stale state.
 */
interface CanvasStateBundle {
  setListData: Dispatch<SetStateAction<CanvasDef[]>>;
  setListState: Dispatch<SetStateAction<ListState>>;
  setListError: (v: string) => void;
  setActiveId: Dispatch<SetStateAction<string | null>>;
  setCanvas: Dispatch<SetStateAction<CanvasDef | null>>;
  setCanvasState: Dispatch<SetStateAction<CanvasLoadState>>;
  setCanvasError: (v: string) => void;
  setSaveState: Dispatch<SetStateAction<SaveState>>;
  setSaveError: (v: string | null) => void;
  toasts: ReturnType<typeof useToast>;
  /** Latest listData snapshot for synchronous read inside async closures. */
  listDataRef: { current: CanvasDef[] };
  /** Latest activeId for synchronous read inside async closures. */
  activeIdRef: { current: string | null };
}

// ── IPC helpers ──────────────────────────────────────────────────────────────

/** Loads the list of canvases and, when the list is non-empty and no canvas
 * is active yet, auto-selects the first one. */
function loadList(s: CanvasStateBundle): void {
  s.setListState("loading");
  s.setListError("");

  void (async () => {
    try {
      const list = await canvasList();
      s.listDataRef.current = list;
      s.setListData(list);
      s.setListState("loaded");
      s.setListError("");
      if (list.length > 0) {
        s.setActiveId(list[0].id);
        s.activeIdRef.current = list[0].id;
        selectCanvas(s, list[0].id);
      }
    } catch {
      s.setListError("The canvas list could not be loaded.");
      s.setListState("failed");
      s.toasts.toast(
        "Couldn't load collections: No canvases were returned by the backend.",
        { variant: "error" },
      );
    }
  })();
}

/** Loads a single canvas definition by id. `active_id` is set synchronously so
 * the view shows a loading state while the IPC is in flight. */
function selectCanvas(s: CanvasStateBundle, id: string): void {
  s.setActiveId(id);
  s.activeIdRef.current = id;
  s.setCanvasState("loading");
  s.setCanvasError("");

  void (async () => {
    try {
      const result = await canvasGet(id);
      if (result) {
        s.setCanvas(result);
        s.setCanvasState("loaded");
        s.setCanvasError("");
      } else {
        s.setCanvas(null);
        s.setCanvasState("failed");
        s.setCanvasError(`Canvas "${id}" was not found.`);
      }
    } catch {
      s.setCanvas(null);
      s.setCanvasState("failed");
      s.setCanvasError(`Canvas "${id}" could not be loaded.`);
    }
  })();
}

/** Persists the currently loaded canvas. Sets save_state to Saving while in
 * flight, Saved on success, and Error (with save_error) on failure —
 * surfacing backend failures both in the workspace toolbar and as a toast. */
function saveCanvas(s: CanvasStateBundle, c: CanvasDef): void {
  s.setSaveState("saving");
  s.setSaveError(null);
  const name = c.name;

  void (async () => {
    try {
      await canvasSave(c);
      s.setSaveState("saved");
      s.setSaveError(null);
      s.toasts.toast("Canvas saved: " + name, { variant: "success" });
    } catch {
      const msg = "Could not save canvas";
      s.setSaveState("error");
      s.setSaveError(msg);
      s.toasts.toast("Save failed: " + msg, { variant: "error" });
    }
  })();
}

/** Creates a new canvas: optimistically selects it, then persists it in the
 * background. On persistence failure it reverts the optimistic state. */
function createCanvas(s: CanvasStateBundle, name: string): void {
  if (!name.trim()) {
    s.toasts.toast("Canvas name cannot be empty.", { variant: "error" });
    return;
  }
  const id = generateCanvasId();
  const c = newCanvasWithId(name, id);

  // Optimistically surface the new canvas while it persists in the background.
  s.setActiveId(id);
  s.activeIdRef.current = id;
  s.setCanvas(c);
  s.setCanvasState("loaded");
  s.setCanvasError("");
  s.setSaveState("idle");
  s.setSaveError(null);
  s.setListData((prev) => [...prev, c]);
  s.listDataRef.current = [...s.listDataRef.current, c];

  void (async () => {
    try {
      await canvasSave(c);
      s.setSaveState("saved");
      loadList(s);
      s.toasts.toast("Canvas created: " + name, { variant: "success" });
    } catch {
      s.setSaveState("error");
      s.toasts.toast("Could not create canvas", { variant: "error" });
      s.setActiveId(null);
      s.activeIdRef.current = null;
      s.setCanvas(null);
      s.setCanvasState("failed");
      s.setListData((prev) => prev.filter((x) => x.id !== id));
      s.listDataRef.current = s.listDataRef.current.filter((x) => x.id !== id);
    }
  })();
}

/** Deletes a canvas by id, refreshing the list and switching the active
 * canvas when the deleted one was the one being edited. */
function deleteCanvas(s: CanvasStateBundle, id: string): void {
  const name = s.listDataRef.current.find((c) => c.id === id)?.name ?? id;
  const wasActive = s.activeIdRef.current === id;
  const nextId = wasActive
    ? s.listDataRef.current.find((c) => c.id !== id)?.id ?? null
    : null;

  void (async () => {
    try {
      await canvasDelete(id);
      s.setListData((prev) => prev.filter((c) => c.id !== id));
      s.listDataRef.current = s.listDataRef.current.filter((c) => c.id !== id);
      if (wasActive) {
        s.setActiveId(null);
        s.activeIdRef.current = null;
        s.setCanvas(null);
        s.setCanvasState("idle");
        s.setCanvasError("");
        if (nextId) {
          s.setActiveId(nextId);
          s.activeIdRef.current = nextId;
          selectCanvas(s, nextId);
        }
      }
      s.toasts.toast("Canvas deleted: " + name, { variant: "success" });
    } catch {
      s.toasts.toast("Could not delete canvas: " + name, { variant: "error" });
    }
  })();
}

// ── Component ────────────────────────────────────────────────────────────────

/**
 * The Canvas view component.
 *
 * Loads canvases from the backend (`canvas_list`), lets the user select one
 * (`canvas_get`), and renders an infinite pannable/zoomable surface with
 * note nodes, edge connectors, and groups. Edits are persisted via
 * `canvas_save`. New canvases are created and deleted via the list toolbar.
 */
export function CanvasView() {
  const nav = useNav();
  const workspace = useWorkspace();
  const toasts = useToast();

  // ── Reactive state ──
  const [listData, setListData] = useState<CanvasDef[]>([]);
  const [listState, setListState] = useState<ListState>("idle");
  const [listError, setListError] = useState("");

  const [activeId, setActiveId] = useState<string | null>(null);
  const [canvas, setCanvas] = useState<CanvasDef | null>(null);
  const [canvasState, setCanvasState] = useState<CanvasLoadState>("idle");
  const [canvasError, setCanvasError] = useState("");

  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [saveError, setSaveError] = useState<string | null>(null);

  const [showNewDialog, setShowNewDialog] = useState(false);
  const [newCanvasName, setNewCanvasName] = useState("");

  // ── Refs for latest-value tracking inside async closures ──
  const listDataRef = useRef<CanvasDef[]>([]);
  const activeIdRef = useRef<string | null>(null);

  // Keep refs in sync with state.
  useEffect(() => {
    listDataRef.current = listData;
  }, [listData]);
  useEffect(() => {
    activeIdRef.current = activeId;
  }, [activeId]);

  // ── Build the state bundle ──
  const s: CanvasStateBundle = {
    setListData,
    setListState,
    setListError,
    setActiveId,
    setCanvas,
    setCanvasState,
    setCanvasError,
    setSaveState,
    setSaveError,
    toasts,
    listDataRef,
    activeIdRef,
  };

  // ── Initial list load on mount ──
  const initializedRef = useRef(false);
  useEffect(() => {
    if (initializedRef.current) return;
    initializedRef.current = true;
    loadList(s);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Derived classification (precompute before JSX) ──
  const listPhase = classifyList(listState, listData.length);
  const listErr = listError;
  const rows = listData;
  const active = activeId;
  const selCanvas = canvas;
  const canvasPhase = classifyCanvas(canvasState, selCanvas, active);
  const canvasErr = canvasError;
  const notesIndex: NoteIndexEntry[] = nav.notesIndex;

  // ── Handlers ──

  const onListRetry = useCallback(() => {
    loadList(s);
  }, [s]);

  const onCanvasRetry = useCallback(() => {
    if (activeIdRef.current) {
      selectCanvas(s, activeIdRef.current);
    }
  }, [s]);

  const onSelectCanvas = useCallback(
    (id: string) => {
      selectCanvas(s, id);
    },
    [s],
  );

  const onSave = useCallback(() => {
    if (canvas) {
      saveCanvas(s, canvas);
    }
  }, [canvas, s]);

  const onOpenNote = useCallback(
    (path: string) => {
      workspace.openTab(path);
      nav.setViewMode("Editor");
    },
    [workspace, nav],
  );

  const onRemoveNode = useCallback(
    (nodeId: string) => {
      if (!canvas) return;
      const updated = removeNode(canvas, nodeId);
      setCanvas(updated);
      saveCanvas(s, updated);
    },
    [canvas, s],
  );

  const handleDeleteCanvas = useCallback(
    (id: string) => {
      deleteCanvas(s, id);
    },
    [s],
  );

  const handleCreateCanvas = useCallback(() => {
    createCanvas(s, newCanvasName);
    setShowNewDialog(false);
    setNewCanvasName("");
  }, [s, newCanvasName]);

  // ── Pre-compute list phase node ──
  let listPhaseNode: React.ReactNode;
  switch (listPhase) {
    case "loading":
      listPhaseNode = (
        <div className="p-3">
          <div className="animate-pulse space-y-2">
            {[...Array(6)].map((_, i) => (
              <div
                key={i}
                className="h-4 bg-gray-700 rounded"
                style={{ width: `${80 - i * 10}%` }}
              />
            ))}
          </div>
        </div>
      );
      break;
    case "error":
      listPhaseNode = (
        <div className="p-3">
          <div className="text-red-300">
            <div className="font-medium">Couldn't load canvases</div>
            <div className="text-sm mt-1">{listErr}</div>
            <button
              type="button"
              className="mt-2 px-2 py-1 text-xs text-blue-400 hover:text-blue-300"
              onClick={onListRetry}
            >
              Retry
            </button>
          </div>
          <div className="text-xs text-gray-500 mt-1">
            Make sure your vault is accessible and try again.
          </div>
        </div>
      );
      break;
    case "empty":
      listPhaseNode = (
        <div className="px-3 py-2 text-xs text-gray-500">
          No canvases yet — create one with the + button above.
        </div>
      );
      break;
    case "loaded":
      listPhaseNode = (
        <>
          {rows.map((c) => {
            const id = c.id;
            const name = c.name;
            const summary = canvasSummary(c);
            const isActive = active === id;
            const rowClass = isActive
              ? "flex items-center justify-between px-3 py-1.5 cursor-pointer bg-gray-800 border-l-2 border-blue-500 rounded"
              : "flex items-center justify-between px-3 py-1.5 cursor-pointer hover:bg-gray-800 rounded";

            return (
              <div
                key={id}
                className={rowClass}
                onDoubleClick={() => onSelectCanvas(id)}
              >
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-gray-200 truncate">
                    {name}
                  </div>
                  <div className="text-xs text-gray-500 truncate">
                    {summary}
                  </div>
                </div>
                <button
                  type="button"
                  className="ml-2 text-xs text-gray-500 hover:text-red-400"
                  aria-label={`Delete canvas ${name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDeleteCanvas(id);
                  }}
                >
                  <Icon name="trash2" className="w-4 h-4" />
                </button>
              </div>
            );
          })}
        </>
      );
      break;
  }

  // ── Pre-compute note palette node ──
  let paletteNode: React.ReactNode;
  if (notesIndex.length === 0) {
    paletteNode = (
      <div className="px-3 py-2 text-xs text-gray-400">
        No notes indexed to place yet.
      </div>
    );
  } else {
    paletteNode = (
      <>
        {notesIndex.map((entry) => {
          const title = entry.title;
          return (
            <div
              key={entry.path}
              className="px-3 py-1.5 text-sm cursor-pointer hover:bg-gray-800 truncate"
              title="Double-click to add to canvas"
              onDoubleClick={() => {
                if (canvas) {
                  const updated = createNode(canvas, entry);
                  setCanvas(updated);
                  saveCanvas(s, updated);
                } else {
                  toasts.toast(
                    "No canvas selected: select a canvas before placing notes on it.",
                    { variant: "info" },
                  );
                }
              }}
            >
              {title}
            </div>
          );
        })}
      </>
    );
  }

  // ── Pre-compute canvas phase node ──
  let canvasPhaseNode: React.ReactNode;
  switch (canvasPhase) {
    case "select":
      canvasPhaseNode = (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="text-center">
            <Icon
              name="palette"
              className="w-8 h-8 mx-auto mb-2 text-gray-600"
            />
            <div className="text-gray-400">No canvas selected</div>
            <div className="text-xs text-gray-600 mt-1">
              Click a canvas in the list to open it, or create a new one.
            </div>
          </div>
        </div>
      );
      break;
    case "loading":
      canvasPhaseNode = (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="w-6 h-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
        </div>
      );
      break;
    case "error":
      canvasPhaseNode = (
        <div className="absolute inset-0 flex items-center justify-center p-8">
          <div className="w-full max-w-md text-center">
            <div className="text-red-300">
              <div className="font-medium">Couldn't load the canvas</div>
              <div className="text-sm mt-1">{canvasErr}</div>
              <button
                type="button"
                className="mt-2 px-2 py-1 text-xs text-blue-400 hover:text-blue-300"
                onClick={onCanvasRetry}
              >
                Retry
              </button>
            </div>
            <div className="text-xs text-gray-500 mt-1">
              Try selecting another canvas or create a new one.
            </div>
          </div>
        </div>
      );
      break;
    case "empty":
    case "ready":
      canvasPhaseNode = canvas ? (
        <CanvasSurface
          canvas={canvas}
          saveState={saveState}
          saveError={saveError}
          onSave={onSave}
          onOpenNote={onOpenNote}
          onRemoveNode={onRemoveNode}
          setCanvas={setCanvas}
        />
      ) : null;
      break;
  }

  return (
    <div className="canvas-view flex h-full bg-gray-950 text-gray-100 overflow-hidden">
      {/* Left sidebar: canvas list + note palette */}
      <div className="flex-none w-64 border-r border-gray-800 flex flex-col h-screen">
        <div className="flex items-center justify-between px-3 py-2 border-b border-gray-800">
          <h2 className="text-sm font-semibold text-gray-300">Canvases</h2>
          <button
            type="button"
            className="px-2 py-1 text-xs bg-blue-600 rounded hover:bg-blue-500"
            aria-label="New canvas"
            onClick={() => setShowNewDialog(true)}
          >
            <Icon name="plus" className="w-4 h-4 inline" /> New
          </button>
        </div>

        <div className="overflow-y-auto max-h-64 border-b border-gray-800">
          {listPhaseNode}
        </div>

        {/* Note palette */}
        <div className="flex-1 overflow-y-auto">
          <div className="px-3 py-2 text-xs text-gray-500 uppercase tracking-wide">
            Notes
          </div>
          {paletteNode}
        </div>
      </div>

      {/* Canvas workspace */}
      <div className="flex-1 relative overflow-hidden bg-gray-950">
        {canvasPhaseNode}
      </div>

      {/* New canvas dialog */}
      {showNewDialog && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-gray-800 rounded-lg border border-gray-700 p-4 w-80">
            <h3 className="text-sm font-medium text-gray-200 mb-3">
              New Canvas
            </h3>
            <input
              type="text"
              placeholder="Canvas name"
              className="input w-full mt-1 mb-3"
              value={newCanvasName}
              onChange={(e) => setNewCanvasName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleCreateCanvas();
                if (e.key === "Escape") setShowNewDialog(false);
              }}
              autoFocus
            />
            <div className="flex gap-2 justify-end">
              <button
                type="button"
                className="px-3 py-1 text-xs rounded text-gray-400 hover:text-gray-200"
                onClick={() => setShowNewDialog(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="px-3 py-1 text-xs rounded bg-blue-600 text-white hover:bg-blue-500"
                onClick={handleCreateCanvas}
              >
                Create
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── CanvasSurface ────────────────────────────────────────────────────────────

interface CanvasSurfaceProps {
  canvas: CanvasDef;
  saveState: SaveState;
  saveError: string | null;
  onSave: () => void;
  onOpenNote: (path: string) => void;
  onRemoveNode: (nodeId: string) => void;
  setCanvas: (c: CanvasDef | null) => void;
}

/**
 * The interactive canvas surface: a pannable/zoomable transform container with
 * SVG edge connectors, positioned DOM node cards, and a screen-fixed toolbar.
 */
function CanvasSurface({
  canvas,
  saveState,
  saveError,
  onSave,
  onOpenNote,
  onRemoveNode,
  setCanvas,
}: CanvasSurfaceProps) {
  // Interaction state (owned by the surface, not persisted on its own).
  const draggingRef = useRef<string | null>(null);
  const dragOriginRef = useRef<[number, number, number, number]>([0, 0, 0, 0]);
  const panningRef = useRef(false);
  const panOriginRef = useRef<[number, number]>([0, 0]);
  const panBaseRef = useRef<[number, number]>([0, 0]);

  const c = canvas;
  const nodes = c.nodes;
  const transform = `translate(${c.pan_x}px, ${c.pan_y}px) scale(${c.zoom})`;
  const zoomPct = Math.round(c.zoom * 100);

  // Precompute edge geometry once per render.
  const edgeSegs = c.edges
    .map((e): [string, number, number, number, number] | null => {
      const pts = edgeEndpoints(c, e);
      if (!pts) return null;
      return [e.id, pts[0], pts[1], pts[2], pts[3]];
    })
    .filter(
      (s): s is [string, number, number, number, number] => s !== null,
    );

  // ── Pre-compute save indicator ──
  let saveIndicator: React.ReactNode;
  switch (saveState) {
    case "idle":
      saveIndicator = <span className="text-xs text-gray-500">Saved</span>;
      break;
    case "saving":
      saveIndicator = (
        <span className="flex items-center gap-1 text-xs text-amber-400">
          <div className="w-3 h-3 animate-spin rounded-full border border-current border-t-transparent" />
          Saving…
        </span>
      );
      break;
    case "saved":
      saveIndicator = (
        <span className="flex items-center gap-1 text-xs text-green-400">
          <Icon name="check" className="w-3 h-3" /> Saved
        </span>
      );
      break;
    case "error":
      saveIndicator = (
        <span className="flex items-center gap-1 text-xs text-red-400">
          <Icon name="x" className="w-3 h-3" /> Save failed
        </span>
      );
      break;
  }

  const saveErrIndicator = saveError ? (
    <span
      className="text-xs text-red-300 max-w-40 truncate"
      title={saveError}
    >
      {saveError}
    </span>
  ) : null;

  // ── Event handlers ──

  const handleWheel = useCallback(
    (ev: React.WheelEvent) => {
      ev.preventDefault();
      const delta = ev.deltaY;
      const factor = delta > 0 ? 0.9 : 1.1;
      const updated = {
        ...c,
        zoom: Math.max(0.1, Math.min(5.0, c.zoom * factor)),
      };
      setCanvas(updated);
      onSave();
    },
    [c, setCanvas, onSave],
  );

  const handleMouseDown = useCallback(
    (ev: React.MouseEvent) => {
      const px = c.pan_x;
      const py = c.pan_y;
      panOriginRef.current = [ev.clientX, ev.clientY];
      panBaseRef.current = [px, py];
      panningRef.current = true;
      draggingRef.current = null;
    },
    [c],
  );

  const handleMouseMove = useCallback(
    (ev: React.MouseEvent) => {
      const nodeId = draggingRef.current;
      if (nodeId) {
        const [sx, sy, nx, ny] = dragOriginRef.current;
        const zoom = c.zoom;
        const dx = (ev.clientX - sx) / zoom;
        const dy = (ev.clientY - sy) / zoom;
        const updated = {
          ...c,
          nodes: c.nodes.map((n) =>
            n.id === nodeId ? { ...n, x: nx + dx, y: ny + dy } : n,
          ),
        };
        setCanvas(updated);
      } else if (panningRef.current) {
        const [ox, oy] = panOriginRef.current;
        const [bx, by] = panBaseRef.current;
        const dx = ev.clientX - ox;
        const dy = ev.clientY - oy;
        const updated = {
          ...c,
          pan_x: bx + dx,
          pan_y: by + dy,
        };
        setCanvas(updated);
      }
    },
    [c, setCanvas],
  );

  const handleMouseUp = useCallback(
    (_ev: React.MouseEvent) => {
      const wasDragging = draggingRef.current !== null || panningRef.current;
      draggingRef.current = null;
      panningRef.current = false;
      if (wasDragging) {
        onSave();
      }
    },
    [onSave],
  );

  const handleMouseLeave = useCallback(() => {
    draggingRef.current = null;
    panningRef.current = false;
  }, []);

  const handleNodeMouseDown = useCallback(
    (ev: React.MouseEvent, nodeId: string, nx: number, ny: number) => {
      ev.stopPropagation();
      dragOriginRef.current = [ev.clientX, ev.clientY, nx, ny];
      draggingRef.current = nodeId;
      panningRef.current = false;
    },
    [],
  );

  // Toolbar handlers
  const handleZoomOut = useCallback(() => {
    const updated = {
      ...c,
      zoom: Math.max(0.1, c.zoom * 0.8),
    };
    setCanvas(updated);
    onSave();
  }, [c, setCanvas, onSave]);

  const handleResetView = useCallback(() => {
    const updated = {
      ...c,
      pan_x: 0,
      pan_y: 0,
      zoom: 1.0,
    };
    setCanvas(updated);
    onSave();
  }, [c, setCanvas, onSave]);

  const handleZoomIn = useCallback(() => {
    const updated = {
      ...c,
      zoom: Math.max(0.1, Math.min(5.0, c.zoom * 1.2)),
    };
    setCanvas(updated);
    onSave();
  }, [c, setCanvas, onSave]);

  return (
    <>
      {/* Pannable/zoomable transform container */}
      <div
        className="absolute inset-0"
        style={{
          transform,
          transformOrigin: "0 0",
        }}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseLeave}
      >
        {/* Subtle grid for orientation */}
        <div
          className="canvas-grid absolute inset-0"
          style={{
            backgroundImage:
              "radial-gradient(circle, #374151 1px, transparent 1px)",
            backgroundSize: "40px 40px",
          }}
        />

        {/* Groups (rendered behind nodes) */}
        {c.groups.map((g) => (
          <div
            key={g.id}
            className="absolute border-2 border-dashed border-gray-700 rounded-lg bg-gray-800/20"
            style={{
              left: g.x,
              top: g.y,
              width: g.width,
              height: g.height,
            }}
          />
        ))}

        {/* Edges (SVG connectors), behind nodes */}
        <svg
          className="absolute inset-0"
          style={{ width: "100%", height: "100%", overflow: "visible" }}
        >
          {edgeSegs.map(([eid, x1, y1, x2, y2]) => (
            <line
              key={eid}
              x1={x1}
              y1={y1}
              x2={x2}
              y2={y2}
              stroke="#4b5563"
              strokeWidth={2}
              fill="none"
            />
          ))}
          <defs>
            <marker
              id="canvas-arrow"
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
        {nodes.map((node) => {
          const w = node.width ?? DEFAULT_NODE_W;
          const h = node.height ?? DEFAULT_NODE_H;
          return (
            <div
              key={node.id}
              className="absolute bg-gray-800 border border-gray-600 rounded-lg shadow-lg cursor-move hover:border-blue-500 transition-colors"
              style={{
                left: node.x,
                top: node.y,
                minWidth: 180,
                width: w,
                minHeight: h,
              }}
              title={node.title}
              onMouseDown={(ev) =>
                handleNodeMouseDown(ev, node.id, node.x, node.y)
              }
              onDoubleClick={() => onOpenNote(node.note_path)}
            >
              <div className="flex items-center justify-between px-2 py-1 border-gray-700">
                <span
                  className="text-xs font-medium text-gray-300 truncate"
                  title={node.note_path}
                >
                  {node.title}
                </span>
                <button
                  type="button"
                  className="text-xs text-gray-500 hover:text-red-400"
                  aria-label="Remove node"
                  onMouseDown={(ev) => ev.stopPropagation()}
                  onClick={() => onRemoveNode(node.id)}
                >
                  <Icon name="x" className="w-3 h-3" />
                </button>
              </div>
              <div className="px-2 py-1 text-xs text-gray-500">
                {node.kind}
              </div>
            </div>
          );
        })}
      </div>

      {/* Screen-fixed toolbar (not transformed) */}
      <div className="absolute top-3 right-3 z-20 flex items-center gap-1.5 bg-gray-800/90 border border-gray-700 rounded-lg px-2 py-1">
        <button
          type="button"
          className="w-7 h-7 flex items-center justify-center hover:bg-gray-700 rounded"
          aria-label="Zoom out"
          onClick={handleZoomOut}
        >
          −
        </button>
        <button
          type="button"
          className="w-7 h-7 flex items-center justify-center hover:bg-gray-700 rounded"
          aria-label="Reset view"
          onClick={handleResetView}
        >
          ◯
        </button>
        <button
          type="button"
          className="w-7 h-7 flex items-center justify-center hover:bg-gray-700 rounded"
          aria-label="Zoom in"
          onClick={handleZoomIn}
        >
          +
        </button>
        <div className="h-4 w-px bg-gray-700" />
        {saveIndicator}
        {saveErrIndicator}
        <span className="text-xs text-gray-500 w-10 text-right">
          {zoomPct}%
        </span>
      </div>
    </>
  );
}
