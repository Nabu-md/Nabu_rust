// ──────────────────────────────────────────────────────────────────────────────
// VaultSetupWizard — first-launch vault selection screen
//
// Mirrors: crates/nabu-ui/src/components/vault_setup_wizard.rs
//
// When `checkVaultExists` returns null (no vault configured), this wizard
// lets the user pick an existing directory or create a new one. On success
// it calls `completeSetup()` to materialise the backend context, then
// notifies the parent via `onVaultSelected(path)`.
// ──────────────────────────────────────────────────────────────────────────────

import { useState } from "react";
import { Icon } from "./icons";
import { checkVaultExists, completeSetup, createVaultDialog, selectVaultDialog } from "../../ipc";

export interface VaultSetupWizardProps {
  onVaultSelected: (path: string) => void;
}

/** First-launch vault selection screen. */
export function VaultSetupWizard({ onVaultSelected }: VaultSetupWizardProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pickVault = async (cmd: "select" | "create") => {
    if (loading) return;
    setLoading(true);
    setError(null);

    try {
      const dialogFn =
        cmd === "select" ? selectVaultDialog : createVaultDialog;
      const path = await dialogFn();

      if (path && path.trim()) {
        await completeSetup();
        onVaultSelected(path);
      } else {
        setError("No directory selected.");
      }
    } catch (err) {
      const fn =
        cmd === "select" ? "select_vault_dialog" : "create_vault_dialog";
      setError(`${fn} failed: ${String(err)}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex h-screen w-screen items-center justify-center bg-gray-950 text-gray-100 p-6 select-none">
      <div className="max-w-md w-full bg-gray-900 border border-gray-800 rounded-xl p-8 shadow-2xl space-y-6">
        {/* Header */}
        <div className="text-center space-y-2">
          <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-blue-600/20 text-blue-400 mb-2">
            <Icon name="book" className="w-8 h-8" />
          </div>
          <h1 className="text-2xl font-bold tracking-tight text-white">
            Welcome to Nabu
          </h1>
          <p className="text-sm text-gray-400">
            Select or create a markdown directory to initialize your knowledge
            vault.
          </p>
        </div>

        {/* Error message */}
        {error && (
          <div className="rounded-md bg-red-900/30 border border-red-700/50 text-red-300 px-4 py-3 text-sm">
            {error}
          </div>
        )}

        {/* Vault options */}
        <div className="space-y-4 pt-2">
          <button
            type="button"
            disabled={loading}
            onClick={() => pickVault("select")}
            className="w-full flex items-center gap-3 text-left px-4 py-3 bg-gray-800 border border-gray-700 rounded-lg hover:bg-gray-700 hover:border-gray-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Icon name="folderOpen" className="w-5 h-5 text-gray-300" />
            <div className="flex-1">
              <div className="text-sm font-semibold text-white">
                Select Existing Vault
              </div>
              <div className="text-xs text-gray-400">
                Open an existing folder with notes
              </div>
            </div>
            <Icon name="externalLink" className="w-4 h-4 text-gray-500" />
          </button>

          <button
            type="button"
            disabled={loading}
            onClick={() => pickVault("create")}
            className="w-full flex items-center gap-3 text-left px-4 py-3 bg-gray-800 border border-gray-700 rounded-lg hover:bg-gray-700 hover:border-gray-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Icon name="sparkles" className="w-5 h-5 text-amber-400" />
            <div className="flex-1">
              <div className="text-sm font-semibold text-white">
                Create New Vault
              </div>
              <div className="text-xs text-gray-400">
                Initialize a new directory for Nabu
              </div>
            </div>
            <Icon name="externalLink" className="w-4 h-4 text-gray-500" />
          </button>
        </div>

        {/* Loading indicator */}
        {loading && (
          <div className="flex items-center justify-center gap-2 text-xs text-blue-400 pt-2">
            <div className="w-3 h-3 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
            <span>Opening system dialog…</span>
          </div>
        )}
      </div>
    </div>
  );
}

// Re-export checkVaultExists for potential use at the App level
export { checkVaultExists };
