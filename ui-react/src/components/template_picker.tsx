// ──────────────────────────────────────────────────────────────────────────────
// template_picker.tsx — searchable template picker
//
// Mirrors: ui-react/src/components/template_picker.rs (TemplatePicker)
//
// Searchable list of templates. Views are projections of existing
// KnowledgeObjects — views never own data. Callers receive the selected
// `TemplateRecord` via the `onSelect` callback.
//
// React patterns: useState for search signal, pure filter projection,
// onChange for input events.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useMemo } from "react";
import type { TemplateRecord } from "../types";

export interface TemplatePickerProps {
  /** Templates to display (passed in, never owned by the picker). */
  templates: TemplateRecord[];
  /** Called when the user selects a template. */
  onSelect: (template: TemplateRecord) => void;
}

/**
 * Searchable template picker. Callers receive the selected `TemplateRecord`
 * via the `onSelect` callback.
 */
export function TemplatePicker({ templates, onSelect }: TemplatePickerProps) {
  const [search, setSearch] = useState("");

  // Re-evaluate on every render when `search` changes; useMemo mirrors the
  // Dioxus "compute during render" pattern.
  const query = search.toLowerCase();
  const filtered = useMemo(
    () =>
      query
        ? templates.filter((t) => t.name.toLowerCase().includes(query))
        : templates,
    [templates, query],
  );

  return (
    <div className="template-picker space-y-2">
      <input
        type="text"
        placeholder="Search templates..."
        className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <div className="template-list space-y-1">
        {filtered.length === 0 && (
          <p className="text-sm text-gray-500 pt-1">No templates found</p>
        )}
        {filtered.map((t) => (
          <button
            key={t.name}
            type="button"
            className="w-full text-left px-3 py-1.5 text-sm text-gray-200 hover:bg-gray-800 rounded transition-colors"
            onClick={() => onSelect(t)}
          >
            {t.name}
          </button>
        ))}
      </div>
    </div>
  );
}
