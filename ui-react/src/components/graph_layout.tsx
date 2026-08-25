// ──────────────────────────────────────────────────────────────────────────────
// components/graph_layout.tsx — pure, deterministic positioning for the knowledge graph
//
// Mirrors: crates/nabu-ui/src/components/graph_layout.rs
//
// The layout engine is intentionally dependency-free and runs on the main
// thread only when graph data changes (never on every render frame). It turns
// a `GraphData` payload into a set of fixed `(x, y)` positions keyed by node
// path, producing a usable static layout:
//
// * a few nodes — well-spaced, readable;
// * many nodes — spread out so they don't overlap;
// * high-degree nodes — spring repulsion prevents clustering;
// * disconnected components — each placed independently on a virtual grid.
//
// There is **no animation loop**: positions are computed once per data load
// and stay fixed. Pan/zoom is handled by the view via CSS transforms.
// ──────────────────────────────────────────────────────────────────────────────

import type { GraphData } from "../types";

// ── Types ────────────────────────────────────────────────────────────────────

/** Position of a node after layout. */
export interface NodePos {
  x: number;
  y: number;
}

/** Result of laying out a graph: a position per node path + bounding box. */
export interface Layout {
  /** Node path → position. */
  positions: Map<string, NodePos>;
  /** Logical bounds of the laid-out graph in canvas coordinates. */
  bounds: Bounds;
}

export interface Bounds {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
}

export function boundsWidth(b: Bounds): number {
  return b.maxX - b.minX;
}

export function boundsHeight(b: Bounds): number {
  return b.maxY - b.minY;
}

// ── Layout constants (mirrors graph_layout.rs) ─────────────────────────────

const NODE_SPACING = 220.0; // ideal distance between connected nodes
const REPULSION_RADIUS = 80.0; // distance at which node-node repulsion fades
const REPULSION_STRENGTH = 4000.0; // repulsion impulse scale
const ATTRACTION_STRENGTH = 0.06; // spring pull toward ideal edge length
const RELAX_ITERATIONS = 40; // fixed relaxation passes — no animation loop
const COMPONENT_GAP = 900.0; // spacing between disconnected components

// ── Entry point ──────────────────────────────────────────────────────────────

/**
 * Compute a full layout for the given graph data.
 *
 * This is the single entry point the view calls when data loads. The result
 * is cached by the caller (via useMemo) for the lifetime of the data so the
 * expensive work is done once per load, not per frame.
 */
export function layoutGraph(data: GraphData): Layout {
  if (data.nodes.length === 0) {
    return {
      positions: new Map(),
      bounds: { minX: 0, maxX: 0, minY: 0, maxY: 0 },
    };
  }

  // Index nodes by path for O(1) lookup. Edges use paths as identities.
  const positions = new Map<string, NodePos>();
  const adjacency = new Map<string, string[]>();
  const degree = new Map<string, number>();
  for (const node of data.nodes) {
    positions.set(node.path, { x: 0, y: 0 });
    adjacency.set(node.path, []);
    degree.set(node.path, node.degree);
  }

  // Build adjacency from real edges. Broken edges are skipped.
  const nodeSet: Set<string> = new Set(positions.keys());
  const realEdges: Array<[string, string]> = [];
  for (const edge of data.edges) {
    if (edge.broken) continue;
    if (nodeSet.has(edge.source) && nodeSet.has(edge.target)) {
      adjacency.get(edge.source)!.push(edge.target);
      adjacency.get(edge.target)!.push(edge.source);
      realEdges.push([edge.source, edge.target]);
    }
  }

  // --- Connected components ---
  const nodePaths = Array.from(positions.keys());
  const components = connectedComponents(nodePaths, adjacency);

  // --- Layout each component independently ---
  const posValues = Array.from(positions.values());
  const compPositions = layoutComponents(components, adjacency, realEdges, degree);

  // Place components on a virtual grid.
  let originX = 0;
  let originY = 0;
  let rowHeight = 0;
  let columnWidth = 0;
  const perRow = 3; // components per row before wrapping
  let placed = 0;

  for (let ci = 0; ci < components.length; ci++) {
    const comp = components[ci];
    const compPos = compPositions[ci];

    // Compute this component's local bounds.
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const path of comp) {
      const p = compPos.get(path);
      if (p) {
        minX = Math.min(minX, p.x);
        maxX = Math.max(maxX, p.x);
        minY = Math.min(minY, p.y);
        maxY = Math.max(maxY, p.y);
      }
    }
    const w = maxX - minX;
    const h = maxY - minY;

    // Translate so the component's top-left is at (originX, originY).
    const dx = originX - minX;
    const dy = originY - minY;
    for (const path of comp) {
      const p = compPos.get(path);
      if (p) {
        const pos = positions.get(path)!;
        pos.x = p.x + dx;
        pos.y = p.y + dy;
      }
    }

    rowHeight = Math.max(rowHeight, h);
    columnWidth = Math.max(columnWidth, w);
    placed += 1;

    if (placed >= perRow) {
      // Wrap to a new row.
      originX = 0;
      originY += rowHeight + COMPONENT_GAP;
      rowHeight = 0;
      columnWidth = 0;
      placed = 0;
    } else {
      originX += columnWidth + COMPONENT_GAP;
    }
  }

  // --- Compute global bounds for viewport centering ---
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const p of posValues) {
    minX = Math.min(minX, p.x);
    maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
  }

  // Guard against all-zero (single node at origin).
  if (!isFinite(minX)) minX = 0;
  if (!isFinite(maxX)) maxX = 0;
  if (!isFinite(minY)) minY = 0;
  if (!isFinite(maxY)) maxY = 0;

  return {
    positions,
    bounds: { minX, maxX, minY, maxY },
  };
}

// ── Connected components (BFS over adjacency) ────────────────────────────────

function connectedComponents(
  nodes: string[],
  adjacency: Map<string, string[]>
): string[][] {
  const visited = new Set<string>();
  const components: string[][] = [];

  for (const start of nodes) {
    if (visited.has(start)) continue;
    const comp: string[] = [];
    const queue: string[] = [start];
    visited.add(start);
    while (queue.length > 0) {
      const current = queue.shift()!;
      comp.push(current);
      for (const neighbor of adjacency.get(current) ?? []) {
        if (visited.has(neighbor)) continue;
        visited.add(neighbor);
        queue.push(neighbor);
      }
    }
    comp.sort(); // deterministic component ordering
    components.push(comp);
  }

  return components;
}

// ── Per-component layout ────────────────────────────────────────────────────

/**
 * Lay out a single connected component.
 *
 * Uses a layered approach: pick the highest-degree node as the root, fan its
 * neighbours in a circle, then relax with a fixed number of spring-physics
 * passes (attraction along edges, repulsion between nearby nodes).
 */
function layoutComponent(
  nodes: string[],
  adjacency: Map<string, string[]>,
  edges: Array<[string, string]>,
  degree: Map<string, number>
): Map<string, NodePos> {
  const pos = new Map<string, NodePos>();

  if (nodes.length === 0) return pos;

  if (nodes.length === 1) {
    pos.set(nodes[0], { x: 0, y: 0 });
    return pos;
  }

  // Choose root: highest degree (ties broken by path for determinism).
  const root = nodes.reduce((best, p) => {
    const bestDeg = degree.get(best)?.valueOf() ?? 0;
    const pDeg = degree.get(p)?.valueOf() ?? 0;
    if (pDeg > bestDeg || (pDeg === bestDeg && p < best)) return p;
    return best;
  });
  // Fallback if all degrees are equal
  const rootPath = root ?? nodes[0];

  // BFS to assign layers (shortest path from root).
  const layer = new Map<string, number>();
  const queue: string[] = [rootPath];
  layer.set(rootPath, 0);
  while (queue.length > 0) {
    const current = queue.shift()!;
    const l = layer.get(current)! + 1;
    for (const neighbor of adjacency.get(current) ?? []) {
      if (!layer.has(neighbor)) {
        layer.set(neighbor, l);
        queue.push(neighbor);
      }
    }
  }

  // Initial placement: each layer on a concentric circle around the root.
  const maxLayer = Math.max(...Array.from(layer.values()), 0);

  // Group nodes by layer.
  const byLayer = new Map<number, string[]>();
  for (const [path, l] of layer) {
    if (!byLayer.has(l)) byLayer.set(l, []);
    byLayer.get(l)!.push(path);
  }
  for (const group of byLayer.values()) {
    group.sort(); // deterministic ordering
  }

  pos.set(rootPath, { x: 0, y: 0 });

  const layerRadius = NODE_SPACING * 0.9;
  for (let l = 1; l <= maxLayer; l++) {
    const group = byLayer.get(l);
    if (!group) continue;
    const radius = layerRadius * l;
    const n = group.length;
    for (let i = 0; i < n; i++) {
      const path = group[i];
      // Stagger so nodes don't stack. Golden-ratio-ish angle offset per layer.
      const angle = (2 * Math.PI * i) / n + l * 0.7;
      const jiggle = (l * 13) * Math.sin(l * 13);
      pos.set(path, {
        x: radius * Math.cos(angle) + jiggle,
        y: radius * Math.sin(angle) - jiggle,
      });
    }
  }

  // Spring relaxation (fixed iterations).
  relax(pos, edges, REPULSION_RADIUS, REPULSION_STRENGTH, ATTRACTION_STRENGTH, NODE_SPACING, RELAX_ITERATIONS);

  return pos;
}

/**
 * Lay out all components and return their pre-placement positions.
 */
function layoutComponents(
  components: string[][],
  adjacency: Map<string, string[]>,
  edges: Array<[string, string]>,
  degree: Map<string, number>
): Map<string, NodePos>[] {
  return components.map((comp) => layoutComponent(comp, adjacency, edges, degree));
}

// ── Spring relaxation ────────────────────────────────────────────────────────

/**
 * Fixed-iteration spring relaxation. No animation, no requestAnimationFrame.
 *
 * Uses index-based displacement arrays so indexed access is straightforward
 * in JS (no borrow checker, but same algorithmic structure as the Rust spec).
 */
function relax(
  positions: Map<string, NodePos>,
  edges: Array<[string, string]>,
  repulsionRadius: number,
  repulsionStrength: number,
  attractionStrength: number,
  idealLength: number,
  iterations: number
): void {
  if (positions.size === 0 || iterations === 0) return;

  // Stable ordering so indices map deterministically to paths.
  const paths = Array.from(positions.keys()).sort();
  const n = paths.length;
  const indexOf = new Map<string, number>();
  for (let i = 0; i < n; i++) indexOf.set(paths[i], i);

  // Current positions as a parallel array for fast indexed access.
  const pts: NodePos[] = paths.map((p) => positions.get(p)!);
  // Displacement accumulator, indexed.
  const disp: Array<[number, number]> = Array.from({ length: n }, () => [0, 0]);

  for (let iter = 0; iter < iterations; iter++) {
    // Reset displacement.
    for (let i = 0; i < n; i++) {
      disp[i][0] = 0;
      disp[i][1] = 0;
    }

    // Repulsion: all pairs within radius.
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const dx = pts[i].x - pts[j].x;
        const dy = pts[i].y - pts[j].y;
        const distSq = dx * dx + dy * dy;
        let fx: number;
        let fy: number;
        if (distSq < 1.0) {
          // Coincident nodes — nudge apart deterministically.
          fx = 1.0;
          fy = 0.0;
        } else if (distSq > repulsionRadius * repulsionRadius) {
          continue;
        } else {
          const dist = Math.sqrt(distSq);
          const force = repulsionStrength / (dist * dist);
          fx = (dx / dist) * force;
          fy = (dy / dist) * force;
        }
        disp[i][0] += fx;
        disp[i][1] += fy;
        disp[j][0] -= fx;
        disp[j][1] -= fy;
      }
    }

    // Attraction along edges.
    for (const [src, tgt] of edges) {
      const si = indexOf.get(src);
      const ti = indexOf.get(tgt);
      if (si === undefined || ti === undefined) continue;
      const dx = pts[si].x - pts[ti].x;
      const dy = pts[si].y - pts[ti].y;
      const distSq = dx * dx + dy * dy;
      if (distSq < 1.0) continue;
      const dist = Math.sqrt(distSq);
      const force = attractionStrength * (dist - idealLength);
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      disp[si][0] -= fx;
      disp[si][1] -= fy;
      disp[ti][0] += fx;
      disp[ti][1] += fy;
    }

    // Apply displacement with cooling (gradually dampened).
    const cooling = 0.1 + 0.02 * iterations;
    for (let i = 0; i < n; i++) {
      const [dx, dy] = disp[i];
      const mag = Math.sqrt(dx * dx + dy * dy);
      const limit = Math.min(mag === 0 ? 0.01 : mag, cooling);
      if (mag < 0.01) {
        // Coincident — apply minimal deterministic nudge.
        pts[i].x += 1.0;
        pts[i].y += 1.0;
      } else {
        pts[i].x += (dx / mag) * limit;
        pts[i].y += (dy / mag) * limit;
      }
    }
  }

  // Write back.
  for (let i = 0; i < n; i++) {
    positions.set(paths[i], { x: pts[i].x, y: pts[i].y });
  }
}
