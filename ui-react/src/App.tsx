// ──────────────────────────────────────────────────────────────────────────────
// App.tsx — app router (vault-check → view switch)
//
// Mirrors: crates/nabu-ui/src/components/app.rs (App + AppRouter)
//
// On mount, calls checkVaultExists:
//  - Loading  → spinner (h-screen, NOT dvh)
//  - Ok(path) → WorkspaceLayout
//  - null     → VaultSetupWizard
//  - Error    → error text
//
// The theme is applied via the data-theme attribute on <html>.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect } from "react";
import {
  NavProvider,
  ThemeProvider,
  WorkspaceProvider,
  ToastProvider,
  HistoryProvider,
  SaveStatusProvider,
  useNav,
  useTheme,
} from "./context";
import { checkVaultExists } from "./ipc";
import { WorkspaceLayout, VaultSetupWizard } from "./components/layout";

// ── Vault check state ─────────────────────────────────────────────────

type VaultCheckState = "loading" | "setup" | "error" | "dashboard";

/**
 * Router component that checks vault state on mount and renders either
 * a loading screen, a vault-setup wizard, or the WorkspaceLayout.
 */
function AppRouter() {
  const [vaultState, setVaultState] = useState<VaultCheckState>("loading");
  const [vaultError, setVaultError] = useState<string | null>(null);
  const nav = useNav();
  const theme = useTheme();

  // Apply theme via data-theme attribute on the root <html> element.
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme.resolvedTheme);
  }, [theme.resolvedTheme]);

  // Vault check lifecycle: call check_vault_exists on mount.
  useEffect(() => {
    checkVaultExists()
      .then((path) => {
        if (path) {
          const name = path.split("/").pop() || path;
          nav.setVaultName(name);
          setVaultState("dashboard");
        } else {
          setVaultState("setup");
        }
      })
      .catch((err) => {
        setVaultError(String(err));
        setVaultState("error");
      });
  }, []);

  // ── Render ──────────────────────────────────────────────────────────
  switch (vaultState) {
    case "loading":
      return (
        <div className="flex h-screen w-screen items-center justify-center bg-gray-950 text-gray-100">
          <div className="flex flex-col items-center gap-4">
            <div className="w-6 h-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
            <div>Opening Nabu…</div>
          </div>
        </div>
      );

    case "error":
      return (
        <div className="flex h-screen w-screen items-center justify-center bg-gray-950 text-red-300">
          <div>{vaultError ?? "Unknown error"}</div>
        </div>
      );

    case "setup":
      return (
        <VaultSetupWizard
          onVaultSelected={(path) => {
            const name = path.split("/").pop() || path;
            nav.setVaultName(name);
            setVaultState("dashboard");
          }}
        />
      );

    case "dashboard":
      return <WorkspaceLayout />;
  }
}

// ── App root ────────────────────────────────────────────────────────────────

function App() {
  return (
    <ThemeProvider initialTheme="system">
      <ToastProvider>
        <HistoryProvider>
          <SaveStatusProvider>
            <NavProvider>
              <WorkspaceProvider>
                <AppRouter />
              </WorkspaceProvider>
            </NavProvider>
          </SaveStatusProvider>
        </HistoryProvider>
      </ToastProvider>
    </ThemeProvider>
  );
}

export default App;
