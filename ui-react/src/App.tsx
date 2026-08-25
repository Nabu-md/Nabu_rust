import { useState } from "react";
import {
  NavProvider,
  ThemeProvider,
  WorkspaceProvider,
  ToastProvider,
  HistoryProvider,
  SaveStatusProvider,
  useNav,
  useWorkspace,
  useToast,
  useTheme,
  useSaveStatus,
  useHistory,
} from "./context";
import { checkVaultExists } from "./ipc";

// ── Demo component that reads all contexts ──────────────────────────────────

function DemoPanel() {
  const nav = useNav();
  const ws = useWorkspace();
  const { toast } = useToast();
  const theme = useTheme();
  const saveStatus = useSaveStatus();
  const history = useHistory();

  const [vaultPath, setVaultPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleCheckVault = async () => {
    setError(null);
    try {
      const path = await checkVaultExists();
      setVaultPath(path);
      if (path) {
        toast("Vault found!", { variant: "success" });
      } else {
        toast("No vault configured", { variant: "warning" });
      }
    } catch (err) {
      setError(String(err));
      toast(`Error: ${String(err)}`, { variant: "error" });
    }
  };

  return (
    <div className="min-h-screen bg-[#0b1220] text-gray-100 flex flex-col items-center justify-center gap-6 p-8">
      <h1 className="text-4xl font-bold tracking-tight">Nabu</h1>
      <p className="text-sm text-gray-400">Wave 2 — IPC + Types + Context Foundation</p>

      <button
        onClick={handleCheckVault}
        className="px-4 py-2 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-medium transition-colors"
      >
        Check vault
      </button>

      {vaultPath !== null && (
        <pre className="text-sm text-green-400 bg-black/30 p-4 rounded max-w-lg overflow-auto">
          {JSON.stringify(vaultPath, null, 2)}
        </pre>
      )}

      {error && (
        <pre className="text-sm text-red-400 bg-black/30 p-4 rounded max-w-lg overflow-auto">
          {error}
        </pre>
      )}

      {/* Context status indicators */}
      <div className="grid grid-cols-3 gap-4 mt-8 text-xs text-gray-400 max-w-2xl">
        <div className="bg-black/20 p-3 rounded">
          <div className="font-medium text-gray-300 mb-1">Nav</div>
          <div>viewMode: {nav.viewMode}</div>
          <div>vaultName: {nav.vaultName || "(none)"}</div>
          <div>sidebar: {nav.showLeftSidebar ? "on" : "off"}</div>
        </div>
        <div className="bg-black/20 p-3 rounded">
          <div className="font-medium text-gray-300 mb-1">Theme</div>
          <div>theme: {theme.theme}</div>
          <div>resolved: {theme.resolvedTheme}</div>
        </div>
        <div className="bg-black/20 p-3 rounded">
          <div className="font-medium text-gray-300 mb-1">Workspace</div>
          <div>tabs: {ws.tabs.length}</div>
          <div>active: {ws.activePath || "(none)"}</div>
        </div>
        <div className="bg-black/20 p-3 rounded">
          <div className="font-medium text-gray-300 mb-1">Save Status</div>
          <div>status: {saveStatus.status}</div>
          <div>lastSaved: {saveStatus.lastSaved || "(never)"}</div>
        </div>
        <div className="bg-black/20 p-3 rounded">
          <div className="font-medium text-gray-300 mb-1">History</div>
          <div>canUndo: {String(history.canUndo)}</div>
          <div>canRedo: {String(history.canRedo)}</div>
          <div>undoLen: {history.undoLen}</div>
        </div>
        <div className="bg-black/20 p-3 rounded">
          <div className="font-medium text-gray-300 mb-1">Toast</div>
          <div>active: {nav.paletteOpen ? "palette" : "closed"}</div>
          <button
            onClick={() => toast("Test toast!", { variant: "info" })}
            className="mt-1 text-indigo-400 hover:text-indigo-300 underline"
          >
            Show toast
          </button>
        </div>
      </div>
    </div>
  );
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
                <DemoPanel />
              </WorkspaceProvider>
            </NavProvider>
          </SaveStatusProvider>
        </HistoryProvider>
      </ToastProvider>
    </ThemeProvider>
  );
}

export default App;
