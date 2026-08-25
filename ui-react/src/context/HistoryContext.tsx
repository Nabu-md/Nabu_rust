// ──────────────────────────────────────────────────────────────────────────────
// HistoryContext — undo/redo state
//
// Mirrors: crates/nabu-ui/src/components/contexts::HistoryContext + HistoryProvider
// Contract: read-only spec consumed by Wave 3 view components.
// ──────────────────────────────────────────────────────────────────────────────

import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  type ReactNode,
} from "react";
import { historyStatus, historyUndo, historyRedo } from "../ipc";
import { useToast } from "./ToastContext";

export interface HistoryContextValue {
  /** Whether an undo operation is available. */
  canUndo: boolean;
  /** Whether a redo operation is available. */
  canRedo: boolean;
  /** Label of the next undo operation (if any). */
  undoLabel: string | null;
  /** Label of the next redo operation (if any). */
  redoLabel: string | null;
  /** Number of undo entries. */
  undoLen: number;
  /** Number of redo entries. */
  redoLen: number;
  /** Maximum history depth. */
  maxDepth: number;

  // ── Actions ──────────────────────────────────────────────────────────

  /** Undo the last operation. Returns the undone label or null. */
  undo: () => Promise<string | null>;
  /** Redo the last undone operation. Returns the redone label or null. */
  redo: () => Promise<string | null>;
  /** Refresh the history state from the backend. */
  refresh: () => Promise<void>;
}

const HistoryContext = createContext<HistoryContextValue | null>(null);

/** Hook to access the history context. */
export function useHistory(): HistoryContextValue {
  const ctx = useContext(HistoryContext);
  if (!ctx) {
    throw new Error("useHistory must be used within a HistoryProvider");
  }
  return ctx;
}

interface HistoryProviderProps {
  children: ReactNode;
}

/** Provider component for undo/redo history. */
export function HistoryProvider({ children }: HistoryProviderProps) {
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [undoLabel, setUndoLabel] = useState<string | null>(null);
  const [redoLabel, setRedoLabel] = useState<string | null>(null);
  const [undoLen, setUndoLen] = useState(0);
  const [redoLen, setRedoLen] = useState(0);
  const [maxDepth, setMaxDepth] = useState(100);

  const { toast } = useToast();

  /** Refresh history state from the backend. */
  const refresh = useCallback(async () => {
    try {
      const status = await historyStatus();
      setCanUndo(status.can_undo);
      setCanRedo(status.can_redo);
      setUndoLabel(status.undo_label);
      setRedoLabel(status.redo_label);
      setUndoLen(status.undo_len);
      setRedoLen(status.redo_len);
      setMaxDepth(status.max_depth);
    } catch {
      // Backend not ready or vault not open — leave state as-is.
    }
  }, []);

  /** Undo the last operation. */
  const undo = useCallback(async (): Promise<string | null> => {
    try {
      const label = await historyUndo();
      await refresh();
      if (label) {
        toast(`Undid: ${label}`, { variant: "info" });
      }
      return label;
    } catch (err) {
      toast(`Undo failed: ${String(err)}`, { variant: "error" });
      return null;
    }
  }, [refresh, toast]);

  /** Redo the last undone operation. */
  const redo = useCallback(async (): Promise<string | null> => {
    try {
      const label = await historyRedo();
      await refresh();
      if (label) {
        toast(`Redid: ${label}`, { variant: "info" });
      }
      return label;
    } catch (err) {
      toast(`Redo failed: ${String(err)}`, { variant: "error" });
      return null;
    }
  }, [refresh, toast]);

  // One-time mount: fetch initial state + install keyboard shortcuts.
  useEffect(() => {
    refresh();

    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;

      // Cmd/Ctrl+Z → undo
      if (e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
      }
      // Cmd/Ctrl+Shift+Z or Cmd/Ctrl+Y → redo
      if ((e.key === "z" && e.shiftKey) || e.key === "y") {
        e.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [refresh, undo, redo]);

  const value: HistoryContextValue = {
    canUndo,
    canRedo,
    undoLabel,
    redoLabel,
    undoLen,
    redoLen,
    maxDepth,
    undo,
    redo,
    refresh,
  };

  return (
    <HistoryContext.Provider value={value}>{children}</HistoryContext.Provider>
  );
}
