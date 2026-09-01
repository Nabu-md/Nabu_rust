// ──────────────────────────────────────────────────────────────────────────────
// dictation_pill.tsx — floating scratchpad / dictation / file-drop pill
//
// Mirrors: crates/nabu-ui/src/components/dictation_pill.rs
//
// Preserves all behaviour from the Dioxus migration spec:
//   - opacity loaded from settings ("floating_pill_opacity") and applied to
//     the DOM via inline style; restored to 0.8 on mount, 1.0 on hover.
//   - clipboard cache panel showing recent clipboard entries (max 10),
//     click to copy-to-restore.
//   - drop zone wired to `capture_file_drop` IPC command.
//   - copy button writes scratchpad contents to the clipboard.
//   - three-mode switch: dictation / scratchpad / drop zone.
//
// Uses Tauri invoke via the typed wrappers in ipc.ts. ToastProvider +
// NavContext are consumed via the standard hooks.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, type ReactNode } from "react";
import { useToast } from "../context";
import {
  startDictation,
  stopDictation,
  captureFileDrop,
  settingsGet,
  openSettings,
} from "../ipc";
import { Icon } from "./layout/icons";

// ── Inline SVG for upload (not in the shared ICONS map; kept local to
//    stay within SCOPE). Matches Heroicons 2 outline style. ────────────

const UploadIcon = (props: { className?: string }) => (
  <svg
    className={props.className ?? "w-5 h-5"}
    fill="none"
    stroke="currentColor"
    viewBox="0 0 24 24"
    aria-hidden="true"
  >
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M7 16a4 4 0 118 0M5 20h14a2 2 0 002-2V8a2 2 0 00-2-2h-2l-2-2h-6L7 6a2 2 0 00-2 2v10a2 2 0 002 2z"
    />
  </svg>
);

// ── Mode type ────────────────────────────────────────────────────────

type PillMode = "dictation" | "scratchpad" | "drop";

// ── Component ────────────────────────────────────────────────────────

/**
 * The floating dictation / scratchpad / drop-zone pill.
 *
 * Renders a small floating panel with a mode selector (dictation, scratchpad,
 * drop zone), a recording/transcription indicator, action buttons (copy,
 * settings), and a clipboard cache panel.
 */
export function DictationPill(): ReactNode {
  const { toast } = useToast();

  const [scratchpad, setScratchpad] = useState("");
  const [mode, setMode] = useState<PillMode>("dictation");
  const [opacity, setOpacity] = useState(0.8);
  const [clipboardCache, setClipboardCache] = useState<string[]>([]);
  const [isDictating, setIsDictating] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isDragging, setIsDragging] = useState(false);

  // Load opacity from backend settings on mount (mirrors Dioxus spawn_local).
  useEffect(() => {
    void (async () => {
      try {
        const result = await settingsGet("floating_pill_opacity");
        const op = Number(result);
        if (!Number.isNaN(op) && op >= 0 && op <= 1) {
          setOpacity(op);
        }
      } catch {
        // Non-fatal: settings not ready — keep default 0.8.
      }
    })();
  }, []);

  // ── Handlers ───────────────────────────────────────────────────────

  const handleMouseEnter = () => setOpacity(1.0);
  const handleMouseLeave = () => setOpacity(0.8);

  const handleCopy = async () => {
    const text = scratchpad;
    if (text.length === 0) {
      toast("Nothing to copy — scratchpad is empty", { variant: "warning" });
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      toast("Copied to clipboard", { variant: "success" });
      setClipboardCache((prev) => {
        const next = [...prev, text];
        if (next.length > 10) next.shift();
        return next;
      });
    } catch {
      toast("Could not write to clipboard", { variant: "error" });
    }
  };

  const handleOpenSettings = async () => {
    try {
      await openSettings();
    } catch {
      toast("Could not open settings", { variant: "error" });
    }
  };

  const handleRecord = async () => {
    if (isProcessing) return;

    if (isDictating) {
      setIsDictating(false);
      setIsProcessing(true);
      try {
        const text = await stopDictation();
        if (text && text.trim().length > 0) {
          setScratchpad((prev) =>
            prev.length > 0 ? `${prev}\n${text}` : text
          );
          toast("Transcription captured", { variant: "success" });
        } else {
          toast("No speech detected", { variant: "warning" });
        }
      } catch (err) {
        toast(
          `Failed to stop: ${err instanceof Error ? err.message : String(err)}`,
          { variant: "error" }
        );
      } finally {
        setIsProcessing(false);
      }
    } else {
      setIsDictating(true);
      try {
        await startDictation();
        toast("Listening…", { variant: "success" });
      } catch (err) {
        toast(
          `Could not start: ${err instanceof Error ? err.message : String(err)}`,
          { variant: "error" }
        );
        setIsDictating(false);
      }
    }
  };

  const handleDrop = async (file: File) => {
    const filename = file.name;
    const mimeType = file.type || "application/octet-stream";
    try {
      const arrayBuffer = await file.arrayBuffer();
      const data = Array.from(new Uint8Array(arrayBuffer));
      const id = await captureFileDrop(filename, mimeType, data);
      toast(`Captured '${id}' to inbox`, { variant: "success" });
    } catch (err) {
      toast(
        `Could not capture '${filename}': ${err instanceof Error ? err.message : String(err)}`,
        { variant: "error" }
      );
    }
  };

  // ── Mode-specific content ────────────────────────────────────────

  const renderModeContent = (): ReactNode => {
    switch (mode) {
      case "dictation":
        return (
          <>
            {/* Recording pulse / processing indicator */}
            {isDictating ? (
              <div className="flex space-x-1">
                <div className="h-4 w-1 bg-white animate-pulse" />
                <div className="h-6 w-1 bg-white animate-pulse delay-75" />
                <div className="h-4 w-1 bg-white animate-pulse delay-150" />
              </div>
            ) : isProcessing ? (
              <span className="text-xs text-gray-400">Transcribing…</span>
            ) : null}

            <button
              type="button"
              onClick={handleRecord}
              disabled={isProcessing}
              className="px-3 py-1 rounded bg-blue-600 text-white text-sm font-medium hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {isProcessing ? "Transcribing" : isDictating ? "Stop" : "Record"}
            </button>
          </>
        );

      case "scratchpad":
        return (
          <textarea
            placeholder="Scratchpad…"
            value={scratchpad}
            onChange={(e) => setScratchpad(e.target.value)}
            className="w-full bg-transparent text-gray-100 placeholder-gray-500 border-none outline-none text-sm resize-none"
            style={{
              background: "transparent",
              color: "white",
              border: "none",
              width: "100%",
            }}
            rows={4}
          />
        );

      case "drop":
        return (
          <div
            className="border-2 border-dashed rounded-lg p-6 text-center transition-colors flex flex-col items-center justify-center gap-2"
            style={{
              borderColor: isDragging ? "#60a5fa" : "#4b5563",
            }}
            onDragEnter={() => setIsDragging(true)}
            onDragLeave={() => setIsDragging(false)}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              setIsDragging(false);
              for (const file of Array.from(e.dataTransfer.files)) {
                void handleDrop(file);
              }
            }}
          >
            <UploadIcon className="w-5 h-5 text-gray-500" />
            <p className="text-sm text-gray-400 mt-2">
              Drop files here or click to browse
            </p>
            <p className="text-xs text-gray-500 mt-1">
              Files will be captured to your inbox
            </p>
          </div>
        );

      default:
        return null;
    }
  };

  // ── Clipboard cache panel ─────────────────────────────────────────

  const renderClipboardCache = (): ReactNode => {
    if (clipboardCache.length === 0) return null;

    return (
      <div className="mt-2">
        <div className="text-xs text-gray-500 mb-1">Recent clipboard:</div>
        <div className="max-h-32 overflow-y-auto space-y-1">
          {clipboardCache
            .slice()
            .reverse()
            .map((entry, i) => (
              <div
                key={`${entry.slice(0, 40)}-${i}`}
                className="text-xs bg-gray-800 rounded px-2 py-1 truncate"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(entry);
                  } catch {
                    toast("Could not restore from clipboard", {
                      variant: "error",
                    });
                  }
                }}
                title="Click to restore"
              >
                {entry}
              </div>
            ))}
        </div>
      </div>
    );
  };

  // ── Render ───────────────────────────────────────────────────────

  return (
    <div
      className="dictation-pill flex flex-col items-center gap-2 p-3 rounded-lg shadow-lg bg-gray-800 text-gray-100"
      style={{ opacity: `${opacity}` }}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      {/* Mode selector */}
      <div className="flex gap-1">
        {(["dictation", "scratchpad", "drop"] as const).map((m) => (
          <button
            key={m}
            type="button"
            onClick={() => setMode(m)}
            className={`px-2 py-1 text-xs rounded capitalize transition-colors ${
              mode === m
                ? "bg-blue-600 text-white"
                : "bg-gray-700 text-gray-400 hover:text-gray-200"
            }`}
            aria-label={`Switch to ${m} mode`}
            aria-pressed={mode === m}
          >
            {m}
          </button>
        ))}
      </div>

      {/* Mode-specific content */}
      {renderModeContent()}

      {/* Action buttons */}
      <div className="flex gap-1">
        <button
          type="button"
          onClick={handleCopy}
          className="p-1 rounded text-gray-400 hover:text-white hover:bg-gray-700 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          aria-label="Copy to clipboard"
          title="Copy to clipboard"
        >
          <Icon name="copy" className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={handleOpenSettings}
          className="p-1 rounded text-gray-400 hover:text-white hover:bg-gray-700 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
          aria-label="Open settings"
          title="Open settings"
        >
          <Icon name="settings" className="w-4 h-4" />
        </button>
      </div>

      {/* Clipboard cache panel */}
      {renderClipboardCache()}
    </div>
  );
}
