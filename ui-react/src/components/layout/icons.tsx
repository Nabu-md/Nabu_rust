// ──────────────────────────────────────────────────────────────────────────────
// icons.tsx — Lightweight inline-SVG icon system for the app shell
//
// Uses Heroicons 2 outline (stroke) style paths. Each icon is rendered
// with fill="none" and stroke="currentColor", making them automatically
// colour-inheriting and resolution-independent.
// ──────────────────────────────────────────────────────────────────────────────

import type { SVGProps } from "react";

/** Map of icon name → array of SVG path "d" strings (supports multi-subpath icons). */
const ICONS: Record<string, string[]> = {
  // ── Folders & files ─────────────────────────────────────
  folder: [
    "M3 6a3 3 0 0 1 3-3h2.586a1 1 0 0 1 .707.293l1.414 1.414A1 1 0 0 0 8.414 5H18a3 3 0 0 1 3 3v8a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3V6z",
  ],
  folderOpen: [
    "M5 7a2 2 0 0 1 2-2h2.586a1 1 0 0 1 .707.293l1.414 1.414A1 1 0 0 0 11.414 7H17a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V7z",
  ],
  fileText: [
    "M9 1h6a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2V3a2 2 0 0 1 2-2z",
  ],

  // ── Navigation ──────────────────────────────────────────
  search: ["M21 21l-4.35-4.35M5 11a6 6 0 1 1 12 0 6 6 0 0 1-12 0z"],
  command: ["M6 9l6-6 6 6", "M12 3v18"],
  graph: [
    "M4 16a4 4 0 1 1 8 0 4 4 0 0 1-8 0zm12 0a4 4 0 1 1 8 0 4 4 0 0 1-8 0zm-6 0a4 4 0 1 1 8 0 4 4 0 0 1-8 0z",
  ],
  calendar: [
    "M19 4h-1V2h-2v2H8V2H6v2H5a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2zm0 14H5V8h14v10z",
  ],
  mic: ["M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3zM8 11a4 4 0 1 1 8 0v2a4 4 0 0 1-8 0v-2z"],
  palette: ["M12 7.5a4.5 4.5 0 1 0 0 9 4.5 4.5 0 1 0 0-9z"],
  activity: ["M4 16.5a4.5 4.5 0 1 1 9 0 4.5 4.5 0 1 1-9 0z", "M13 10.5a4.5 4.5 0 1 0 0 9 4.5 4.5 0 0 0 0-9z"],
  sparkles: [
    "M12 2l2.59 5.98a.75.75 0 0 0 .41.41l5.98 2.59a.75.75 0 0 1 0 1.38l-5.98 2.59a.75.75 0 0 0-.41.41l-2.59 5.98a.75.75 0 0 1-1.38 0l-2.59-5.98a.75.75 0 0 0-.41-.41l-5.98-2.59a.75.75 0 0 1 0-1.38l5.98-2.59a.75.75 0 0 0 .41-.41L12 2z",
  ],
  zap: ["M13 10V3L4 13h7v7l9-10h-7z"],
  keyboard: ["M2 8h20v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8z"],

  // ── Actions ─────────────────────────────────────────────
  undo: ["M9 14a5 5 0 1 0-2.5-1.5m0 0L4 14m0 0l2.5-2.5"],
  redo: ["M15 14a5 5 0 1 1 2.5-1.5m0 0L20 14m0 0l-2.5-2.5"],
  x: ["M18 6L6 18M6 6l12 12"],
  plus: ["M12 5v14m7-7H5"],
  check: ["M20 6L9 17l-4-4"],
  chevronLeft: ["M15.41 16.59L10.83 12l4.59-4.59"],
  chevronRight: ["M8.59 16.59L13.17 12l-4.59-4.59"],
  chevronDown: ["M6 9l6 6 6-6"],
  pin: ["M12 1.5l2.72 5.73L20 9.25l-5 4.25 1.5 7.5L12 20l-2.5 3 1.5-7.5-5-4.25 2.72-2.02z"],
  externalLink: ["M18 13v6a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5m3-3h6m0 0v6m0 0l-7-7"],
  save: ["M5 5h14v14H5z", "M9 9h6v6H9z"],

  // ── Inspector ───────────────────────────────────────────
  tag: ["M12 7v10m-6-5h12"],
  link: ["M13.19 8.69a4.5 4.5 0 1 1-1.41 0 4.5 4.5 0 1 1 1.41 0zm0 0a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9z"],
  forward: ["M17 8l4 4m0 0l-4 4m4-4H3"],
  messageCircle: ["M12 7.5h4.5a3.5 3.5 0 0 1 3.5 3.5v5a3.5 3.5 0 0 1-3.5 3.5H12l-4 4v-4H7.5a3.5 3.5 0 0 1 0-7h4.5z"],
  bell: [
    "M12 2a7 7 0 0 0-7 7v5.09a3 3 0 0 1-2 2.82v1.18h18v-1.18a3 3 0 0 1-2-2.82V9a7 7 0 0 0-7-7zm5 14v1a5 5 0 0 1-10 0v-1a5 5 0 0 1 10 0z",
  ],
  book: ["M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6z"],
  copy: ["M8 5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-8a2 2 0 0 1-2-2V5z", "M4 9a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V9z"],

  // ── Navigation & view icons ──────────────────────────────
  dashboard: ["M3 3v2h2V3H3zm4 0v2H5V3h2zm4 0v2H9V3h2zm4 0v2h-2V3h2zm4 0v2h-2V3h2zm0 2v2H3V5h18zm0 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7h18z"],
  filePen: ["M2 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5zm15 9a2 2 0 0 1 .586 0l4.307 1.436a1 1 0 0 1 .694 1.337l-2 5.003a1 1 0 0 1-1.258.587l-5.5-2.5a1 1 0 0 1-.498-.498l-2.5-5.5a1 1 0 0 1 .587-1.258l5.003-2A2 2 0 0 1 17 14z"],
  network: ["M4 16a4 4 0 1 1 8 0 4 4 0 0 1-8 0zm12 0a4 4 0 1 1 8 0 4 4 0 0 1-8 0zm-6 0a4 4 0 1 1 8 0 4 4 0 0 1-8 0z"],
  inbox: ["M4 4a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4zm0 0V4zm12 0v4a2 2 0 0 1-2 2h-.5l-1 2h-3l-1-2H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2z", "M9 14h6v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-2h2a2 2 0 0 0 2-2V9h2v2a2 2 0 0 0 2 2h2v2a2 2 0 0 1-2 2v2a2 2 0 0 1-2-2v-6z"],
  bookOpen: ["M4 19V5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z", "M9 9h6v6H9z"],
  clipboardList: ["M16 1V5a2 2 0 0 1-2 2H9.87a1 1 0 0 1-.87-.5a1 1 0 0 0-.87-.5H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 1-2-2h-2z", "M8 3a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2h-4a2 2 0 0 1-2-2V3z"],
  settings: ["M12 15.5a3.5 3.5 0 0 1 0-7 3.5 3.5 0 0 1 0 7z", "M12 1l2.991 6.179a1 1 0 0 0 .447.534L19 9.5l-4.5 4.393.947 6.457-5.447-2.87-5.447 2.87 1.447-6.457L5 9.5l3.559-1.737a1 1 0 0 0 .441-.534L12 1z"],
  trash2: ["M3 6h18", "M9 6V4a3 3 0 0 1 6 0v2", "M4 6h16", "M10 11v6", "M14 11v6", "M5 6l1 14a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2L19 6"],
  history: ["M12 8v4l2 2", "M4 12a8 8 0 1 1 16 0 8 8 0 0 1-16 0z"],
  lifeBuoy: ["M12 2a9.955 9.955 0 0 1 8.764 5.236l-1.414 1.414A7.961 7.961 0 0 0 12 5a7.961 7.961 0 0 0-7.35 5.076l-1.414 1.414A9.955 9.955 0 0 1 12 2zm0 14.076a7.961 7.961 0 0 0 7.35-5.076l1.414-1.414A9.955 9.955 0 0 1 12 22a9.955 9.955 0 0 1-8.764-5.236l1.414-1.414A7.961 7.961 0 0 0 12 16.076z"],
  archive: ["M4 6h16", "M4 6l2 14a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2L20 6", "M9 11h6v2H9z"],
  folderTree: ["M12 7.5v8.5m0-8.5h4a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2h-4m0-8.5V9a3 3 0 0 0-3-3H5a3 3 0 0 0-3 3v8a3 3 0 0 0 3 3h4a3 3 0 0 0 3-3v-3.5z"],
  bookText: ["M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6z", "M8 10h8v2H8zm0 4h5v2H8z"],
  comparison: ["M17 6h2a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-2", "M7 6H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h2", "12 6l-2 6-2-6z"],
  trendingUp: ["M3 17l6-6 4 4 8-8v10H3z"],
  clock: ["M12 7v5l3 3", "M12 2a10 10 0 1 0 0 20 10A10 10 0 0 0 12 2z"],
  filePlus: ["M14 2v4a2 2 0 0 0 2 2h4l-6-6z", "M4 2h10v4a2 2 0 0 0 2 2h4v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z"],
  hardDrive: ["M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6z", "M8 10h.01M16 16h.01M12 16h.01M16 10h.01"],
  database: ["M12 2C6.48 2 2 5.74 2 10.25v5.5C2 18.26 6.48 21 12 21s10-3.74 10-10.75v-5.5C22 5.74 17.52 2 12 2zm0 16.5c-4.14-.65-7.5-3.85-7.5-8.19V10c0-.55.45-1 1-1h1v5c0 2.21 1.27 4.16 3.14 5.14l.36-.36C11.64 14.5 13.36 16 15.5 16c.14 0 .27-.01.41-.03A7.48 7.48 0 0 0 19 10v2.06c0 4.34-3.36 7.55-7.5 8.19z", "M15 9V5c0-.55-.45-1-1-1h-1.5V9H15z"],
  bookMarked: ["M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6z", "M9 9h1v6H9V9zm5 0h1v6h-1V9z"],
  star: ["M12 2l3.09 6.26L22 9.27l-5 4.87L15.18 17L12 15.37L8.82 17L6.82 14.14L2 9.27L8.91 8.26L12 2z"],
  starHalf: ["M12 2l3.09 6.26L22 9.27l-5 4.87L15.18 17L12 15.37l-3.18 1.63L5 14.14 2 9.27 8.91 8.26z", "M12 15V2l-3.09 6.26L2 9.27l5 4.87L5.82 17z"],

  // ── Misc ────────────────────────────────────────────────
  info: ["M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zm1-5v-4m0-4h.01M12 7a1 1 0 1 0 0 2 1 1 0 0 0 0-2z"],
  eye: ["M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z", "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"],
};

export interface IconProps extends SVGProps<SVGSVGElement> {
  /** Name key into the ICONS map. */
  name: string;
}

/**
 * A lightweight icon component that renders an inline SVG from the ICONS map.
 * Uses stroke-based (outline) styling so it inherits `currentColor`.
 */
export function Icon({ name, className, ...props }: IconProps) {
  const paths = ICONS[name];
  if (!paths) return null;
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {paths.map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  );
}
