// ──────────────────────────────────────────────────────────────────────────────
// QuickCapture.tsx — quick-capture entry point + modal
//
// Mirrors the `inbox_quick_capture` IPC command from inbox.rs.  Renders a
// floating action button (bottom-right) that opens a small dialog with a
// title + content editor.  On submit it calls `inboxQuickCapture` and
// refreshes the queue so the new capture appears.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, type FormEvent } from "react";
import { inboxQuickCapture } from "../../ipc";
import { useNav, useToast } from "../../context";
import { InboxIcon } from "./icons";

export interface QuickCaptureProps {
  /** Refresh the inbox queue after a successful capture. */
  onRefresh: () => void;
}

/** Floating action button that opens the quick-capture dialog. */
export function QuickCapture({ onRefresh }: QuickCaptureProps) {
  const { toast } = useToast();
  const nav = useNav();
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");

  const handleClose = () => {
    setOpen(false);
    setTitle("");
    setContent("");
  };

  const handleCapture = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = title.trim();
    if (!trimmed) {
      toast("Quick capture needs a title", { variant: "warning" });
      return;
    }
    try {
      await inboxQuickCapture(trimmed, content);
      toast(`Captured "${trimmed}" to inbox`, { variant: "success" });
      void onRefresh();
      // Ensure the user is looking at the inbox to review the new capture.
      nav.setViewMode("Inbox");
      handleClose();
    } catch {
      toast("Could not quick-capture to inbox", { variant: "error" });
    }
  };

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="fixed bottom-6 right-6 z-20 flex items-center justify-center w-12 h-12 rounded-full shadow-lg bg-blue-600 text-white hover:bg-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-500/50"
        aria-label="Quick capture"
        title="Quick capture (opens a note into the inbox)"
      >
        <InboxIcon name="plus" className="w-5 h-5" />
      </button>

      {open && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
          onClick={handleClose}
        >
          <div
            className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-lg shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <form onSubmit={handleCapture} className="flex flex-col gap-3 p-4">
              <h3 className="text-sm font-medium text-gray-200">
                Quick Capture
              </h3>
              <input
                type="text"
                placeholder="Title"
                className="w-full bg-gray-800 text-gray-100 rounded px-3 py-2 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                autoFocus
              />
              <textarea
                placeholder="Content (optional markdown)"
                className="w-full bg-gray-800 text-gray-100 rounded px-3 py-2 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none resize-y min-h-[120px]"
                value={content}
                onChange={(e) => setContent(e.target.value)}
              />
              <div className="flex justify-end gap-2 pt-1">
                <button
                  type="button"
                  onClick={handleClose}
                  className="px-3 py-1.5 text-sm text-gray-300 bg-gray-800 rounded hover:bg-gray-700"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-3 py-1.5 text-sm text-white bg-blue-700 rounded hover:bg-blue-600"
                >
                  Capture
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </>
  );
}
