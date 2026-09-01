// ──────────────────────────────────────────────────────────────────────────────
// template_editor.tsx — backend-wired template manager
//
// Mirrors: ui-react/src/components/template_editor.rs (TemplateEditor)
//
// Wires the template manager to the real backend (`template_list` /
// `template_save` / `template_delete` / `template_duplicate` /
// `template_set_favourite`), persisted in settings:
//
// - browse templates with search and category grouping
// - create / edit (name, description, icon, category, default folder, body)
// - duplicate, delete, and favourite (star) any template
//
// React patterns: useState + useEffect for lifecycle, useCallback for IPC
// handlers, pre-computed derived values before JSX.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useCallback } from "react";
import {
  templateList,
  templateSave,
  templateDelete,
  templateDuplicate,
  templateSetFavourite,
} from "../ipc";
import { useToast } from "../context";
import type { TemplateRecord } from "../types";
import { Icon } from "./layout/icons";

// ── Internal tab enum ────────────────────────────────────────────────────────

type TemplateTab = "list" | "create" | "edit";

// ── IPC helpers ──────────────────────────────────────────────────────────────

/** Loads the template list from the backend via `template_list`. */
async function loadTemplates(
  setTemplates: (templates: TemplateRecord[]) => void,
): Promise<void> {
  try {
    const list = await templateList();
    setTemplates(list);
  } catch {
    // Keep existing state on failure; the caller surfaces errors via toasts
    // where appropriate. The list simply won't refresh.
  }
}

// ── Component ────────────────────────────────────────────────────────────────

/**
 * The template manager workspace (self-contained, backend-backed).
 *
 * Supports three tabs: List (browse + manage templates), Create (new template
 * form), and Edit (modify an existing template). Uses the toast system for
 * user feedback on save/delete/duplicate/favourite actions.
 */
export function TemplateEditor() {
  const toasts = useToast();

  // ── Reactive state ──
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [activeTab, setActiveTab] = useState<TemplateTab>("list");
  const [editingTemplate, setEditingTemplate] = useState<TemplateRecord | null>(
    null,
  );
  const [searchQuery, setSearchQuery] = useState("");
  const [saving, setSaving] = useState(false);

  // Create-form local state
  const [newName, setNewName] = useState("");
  const [newBody, setNewBody] = useState("");
  const [newIcon, setNewIcon] = useState("📋");
  const [newCategory, setNewCategory] = useState("");
  const [newFolder, setNewFolder] = useState("");
  const [newDesc, setNewDesc] = useState("");

  // Edit-form local state
  const [editName, setEditName] = useState("");
  const [editBody, setEditBody] = useState("");
  const [editDesc, setEditDesc] = useState("");
  const [editIcon, setEditIcon] = useState("");
  const [editCategory, setEditCategory] = useState("");
  const [editFolder, setEditFolder] = useState("");

  // ── Initial load ──
  useEffect(() => {
    loadTemplates(setTemplates);
  }, []);

  // ── Derived: filtered templates (precompute before JSX) ──
  const query = searchQuery.toLowerCase();
  const filtered: TemplateRecord[] = query
    ? templates.filter(
        (t) =>
          t.name.toLowerCase().includes(query) ||
          (t.category ? t.category.toLowerCase().includes(query) : false),
      )
    : templates;

  // ── Handlers ──

  const saveTemplate = useCallback(
    async (t: TemplateRecord) => {
      setSaving(true);
      try {
        await templateSave(t);
        await loadTemplates(setTemplates);
        setNewName("");
        setNewBody("");
        setNewIcon("📋");
        setNewCategory("");
        setNewFolder("");
        setNewDesc("");
        setEditingTemplate(null);
        setActiveTab("list");
        toasts.toast("Saved", { variant: "success" });
      } catch {
        toasts.toast("Could not save that template", { variant: "error" });
      } finally {
        setSaving(false);
      }
    },
    [toasts],
  );

  const deleteTemplate = useCallback(
    async (name: string) => {
      try {
        await templateDelete(name);
        await loadTemplates(setTemplates);
        toasts.toast("Deleted", { variant: "success" });
      } catch {
        toasts.toast("Could not delete that template", { variant: "error" });
      }
    },
    [toasts],
  );

  const duplicateTemplate = useCallback(
    async (name: string) => {
      try {
        await templateDuplicate(name);
        await loadTemplates(setTemplates);
        toasts.toast("Duplicated", { variant: "success" });
      } catch {
        toasts.toast("Could not duplicate that template", { variant: "error" });
      }
    },
    [toasts],
  );

  const toggleFavourite = useCallback(
    async (name: string, favourite: boolean) => {
      try {
        await templateSetFavourite(name, favourite);
        await loadTemplates(setTemplates);
      } catch {
        // Best-effort; the list will show the current state.
      }
    },
    [],
  );

  const handleCreate = useCallback(() => {
    const name = newName.trim();
    if (!name) {
      toasts.toast("Give the template a name first.", { variant: "warning" });
      return;
    }
    const templateToSave: TemplateRecord = {
      name,
      description: newDesc.trim() ? newDesc : null,
      icon: newIcon.trim() ? newIcon : null,
      default_folder: newFolder.trim() ? newFolder : null,
      category: newCategory.trim() ? newCategory : null,
      favourite: false,
      frontmatter_defaults: {},
      property_presets: {},
      body: newBody,
      object_type: null,
    };
    saveTemplate(templateToSave);
  }, [newName, newBody, newIcon, newCategory, newFolder, newDesc, saveTemplate, toasts]);

  const handleEdit = useCallback(() => {
    if (!editingTemplate) return;
    const updated: TemplateRecord = {
      ...editingTemplate,
      name: editName,
      description: editDesc.trim() ? editDesc : null,
      icon: editIcon.trim() ? editIcon : null,
      default_folder: editFolder.trim() ? editFolder : null,
      category: editCategory.trim() ? editCategory : null,
      body: editBody,
    };
    saveTemplate(updated);
  }, [
    editingTemplate,
    editName,
    editBody,
    editDesc,
    editIcon,
    editCategory,
    editFolder,
    saveTemplate,
  ]);

  const startEditing = useCallback((t: TemplateRecord) => {
    setEditName(t.name);
    setEditBody(t.body);
    setEditDesc(t.description ?? "");
    setEditIcon(t.icon ?? "");
    setEditCategory(t.category ?? "");
    setEditFolder(t.default_folder ?? "");
    setEditingTemplate(t);
    setActiveTab("edit");
  }, []);

  // ── Pre-compute tab button classes ──
  const listTabClass =
    activeTab === "list"
      ? "px-3 py-1 text-sm rounded bg-blue-700 text-white"
      : "px-3 py-1 text-sm rounded text-gray-400 hover:text-gray-200";

  const createTabClass =
    activeTab === "create"
      ? "px-3 py-1 text-sm rounded bg-blue-700 text-white"
      : "px-3 py-1 text-sm rounded text-gray-400 hover:text-gray-200";

  // ── Render ──
  return (
    <div className="template-editor space-y-4 h-screen overflow-auto p-4">
      {/* Header */}
      <div>
        <h1 className="text-xl font-semibold text-gray-100">Templates</h1>
        <p className="text-sm text-gray-400 mt-1">
          Reusable note skeletons with variables, dates and property presets.
        </p>
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-2 border-b border-gray-700 pb-2">
        <button
          type="button"
          className={listTabClass}
          onClick={() => setActiveTab("list")}
        >
          Templates
        </button>
        <button
          type="button"
          className={createTabClass}
          onClick={() => {
            setNewName("");
            setNewBody("");
            setNewIcon("📋");
            setNewCategory("");
            setNewFolder("");
            setNewDesc("");
            setActiveTab("create");
          }}
        >
          + New Template
        </button>
      </div>

      {/* Tab content */}
      {activeTab === "list" && (
        <div className="space-y-2">
          <input
            type="text"
            placeholder="Search templates or categories…"
            className="input w-full"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          {filtered.length === 0 ? (
            <div className="text-sm text-gray-500 py-6 text-center">
              No templates yet — create your first one.
            </div>
          ) : (
            <div className="divide-y divide-gray-800">
              {filtered.map((template) => (
                <TemplateListItem
                  key={template.name}
                  template={template}
                  onEdit={() => startEditing(template)}
                  onDuplicate={() => duplicateTemplate(template.name)}
                  onDelete={() => deleteTemplate(template.name)}
                  onToggleFavourite={(f: boolean) =>
                    toggleFavourite(template.name, f)
                  }
                />
              ))}
            </div>
          )}
        </div>
      )}

      {activeTab === "create" && (
        <CreateTemplateForm
          name={newName}
          setName={setNewName}
          body={newBody}
          setBody={setNewBody}
          icon={newIcon}
          setIcon={setNewIcon}
          category={newCategory}
          setCategory={setNewCategory}
          folder={newFolder}
          setFolder={setNewFolder}
          desc={newDesc}
          setDesc={setNewDesc}
          saving={saving}
          onSave={handleCreate}
          onCancel={() => setActiveTab("list")}
        />
      )}

      {activeTab === "edit" && editingTemplate && (
        <EditTemplateForm
          name={editName}
          setName={setEditName}
          body={editBody}
          setBody={setEditBody}
          icon={editIcon}
          setIcon={setEditIcon}
          category={editCategory}
          setCategory={setEditCategory}
          folder={editFolder}
          setFolder={setEditFolder}
          desc={editDesc}
          setDesc={setEditDesc}
          saving={saving}
          onSave={handleEdit}
          onCancel={() => {
            setEditingTemplate(null);
            setActiveTab("list");
          }}
        />
      )}
    </div>
  );
}

// ── Sub-components ───────────────────────────────────────────────────────────

interface TemplateListItemProps {
  template: TemplateRecord;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onToggleFavourite: (favourite: boolean) => void;
}

/** A single template card in the list view, with star/duplicate/edit/delete. */
function TemplateListItem({
  template,
  onEdit,
  onDuplicate,
  onDelete,
  onToggleFavourite,
}: TemplateListItemProps) {
  const {
    name,
    description,
    icon,
    category,
    default_folder,
    favourite,
  } = template;

  const iconVal = icon || "📋";
  const descVal = description || "";
  const folderVal = default_folder || "";
  const categoryVal = category || "";

  return (
    <div className="bg-gray-800 rounded-lg border border-gray-700 p-3 hover:border-gray-600 transition-colors flex items-start gap-3">
      <span className="text-xl" aria-hidden="true">
        {iconVal}
      </span>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <h4 className="text-sm font-medium text-gray-200 truncate">
            {name}
          </h4>
          {favourite && (
            <span
              className="text-xs text-yellow-400"
              title="Favourite"
              aria-label="Favourite"
            >
              ★
            </span>
          )}
          {categoryVal && (
            <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-gray-700 text-gray-400">
              {categoryVal}
            </span>
          )}
        </div>
        {descVal && (
          <p className="text-xs text-gray-500 mt-1">{descVal}</p>
        )}
        {folderVal && (
          <span className="text-xs text-blue-400 mt-1 block">
            📁 {folderVal}
          </span>
        )}
      </div>
      <div className="flex gap-1 flex-none">
        <button
          type="button"
          className="btn btn-sm btn-ghost"
          title={favourite ? "Unfavourite" : "Favourite"}
          aria-label={favourite ? "Unfavourite" : "Favourite"}
          onClick={() => onToggleFavourite(!favourite)}
        >
          {favourite ? "★" : "☆"}
        </button>
        <button
          type="button"
          className="btn btn-sm btn-ghost"
          title="Duplicate"
          aria-label="Duplicate"
          onClick={onDuplicate}
        >
          <Icon name="copy" className="w-4 h-4" />
        </button>
        <button
          type="button"
          className="text-xs text-blue-400 hover:text-blue-300 px-2 py-1"
          onClick={onEdit}
        >
          Edit
        </button>
        <button
          type="button"
          className="text-xs text-red-400 hover:text-red-300 px-2 py-1"
          onClick={onDelete}
        >
          Delete
        </button>
      </div>
    </div>
  );
}

interface TemplateFormProps {
  name: string;
  setName: (v: string) => void;
  body: string;
  setBody: (v: string) => void;
  icon: string;
  setIcon: (v: string) => void;
  category: string;
  setCategory: (v: string) => void;
  folder: string;
  setFolder: (v: string) => void;
  desc: string;
  setDesc: (v: string) => void;
  saving: boolean;
  onSave: () => void;
  onCancel: () => void;
}

/** Shared form layout used by both Create and Edit tabs. */
function CreateTemplateForm(props: TemplateFormProps) {
  const {
    name, setName, body, setBody, icon, setIcon,
    category, setCategory, folder, setFolder,
    desc, setDesc, saving, onSave, onCancel,
  } = props;

  return (
    <div className="space-y-3 bg-gray-800 rounded-lg border border-gray-700 p-4">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <div>
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Template Name
          </label>
          <input
            type="text"
            className="input w-full mt-1"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Template"
          />
        </div>
        <div>
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Icon (emoji)
          </label>
          <input
            type="text"
            className="input w-full mt-1"
            value={icon}
            onChange={(e) => setIcon(e.target.value)}
            placeholder="📋"
          />
        </div>
        <div>
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Category
          </label>
          <input
            type="text"
            className="input w-full mt-1"
            value={category}
            onChange={(e) => setCategory(e.target.value)}
            placeholder="Work / Personal / Journal…"
          />
        </div>
        <div>
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Default Folder
          </label>
          <input
            type="text"
            className="input w-full mt-1"
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
            placeholder="projects (optional)"
          />
        </div>
        <div className="md:col-span-2">
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Description
          </label>
          <input
            type="text"
            className="input w-full mt-1"
            value={desc}
            onChange={(e) => setDesc(e.target.value)}
            placeholder="What is this template for?"
          />
        </div>
        <div className="md:col-span-2">
          <label className="text-xs text-gray-500 uppercase tracking-wide">
            Body (Markdown)
          </label>
          <textarea
            className="input w-full mt-1 h-40 resize-y font-mono"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="# {{title}}

Tags:
"
          />
          <p className="text-xs text-gray-500 mt-1">
            Supports &#123;&#123;title&#125;&#125;, &#123;&#123;date&#125;&#125;, &#123;&#123;tags&#125;&#125; and &#123;&#123;custom&#125;&#125; variables.
          </p>
        </div>
      </div>
      <div className="flex gap-2">
        <button
          type="button"
          className="px-3 py-1 text-sm rounded bg-blue-700 text-white disabled:opacity-50"
          disabled={saving}
          onClick={onSave}
        >
          {saving ? "Saving…" : "Save Template"}
        </button>
        <button
          type="button"
          className="px-3 py-1 text-sm rounded text-gray-400 hover:text-gray-200"
          onClick={onCancel}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

// Re-export as alias for edit tab (same form layout).
const EditTemplateForm = CreateTemplateForm;
