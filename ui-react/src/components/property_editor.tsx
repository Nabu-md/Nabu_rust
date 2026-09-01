// ─────────────────────────────────────────────────────────────────────────────
// components/property_editor — Metadata property editing interface
//
// Mirrors: ui-react/src/components/property_editor.rs
// ─────────────────────────────────────────────────────────────────────────────

import { useMemo } from "react";
import { TextInput, NumberInput } from "../ui";
import type {
  PropertyDefinition,
  PropertyType,
  PropertyValue,
  ValidationState,
} from "../types";

export interface PropertyEditorProps {
  properties: PropertyDefinition[];
  values: Record<string, PropertyValue>;
  onChange?: (id: string, value: PropertyValue | null) => void;
  onValidate?: (id: string, state: ValidationState) => void;
  customProperties?: PropertyDefinition[];
  className?: string;
}

export interface PropertyFieldProps {
  id: string;
  definition: PropertyDefinition;
  value: PropertyValue | null;
  onChange?: (id: string, value: PropertyValue | null) => void;
  onValidate?: (id: string, state: ValidationState) => void;
  isCustom?: boolean;
}

interface BaseFieldProps {
  id: string;
  definition: PropertyDefinition;
  value: PropertyValue | null;
  onChange?: (id: string, value: PropertyValue | null) => void;
  onValidate?: (id: string, state: ValidationState) => void;
  isCustom?: boolean;
}

function valueToText(value: PropertyValue | null, ptype: PropertyType): string {
  if (!value || value.type !== ptype) return "";
  switch (value.type) {
    case "text": return value.value;
    case "url": return value.value;
    case "number": return value.value === 0 ? "" : String(value.value);
    case "date": return value.value;
    case "select": return value.value;
    case "multi-select": return value.value.join(", ");
    default: return "";
  }
}

function textToValue(raw: string, ptype: PropertyType): PropertyValue | null {
  const v = raw.trim();
  if (v === "") return null;
  switch (ptype) {
    case "text": return { type: "text", value: v };
    case "url": return { type: "url", value: v };
    case "number": { const n = parseFloat(v); return Number.isNaN(n) ? null : { type: "number", value: n }; }
    case "date": return { type: "date", value: v };
    case "select": return { type: "select", value: v };
    case "multi-select": return { type: "multi-select", value: v.split(",").map((s) => s.trim()).filter(Boolean) };
    default: return null;
  }
}

function validateUrl(text: string): ValidationState {
  if (text === "" || text.startsWith("http://") || text.startsWith("https://") || text.startsWith("mailto:")) return "valid";
  return { invalid: "URL must start with http://, https://, or mailto:" };
}

function validateNumber(text: string): ValidationState {
  if (text === "") return "valid";
  const n = parseFloat(text);
  return Number.isNaN(n) ? { invalid: "Must be a valid number" } : "valid";
}

export function PropertyEditor({
  properties,
  values,
  onChange,
  onValidate,
  customProperties,
  className = "",
}: PropertyEditorProps) {
  const rows = useMemo(
    () => properties.map((def) => ({ id: def.id, def, value: values[def.id] ?? null })),
    [properties, values]
  );
  const customRows = useMemo(
    () => customProperties?.map((def) => ({ id: def.id, def, value: values[def.id] ?? null })) ?? [],
    [customProperties, values]
  );

  return (
    <div className={["property-editor space-y-3", className].filter(Boolean).join(" ")} role="group" aria-label="Note properties">
      {rows.map((row) => (
        <div key={row.id} className="property-field">
          <PropertyField id={row.id} definition={row.def} value={row.value} onChange={onChange} onValidate={onValidate} />
        </div>
      ))}
      {customRows.length > 0 && (
        <>
          <div className="property-section" />
          <div className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-2">Custom Properties</div>
          {customRows.map((row) => (
            <div key={row.id} className="property-field">
              <PropertyField id={row.id} definition={row.def} value={row.value} onChange={onChange} onValidate={onValidate} isCustom />
            </div>
          ))}
        </>
      )}
    </div>
  );
}

export function PropertyField({ id, definition, value, onChange, onValidate, isCustom = false }: PropertyFieldProps) {
  const ptype = definition.property_type;
  switch (ptype) {
    case "text": return <TextField id={id} definition={definition} value={value} onChange={onChange} onValidate={onValidate} isCustom={isCustom} />;
    case "number": return <NumberField id={id} definition={definition} value={value} onChange={onChange} onValidate={onValidate} isCustom={isCustom} />;
    case "date": return <DateField id={id} definition={definition} value={value} onChange={onChange} onValidate={onValidate} isCustom={isCustom} />;
    case "select": return <SelectField id={id} definition={definition} value={value} onChange={onChange} onValidate={onValidate} isCustom={isCustom} />;
    case "multi-select": return <MultiSelectField id={id} definition={definition} value={value} onChange={onChange} onValidate={onValidate} isCustom={isCustom} />;
    case "url": return <UrlField id={id} definition={definition} value={value} onChange={onChange} onValidate={onValidate} isCustom={isCustom} />;
    default: return null;
  }
}

function TextLabel({ definition, isCustom }: { definition: PropertyDefinition; isCustom?: boolean }) {
  return (
    <label className="field-label flex items-center gap-1">
      <span className={isCustom ? "text-gray-400" : "text-gray-200"}>{definition.display_name}</span>
      {definition.description && <span className="text-gray-600 ml-1">— {definition.description}</span>}
    </label>
  );
}

function TextField({ id, definition, value, onChange, onValidate, isCustom }: BaseFieldProps) {
  const current = valueToText(value, "text");
  return (
    <div className="field flex flex-col gap-1">
      <TextLabel definition={definition} isCustom={isCustom} />
      <TextInput className="w-full" placeholder={definition.description ?? ""} value={current}
        onChange={(e) => { onChange?.(id, textToValue(e.target.value, "text")); onValidate?.(id, "valid"); }} />
    </div>
  );
}

function NumberField({ id, definition, value, onChange, onValidate, isCustom }: BaseFieldProps) {
  const current = valueToText(value, "number");
  return (
    <div className="field flex flex-col gap-1">
      <TextLabel definition={definition} isCustom={isCustom} />
      <NumberInput className="w-full" placeholder={definition.description ?? ""}
        value={current === "" ? undefined : current}
        onChange={(e) => { const val = e.target.value; onChange?.(id, textToValue(val, "number")); onValidate?.(id, validateNumber(val)); }} />
    </div>
  );
}

function DateField({ id, definition, value, onChange, onValidate, isCustom }: BaseFieldProps) {
  const current = valueToText(value, "date");
  return (
    <div className="field flex flex-col gap-1">
      <TextLabel definition={definition} isCustom={isCustom} />
      <input type="date" className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
        value={current} onChange={(e) => { const val = e.target.value; onChange?.(id, textToValue(val, "date")); onValidate?.(id, "valid"); }} />
    </div>
  );
}

function SelectField({ id, definition, value, onChange, onValidate, isCustom }: BaseFieldProps) {
  const current = valueToText(value, "select");
  const options = definition.options ?? [];
  return (
    <div className="field flex flex-col gap-1">
      <TextLabel definition={definition} isCustom={isCustom} />
      <select className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none"
        value={current} onChange={(e) => { const val = e.target.value; onChange?.(id, textToValue(val, "select")); onValidate?.(id, "valid"); }}>
        <option value="" disabled>-- Select --</option>
        {options.map((opt) => <option key={opt} value={opt}>{opt}</option>)}
      </select>
    </div>
  );
}

function MultiSelectField({ id, definition, value, onChange, onValidate, isCustom }: BaseFieldProps) {
  const options = definition.options ?? [];
  const current: string[] = value && value.type === "multi-select" ? value.value : [];
  const toggle = (opt: string) => {
    const next = current.includes(opt) ? current.filter((v) => v !== opt) : [...current, opt];
    onChange?.(id, { type: "multi-select", value: next });
    onValidate?.(id, "valid");
  };
  return (
    <div className="field flex flex-col gap-1">
      <TextLabel definition={definition} isCustom={isCustom} />
      <div className="flex flex-wrap gap-1">
        {options.map((opt) => {
          const isSelected = current.includes(opt);
          return (
            <button key={opt} type="button"
              className={isSelected ? "px-2 py-0.5 text-xs rounded-full border bg-blue-700 border-blue-500 text-blue-100" : "px-2 py-0.5 text-xs rounded-full border border-gray-600 text-gray-400 hover:border-gray-500"}
              onClick={() => toggle(opt)}>{opt}</button>
          );
        })}
      </div>
    </div>
  );
}

function UrlField({ id, definition, value, onChange, onValidate, isCustom }: BaseFieldProps) {
  const current = valueToText(value, "url");
  return (
    <div className="field flex flex-col gap-1">
      <TextLabel definition={definition} isCustom={isCustom} />
      <TextInput className="w-full" type="url" placeholder="https://..." value={current}
        onChange={(e) => { const val = e.target.value; onChange?.(id, textToValue(val, "url")); onValidate?.(id, validateUrl(val)); }} />
    </div>
  );
}
