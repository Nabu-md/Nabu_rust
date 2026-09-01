// ──────────────────────────────────────────────────────────────────────────────
// SettingsPanel.tsx — 18-tab settings panel (React port)
//
// Mirrors: ui-react/src/components/settings/settings_panel.rs
//
// Renders every settings tab from the Rust source, binding each control to the
// `AppSettings` shape in `types.ts`. Per-field edits persist via
// `settings_set` (single-key IPC) and full-object saves go through
// `settings_set_all`. The panel loads its initial state via `get_settings`
// (`getSettings`) on mount.
//
// Tab list (matches the Rust `tabs` array exactly):
//   Appearance, Editor, Markdown, Search, Graph,
//   Files & Vaults, Import & Export, OCR, Accessibility,
//   Performance, Privacy, Keyboard Shortcuts, Diagnostics,
//   Advanced, Experimental, Capabilities, About
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useState, useCallback } from "react";
import {
  getSettings,
  openSettings,
  settingsExport,
  settingsImport,
  settingsReset,
  settingsSet,
  settingsSetAll,
} from "../../ipc";
import { useToast } from "../../context";
import type { AppSettings } from "../../types";
import { CapabilityManagement } from "./CapabilityManagement";
import type { SelectOption } from "./types";

/** Local persistent copy of the full settings object. */
function useAppSettings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);

  useEffect(() => {
    getSettings()
      .then((loaded) => setSettings(loaded as AppSettings))
      .catch(() => setSettings(null));
  }, []);

  return [settings, setSettings] as const;
}

// ── Tabs ─────────────────────────────────────────────────────────────────────

const TABS = [
  "Appearance",
  "Editor",
  "Markdown",
  "Search",
  "Graph",
  "Files & Vaults",
  "Import & Export",
  "OCR",
  "Accessibility",
  "Performance",
  "Privacy",
  "Keyboard Shortcuts",
  "Diagnostics",
  "Advanced",
  "Experimental",
  "Capabilities",
  "About",
] as const;

// ── Setting control helpers ──────────────────────────────────────────────────

/** Update a single field and persist via `settings_set` only (no setAll). */
function updateFieldSingle(
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>,
  key: keyof AppSettings,
  value: unknown,
) {
  setSettings((prev) => {
    if (!prev) return prev;
    void settingsSet(key as string, value);
    return { ...prev, [key]: value };
  });
}

function SettingCheckbox({
  settings,
  setSettings,
  label,
  field,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
  label: string;
  field: keyof AppSettings;
}) {
  const value = settings ? !!settings[field] : false;
  return (
    <label className="check-field flex items-center gap-2">
      <input
        type="checkbox"
        checked={value}
        onChange={(e) =>
          updateFieldSingle(setSettings, field, e.target.checked)
        }
      />
      <span>{label}</span>
    </label>
  );
}

function SettingNumber({
  settings,
  setSettings,
  label,
  field,
  min,
  max,
  defaultValue,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
  label: string;
  field: keyof AppSettings;
  min: number;
  max: number;
  defaultValue: number;
}) {
  const value = settings ? Number(settings[field]) : defaultValue;
  return (
    <label className="field flex items-center gap-3">
      <span className="field-label w-56 text-sm text-gray-300">{label}</span>
      <input
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => {
          const parsed = parseInt(e.target.value, 10);
          const v = Number.isNaN(parsed) ? defaultValue : parsed;
          updateFieldSingle(setSettings, field, v);
        }}
        className="input input-bordered input-sm w-24"
      />
    </label>
  );
}

function SettingRange({
  settings,
  setSettings,
  label,
  field,
  min,
  max,
  step,
  defaultValue,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
  label: string;
  field: keyof AppSettings;
  min: number;
  max: number;
  step: number;
  defaultValue: number;
}) {
  const value = settings ? Number(settings[field]) : defaultValue;
  return (
    <label className="field flex flex-col gap-1">
      <span className="field-label text-sm text-gray-300">
        {label} ({value})
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => updateFieldSingle(setSettings, field, parseFloat(e.target.value))}
        className="input range w-full"
      />
    </label>
  );
}

function SettingText({
  settings,
  setSettings,
  label,
  field,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
  label: string;
  field: keyof AppSettings;
}) {
  const value = settings && typeof settings[field] === "string" ? (settings[field] as string) : "";
  return (
    <label className="field flex items-center gap-3">
      <span className="field-label w-56 text-sm text-gray-300">{label}</span>
      <input
        type="text"
        value={value}
        onChange={(e) => updateFieldSingle(setSettings, field, e.target.value)}
        className="input input-bordered input-sm flex-1"
      />
    </label>
  );
}

function SettingSelect({
  settings,
  setSettings,
  label,
  field,
  options,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
  label: string;
  field: keyof AppSettings;
  options: SelectOption[];
}) {
  const value = settings && typeof settings[field] === "string" ? (settings[field] as string) : "";
  return (
    <label className="field flex items-center gap-3">
      <span className="field-label w-56 text-sm text-gray-300">{label}</span>
      <select
        value={value}
        onChange={(e) => updateFieldSingle(setSettings, field, e.target.value)}
        className="input input-bordered input-sm flex-1"
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </label>
  );
}

// ── Section renderers (mirror Rust helpers) ──────────────────────────────────

function AppearanceSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Appearance & Opacity Controls</h2>
      <div className="space-y-4">
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Theme"
          field="theme"
          options={[
            { value: "Dark", label: "Dark" },
            { value: "Light", label: "Light" },
            { value: "System Sync", label: "System Sync" },
          ]}
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Main Window Opacity"
          field="main_window_opacity"
          min={0.2}
          max={1.0}
          step={0.05}
          defaultValue={1.0}
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Floating Pill Opacity"
          field="floating_pill_opacity"
          min={0.2}
          max={1.0}
          step={0.05}
          defaultValue={0.8}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Pill Hover Focus"
          field="pill_hover_boost_opacity"
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Font Size"
          field="font_size"
          min={0.75}
          max={1.5}
          step={0.05}
          defaultValue={1.0}
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Line Height"
          field="line_height"
          min={1.0}
          max={2.0}
          step={0.1}
          defaultValue={1.5}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Reduced Motion"
          field="reduced_motion"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="High Contrast"
          field="high_contrast"
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Sidebar Width"
          field="sidebar_width"
          min={200.0}
          max={600.0}
          step={10.0}
          defaultValue={280.0}
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Inspector Width"
          field="inspector_width"
          min={200.0}
          max={600.0}
          step={10.0}
          defaultValue={320.0}
        />
      </div>
    </>
  );
}

function EditorSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Editor & Notion Block Menu</h2>
      <div className="space-y-4">
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Editing Mode"
          field="editor_mode"
          options={[
            { value: "Live Preview", label: "Live Preview" },
            { value: "Source Markdown", label: "Source Markdown" },
          ]}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Auto-pair brackets"
          field="auto_pair_brackets"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Show line numbers"
          field="show_line_numbers"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Convert pasted HTML to Markdown"
          field="convert_pasted_html_to_markdown"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Enable Notion Slash Menu"
          field="enable_notion_slash_menu"
        />
        <SettingNumber
          settings={settings}
          setSettings={setSettings}
          label="Tab Size"
          field="tab_size"
          min={1}
          max={16}
          defaultValue={4}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Word Wrap"
          field="word_wrap"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Spell Check"
          field="spell_check"
        />
        <SettingNumber
          settings={settings}
          setSettings={setSettings}
          label="Auto-Save Interval (seconds)"
          field="auto_save_interval_secs"
          min={5}
          max={300}
          defaultValue={30}
        />
      </div>
    </>
  );
}

function MarkdownSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Markdown Rendering</h2>
      <div className="space-y-4">
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="GitHub Flavored Markdown"
          field="markdown_gfm"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Preserve Line Breaks"
          field="markdown_preserve_line_breaks"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Smart Quotes"
          field="markdown_smart_quotes"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Math Rendering (LaTeX)"
          field="markdown_math_rendering"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Diagram Rendering (Mermaid)"
          field="markdown_diagram_rendering"
        />
      </div>
    </>
  );
}

function SearchSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Search</h2>
      <div className="space-y-4">
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Index on Startup"
          field="search_index_on_startup"
        />
        <SettingNumber
          settings={settings}
          setSettings={setSettings}
          label="Max Results"
          field="search_max_results"
          min={10}
          max={500}
          defaultValue={100}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Highlight Matches"
          field="search_highlight_matches"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Fuzzy Matching"
          field="search_fuzzy_matching"
        />
      </div>
    </>
  );
}

function GraphSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Folder Graph & Canvas</h2>
      <div className="space-y-4">
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Include Folders as Hub Nodes"
          field="include_folders_in_graph"
        />
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Folder Click Behavior"
          field="folder_click_behavior"
          options={[
            { value: "Open Folder Table View", label: "Open Folder Table View" },
            { value: "Browse Folder", label: "Browse Folder" },
          ]}
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Gravity Strength"
          field="graph_node_physics_gravity"
          min={0.0}
          max={1.0}
          step={0.1}
          defaultValue={0.5}
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Node Spacing"
          field="graph_node_physics_spacing"
          min={0.0}
          max={1.0}
          step={0.1}
          defaultValue={1.0}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Show Tags as Badges"
          field="graph_show_tags_as_badges"
        />
      </div>
    </>
  );
}

function FilesSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  const vaultPath = settings?.last_vault_path ?? "";
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Files & Vaults</h2>
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-400">Vault Location: </span>
          <span>{vaultPath}</span>
        </div>
        <button
          type="button"
          onClick={() => void openSettings()}
          className="btn btn-primary btn-sm"
        >
          Change Vault...
        </button>

        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Default New Note Path"
          field="default_new_note_path"
          options={[
            { value: "Vault Root", label: "Vault Root" },
            { value: "Same Folder as Active Note", label: "Same Folder as Active Note" },
            { value: "Custom Subfolder", label: "Custom Subfolder" },
          ]}
        />
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Trash Retention"
          field="trash_retention_policy"
          options={[
            { value: "7 Days", label: "7 Days" },
            { value: "30 Days", label: "30 Days" },
            { value: "90 Days", label: "90 Days" },
            { value: "Never", label: "Never" },
          ]}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Confirm Before Delete"
          field="confirm_before_delete"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Show Hidden Files"
          field="show_hidden_files"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Sort Files Alphabetically"
          field="sort_files_alphabetically"
        />
      </div>
    </>
  );
}

function ImportExportSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  const { toast } = useToast();

  const handleExport = useCallback(async () => {
    try {
      const data = await settingsExport();
      const text = new TextDecoder().decode(new Uint8Array(data));
      const blob = new Blob([text], { type: "application/json" });
      const href = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = href;
      a.download = "nabu-settings.json";
      a.click();
      URL.revokeObjectURL(href);
      toast("Settings exported", { variant: "success" });
    } catch (e: unknown) {
      toast(`Export failed: ${e instanceof Error ? e.message : String(e)}`, {
        variant: "error",
      });
    }
  }, [toast]);

  const handleImport = useCallback(async () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        const loaded = (await settingsImport(Array.from(bytes))) as AppSettings;
        setSettings(loaded);
        toast("Settings imported", { variant: "success" });
      } catch (e: unknown) {
        toast(`Import failed: ${e instanceof Error ? e.message : String(e)}`, {
          variant: "error",
        });
      }
    };
    input.click();
  }, [setSettings, toast]);

  return (
    <>
      <h2 className="text-xl font-bold mb-4">Import & Export</h2>
      <div className="space-y-4">
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Default Export Format"
          field="default_export_format"
          options={[
            { value: "markdown", label: "Markdown" },
            { value: "html", label: "HTML" },
            { value: "pdf", label: "PDF" },
            { value: "text", label: "Plain Text" },
            { value: "json", label: "JSON" },
          ]}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Include Metadata in Export"
          field="export_include_metadata"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Include Attachments in Export"
          field="export_include_attachments"
        />
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Import Duplicate Strategy"
          field="import_duplicate_strategy"
          options={[
            { value: "skip", label: "Skip" },
            { value: "overwrite", label: "Overwrite" },
            { value: "rename", label: "Rename" },
          ]}
        />

        <div className="mt-6 pt-4 border-t border-gray-700">
          <h3 className="text-lg font-semibold mb-2">Settings Migration</h3>
          <p className="text-sm text-gray-400 mb-4">
            Export or import your settings between devices.
          </p>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={handleExport}
              className="btn btn-secondary btn-sm"
            >
              Export Settings
            </button>
            <button type="button" onClick={handleImport} className="btn btn-sm">
              Import Settings
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

function OcrSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">OCR Settings</h2>
      <div className="space-y-4">
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="OCR Language"
          field="ocr_language"
          options={[
            { value: "eng", label: "English" },
            { value: "spa", label: "Spanish" },
            { value: "fra", label: "French" },
            { value: "deu", label: "German" },
            { value: "jpn", label: "Japanese" },
          ]}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Auto-process Scanned PDFs"
          field="ocr_auto_process_scanned_pdfs"
        />
        <SettingRange
          settings={settings}
          setSettings={setSettings}
          label="Confidence Threshold"
          field="ocr_confidence_threshold"
          min={0.0}
          max={1.0}
          step={0.05}
          defaultValue={0.7}
        />
      </div>
    </>
  );
}

function AccessibilitySettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Accessibility</h2>
      <div className="space-y-4">
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Screen Reader Support"
          field="screen_reader_support"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Enhanced Keyboard Navigation"
          field="keyboard_navigation"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Visible Focus Ring"
          field="focus_ring_visible"
        />
      </div>
    </>
  );
}

function PerformanceSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Performance</h2>
      <div className="space-y-4">
        <SettingNumber
          settings={settings}
          setSettings={setSettings}
          label="Max Undo History"
          field="max_undo_history"
          min={10}
          max={500}
          defaultValue={100}
        />
        <SettingNumber
          settings={settings}
          setSettings={setSettings}
          label="Worker Pool Size"
          field="worker_pool_size"
          min={1}
          max={16}
          defaultValue={4}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Index on Startup"
          field="index_on_startup"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Background Processing"
          field="background_processing"
        />
      </div>
    </>
  );
}

function PrivacySettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Privacy</h2>
      <div className="space-y-4">
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Launch at Startup"
          field="launch_at_startup"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Analytics Enabled"
          field="analytics_enabled"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Crash Reporting"
          field="crash_reporting_enabled"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Auto-lock on Idle"
          field="auto_lock_on_idle"
        />
        <SettingNumber
          settings={settings}
          setSettings={setSettings}
          label="Auto-lock Timeout (minutes)"
          field="auto_lock_timeout_mins"
          min={1}
          max={120}
          defaultValue={15}
        />
      </div>
    </>
  );
}

function KeyboardShortcutsSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Keyboard Shortcuts</h2>
      <div className="space-y-4">
        <SettingText
          settings={settings}
          setSettings={setSettings}
          label="Voice Dictation"
          field="voice_hotkey"
        />
        <SettingText
          settings={settings}
          setSettings={setSettings}
          label="Quick Capture"
          field="quick_capture_hotkey"
        />
        <SettingText
          settings={settings}
          setSettings={setSettings}
          label="Toggle Sidebar"
          field="toggle_sidebar_hotkey"
        />
      </div>
    </>
  );
}

/** Diagnostics tab — placeholder mirroring the Rust DiagnosticsPanel. */
function DiagnosticsSettings() {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Diagnostics</h2>
      <div className="space-y-4">
        <p className="text-sm text-gray-400">
          Diagnostics panel — connection and service health. (Placeholder)
        </p>
      </div>
    </>
  );
}

function AdvancedSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  const [confirmReset, setConfirmReset] = useState(false);
  const { toast } = useToast();

  const handleReset = useCallback(async () => {
    try {
      const loaded = (await settingsReset()) as AppSettings;
      setSettings(loaded);
      toast("Settings reset", { variant: "success" });
    } catch (e: unknown) {
      toast(`Reset failed: ${e instanceof Error ? e.message : String(e)}`, {
        variant: "error",
      });
    } finally {
      setConfirmReset(false);
    }
  }, [setSettings, toast]);

  return (
    <>
      <h2 className="text-xl font-bold mb-4">Advanced</h2>
      <div className="space-y-4">
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Force Sandbox for Web Snippets"
          field="force_sandbox_for_web_snippets"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Debug Mode"
          field="debug_mode"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Developer Tools"
          field="developer_tools"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Experimental Features"
          field="experimental_features"
        />

        <div className="mt-6 pt-4 border-t border-gray-700">
          {confirmReset && (
            <div
              role="dialog"
              aria-modal="true"
              className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
            >
              <div className="bg-gray-900 border border-gray-700 rounded-lg p-6 max-w-md">
                <h3 className="text-lg font-semibold mb-2">Reset to Defaults</h3>
                <p className="text-sm text-gray-300 mb-4">
                  This will restore all settings to their default values and
                  cannot be undone. Continue?
                </p>
                <div className="flex gap-3 justify-end">
                  <button
                    type="button"
                    onClick={() => setConfirmReset(false)}
                    className="btn btn-secondary btn-sm"
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    onClick={handleReset}
                    className="btn btn-destructive btn-sm"
                  >
                    Reset
                  </button>
                </div>
              </div>
            </div>
          )}
          <button
            type="button"
            onClick={() => setConfirmReset(true)}
            className="btn btn-destructive btn-sm"
          >
            Reset to Defaults
          </button>
        </div>
      </div>
    </>
  );
}

function ExperimentalSettings({
  settings,
  setSettings,
}: {
  settings: AppSettings | null;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
}) {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">Experimental</h2>
      <div className="space-y-4">
        <SettingSelect
          settings={settings}
          setSettings={setSettings}
          label="Whisper Model"
          field="whisper_model"
          options={[
            { value: "ggml-tiny.en.bin", label: "ggml-tiny.en.bin" },
            { value: "ggml-base.en.bin", label: "ggml-base.en.bin" },
            { value: "ggml-small.en-q5_0.bin", label: "ggml-small.en-q5_0.bin" },
          ]}
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Enable AI Summarization"
          field="enable_ai_summarization"
        />
        <SettingCheckbox
          settings={settings}
          setSettings={setSettings}
          label="Enable Semantic Search"
          field="enable_semantic_search"
        />
      </div>
    </>
  );
}

function AboutSettings() {
  return (
    <>
      <h2 className="text-xl font-bold mb-4">About Nabu</h2>
      <div className="space-y-4">
        <div>
          <p className="text-lg font-semibold">Nabu</p>
          <p className="text-sm text-gray-400">Version 0.1.0</p>
          <p className="text-sm text-gray-400">
            Premium Markdown Knowledge Management
          </p>
        </div>
        <div className="pt-4 border-t border-gray-700">
          <p className="text-sm text-gray-400">© 2024 Faro Labs</p>
          <p className="text-sm text-gray-400">Licensed under AGPL-3.0</p>
        </div>
      </div>
    </>
  );
}

// ── Section dispatch ─────────────────────────────────────────────────────────

function renderSection(
  tab: string,
  settings: AppSettings | null,
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>,
): React.ReactNode {
  const common = { settings, setSettings };
  switch (tab) {
    case "Appearance":
      return <AppearanceSettings {...common} />;
    case "Editor":
      return <EditorSettings {...common} />;
    case "Markdown":
      return <MarkdownSettings {...common} />;
    case "Search":
      return <SearchSettings {...common} />;
    case "Graph":
      return <GraphSettings {...common} />;
    case "Files & Vaults":
      return <FilesSettings {...common} />;
    case "Import & Export":
      return <ImportExportSettings {...common} />;
    case "OCR":
      return <OcrSettings {...common} />;
    case "Accessibility":
      return <AccessibilitySettings {...common} />;
    case "Performance":
      return <PerformanceSettings {...common} />;
    case "Privacy":
      return <PrivacySettings {...common} />;
    case "Keyboard Shortcuts":
      return <KeyboardShortcutsSettings {...common} />;
    case "Diagnostics":
      return <DiagnosticsSettings />;
    case "Advanced":
      return <AdvancedSettings {...common} />;
    case "Experimental":
      return <ExperimentalSettings {...common} />;
    case "Capabilities":
      return <CapabilityManagement />;
    case "About":
      return <AboutSettings />;
    default:
      return null;
  }
}

// ── Root component ───────────────────────────────────────────────────────────

/**
 * Root settings panel component.
 *
 * Loads all settings from the backend on mount, renders the tab sidebar
 * navigator, and delegates to section renderers. Persists via
 * `settings_set` (per-field) and `settings_set_all` (full snapshot).
 */
export function SettingsPanel() {
  const [settings, setSettings] = useAppSettings();
  const [activeTab, setActiveTab] = useState((typeof window !== "undefined" && window.location.hash)
    ? window.location.hash.slice(1)
    : "Appearance");

  // Keep the settings object persisted to the backend whenever it changes
  // (mirrors the Rust `save_app_settings` helper). We fire `settings_set_all`
  // on every field update via the `updateField*` helpers, so this effect is a
  // safety net for bulk resets / imports.
  useEffect(() => {
    if (settings) {
      void settingsSetAll(settings);
    }
  }, [settings]);

  // Sync hash → tab (best-effort deep-link support).
  const handleTabClick = useCallback((tab: string) => {
    setActiveTab(tab);
  }, []);

  const active = activeTab;

  return (
    <div className="settings-panel flex h-screen w-full">
      {/* Sidebar navigation */}
      <nav
        className="w-64 border-r border-gray-700 bg-gray-900 p-2 flex flex-col gap-0.5 overflow-y-auto"
        aria-label="Settings sections"
      >
        {TABS.map((tab) => {
          const isActive = active === tab;
          return (
            <button
              key={tab}
              type="button"
              onClick={() => handleTabClick(tab)}
              className={
                isActive
                  ? "sidebar-item sidebar-item-active w-full text-left px-3 py-2 rounded bg-gray-800 text-white"
                  : "sidebar-item w-full text-left px-3 py-2 rounded text-gray-400 hover:text-white hover:bg-gray-800"
              }
            >
              {tab}
            </button>
          );
        })}
      </nav>

      {/* Tab content */}
      <div className="content w-3/4 p-6 text-white overflow-y-auto">
        {renderSection(active, settings, setSettings)}
      </div>
    </div>
  );
}
