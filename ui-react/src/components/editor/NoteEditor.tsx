// ──────────────────────────────────────────────────────────────────────────────
// NoteEditor.tsx — editable note surface
//
// Mirrors: ui-react/src/components/note_editor.rs (NoteEditor)
//
// Autosaves the current content to the backend (`note_save`) after a short
// debounce, drives the shared SaveStatusContext indicator, and renders a live
// preview of the markdown via NoteView.
//
// Phase 12.1: drag-and-drop into the editor inserts wikilinks, images, or
// file links at the cursor.
//
// Keyboard shortcuts:
// - Cmd/Ctrl + B → **bold**
// - Cmd/Ctrl + I → *italic*
// - `/` (at line start) → opens the SlashMenu
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback } from "react";
import { noteRead, noteSave } from "../../ipc";
import { useWorkspace, useSaveStatus, useToast } from "../../context";
import type { SaveStatusType } from "../../context";
import { NoteView } from "../note_view";
import { SlashMenu } from "./SlashMenu";

/** MIME type for internal note drags from the file tree. */
const NABU_NOTE_MIME = "application/x-nabu-note";

/** Debounce delay (ms) between the last keystroke and an autosave. */
const AUTOSAVE_DELAY_MS = 800;

/** Returns `true` when the file name looks like an image. */
function isImage(name: string): boolean {
  const lower = name.toLowerCase();
  return ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif"].some(
    (ext) => lower.endsWith(ext),
  );
}

/** Inserts `snippet` at the textarea's cursor position (or appends). */
function insertAtCursor(
  textarea: HTMLTextAreaElement | null,
  snippet: string,
): string {
  if (textarea) {
    const end = textarea.selectionEnd;
    const value = textarea.value;
    const newValue = value.slice(0, end) + snippet + value.slice(end);
    const newCaret = end + snippet.length;
    // Defer DOM mutations to let React's controlled update settle.
    requestAnimationFrame(() => {
      textarea.setSelectionRange(newCaret, newCaret);
    });
    return newValue;
  }
  // Fallback: append.
  return snippet;
}

/** Wraps the current textarea selection in `**...**` (bold) or `*...*` (italic). */
function wrapSelection(
  textarea: HTMLTextAreaElement | null,
  content: string,
  before: string,
  after: string,
): string {
  if (!textarea) return content;
  const start = textarea.selectionStart;
  const end = textarea.selectionEnd;
  if (start === end) {
    return content;
  }
  const selected = content.slice(start, end);
  const newValue =
    content.slice(0, start) + before + selected + after + content.slice(end);
  const caret = end + before.length + after.length;
  requestAnimationFrame(() => {
    textarea.setSelectionRange(caret, caret);
  });
  return newValue;
}

/** Handles drag-and-drop into the editor. */
function handleEditorDrop(
  ev: React.DragEvent<HTMLTextAreaElement>,
  content: string,
  textarea: HTMLTextAreaElement | null,
): string {
  let snippets: string[] = [];

  // Internal note (from the file tree)?
  const nabuPath = ev.dataTransfer.getData(NABU_NOTE_MIME);
  if (nabuPath) {
    const stem = nabuPath
      .split("/")
      .pop()
      ?.replace(/\.md$/, "") ?? nabuPath;
    return insertAtCursor(textarea, `[[${stem}]]`);
  }

  // External files
  if (ev.dataTransfer.files && ev.dataTransfer.files.length > 0) {
    for (const file of Array.from(ev.dataTransfer.files)) {
      const name = file.name;
      if (!name) continue;
      let snippet: string;
      if (isImage(name)) {
        snippet = `![${name}](${name})`;
      } else if (name.toLowerCase().endsWith(".md")) {
        snippet = `[[${name.replace(/\.md$/, "")}]]`;
      } else {
        snippet = `[${name}](${name})`;
      }
      snippets.push(snippet);
    }
  } else {
    // Plain text drop
    const text = ev.dataTransfer.getData("text/plain");
    if (text) {
      snippets = [text];
    }
  }

  if (snippets.length > 0) {
    return insertAtCursor(textarea, snippets.join("\n"));
  }
  return content;
}

export interface NoteEditorProps {
  /** Height override; defaults to full height. */
  className?: string;
}

/**
 * The note editor component.
 *
 * On mount / active-path change: loads the note via `note_read`.
 * Empty result → "new note" hint (not an error).
 * On content change: debounces and calls `note_save`.
 * Keyboard: `/` opens SlashMenu, Cmd/Ctrl+B/I wraps selection.
 * Drag/Drop: inserts wikilinks, images, file links, or plain text.
 */
export function NoteEditor({ className = "" }: NoteEditorProps) {
  const ws = useWorkspace();
  const saveStatus = useSaveStatus();
  const toasts = useToast();

  const {
    status: saveStatusType,
    setStatus,
    setLastSaved,
  } = saveStatus;

  // ── Content & edit state ──────────────────────────────────────────────
  const [content, setContent] = useState("");
  const [dirty, setDirty] = useState(0);
  const [hasUnsaved, setHasUnsaved] = useState(false);
  const [noteLoaded, setNoteLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [noteMissing, setNoteMissing] = useState(false);
  const [showMenu, setShowMenu] = useState(false);

  // Ref to the textarea element (for cursor manipulation, selection wrapping).
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Refs to latest state for async callbacks (autosave, load).
  const contentRef = useRef(content);
  const activePathRef = useRef(ws.activePath);

  // Keep refs in sync with state.
  contentRef.current = content;
  activePathRef.current = ws.activePath;

  // ── Load note content on mount / active path change ───────────────────
  useEffect(() => {
    const path = ws.activePath;
    if (!path) {
      return;
    }
    // Don't reload if there are unsaved changes.
    if (hasUnsaved) {
      return;
    }

    const loadPath = path;
    async function loadNote() {
      setNoteLoaded(false);
      setLoadError(null);
      setNoteMissing(false);
      setContent("");

      try {
        const saved = await noteRead(loadPath);
        if (saved !== undefined && saved !== null) {
          if (saved.length > 0) {
            setContent(saved);
          } else {
            // Empty content → the note does not exist yet.
            setNoteMissing(true);
          }
        } else {
          setLoadError("Note content could not be loaded.");
        }
      } catch (err) {
        setLoadError(
          err instanceof Error ? err.message : String(err),
        );
      } finally {
        setNoteLoaded(true);
      }
    }

    loadNote();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ws.activePath]);

  // ── Debounced autosave ────────────────────────────────────────────────
  useEffect(() => {
    // Read dirty to trigger this effect when it changes.
    // The actual content comes from contentRef (latest value).
    void dirty;
    const path = ws.activePath;
    if (!path) return;

    setStatus("saving" as SaveStatusType);
    setLastSaved(`Saving ${path}`);

    const timer = setTimeout(async () => {
      const current = contentRef.current;
      try {
        await noteSave(path, current);
        setStatus("saved" as SaveStatusType);
        setLastSaved(path);
        setHasUnsaved(false);
      } catch {
        setStatus("error" as SaveStatusType);
        setLastSaved(null);
        toasts.toast("Save failed", {
          variant: "error",
          duration: 4000,
        });
      }
    }, AUTOSAVE_DELAY_MS);

    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dirty]);

  // ── Periodic retry of a failed save ───────────────────────────────────
  useEffect(() => {
    if (saveStatusType === "error") {
      // Bump dirty to trigger a retry save.
      setDirty((d) => d + 1);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [saveStatusType]);

  // ── Slash menu callback ───────────────────────────────────────────────
  const onSlash = useCallback((item: string) => {
    setShowMenu(false);
    // Insert the markdown at the cursor.
    const ta = textareaRef.current;
    // Map slash menu items to their markdown equivalents.
    let snippet: string | null = null;
    if (item.startsWith("#")) {
      snippet = item; // "# Heading 1" etc.
    }
    if (snippet) {
      const newContent = insertAtCursor(ta, snippet + "\n");
      setContent(newContent);
      setDirty((d) => d + 1);
      setHasUnsaved(true);
    }
  }, []);

  // ── Input change handler ──────────────────────────────────────────────
  const handleContentChange = useCallback(
    (ev: React.ChangeEvent<HTMLTextAreaElement>) => {
      const val = ev.target.value;
      setContent(val);
      setDirty((d) => d + 1);
      setHasUnsaved(true);
    },
    [],
  );

  // ── Keyboard handler ──────────────────────────────────────────────────
  const handleKeyDown = useCallback(
    (ev: React.KeyboardEvent<HTMLTextAreaElement>) => {
      const meta = ev.metaKey || ev.ctrlKey;

      if (ev.key === "/") {
        setShowMenu(true);
      }

      if (meta && ev.key.toLowerCase() === "b") {
        ev.preventDefault();
        const ta = textareaRef.current;
        setContent((prev) =>
          wrapSelection(ta, prev, "**", "**"),
        );
        setDirty((d) => d + 1);
        setHasUnsaved(true);
      } else if (meta && ev.key.toLowerCase() === "i") {
        ev.preventDefault();
        const ta = textareaRef.current;
        setContent((prev) =>
          wrapSelection(ta, prev, "*", "*"),
        );
        setDirty((d) => d + 1);
        setHasUnsaved(true);
      }
    },
    [],
  );

  // ── Drag handlers ─────────────────────────────────────────────────────
  const handleDragOver = useCallback((ev: React.DragEvent) => {
    ev.preventDefault();
  }, []);

  const handleDrop = useCallback(
    (ev: React.DragEvent<HTMLTextAreaElement>) => {
      ev.preventDefault();
      const ta = textareaRef.current;
      setContent((prev) => handleEditorDrop(ev, prev, ta));
      setDirty((d) => d + 1);
      setHasUnsaved(true);
    },
    [],
  );

  // ── Retry load ────────────────────────────────────────────────────────
  const onRetryLoad = useCallback(() => {
    const path = ws.activePath;
    if (!path) return;
    setNoteLoaded(false);
    setLoadError(null);
    setNoteMissing(false);
    setContent("");
    void (async () => {
      try {
        const saved = await noteRead(path);
        if (saved && saved.length > 0) {
          setContent(saved);
        } else {
          setNoteMissing(true);
        }
      } catch (err) {
        setLoadError(
          err instanceof Error ? err.message : String(err),
        );
      } finally {
        setNoteLoaded(true);
      }
    })();
  }, [ws.activePath]);

  // ── Render prep ───────────────────────────────────────────────────────
  const activeLabel = ws.activePath || "new_note.md";

  const renderContent = () => {
    if (!noteLoaded) {
      // Loading — show skeleton rows
      return (
        <div className="flex-1 flex items-center justify-center">
          <div className="space-y-2 w-3/4">
            {Array.from({ length: 6 }).map((_, i) => (
              <div
                key={i}
                className="h-3 bg-gray-800 rounded animate-pulse"
                style={{ width: `${60 - i * 10}%` }}
              />
            ))}
          </div>
        </div>
      );
    }

    if (loadError) {
      // Error panel
      return (
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
      );
    }

    return (
      <div className="relative flex-1 flex flex-col">
        {/* New note / save-failed alerts */}
        {noteMissing && (
          <div className="mb-2 p-2 border border-blue-900/50 bg-blue-950/30 rounded text-sm">
            <span className="text-blue-300 font-medium">New note</span>
            <span className="text-blue-300/80">
              {" — This note doesn't exist yet. Start typing and it will be "}
              created on save.
            </span>
          </div>
        )}
        {saveStatusType === "error" && (
          <div className="mb-2 p-2 border border-red-900/50 bg-red-950/30 rounded text-sm">
            <span className="text-red-300 font-medium">Save failed</span>
            <span className="text-red-300/80">
              {" — Your latest changes were not saved. Editing continues "}
              locally and will retry automatically.
            </span>
          </div>
        )}

        {/* Editor textarea — controlled */}
        <textarea
          ref={textareaRef}
          className="editor-textarea flex-1 resize-none bg-transparent text-gray-200 font-mono text-sm outline-none"
          value={content}
          onChange={handleContentChange}
          onKeyDown={handleKeyDown}
          onDragOver={handleDragOver}
          onDrop={handleDrop}
          spellCheck
        />

        {/* Live preview */}
        <NoteView content={content} />
      </div>
    );
  };

  return (
    <div
      className={`note-editor relative h-full flex flex-col ${className}`.trim()}
    >
      {/* Header bar — shows active note label */}
      <div className="flex items-center justify-between px-1 pb-1 text-xs text-gray-500">
        <span className="truncate">{activeLabel}</span>
      </div>

      {/* Content area */}
      {renderContent()}

      {/* Slash menu */}
      {showMenu && <SlashMenu onSelect={onSlash} />}
    </div>
  );
}
