// ──────────────────────────────────────────────────────────────────────────────
// recovery/recoveryBanner.tsx — crash-recovery prompt banner
//
// Mirrors: ui-react/src/components/recovery/recovery_banner.rs
//          (RecoveryBanner)
//
// Shown at the top of the dashboard when the previous run ended
// unexpectedly. Offers "Restore session", "Inspect", and "Discard" actions.
// Uses the session + recovery IPC helpers to clear server-side state.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useCallback } from "react";
import { useToast } from "../../context";
import {
  recoveryCheck,
  recoveryDiscard,
  sessionClear,
} from "../../ipc";
import type { RecoveryStatus, SessionState } from "../../types";
import { Icon } from "../layout/icons";

/** Props for {@link RecoveryBanner}. */
export interface RecoveryBannerProps {
  /**
   * Called with the recovered session when the user clicks "Restore session".
   * The parent is responsible for replaying the session state (open tabs,
   * view mode, active note, etc.).
   */
  onRestore?: (session: SessionState) => void;
  /**
   * Called when the user clicks "Inspect" to navigate to the Recovery Manager.
   */
  onInspect?: () => void;
}

/**
 * Crash-recovery banner.
 *
 * On mount it calls `recovery_check` to determine whether the previous run
 * crashed and whether a session is available. The banner is hidden once the
 * user restores or discards the session.
 */
export function RecoveryBanner({ onRestore, onInspect }: RecoveryBannerProps) {
  const toasts = useToast();
  const [status, setStatus] = useState<RecoveryStatus | null>(null);
  const [loading, setLoading] = useState(true);

  const checkRecovery = useCallback(async () => {
    setLoading(true);
    try {
      const result = await recoveryCheck();
      setStatus(result);
    } catch {
      setStatus(null);
    } finally {
      setLoading(false);
    }
  }, []);

  // Check for a pending crash recovery on mount.
  useEffect(() => {
    void checkRecovery();
  }, [checkRecovery]);

  if (loading || status === null || !status.has_session) {
    // Don't render the banner if we're still loading, there's no status,
    // or there's genuinely nothing to recover.
    if (loading) return null;
    return null;
  }

  const handleDiscard = () => {
    recoveryDiscard();
    sessionClear();
    setStatus(null);
    toasts.toast("The previous session was discarded.", { variant: "info" });
  };

  const handleRestore = () => {
    if (status.session && status.crashed) {
      recoveryDiscard();
      sessionClear();
      setStatus(null);
      onRestore?.(status.session);
    } else if (status.session) {
      recoveryDiscard();
      sessionClear();
      setStatus(null);
      onRestore?.(status.session);
    }
  };

  const handleInspect = () => {
    onInspect?.();
  };

  const bodyText = status.crashed
    ? "Nabu closed unexpectedly last time. You can restore your previous session — nothing is lost."
    : "A saved session is available from a previous run.";

  const savedAt = status.session?.saved_at;

  return (
    <div
      className="recovery-banner flex items-center gap-3 px-4 py-2.5 border-b border-blue-900/50 bg-blue-900/20 text-gray-200"
      role="status"
      aria-live="polite"
    >
      <Icon
        name="lifeBuoy"
        className="w-5 h-5 text-blue-400 flex-none"
        aria-hidden="true"
      />

      <div className="flex-1 min-w-0">
        <div className="text-sm font-semibold text-gray-100">
          Recover previous session?
        </div>
        <div className="text-xs text-gray-300">
          {bodyText}
          {savedAt && savedAt.length > 0
            ? ` Saved ${savedAt}.`
            : ""}
        </div>
      </div>

      <div className="flex items-center gap-2 flex-none">
        <button
          type="button"
          onClick={handleRestore}
          className="px-3 py-1 text-xs rounded bg-blue-600 hover:bg-blue-500 text-white transition-colors"
        >
          Restore session
        </button>
        <button
          type="button"
          onClick={handleInspect}
          className="px-3 py-1 text-xs rounded border border-gray-600 text-gray-300 hover:bg-gray-700 transition-colors"
        >
          Inspect
        </button>
        <button
          type="button"
          onClick={handleDiscard}
          className="px-3 py-1 text-xs rounded border border-gray-600 text-gray-300 hover:bg-gray-700 transition-colors"
        >
          Discard
        </button>
      </div>
    </div>
  );
}
