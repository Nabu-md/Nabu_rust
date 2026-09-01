// ──────────────────────────────────────────────────────────────────────────────
// LeftSidebar — vault file explorer + organisational shortcuts
//
// Mirrors: ui-react/src/components/layout/left_sidebar.rs
//
// Renders the vault file tree backed by the `tree_list` IPC command:
//  - loads on mount and when workspace.refreshTree changes
//  - expand/collapse folders (local UI state)
//  - click to open notes in tabs (workspace.openTab)
//  - collections section with smart folders + saved searches from NavContext
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useState } from "react";
import { Icon } from "./icons";
import { treeList } from "../../ipc";
import { useNav, useWorkspace } from "../../context";
import type { TreeEntry } from "../../types";

/** One node of the vault tree. */
interface TreeNode {
  name: string;
  path: string;
  is_folder: boolean;
  children: TreeNode[];
}

/** Recursively converts TreeEntry to TreeNode. */
function deepConvert(entry: TreeEntry): TreeNode {
  return {
    name: entry.name,
    path: entry.path,
    is_folder: entry.is_folder,
    children: entry.children.map(deepConvert),
  };
}

/** The vault file tree component. */
function FileTree() {
  const ws = useWorkspace();
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);

  // Load the tree on mount and whenever the workspace asks for a refresh.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      try {
        const raw = await treeList();
        if (!cancelled) {
          setTree(raw.map(deepConvert));
        }
      } catch {
        if (!cancelled) {
          setTree([]);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [ws.refreshTree]);

  const toggleExpand = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleNodeClick = (node: TreeNode) => {
    if (node.is_folder) {
      toggleExpand(node.path);
    } else {
      ws.openTab(node.path);
    }
  };

  const renderNode = (node: TreeNode, depth: number) => {
    const isExpanded = expanded.has(node.path);
    const indent = depth * 12;

    return (
      <li key={node.path}>
        <div
          onClick={() => handleNodeClick(node)}
          className="flex items-center gap-1 px-1 py-0.5 text-sm text-gray-300 hover:bg-gray-800 rounded cursor-pointer select-none"
          style={{ paddingLeft: `${indent + 8}px` }}
        >
          {node.is_folder && (
            <Icon
              name={isExpanded ? "chevronDown" : "chevronRight"}
              className="w-3 h-3 text-gray-500 shrink-0"
            />
          )}
          <Icon
            name={node.is_folder ? "folder" : "fileText"}
            className="w-4 h-4 text-gray-400 shrink-0"
          />
          <span className="truncate flex-1 min-w-0">{node.name}</span>
        </div>
        {node.is_folder && isExpanded && node.children.length > 0 && (
          <ul>
            {node.children.map((child) => renderNode(child, depth + 1))}
          </ul>
        )}
      </li>
    );
  };

  if (loading && tree.length === 0) {
    return (
      <div className="overflow-y-auto flex-1 min-h-0 p-2">
        <div className="space-y-1">
          {Array.from({ length: 5 }).map((_, i) => (
            <div
              key={i}
              className="h-4 bg-gray-800 rounded animate-pulse"
              style={{ width: `${40 + i * 10}%` }}
            />
          ))}
        </div>
      </div>
    );
  }

  if (!loading && tree.length === 0) {
    return (
      <div className="overflow-y-auto flex-1 min-h-0 p-4">
        <div className="text-center text-gray-500 text-sm py-8">
          Your vault is empty.
        </div>
      </div>
    );
  }

  return (
    <div className="overflow-y-auto flex-1 min-h-0 py-1">
      <ul>{tree.map((node) => renderNode(node, 0))}</ul>
    </div>
  );
}

/** Left sidebar — the vault file explorer plus organisational shortcuts. */
export function LeftSidebar() {
  const nav = useNav();

  // Pre-compute collections data.
  const smartFolders = nav.smartFolders;
  const savedSearches = nav.savedSearches;
  const showHint = smartFolders.length === 0 && savedSearches.length === 0;

  const handleSavedSearch = (query: string) => {
    nav.setSearchQuery(query);
    nav.setViewMode("Search");
  };

  const handleSmartFolder = () => {
    nav.setViewMode("SmartFolders");
  };

  return (
    <aside className="sidebar-left w-64 border-r border-gray-700 bg-gray-900 h-screen flex flex-col min-w-0">
      {/* File tree */}
      <FileTree />

      {/* Collections — smart folders + saved searches */}
      <div className="border-t border-gray-800 px-2 py-2 space-y-1 overflow-y-auto flex-none max-h-64">
        <span className="text-xs font-semibold uppercase tracking-wider text-gray-500">
          Collections
        </span>

        <button
          type="button"
          onClick={handleSmartFolder}
          className="btn btn-sm btn-ghost w-full justify-start text-xs text-gray-400 hover:text-gray-200"
          title="Manage smart folders"
          aria-label="Manage smart folders"
        >
          <Icon name="folder" className="w-3 h-3 mr-1" />
          Smart Folders
        </button>

        {showHint && (
          <p className="text-[11px] text-gray-600 px-1">
            Save a search or create a smart folder to pin it here.
          </p>
        )}

        {smartFolders.map((f) => (
          <button
            key={f.id}
            type="button"
            onClick={handleSmartFolder}
            className="w-full text-left px-2 py-1 rounded text-xs text-gray-300 hover:bg-gray-800 flex items-center gap-1.5"
            title={f.name}
          >
            <Icon name="folder" className="w-3 h-3 text-gray-400" />
            <span className="flex-1 truncate">{f.name}</span>
            {f.pinned && <Icon name="pin" className="w-3 h-3 text-yellow-500" />}
          </button>
        ))}

        {savedSearches.length > 0 && (
          <div className="border-t border-gray-800 pt-1.5 space-y-0.5" />
        )}

        {savedSearches.map((s) => (
          <button
            key={s.name}
            type="button"
            onClick={() => handleSavedSearch(s.query)}
            className="w-full text-left px-2 py-1 rounded text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200 flex items-center gap-1.5"
            title={`Search: ${s.query}`}
          >
            <Icon name="search" className="w-3 h-3 text-gray-400" />
            <span className="flex-1 truncate">{s.name}</span>
          </button>
        ))}
      </div>
    </aside>
  );
}


