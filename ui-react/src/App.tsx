import { useState } from "react";
import { checkVaultExists } from "./ipc";

function App() {
  const [vaultPath, setVaultPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleClick = async () => {
    setError(null);
    try {
      const path = await checkVaultExists();
      setVaultPath(path);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="min-h-screen bg-[#0b1220] text-gray-100 flex flex-col items-center justify-center gap-6">
      <h1 className="text-4xl font-bold tracking-tight">Nabu</h1>
      <button
        onClick={handleClick}
        className="px-4 py-2 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-medium transition-colors"
      >
        Check vault
      </button>
      {vaultPath !== null && (
        <pre className="text-sm text-green-400 bg-black/30 p-4 rounded">
          {JSON.stringify(vaultPath, null, 2)}
        </pre>
      )}
      {error && (
        <pre className="text-sm text-red-400 bg-black/30 p-4 rounded">
          {error}
        </pre>
      )}
    </div>
  );
}

export default App;
