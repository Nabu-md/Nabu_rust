// ──────────────────────────────────────────────────────────────────────────────
// CapabilityManagement.tsx — capability management settings page
//
// Mirrors: crates/nabu-ui/src/components/settings/capability_management.rs
//
// Lists all registered capabilities with their current enabled state and
// provider. Users can toggle enable/disable, which calls the
// `capability_enable` / `capability_disable` IPC commands. Toasts fire on
// success/failure.
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useState, useCallback } from "react";
import {
  capabilityDisable,
  capabilityEnable,
  capabilityListWithState,
} from "../../ipc";
import { useToast } from "../../context";
import type { CapabilitySummaryWithState } from "../../types";

type StatusKind = "success" | "warning" | "info" | "error" | "neutral";

interface ToggleState {
  enabled: boolean;
  pending: boolean;
  error: string | null;
}

function statusKindFor(enabled: boolean): StatusKind {
  return enabled ? "success" : "warning";
}

function statusLabel(enabled: boolean): string {
  return enabled ? "Enabled" : "Disabled";
}

/** Root component for capability management. */
export function CapabilityManagement() {
  const [caps, setCaps] = useState<CapabilitySummaryWithState[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { toast } = useToast();

  const loadCapabilities = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await capabilityListWithState();
      setCaps(list);
    } catch (e: unknown) {
      setError(
        `Could not contact the Tauri backend. Ensure Nabu is running. (${
          e instanceof Error ? e.message : String(e)
        })`,
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadCapabilities();
  }, [loadCapabilities]);

  const total = caps.length;
  const enabledCount = caps.filter((c) => c.enabled).length;
  const disabledCount = total - enabledCount;

  if (error) {
    return (
      <div className="capability-management flex flex-col gap-4">
        <div className="flex items-center justify-between">
          <h2 className="text-xl font-bold">Capabilities</h2>
          <button
            type="button"
            onClick={() => void loadCapabilities()}
            className="btn btn-ghost btn-sm"
            aria-label="Refresh capability list"
          >
            Refresh
          </button>
        </div>
        <div
          className="p-4 bg-red-900/20 border border-red-700 rounded-lg text-red-300"
          role="alert"
        >
          <p className="font-semibold">Failed to load capabilities</p>
          <p className="text-sm">{error}</p>
          <p className="text-sm mt-1">
            Check that the Tauri backend is running.
          </p>
        </div>
      </div>
    );
  }

  if (loading && caps.length === 0) {
    return (
      <div className="capability-management flex flex-col gap-4">
        <h2 className="text-xl font-bold">Capabilities</h2>
        <div className="flex items-center gap-2 text-gray-400">
          <div className="w-4 h-4 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
          Loading capabilities…
        </div>
      </div>
    );
  }

  if (caps.length === 0 && !loading) {
    return (
      <div className="capability-management flex flex-col gap-4">
        <h2 className="text-xl font-bold">Capabilities</h2>
        <div className="text-center py-12 text-gray-400">
          <div className="text-lg mb-1">No capabilities registered</div>
          <p className="text-sm">
            No capabilities are currently available on this platform.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="capability-management flex flex-col gap-4">
      {/* Header with refresh */}
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Capabilities</h2>
        <button
          type="button"
          onClick={() => void loadCapabilities()}
          disabled={loading}
          className="btn btn-ghost btn-sm"
          aria-label="Refresh capability list"
        >
          Refresh
        </button>
      </div>

      {/* Summary bar */}
      <div
        className="flex items-center gap-4 text-sm text-gray-400"
        aria-label={`Capability summary: ${total} total, ${enabledCount} enabled, ${disabledCount} disabled`}
      >
        <StatusDot kind="success" label={`Enabled: ${enabledCount}`} />
        <span>{enabledCount} enabled</span>
        <StatusDot kind="warning" label={`Disabled: ${disabledCount}`} />
        <span>{disabledCount} disabled</span>
        <span>— {total} total</span>
      </div>

      {/* Capability list */}
      <div className="capability-list border border-gray-700 rounded-lg overflow-hidden">
        {/* Table header */}
        <div className="flex items-center gap-4 px-4 py-2 bg-gray-800 border-b border-gray-700 text-xs font-medium text-gray-400 uppercase">
          <div className="w-[300px]">Capability</div>
          <div className="flex-1">Description</div>
          <div className="w-32">Provider</div>
          <div className="w-24 text-center">Status</div>
          <div className="w-32 text-center">Enabled</div>
        </div>
        {/* Rows */}
        <div className="divide-y divide-gray-700">
          {caps.map((cap) => (
            <CapabilityRow
              key={cap.id}
              cap={cap}
              onToggle={loadCapabilities}
              toast={toast}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

/** A single capability row with metadata, status indicator, and enable/disable toggle. */
function CapabilityRow({
  cap,
  onToggle,
  toast,
}: {
  cap: CapabilitySummaryWithState;
  onToggle: () => void;
  toast: (message: string, opts?: { variant?: "info" | "success" | "warning" | "error" }) => void;
}) {
  const displayName = cap.name;
  const [toggle, setToggle] = useState<ToggleState>({
    enabled: cap.enabled,
    pending: false,
    error: null,
  });

  const handleToggle = useCallback(
    async (newVal: boolean) => {
      setToggle((prev) => ({ ...prev, pending: true, error: null }));
      const verb = newVal ? "enabled" : "disabled";
      const action = newVal ? "Enable" : "Disable";

      try {
        if (newVal) {
          await capabilityEnable(cap.id);
        } else {
          await capabilityDisable(cap.id);
        }
        setToggle({ enabled: newVal, pending: false, error: null });
        toast(`${action} succeeded`, { variant: "success" });
        void onToggle();
      } catch (e: unknown) {
        // Rollback
        setToggle({
          enabled: !newVal,
          pending: false,
          error: `${verb} operation failed`,
        });
        toast(`${action} failed: ${e instanceof Error ? e.message : String(e)}`, {
          variant: "error",
        });
      }
    },
    [cap, onToggle, toast],
  );

  const kind = statusKindFor(toggle.enabled);
  const label = statusLabel(toggle.enabled);
  const isDisabled = toggle.pending;

  return (
    <div className="capability-row flex items-center gap-4 px-4 py-3">
      <div className="w-[300px] flex items-center gap-2">
        <div className="font-medium text-gray-100">{displayName}</div>
      </div>
      <div className="flex-1 text-sm text-gray-400">{cap.description}</div>
      <div className="w-32 text-sm text-gray-500 truncate" title={cap.provider}>
        {cap.provider}
      </div>
      <div className="w-24 flex items-center justify-center">
        <StatusDot kind={kind} label={label} />
      </div>
      <div className="w-32 flex items-center justify-center">
        <label className="relative inline-flex h-6 w-12 items-center rounded-full">
          <input
            type="checkbox"
            checked={toggle.enabled}
            disabled={isDisabled}
            onChange={(e) => void handleToggle(e.target.checked)}
            className="sr-only"
            aria-label={`Enable "${displayName}"`}
          />
          <span
            className={`inline-block h-6 w-12 rounded-full transition-colors ${
              toggle.enabled ? "bg-blue-500" : "bg-gray-600"
            } ${isDisabled ? "opacity-50" : ""}`}
          >
            <span
              className={`inline-block h-5 w-5 transform rounded-full bg-white transition-transform ${
                toggle.enabled ? "translate-x-6" : "translate-x-1"
              }`}
            />
          </span>
        </label>
      </div>
      {toggle.error && (
        <span className="text-xs text-red-400">{toggle.error}</span>
      )}
    </div>
  );
}

/** Small status dot (mirrors Rust `StatusDot`). */
function StatusDot({ kind, label }: { kind: StatusKind; label: string }) {
  const colorClass = {
    success: "bg-green-400",
    warning: "bg-amber-400",
    info: "bg-blue-400",
    error: "bg-red-400",
    neutral: "bg-gray-400",
  }[kind];
  return (
    <span
      className="inline-flex items-center gap-1"
      aria-label={label}
      title={label}
    >
      <span className={`h-2 w-2 rounded-full ${colorClass}`} aria-hidden="true" />
      {label}
    </span>
  );
}
