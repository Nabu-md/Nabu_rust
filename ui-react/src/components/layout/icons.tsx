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

  // ── Misc ────────────────────────────────────────────────
  info: ["M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zm1-5v-4m0-4h.01M12 7a1 1 0 1 0 0 2 1 1 0 0 0 0-2z"],
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
