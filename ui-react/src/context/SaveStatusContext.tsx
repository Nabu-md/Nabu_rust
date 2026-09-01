// ──────────────────────────────────────────────────────────────────────────────
// SaveStatusContext — current save state of the active document
//
// Mirrors: ui-react/src/components/contexts::SaveStatusContext
// Contract: read-only spec consumed by Wave 3 view components.
// ──────────────────────────────────────────────────────────────────────────────

import { createContext, useContext, useState, type ReactNode } from "react";

export type SaveStatusType = "unsaved" | "saving" | "saved" | "error";

export interface SaveStatusContextValue {
  /** Current save status. */
  status: SaveStatusType;
  /** Set the save status. */
  setStatus: (status: SaveStatusType) => void;
  /** RFC 3339 timestamp of the last successful save, or null. */
  lastSaved: string | null;
  /** Update the last-saved timestamp. */
  setLastSaved: (timestamp: string | null) => void;
}

const SaveStatusContext = createContext<SaveStatusContextValue | null>(null);

/** Hook to access the save-status context. */
export function useSaveStatus(): SaveStatusContextValue {
  const ctx = useContext(SaveStatusContext);
  if (!ctx) {
    throw new Error("useSaveStatus must be used within a SaveStatusProvider");
  }
  return ctx;
}

interface SaveStatusProviderProps {
  children: ReactNode;
}

/** Provider component for save-status tracking. */
export function SaveStatusProvider({ children }: SaveStatusProviderProps) {
  const [status, setStatus] = useState<SaveStatusType>("saved");
  const [lastSaved, setLastSaved] = useState<string | null>(null);

  return (
    <SaveStatusContext.Provider
      value={{ status, setStatus, lastSaved, setLastSaved }}
    >
      {children}
    </SaveStatusContext.Provider>
  );
}
