// ─────────────────────────────────────────────────────────────────────────────
// components/diagnostics — Diagnostic analysis panel
//
// Mirrors: crates/nabu-ui/src/components/diagnostics.rs
// ─────────────────────────────────────────────────────────────────────────────

import { useState } from "react";
import { Button, ButtonVariant } from "../ui";
import { diagnosticRequested } from "../ipc";

interface DiagnosticResponse {
  batch: DiagnosticBatch;
}

interface DiagnosticBatch {
  diagnostics: Diagnostic[];
  origin: string;
  resource_id: string;
}

interface Diagnostic {
  code: string | null;
  source: string | null;
  category: string | null;
  message: string;
  range: TextRange;
  severity: number;
}

interface TextRange { start: TextPosition; end: TextPosition; }
interface TextPosition { line: number; character: number; }

function severityLabel(sev: number): string {
  const labels: Record<number, string> = { 1: "Hint", 2: "Information", 3: "Warning", 4: "Error", 5: "Critical" };
  return labels[sev] ?? "Unknown";
}

function severityClass(sev: number): string {
  const map: Record<number, string> = {
    1: "text-sky-400", 2: "text-blue-400", 3: "text-amber-400",
    4: "text-red-400", 5: "text-red-600 font-bold",
  };
  return map[sev] ?? "text-gray-400";
}

function formatRange(range: TextRange): string {
  if (range.start.line === range.end.line && range.start.character === range.end.character) {
    return `L${range.start.line + 1}:${range.start.character + 1}`;
  }
  return `L${range.start.line + 1}-${range.end.line + 1}`;
}

export interface DiagnosticsPanelProps {
  resourceId: string;
  className?: string;
}

export function DiagnosticsPanel({ resourceId, className = "" }: DiagnosticsPanelProps) {
  const [text, setText] = useState("");
  const [origin, setOrigin] = useState("harper");
  const [response, setResponse] = useState<DiagnosticResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const runDiagnostics = async () => {
    if (!text.trim()) return;
    setLoading(true);
    setError("");
    try {
      const result = await diagnosticRequested({ text, resource_id: resourceId, origin });
      setResponse(result as unknown as DiagnosticResponse);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const btnLabel = loading ? "Running…" : "Run Diagnostics";
  const origins = [
    { value: "harper", label: "Harper (Spelling/Grammar)" },
    { value: "ai-assistant", label: "AI Assistant" },
    { value: "lsp-markdown", label: "LSP (Markdown)" },
    { value: "plugin", label: "Plugin" },
  ];

  return (
    <div className={["diagnostics-panel h-full flex flex-col", className].filter(Boolean).join(" ")}>
      <h2 className="text-xl font-bold mb-4">Diagnostic Analyzer</h2>

      <div className="flex gap-3 mb-4">
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Origin</label>
          <select className="input w-full" value={origin} onChange={(e) => setOrigin(e.target.value)}>
            {origins.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
          </select>
        </div>
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Resource ID</label>
          <input type="text" className="input w-full" value={resourceId} readOnly />
        </div>
      </div>

      <label className="block text-sm text-gray-400 mb-1">Text to analyze</label>
      <textarea className="input w-full h-48 font-mono text-sm"
        placeholder="Paste or edit markdown text to run diagnostics..."
        value={text} onChange={(e) => setText(e.target.value)} />

      <div className="mt-4 flex gap-2">
        <Button variant={ButtonVariant.Primary} disabled={loading} onClick={runDiagnostics}>
          {btnLabel}
        </Button>
      </div>

      {error && <ErrorPanel title="Diagnostics Error" message={error} />}
      {loading && <LoadingBlock label="Analyzing…" />}
      {response && !loading && renderResults(response)}
    </div>
  );
}

function renderResults(data: DiagnosticResponse) {
  const batch = data.batch;
  if (batch.diagnostics.length === 0) {
    return <div className="mt-4 p-4 text-center text-gray-500">No diagnostics found.</div>;
  }
  const severityOrder = [5, 4, 3, 2, 1];
  const groups = severityOrder
    .map((sev) => batch.diagnostics.filter((d) => d.severity === sev))
    .filter((g) => g.length > 0);

  return (
    <div className="mt-6 border-t border-gray-700 pt-4">
      <div className="flex justify-between items-center mb-3">
        <h3 className="text-lg font-semibold">Results ({batch.diagnostics.length} diagnostics)</h3>
        <span className="text-sm text-gray-500">Origin: {batch.origin} · Resource: {batch.resource_id}</span>
      </div>
      <div className="flex flex-wrap gap-2 mb-4">
        {severityOrder.map((sev) => {
          const count = batch.diagnostics.filter((d) => d.severity === sev).length;
          if (count === 0) return null;
          return (
            <span key={sev} className="px-2 py-1 bg-gray-800 rounded text-xs">
              <span className={severityClass(sev)}>{severityLabel(sev)}</span>
              <span className="text-gray-500 ml-1">({count})</span>
            </span>
          );
        })}
      </div>
      {groups.map((group, i) => {
        const sev = group[0].severity;
        return (
          <div key={i} className="mb-4">
            <h4 className={`${severityClass(sev)} text-md font-medium mb-2`}>
              {severityLabel(sev)} ({group.length})
            </h4>
            <ul className="space-y-2">
              {group.map((diag, j) => (
                <li key={j} className="border-l-2 border-gray-700 pl-4 pb-3">
                  <div className="flex items-baseline gap-3">
                    <span className="text-xs text-gray-500 w-16 shrink-0">{formatRange(diag.range)}</span>
                    <span className={`${severityClass(sev)} text-sm font-medium`}>{severityLabel(sev)}</span>
                    {diag.code && <span className="text-xs text-gray-600">[{diag.code}]</span>}
                    {diag.source && <span className="text-xs text-gray-600">· {diag.source}</span>}
                    {diag.category && <span className="text-xs text-gray-600">· {diag.category}</span>}
                  </div>
                  <p className="mt-1 text-sm text-gray-200">{diag.message}</p>
                </li>
              ))}
            </ul>
          </div>
        );
      })}
    </div>
  );
}

function ErrorPanel({ title, message }: { title: string; message: string }) {
  return (
    <div className="bg-red-900/20 border border-red-800 rounded-lg p-3 mt-2" role="alert">
      <p className="text-sm font-semibold text-red-300">{title}</p>
      <p className="text-xs text-red-400 mt-1">{message}</p>
    </div>
  );
}

function LoadingBlock({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center gap-2 py-4">
      <span className="animate-spin rounded-full border-2 border-current border-t-transparent w-4 h-4" />
      <span className="text-sm text-gray-400">{label}</span>
    </div>
  );
}
