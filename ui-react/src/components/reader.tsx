// ──────────────────────────────────────────────────────────────────────────────
// reader.tsx — distraction-free reader view
//
// Mirrors: ui-react/src/components/shipped/reader.rs (ReaderView)
//
// Loads the active note from the backend via `note_read` and renders it through
// an inline markdown→HTML renderer. Reader preferences (font size, line width,
// theme, focus mode) are persisted via the settings store.
//
// A race-safety nonce guards against stale IPC results when the active path
// changes faster than a load completes — each call increments `nonce` and
// captures the value; the async callback checks that the nonce still matches
// before writing to any state.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback } from "react";
import { noteRead, settingsGet, settingsSet } from "../ipc";
import { useWorkspace } from "../context";
import type { LoadState } from "../types";
import { nonceIsStale } from "../types";

/** Settings key used to persist reader preferences. */
const READER_SETTINGS_KEY = "nabu.reader.settings";

/** Reader settings persisted in the settings store. */
interface ReaderSettings {
  font_size: number;
  line_width: number;
  theme: string;
  focus_mode: boolean;
}

const DEFAULT_READER_SETTINGS: ReaderSettings = {
  font_size: 18,
  line_width: 720,
  theme: "dark",
  focus_mode: false,
};

/** Load-state classification for the Reader content area. */
type ReaderPhase = "loading" | "empty" | "error" | "loaded";

// ── Phase classification ─────────────────────────────────────────────────────

/**
 * Maps reactive state to a single phase. Error takes precedence over every
 * other state.
 *
 * Mirrors `classify_reader_phase` in the Dioxus reference.
 */
function classifyReaderPhase(
  loaded: boolean,
  noteMissing: boolean,
  loadError: string | null,
): ReaderPhase {
  if (loadError && loadError.length > 0) {
    return "error";
  }
  if (!loaded) {
    return "loading";
  }
  if (noteMissing) {
    return "empty";
  }
  return "loaded";
}

// ── Markdown renderer (pure TS — no LePtOS/Dioxus dependency) ─────────────────

function htmlEscape(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Renders inline markdown: bold, italic, code, links, wikilinks. */
function renderInline(text: string): string {
  const chars = Array.from(text);
  let result = "";
  let i = 0;

  while (i < chars.length) {
    // Bold: **text**
    if (
      i + 1 < chars.length &&
      chars[i] === "*" &&
      chars[i + 1] === "*"
    ) {
      const end = chars
        .slice(i + 2)
        .findIndex((c, idx) => c === "*" && chars[i + 2 + idx + 1] === "*");
      if (end !== -1) {
        const inner = chars.slice(i + 2, i + 2 + end).join("");
        result += `<strong>${renderInline(inner)}</strong>`;
        i = i + 2 + end + 2;
        continue;
      }
    }
    // Italic: *text* or _text_
    if (chars[i] === "*" || chars[i] === "_") {
      const marker = chars[i];
      const end = chars.slice(i + 1).findIndex((c) => c === marker);
      if (end !== -1) {
        const inner = chars.slice(i + 1, i + 1 + end).join("");
        result += `<em>${renderInline(inner)}</em>`;
        i = i + 1 + end + 1;
        continue;
      }
    }
    // Inline code: `text`
    if (chars[i] === "`") {
      const rest = chars.slice(i + 1);
      const closeIdx = rest.findIndex((c) => c === "`");
      if (closeIdx !== -1) {
        const inner = rest.slice(0, closeIdx).join("");
        result += `<code class="inline-code">${inner}</code>`;
        i = i + 1 + closeIdx + 1;
        continue;
      }
    }
    // Wikilink: [[text]]
    if (
      i + 1 < chars.length &&
      chars[i] === "[" &&
      chars[i + 1] === "["
    ) {
      const rest = chars.slice(i + 2);
      const closeIdx = rest
        .map((c, idx) => (c === "]" && rest[idx + 1] === "]" ? idx : -1))
        .findIndex((v) => v !== -1);
      if (closeIdx !== -1) {
        const inner = rest.slice(0, closeIdx).join("");
        const target = inner.split("|")[0] ?? inner;
        result += `<a class="wikilink" href="#" data-path="${htmlEscape(target)}">${inner}</a>`;
        i = i + 2 + closeIdx + 2;
        continue;
      }
    }
    // Link: [text](url)
    if (chars[i] === "[") {
      const closeBracket = chars
        .slice(i + 1)
        .findIndex((c) => c === "]");
      if (closeBracket !== -1) {
        const after = chars[i + 1 + closeBracket + 1];
        if (after === "(") {
          const rest = chars.slice(i + 1 + closeBracket + 2);
          const closeParen = rest.findIndex((c) => c === ")");
          if (closeParen !== -1) {
            const textPart = chars.slice(i + 1, i + 1 + closeBracket).join("");
            const urlPart = rest.slice(0, closeParen).join("");
            result += `<a href="${htmlEscape(
              urlPart,
            )}" target="_blank" rel="noopener">${textPart}</a>`;
            i = i + 1 + closeBracket + 2 + closeParen + 1;
            continue;
          }
        }
      }
    }
    result += chars[i];
    i += 1;
  }

  return result;
}

/** Renders a markdown string as safe HTML. Escapes HTML entities first. */
function renderMarkdown(md: string): string {
  const escaped = htmlEscape(md);
  const lines = escaped.split("\n");
  let html = "";
  let inCodeBlock = false;
  let inTable = false;
  let tableHeaderDone = false;

  for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
    const line = lines[lineIdx]!;
    const peek = lines[lineIdx + 1] ?? null;

    if (line.trimStart().startsWith("```")) {
      if (inCodeBlock) {
        html += "</code></pre>\n";
        inCodeBlock = false;
      } else {
        const lang = line.trimStart().replace(/`+/g, "").trim();
        html += `<pre><code class="code-block" data-lang="${lang}">`;
        inCodeBlock = true;
      }
      continue;
    }
    if (inCodeBlock) {
      html += line + "\n";
      continue;
    }

    // Table detection
    if (line.includes("|") && line.trim().startsWith("|")) {
      if (peek && peek.includes("---") && peek.includes("|")) {
        if (!inTable) {
          html += "<table class=\"md-table\">\n";
          inTable = true;
          tableHeaderDone = false;
        }
        if (!tableHeaderDone) {
          html += "<thead><tr>";
          for (const cell of line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|")) {
            html += `<th>${cell.trim()}</th>`;
          }
          html += "</tr></thead><tbody>\n";
          tableHeaderDone = true;
          lineIdx++; // skip the separator line
          continue;
        }
      }
      if (inTable) {
        html += "<tr>";
        for (const cell of line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|")) {
          html += `<td>${cell.trim()}</td>`;
        }
        html += "</tr>\n";
        continue;
      }
    }
    if (inTable) {
      html += "</tbody></table>\n";
      inTable = false;
    }

    const trimmed = line.trimStart();

    if (trimmed.startsWith("# ")) {
      html += `<h1>${renderInline(trimmed.slice(2))}</h1>\n`;
      continue;
    }
    if (trimmed.startsWith("## ")) {
      html += `<h2>${renderInline(trimmed.slice(3))}</h2>\n`;
      continue;
    }
    if (trimmed.startsWith("### ")) {
      html += `<h3>${renderInline(trimmed.slice(4))}</h3>\n`;
      continue;
    }
    if (trimmed.startsWith("#### ")) {
      html += `<h4>${renderInline(trimmed.slice(5))}</h4>\n`;
      continue;
    }
    if (trimmed.startsWith("##### ")) {
      html += `<h5>${renderInline(trimmed.slice(6))}</h5>\n`;
      continue;
    }
    if (trimmed.startsWith("###### ")) {
      html += `<h6>${renderInline(trimmed.slice(7))}</h6>\n`;
      continue;
    }

    if (trimmed.startsWith("> ")) {
      html += `<blockquote>${renderInline(trimmed.slice(2))}</blockquote>\n`;
      continue;
    }

    if (trimmed === "---" || trimmed === "***" || trimmed === "___") {
      html += "<hr/>\n";
      continue;
    }

    if (trimmed.startsWith("- [ ] ")) {
      html += `<div class="task-item task-unchecked"><input type="checkbox" disabled /> ${renderInline(
        trimmed.slice(6),
      )}</div>\n`;
      continue;
    }
    if (trimmed.startsWith("- [x] ")) {
      html += `<div class="task-item task-checked"><input type="checkbox" checked disabled /> ${renderInline(
        trimmed.slice(6),
      )}</div>\n`;
      continue;
    }

    if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
      const item = trimmed.replace(/^[-*]\s/, "").trim();
      html += `<li>${renderInline(item)}</li>\n`;
      continue;
    }
    if (trimmed.startsWith("1. ")) {
      html += `<li>${renderInline(trimmed.slice(3))}</li>\n`;
      continue;
    }

    if (trimmed.length === 0) {
      html += "\n";
      continue;
    }

    html += `<p>${renderInline(line)}</p>\n`;
  }

  if (inCodeBlock) {
    html += "</code></pre>\n";
  }
  if (inTable) {
    html += "</tbody></table>\n";
  }

  return html;
}

// ── IPC helpers ──────────────────────────────────────────────────────────────

/**
 * Loads a note's content from the `note_read` backend command.
 *
 * A race-safety nonce guards against stale results: each call increments
 * `nonce` and captures the value; the async callback checks that the nonce
 * still matches before writing to any state.
 */
function loadNoteContent(
  path: string,
  nonce: { current: number },
  setContent: (v: string) => void,
  setContentState: (v: LoadState) => void,
  setLoadError: (v: string | null) => void,
  setNoteMissing: (v: boolean) => void,
): void {
  nonce.current += 1;
  const thisNonce = nonce.current;

  setContentState("loading" as LoadState);
  setLoadError(null);
  setNoteMissing(false);

  void (async () => {
    try {
      const saved = await noteRead(path);
      if (nonceIsStale(nonce.current, thisNonce)) return;
      if (saved && saved.length > 0) {
        setContent(saved);
        setNoteMissing(false);
      } else {
        setContent("");
        setNoteMissing(true);
      }
      setContentState("loaded" as LoadState);
    } catch (err) {
      if (nonceIsStale(nonce.current, thisNonce)) return;
      setLoadError(
        err instanceof Error ? err.message : "note_read returned no data.",
      );
      setContentState("failed" as LoadState);
    }
  })();
}

/** Persist reader settings via the settings store. */
function persistReaderSettings(settings: ReaderSettings): void {
  void (async () => {
    try {
      await settingsSet(READER_SETTINGS_KEY, settings);
    } catch {
      // Best-effort persistence; ignore failures.
    }
  })();
}

// ── Component ────────────────────────────────────────────────────────────────

/**
 * Distraction-free Reader view (`ViewMode::Reader`).
 *
 * Loads the active note from the workspace via `note_read` and renders it
 * through an inline markdown→HTML renderer. Reader preferences are persisted.
 */
export function ReaderView() {
  const ws = useWorkspace();

  // ── Content / load state ──
  const [content, setContent] = useState("");
  const [contentState, setContentState] = useState<LoadState>("idle");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [noteMissing, setNoteMissing] = useState(false);
  const nonceRef = useRef(0);

  // ── Settings ──
  const [settings, setSettings] = useState<ReaderSettings>(
    DEFAULT_READER_SETTINGS,
  );
  const [showSettings, setShowSettings] = useState(false);

  // ── Load reader settings on mount (runs once) ──
  useEffect(() => {
    void (async () => {
      try {
        const val = await settingsGet(READER_SETTINGS_KEY);
        if (val) {
          // The backend returns the persisted JSON value directly.
          const parsed = val as Partial<ReaderSettings>;
          setSettings((prev) => ({ ...prev, ...parsed }));
        }
      } catch {
        // Keep defaults on failure.
      }
    })();
  }, []);

  // ── Load note content when the active path changes ──
  useEffect(() => {
    const path = ws.activePath;
    if (!path) {
      nonceRef.current += 1;
      setContent("");
      setContentState("idle");
      setLoadError(null);
      setNoteMissing(false);
      return;
    }
    loadNoteContent(
      path,
      nonceRef,
      setContent,
      setContentState,
      setLoadError,
      setNoteMissing,
    );
  }, [ws.activePath]);

  // ── Setting handlers ──
  const onFontSize = useCallback(
    (ev: React.ChangeEvent<HTMLInputElement>) => {
      const val = parseInt(ev.target.value, 10) || 18;
      setSettings((prev) => {
        const next = { ...prev, font_size: val };
        persistReaderSettings(next);
        return next;
      });
    },
    [],
  );

  const onLineWidth = useCallback(
    (ev: React.ChangeEvent<HTMLInputElement>) => {
      const val = parseInt(ev.target.value, 10) || 720;
      setSettings((prev) => {
        const next = { ...prev, line_width: val };
        persistReaderSettings(next);
        return next;
      });
    },
    [],
  );

  const onFocusToggle = useCallback(() => {
    setSettings((prev) => {
      const next = { ...prev, focus_mode: !prev.focus_mode };
      persistReaderSettings(next);
      return next;
    });
  }, []);

  const onThemeSelect = useCallback((theme: string) => {
    setSettings((prev) => {
      const next = { ...prev, theme };
      persistReaderSettings(next);
      return next;
    });
  }, []);

  const onRetryLoad = useCallback(() => {
    const path = ws.activePath;
    if (!path) return;
    loadNoteContent(
      path,
      nonceRef,
      setContent,
      setContentState,
      setLoadError,
      setNoteMissing,
    );
  }, [ws.activePath]);

  // ── Pre-compute render values ──
  const activePathStr = ws.activePath ?? "";
  const isContentLoaded = contentState === "loaded";
  const isMissing = noteMissing;
  const phase =
    activePathStr.length === 0
      ? (classifyReaderPhase(false, true, null) as ReaderPhase)
      : classifyReaderPhase(isContentLoaded, isMissing, loadError);

  const proseClass = settings.focus_mode
    ? "reader-prose reader-focus-mode"
    : "reader-prose";

  const contentHtml =
    phase === "loaded" ? renderMarkdown(content) : null;

  const settingsStyle: React.CSSProperties = {
    maxWidth: `${settings.line_width}px`,
  };
  const contentStyle: React.CSSProperties = {
    maxWidth: `${settings.line_width}px`,
    fontSize: `${settings.font_size}px`,
    lineHeight: 1.7,
  };

  const focusBtnClass = settings.focus_mode
    ? "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
    : "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200";

  const settingsBtnClass = showSettings
    ? "px-2 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
    : "px-2 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200";

  const fontSizeVal = settings.font_size;
  const lineWidthVal = settings.line_width;

  // ── Render ──
  return (
    <div
      className="reader-view h-full overflow-auto bg-gray-950 text-gray-100"
      data-theme={settings.theme}
    >
      {/* Toolbar */}
      <div className="sticky top-0 z-10 flex items-center justify-between px-4 py-2 bg-gray-950/80 backdrop-blur border-b border-gray-800/50">
        <div className="flex items-center gap-3">
          <span className="text-sm text-gray-400 truncate max-w-xs">
            {activePathStr || "No note selected"}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onFocusToggle}
            className={focusBtnClass}
            title="Toggle focus mode"
          >
            🎯 Focus
          </button>
          <button
            type="button"
            onClick={() => setShowSettings((v) => !v)}
            className={settingsBtnClass}
            title="Reader settings"
          >
            ⚙️
          </button>
        </div>
      </div>

      {/* Settings panel */}
      {showSettings && (
        <div
          className="sticky top-12 z-10 mx-auto bg-gray-900 border border-gray-700 rounded-lg p-4 mb-4"
          style={settingsStyle}
        >
          <div className="space-y-3">
            <div>
              <label className="text-xs text-gray-500 uppercase tracking-wide">
                Font Size
              </label>
              <div className="flex items-center gap-2 mt-1">
                <input
                  type="range"
                  min={14}
                  max={28}
                  value={fontSizeVal}
                  onChange={onFontSize}
                  className="flex-1"
                />
                <span className="text-sm text-gray-400">{fontSizeVal}px</span>
              </div>
            </div>

            <div>
              <label className="text-xs text-gray-500 uppercase tracking-wide">
                Line Width
              </label>
              <div className="flex items-center gap-2 mt-1">
                <input
                  type="range"
                  min={480}
                  max={960}
                  step={40}
                  value={lineWidthVal}
                  onChange={onLineWidth}
                  className="flex-1"
                />
                <span className="text-sm text-gray-400">{lineWidthVal}px</span>
              </div>
            </div>

            <div>
              <label className="text-xs text-gray-500 uppercase tracking-wide">
                Theme
              </label>
              <div className="flex gap-2 mt-1">
                {["dark", "sepia", "light"].map((t) => {
                  const isActive = settings.theme === t;
                  const btnClass = isActive
                    ? "px-3 py-1 text-xs rounded border bg-blue-900/50 border-blue-600 text-blue-300"
                    : "px-3 py-1 text-xs rounded border border-gray-700 text-gray-400 hover:text-gray-200";
                  return (
                    <button
                      key={t}
                      type="button"
                      onClick={() => onThemeSelect(t)}
                      className={btnClass}
                    >
                      {t}
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Content area */}
      <div className="reader-content mx-auto px-8 py-8" style={contentStyle}>
        {phase === "loading" && (
          <div className="flex items-center justify-center py-20">
            <div className="w-6 h-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
            <span className="ml-2 text-sm text-gray-400">Loading note…</span>
          </div>
        )}

        {phase === "error" && (
          <div className="flex-1 flex items-center justify-center p-6">
            <div className="w-full max-w-md">
              <div className="p-4 border border-red-900/50 bg-red-950/30 rounded">
                <h3 className="text-red-300 font-semibold mb-1">
                  Couldn&apos;t open note
                </h3>
                <p className="text-sm text-red-300/80 mb-2">
                  The note content could not be loaded.
                </p>
                {loadError && (
                  <pre className="text-xs text-red-400/60 bg-red-950/50 p-2 rounded mb-3 overflow-auto">
                    {loadError}
                  </pre>
                )}
                <p className="text-xs text-red-300/70 mb-3">
                  Make sure the note is accessible and the backend is running,
                  then retry.
                </p>
                <button
                  type="button"
                  onClick={onRetryLoad}
                  className="px-3 py-1 text-sm bg-blue-600 rounded hover:bg-blue-500"
                >
                  Retry
                </button>
              </div>
            </div>
          </div>
        )}

        {phase === "empty" && (
          <div className="flex h-full items-center justify-center py-20">
            <div className="text-center">
              <div className="text-3xl mb-2">📝</div>
              <h3 className="text-lg font-medium text-gray-300 mb-1">
                No note selected
              </h3>
              <p className="text-sm text-gray-500">
                Open a note and switch to Reader mode to start reading.
              </p>
            </div>
          </div>
        )}

        {phase === "loaded" && contentHtml !== null && (
          <div
            className={proseClass}
            dangerouslySetInnerHTML={{ __html: contentHtml }}
          />
        )}
      </div>
    </div>
  );
}
