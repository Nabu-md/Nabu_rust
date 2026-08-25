// ──────────────────────────────────────────────────────────────────────────────
// inbox/icons.tsx — self-contained, shell-independent icon set for the Inbox
//
// Mirrors the thumbnail → icon mapping performed by `thumbnail_to_icon` in
// crates/nabu-ui/src/components/inbox.rs.  Drawn as primitive SVG shapes so the
// inbox folder has no dependency on the shell icon registry and stays fully
// within the inbox/** scope fence.
// ──────────────────────────────────────────────────────────────────────────────

import type { ReactNode, SVGProps } from "react";

/** A miniature stroke-style SVG (Heroicons outline aesthetic). */
export interface InboxIconProps extends SVGProps<SVGSVGElement> {
  /** Lookup key into ICON_CONTENT (or a thumbnail string). */
  name: string;
}

const COMMON: SVGProps<SVGSVGElement> = {
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
  "aria-hidden": true,
};

/**
 * Inner SVG primitives keyed by name.  Values are valid JSX children for an
 * `<svg>` viewport of `0 0 24 24`.  Extra (unused) keys are harmless under
 * `noUnusedLocals` — they are object properties, not locals.
 */
const ICON_CONTENT: Record<string, ReactNode> = {
  // ── Actions ─────────────────────────────────────────────
  check: <path d="M20 6L9 17l-4-4" />,
  x: <path d="M18 6L6 18M6 6l12 12" />,
  "trash-2": (
    <>
      <path d="M3 6h18" />
      <path d="M9 2h6a2 2 0 0 1 2 2v2H7V4a2 2 0 0 1 2-2h2z" />
      <path d="M4 6h2m14 0l-1 15a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </>
  ),
  "refresh-cw": (
    <>
      <circle cx={12} cy={12} r={10} />
      <path d="M12 2v6l3-3" />
      <path d="M12 22v-6l-3 3" />
    </>
  ),
  "alert-triangle": (
    <>
      <path d="M10.26 2.22A2 2 0 0 1 12 2h7a2 2 0 0 1 1.7.98l3 5a2 2 0 0 1-.14 2.24l-7 10a2 2 0 0 1-2.52.58L6 15.5a2 2 0 0 1-.7-1.7V9a2 2 0 0 1 1-1.75z" />
      <line x1={12} y1={7} x2={12} y2={13} />
      <line x1={12} y1={17} x2={12.01} y2={17.01} />
    </>
  ),
  "map-pin": (
    <>
      <path d="M21 10.5c0 5.5-9 13.5-9 13.5S3 16 3 10.5a9 9 0 1 1 18 0z" />
      <circle cx={12} cy={10.5} r={2.5} />
    </>
  ),
  inbox: (
    <>
      <rect x={3} y={7} width={18} height={13} rx={2} />
      <path d="M3 11l9-6 9 6" />
      <path d="M8 15h1" />
      <path d="M15 15h1" />
    </>
  ),
  plus: <path d="M12 5v14m7-7H5" />,
  search: (
    <>
      <circle cx={11} cy={11} r={8} />
      <path d="M21 21l-4.35-4.35" />
    </>
  ),
  "chevron-up": <path d="M18 15l-6-6-6 6" />,
  "chevron-down": <path d="M6 9l6 6 6-6" />,
  "grip-vertical": (
    <>
      <circle cx={9} cy={12} r={1} />
      <circle cx={15} cy={12} r={1} />
      <path d="M9 4a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm0 16a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm0-8a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm3-9a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm0 8a2 2 0 1 1 0 4 2 2 0 0 1 0-4z" />
    </>
  ),
  eye: (
    <>
      <path d="M2 12s8-8 10-10 10 8 10 10-8 8-10 8-10-8-10-10z" />
      <circle cx={12} cy={12} r={3} />
    </>
  ),

  // ── File-type thumbnails (mirrors crates/nabu-ui icon enum) ─────────
  file: (
    <path d="M4 4a2 2 0 0 1 2-2h8.586a1 1 0 0 1 .707.293l5.414 5.414A1 1 0 0 1 20 9.414V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z" />
  ),
  image: (
    <>
      <rect x={3} y={5} width={18} height={14} rx={2} />
      <path d="M21 15l-5-5-4 5-3-3-4 4v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2z" />
      <circle cx={8.5} cy={9} r={1.5} />
    </>
  ),
  music: (
    <>
      <rect x={3} y={5} width={18} height={14} rx={2} />
      <path d="M9 15V5l10-3v10" />
      <circle cx={15} cy={15} r={2.5} />
    </>
  ),
  play: (
    <>
      <circle cx={12} cy={12} r={10} />
      <path d="M10 8l6 4-6 4V8z" />
    </>
  ),
  code: (
    <>
      <rect x={3} y={5} width={18} height={14} rx={2} />
      <path d="M8 8l2 2 2-2m4 5l2-2 2 2" />
    </>
  ),
  mail: (
    <>
      <rect x={3} y={5} width={18} height={14} rx={2} />
      <path d="M3 7l9 6 9-6" />
      <path d="M7 9l5 4 5-4" />
    </>
  ),
  bookmark: (
    <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
  ),
  "book-open": (
    <>
      <path d="M2 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H7a2 2 0 0 0-2 2V6z" />
      <path d="M16 4h2a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-2" />
    </>
  ),
  "file-text": (
    <>
      <path d="M4 4a2 2 0 0 1 2-2h8.586a1 1 0 0 1 .707.293l5.414 5.414A1 1 0 0 1 20 9.414V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z" />
      <path d="M9 2v6a2 2 0 0 0 2 2h3.5" />
      <path d="M9 12h6" />
      <path d="M9 16h6" />
    </>
  ),
  "pen-line": <path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 21H3v-4.5L17 3z" />,
  user: (
    <>
      <circle cx={12} cy={8} r={4} />
      <path d="M20 21V12A4 4 0 0 0 4 12v9" />
    </>
  ),
  calendar: (
    <>
      <rect x={3} y={5} width={18} height={16} rx={2} />
      <path d="M7 9h1v1H7zm7 0h1v1h-1zm5 0h1v1h-1z" />
      <path d="M4 9V5a2 2 0 0 1 2-2h2m10 0h2a2 2 0 0 1 2 2v4" />
    </>
  ),
  "list-checks": (
    <>
      <path d="M9 12h6" />
      <path d="M9 16h6" />
      <path d="M9 8h6" />
      <path d="M9 6a3 3 0 1 1-6 0 3 3 0 0 1 6 0zm9 6a3 3 0 1 0 0 6 3 3 0 0 0 0-6z" />
    </>
  ),
  "file-pen": (
    <>
      <path d="M4 4a2 2 0 0 1 2-2h8.586a1 1 0 0 1 .707.293l5.414 5.414A1 1 0 0 1 20 9.414V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z" />
      <path d="M9 2v6a2 2 0 0 0 2 2h3.5" />
      <path d="M15.5 3.5l4 4" />
      <path d="M9 12h6" />
      <path d="M9 16h6" />
    </>
  ),
  "sticky-note": (
    <>
      <path d="M4 4a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z" />
      <path d="M8 8h8" />
      <path d="M8 12h8" />
      <path d="M8 16h5" />
    </>
  ),
  "folder-tree": (
    <path d="M3 7a2 2 0 0 1 2-2h4l3-3h4a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-4l-3 3H5a2 2 0 0 1-2-2V7z" />
  ),
  dashboard: (
    <>
      <rect x={3} y={3} width={5} height={5} rx={1} />
      <rect x={10} y={3} width={5} height={5} rx={1} />
      <rect x={17} y={3} width={4} height={5} rx={1} />
      <rect x={3} y={10} width={5} height={5} rx={1} />
      <rect x={10} y={10} width={5} height={5} rx={1} />
      <rect x={17} y={10} width={4} height={5} rx={1} />
    </>
  ),
  folder: (
    <path d="M3 6a2 2 0 0 1 2-2h4l3 3h6a2 2 0 0 1 2 2v1H5a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7a2 2 0 0 0-2-2h-1V9a2 2 0 0 0-2-2h-6L7 4H5a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 1 2-2" />
  ),
};

/** Maps a backend thumbnail string (snake-case) to a local icon name. */
export function thumbnailToName(thumbnail: string | null | undefined): string {
  if (!thumbnail) {
    return "file";
  }
  const map: Record<string, string> = {
    image: "image",
    "music-3": "music",
    play: "play",
    "code-block": "code",
    code: "code",
    mail: "mail",
    bookmark: "bookmark",
    "book-text": "book-open",
    "file-text": "file-text",
    "pen-line": "pen-line",
    user: "user",
    calendar: "calendar",
    "list-checks": "list-checks",
    folder: "folder",
    "file-pen": "file-pen",
    "sticky-note": "sticky-note",
    "folder-tree": "folder-tree",
    "layout-dashboard": "dashboard",
    dashboard: "dashboard",
    file: "file",
  };
  return map[thumbnail] ?? "file";
}

/** A stroke-style icon resolved from the local icon registry. */
export function InboxIcon({ name, className, ...props }: InboxIconProps) {
  const content = ICON_CONTENT[name] ?? ICON_CONTENT["file"];
  return (
    <svg className={className} {...COMMON} {...props}>
      {content}
    </svg>
  );
}

/** Renders a file-type thumbnail icon from a backend thumbnail string. */
export interface FileThumbnailIconProps extends SVGProps<SVGSVGElement> {
  thumbnail: string | null;
}

export function FileThumbnailIcon({
  thumbnail,
  className,
  ...props
}: FileThumbnailIconProps) {
  return (
    <InboxIcon name={thumbnailToName(thumbnail)} className={className} {...props} />
  );
}
