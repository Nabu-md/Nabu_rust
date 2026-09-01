// ─────────────────────────────────────────────────────────────────────────────
// components/relation_editor — Knowledge graph relation management
//
// Mirrors: ui-react/src/components/relation_editor.rs
// ─────────────────────────────────────────────────────────────────────────────

import { useMemo, useState } from "react";
import { Button, ButtonVariant } from "../ui";
import type {
  KnowledgeObject,
  RelationType,
  GraphEdge,
} from "../types";

export interface RelationEditorProps {
  object: KnowledgeObject;
  relations: GraphEdge[];
  allObjects: KnowledgeObject[];
  onAddRelation?: (targetId: string, relationType: RelationType) => void;
  onRemoveRelation?: (targetId: string) => void;
  onCreateEntity?: (name: string, relationType: RelationType) => void;
  className?: string;
}

/** All supported relation types (mirrors backend RelationType enum). */
const RELATION_TYPES: string[] = [
  "references", "referenced-by", "parent", "child", "attached", "related",
];

type RelationTab = "existing" | "search" | "create";

function objectTitle(obj: KnowledgeObject): string {
  const m = obj.metadata;
  if (m && typeof m === "object" && "title" in m && m.title) return String(m.title);
  return `Entity ${obj.id}`;
}
function objectTypeLabel(obj: KnowledgeObject): string {
  return String(obj.object_type ?? "unknown");
}

export function RelationEditor({
  object,
  relations,
  allObjects,
  onAddRelation,
  onRemoveRelation,
  onCreateEntity,
  className = "",
}: RelationEditorProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedRelationType, setSelectedRelationType] = useState<string>("related");
  const [newEntityName, setNewEntityName] = useState("");
  const [activeTab, setActiveTab] = useState<RelationTab>("existing");

  const filtered = useMemo(() => {
    const q = searchQuery.toLowerCase().trim();
    if (q === "") return allObjects;
    return allObjects.filter((obj) => {
      const title = objectTitle(obj).toLowerCase();
      const type = objectTypeLabel(obj).toLowerCase();
      return title.includes(q) || type.includes(q);
    });
  }, [allObjects, searchQuery]);

  const ownRelations = useMemo(
    () => relations.filter((edge) => edge.source === object.id),
    [relations, object.id]
  );

  const objectTitleStr = objectTitle(object);

  const tabBtnClass = (tab: RelationTab) =>
    "px-3 py-1 text-xs rounded border " +
    (activeTab === tab
      ? "bg-blue-700 border-blue-500 text-blue-100"
      : "border-gray-600 text-gray-400 hover:text-gray-200");

  return (
    <div className={["relation-editor space-y-4", className].filter(Boolean).join(" ")}>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-gray-300">Relations for {objectTitleStr}</h3>
        <div className="flex gap-2">
          <button type="button" className={tabBtnClass("existing")} onClick={() => setActiveTab("existing")}>Existing</button>
          <button type="button" className={tabBtnClass("search")} onClick={() => setActiveTab("search")}>Search</button>
          <button type="button" className={tabBtnClass("create")} onClick={() => setActiveTab("create")}>+ New Entity</button>
        </div>
      </div>

      {/* Existing tab */}
      {activeTab === "existing" && (
        <div className="space-y-2">
          {ownRelations.length === 0 ? (
            <p className="text-sm text-gray-500">No relations yet</p>
          ) : (
            <div className="space-y-1">
              {ownRelations.map((edge) => {
                const target = allObjects.find((o) => o.id === edge.target);
                const label = target ? objectTitle(target) : `Entity ${edge.target}`;
                return (
                  <div key={edge.target} className="flex items-center justify-between p-2 bg-gray-800 rounded border border-gray-700">
                    <div className="flex items-center gap-2">
                      <span className="text-xs px-1.5 py-0.5 rounded bg-gray-700 text-gray-300">{edge.relationship}</span>
                      <span className="text-sm text-gray-200">{label}</span>
                    </div>
                    <button type="button" className="text-xs text-red-400 hover:text-red-300"
                      onClick={() => onRemoveRelation?.(edge.target)}>Remove</button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      {/* Search tab */}
      {activeTab === "search" && (
        <div className="space-y-3">
          <input type="text" placeholder="Search entities..." value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none" />
          <div className="flex items-center gap-2">
            <span className="text-xs text-gray-500">Relation:</span>
            <select value={selectedRelationType}
              onChange={(e) => setSelectedRelationType(e.target.value)}
              className="bg-gray-800 text-gray-100 rounded px-2 py-1 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none">
              {RELATION_TYPES.map((rt) => <option key={rt} value={rt}>{rt}</option>)}
            </select>
          </div>
          <div className="max-h-48 overflow-y-auto space-y-1">
            {filtered.length === 0 ? (
              <p className="text-sm text-gray-500">No entities found</p>
            ) : (
              filtered.map((obj) => (
                <div key={obj.id}
                  className="flex items-center justify-between p-2 bg-gray-800 rounded border border-gray-700 hover:border-gray-600 cursor-pointer"
                  onClick={() => onAddRelation?.(obj.id, selectedRelationType as RelationType)}>
                  <div>
                    <span className="text-sm text-gray-200">{objectTitle(obj)}</span>
                    <span className="text-xs text-gray-500 ml-2">{objectTypeLabel(obj)}</span>
                  </div>
                  <span className="text-xs text-blue-400">Add</span>
                </div>
              ))
            )}
          </div>
        </div>
      )}

      {/* Create tab */}
      {activeTab === "create" && (
        <div className="space-y-3">
          <div>
            <label className="text-xs text-gray-500 uppercase tracking-wide">New Entity Name</label>
            <input type="text" value={newEntityName}
              onChange={(e) => setNewEntityName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  const name = newEntityName.trim();
                  if (name) { onCreateEntity?.(name, selectedRelationType as RelationType); setNewEntityName(""); }
                }
              }}
              placeholder="Enter entity name..."
              className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none mt-1" />
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs text-gray-500">Relation:</span>
            <select value={selectedRelationType}
              onChange={(e) => setSelectedRelationType(e.target.value)}
              className="bg-gray-800 text-gray-100 rounded px-2 py-1 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none">
              {RELATION_TYPES.map((rt) => <option key={rt} value={rt}>{rt}</option>)}
            </select>
          </div>
          <Button variant={ButtonVariant.Primary}
            onClick={() => {
              const name = newEntityName.trim();
              if (name) { onCreateEntity?.(name, selectedRelationType as RelationType); setNewEntityName(""); }
            }}>
            Create Entity
          </Button>
        </div>
      )}
    </div>
  );
}
