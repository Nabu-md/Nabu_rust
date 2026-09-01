// ─────────────────────────────────────────────────────────────────────────────
// components/file_tree — Vault file tree with full interactive features
//
// Mirrors: crates/nabu-ui/src/components/file_tree.rs
//
// Features:
//  - Loads vault tree via `tree_list` IPC (folders first, alphabetical)
//  - Expand / collapse folders, single-click opens notes into a tab
//  - Multi-select: Cmd/Ctrl-click toggles, Shift-click range-selects,
//    Cmd/Ctrl+A select all visible
//  - Inline rename (double-click or context menu → Rename), committed with
//    Enter, cancelled with Escape
//  - Context menu per node: New Note, New Folder, Rename, Delete, Duplicate,
//    Move, Copy Path, Copy Wikilink, Reveal in File Manager
//  - Drag-and-drop: drag a note/folder onto a folder (or empty tree area)
//    to move it
//  - Batch action bar (Delete / Move to… / Copy Paths / Open All) when
//    two or more items are selected
//  - All destructive operations route through the reversible history layer
//    (note_delete → trash with undo toast)
//
// Consumes: WorkspaceContext + ToastContext + HistoryContext
// Commands: tree_list, note_create_file, folder_create, note_rename,
//           folder_rename, note_duplicate, archive_note, note_delete,
//           items_move, reveal_in_file_manager, history_undo
// ─────────────────────────────────────────────────────────────────────────────

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { Icon } from "./layout/icons";
import {
  treeList,
  noteCreateFile,
  folderCreate,
  noteRename,
  folderRename,
  noteDuplicate,
  archiveNote,
  noteDelete,
  itemsMove,
  revealInFileManager,
} from "../ipc";
import { useWorkspace, useToast, useHistory } from "../context";
import {
  ConfirmDialog,
  MenuItem,
  MenuSeparator,
  SkeletonList,
  EmptyState,
} from "../ui";
import type { TreeEntry } from "../types";

// ── Data model ──────────────────────────────────────────────────────────────

interface TreeNode {
  name: string;
  path: string;
  is_folder: boolean;
  children: TreeNode[];
}

interface MenuState {
  x: number;
  y: number;
  path: string;
  is_folder: boolean;
}

function deepConvert(entry: TreeEntry): TreeNode {
  return {
    name: entry.name,
    path: entry.path,
    is_folder: entry.is_folder,
    children: entry.children.map(deepConvert),
  };
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function fileStem(path: string): string {
  const last = path.split("/").pop() ?? path;
  return last.replace(/\.md$/, "");
}

function parentDir(path: string): string {
  const idx = path.lastIndexOf("/");
  return idx >= 0 ? path.slice(0, idx) : "";
}

function displayName(path: string): string {
  const last = path.split("/").pop() ?? path;
  return path.endsWith(".md") ? last.replace(/\.md$/, "") : last;
}

function visiblePaths(nodes: TreeNode[], expanded: Set<string>): string[] {
  const out: string[] = [];
  const walk = (list: TreeNode[]) => {
    for (const n of list) {
      out.push(n.path);
      if (n.is_folder && expanded.has(n.path)) {
        walk(n.children);
      }
    }
  };
  walk(nodes);
  return out;
}

function collectFolders(
  nodes: TreeNode[],
  depth: number,
  out: { path: string; depth: number }[]
): void {
  for (const n of nodes) {
    if (n.is_folder) {
      out.push({ path: n.path, depth });
      collectFolders(n.children, depth + 1, out);
    }
  }
}

async function loadTree(): Promise<TreeNode[]> {
  try {
    const entries = await treeList();
    return entries.map(deepConvert);
  } catch {
    return [];
  }
}

// ── Context ─────────────────────────────────────────────────────────────────

interface TreeContextValue {
  nodes: TreeNode[];
  expanded: Set<string>;
  setExpanded: React.Dispatch<React.SetStateAction<Set<string>>>;
  selected: string[];
  setSelected: React.Dispatch<React.SetStateAction<string[]>>;
  anchor: string | null;
  setAnchor: React.Dispatch<React.SetStateAction<string | null>>;
  renaming: string | null;
  setRenaming: React.Dispatch<React.SetStateAction<string | null>>;
  renameValue: string;
  setRenameValue: React.Dispatch<React.SetStateAction<string>>;
  creating: { parent: string; is_folder: boolean } | null;
  setCreating: React.Dispatch<
    React.SetStateAction<{ parent: string; is_folder: boolean } | null>
  >;
  createValue: string;
  setCreateValue: React.Dispatch<React.SetStateAction<string>>;
  menu: MenuState | null;
  setMenu: React.Dispatch<React.SetStateAction<MenuState | null>>;
  dragging: string | null;
  setDragging: React.Dispatch<React.SetStateAction<string | null>>;
  dropTarget: string | null;
  setDropTarget: React.Dispatch<React.SetStateAction<string | null>>;
  movePicker: string[] | null;
  setMovePicker: React.Dispatch<React.SetStateAction<string[] | null>>;
  confirmOpen: boolean;
  setConfirmOpen: React.Dispatch<React.SetStateAction<boolean>>;
  confirmTargets: string[];
  setConfirmTargets: React.Dispatch<React.SetStateAction<string[]>>;
  doRename: (path: string, isFolder: boolean) => void;
  doDuplicate: (path: string) => void;
  doArchive: (path: string) => void;
  doDelete: (paths: string[]) => void;
  doMoveItems: (items: string[], dest: string) => void;
  doCreate: () => void;
  copyText: (text: string, label: string) => void;
  revealInFM: (path: string) => void;
  openTab: (path: string) => void;
  refreshTree: () => void;
}

const TreeContext = createContext<TreeContextValue | null>(null);
const useTree = (): TreeContextValue => {
  const ctx = useContext(TreeContext);
  if (!ctx) throw new Error("useTree must be used within FileTree");
  return ctx;
};

// ── Main component ──────────────────────────────────────────────────────────

export interface FileTreeProps {
  className?: string;
}

export function FileTree({ className = "" }: FileTreeProps) {
  const ws = useWorkspace();
  const toasts = useToast();
  const history = useHistory();

  const [nodes, setNodes] = useState<TreeNode[]>([]);
  const [treeLoaded, setTreeLoaded] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<string[]>([]);
  const [anchor, setAnchor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [creating, setCreating] = useState<{ parent: string; is_folder: boolean } | null>(null);
  const [createValue, setCreateValue] = useState("");
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [movePicker, setMovePicker] = useState<string[] | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmTargets, setConfirmTargets] = useState<string[]>([]);

  const doRename = (oldPath: string, isFolder: boolean) => {
    const newName = renameValue.trim();
    setRenaming(null);
    if (!newName || newName === displayName(oldPath)) return;

    const parent = parentDir(oldPath);
    let newPath = parent ? `${parent}/${newName}` : newName;
    if (!isFolder && !newPath.endsWith(".md")) newPath += ".md";
    if (newPath === oldPath) return;

    (async () => {
      try {
        if (isFolder) await folderRename(oldPath, newPath);
        else await noteRename(oldPath, newPath);
        ws.renameTabPrefix(oldPath, newPath);
        ws.refreshTreeBump();
        toasts.toast(`Renamed to ${newPath}`, { variant: "success" });
      } catch {
        toasts.toast("Rename failed: Could not rename that item", { variant: "error" });
      }
    })();
  };

  const doMoveItems = (items: string[], destFolder: string) => {
    if (items.length === 0) return;
    const invalid = items.some((src) =>
      destFolder === src || destFolder.startsWith(`${src}/`) || destFolder === parentDir(src)
    );
    if (invalid) {
      toasts.toast("That item is already in the destination folder", { variant: "error" });
      return;
    }
    (async () => {
      try {
        await itemsMove(items, destFolder);
        for (const oldPath of items) {
          const n = oldPath.split("/").pop() ?? oldPath;
          const newPath = destFolder ? `${destFolder}/${n}` : n;
          ws.renameTabPrefix(oldPath, newPath);
        }
        ws.refreshTreeBump();
        toasts.toast("Moved items", { variant: "success" });
      } catch {
        toasts.toast("Could not move the items", { variant: "error" });
      }
    })();
  };

  const doDuplicate = (path: string) => {
    (async () => {
      try {
        const newPath = await noteDuplicate(path, path);
        ws.refreshTreeBump();
        toasts.toast(`Created ${displayName(newPath)}`, { variant: "success" });
      } catch {
        toasts.toast("Could not duplicate that item", { variant: "error" });
      }
    })();
  };

  const doArchive = (path: string) => {
    (async () => {
      try {
        await archiveNote(path);
        ws.closeTab(path);
        ws.refreshTreeBump();
        toasts.toast(`Archived ${displayName(path)}`, { variant: "success" });
      } catch {
        toasts.toast("Could not archive that item", { variant: "error" });
      }
    })();
  };

  const doDelete = (paths: string[]) => {
    if (paths.length === 0) return;
    void history;
    (async () => {
      let ok = 0;
      for (const p of paths) {
        try { await noteDelete(p); ok++; ws.closeTab(p); } catch {}
      }
      if (ok > 0) {
        toasts.toast(`${ok} ${ok === 1 ? "item" : "items"} moved to Trash (Cmd+Z to undo)`, { variant: "info", duration: 8000 });
        ws.refreshTreeBump();
      } else {
        toasts.toast("Could not move those items to Trash", { variant: "error" });
      }
    })();
  };

  const copyText = (text: string, label: string) => {
    (async () => {
      try { await navigator.clipboard.writeText(text); toasts.toast(`${label}: Copied to clipboard`, { variant: "success" }); }
      catch { toasts.toast(`${label}: Could not copy`, { variant: "error" }); }
    })();
  };

  const revealInFM = (path: string) => {
    (async () => { try { await revealInFileManager(path); } catch {} })();
  };

  const doCreate = () => {
    if (!creating) return;
    const { parent, is_folder } = creating;
    const name = createValue.trim();
    setCreating(null);
    setCreateValue("");
    if (!name) return;
    const path = parent ? `${parent}/${name}` : name;
    const finalPath = is_folder ? path : path.endsWith(".md") ? path : `${path}.md`;
    (async () => {
      try {
        if (is_folder) await folderCreate(finalPath);
        else { await noteCreateFile(finalPath); ws.openTab(finalPath); }
        ws.refreshTreeBump();
        toasts.toast(is_folder ? `Folder created: ${finalPath}` : `Note created: ${finalPath}`, { variant: "success" });
      } catch {
        toasts.toast(`Could not create that ${is_folder ? "folder" : "note"}`, { variant: "error" });
      }
    })();
  };

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const loaded = await loadTree();
      if (!cancelled) { setNodes(loaded); setTreeLoaded(true); }
    })();
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ws.refreshTree]);

  useEffect(() => {
    const handler = (ev: CustomEvent) => {
      const path = ev.detail;
      if (typeof path !== "string") return;
      const snapshot = new Set(expanded);
      let parent = parentDir(path);
      while (parent) { snapshot.add(parent); parent = parentDir(parent); }
      setExpanded(snapshot);
      setSelected([path]);
    };
    window.addEventListener("nabu:reveal-note", handler as EventListener);
    return () => window.removeEventListener("nabu:reveal-note", handler as EventListener);
  }, [expanded]);

  const treeKeydown = (ev: React.KeyboardEvent) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key.toLowerCase() === "a") {
      ev.preventDefault();
      setSelected(visiblePaths(nodes, expanded));
    } else if (ev.key === "Escape") {
      setSelected([]);
      setMenu(null);
      setRenaming(null);
    }
  };

  const containerDragOver = (ev: React.DragEvent) => {
    if (dragging) { ev.preventDefault(); ev.dataTransfer.effectAllowed = "move"; setDropTarget(""); }
  };
  const containerDrop = (ev: React.DragEvent) => {
    ev.preventDefault();
    const src = dragging;
    if (!src) return;
    setDragging(null);
    setDropTarget(null);
    doMoveItems([src], "");
  };
  const containerDragEnd = () => { setDragging(null); setDropTarget(null); };

  const ctx: TreeContextValue = {
    nodes, expanded, setExpanded, selected, setSelected, anchor, setAnchor,
    renaming, setRenaming, renameValue, setRenameValue, creating, setCreating,
    createValue, setCreateValue, menu, setMenu, dragging, setDragging,
    dropTarget, setDropTarget, movePicker, setMovePicker,
    confirmOpen, setConfirmOpen, confirmTargets, setConfirmTargets,
    doRename, doDuplicate, doArchive, doDelete, doMoveItems, doCreate,
    copyText, revealInFM, openTab: ws.openTab, refreshTree: ws.refreshTreeBump,
  };

  const count = selected.length;

  return (
    <TreeContext.Provider value={ctx}>
      <div
        className={["file-tree flex flex-col h-full min-h-0", className].filter(Boolean).join(" ")}
        tabIndex={0}
        onKeyDown={treeKeydown}
        onDragOver={containerDragOver}
        onDrop={containerDrop}
        onDragEnd={containerDragEnd}
      >
        <div className="file-tree-header flex items-center justify-between px-2 pt-1.5 pb-1">
          <span className="text-xs font-semibold uppercase tracking-wider text-gray-500">Notes</span>
          <button type="button" className="btn btn-sm btn-ghost" title="New note" aria-label="New note"
            onClick={() => { setCreating({ parent: "", is_folder: false }); setCreateValue(""); }}>+ Note</button>
          <button type="button" className="btn btn-sm btn-ghost" title="New folder" aria-label="New folder"
            onClick={() => { setCreating({ parent: "", is_folder: true }); setCreateValue(""); }}>+ Folder</button>
        </div>

        {creating && (
          <div className="px-2 py-1 flex items-center gap-1">
            <input className="input flex-1 text-xs py-0"
              placeholder={creating.is_folder ? "Folder name…" : "Note name…"}
              value={createValue}
              onChange={(e) => setCreateValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") doCreate();
                else if (e.key === "Escape") { setCreating(null); setCreateValue(""); }
              }}
              onClick={(e) => e.stopPropagation()}
              onDoubleClick={(e) => e.stopPropagation()}
            />
          </div>
        )}

        {dragging && (
          <div className={
            "tree-root-drop text-[11px] px-2 py-1 border-t border-b mx-1 rounded text-center text-gray-400 " +
            (dropTarget === "" ? "tree-root-drop-active" : "")
          }>Drop to move to the vault root</div>
        )}

        {!treeLoaded ? (
          <SkeletonList rows={6} />
        ) : nodes.length === 0 ? (
          <div className="flex-1 overflow-y-auto min-h-0">
            <EmptyState icon="folderOpen" title="Your vault is empty"
              description="Create your first note to start building your knowledge base." />
            <button type="button" className="btn btn-sm mt-2 mx-2"
              onClick={() => { setCreating({ parent: "", is_folder: false }); setCreateValue(""); }}>
              <Icon name="plus" className="w-3 h-3 mr-1" /> New Note
            </button>
          </div>
        ) : (
          <ul className="tree-list flex-1 overflow-y-auto min-h-0 py-1">
            {nodes.map((node) => (
              <TreeNodeView key={node.path} node={node} depth={0} />
            ))}
          </ul>
        )}

        {count >= 2 && <BatchActionBar count={count} />}

        {movePicker && (
          <MovePicker
            items={movePicker}
            onMove={(items, dest) => { setMovePicker(null); doMoveItems(items, dest); }}
            onCancel={() => setMovePicker(null)}
          />
        )}

        {confirmOpen && (
          <ConfirmDialog
            open={confirmOpen}
            title="Move to Trash?"
            message={
              confirmTargets.length === 1
                ? "This item will be moved to Trash. You can restore it or undo."
                : `These ${confirmTargets.length} items will be moved to Trash. You can restore them or undo.`
            }
            confirmLabel="Move to Trash"
            cancelLabel="Cancel"
            danger
            onClose={() => { setConfirmOpen(false); setConfirmTargets([]); }}
            onConfirm={() => { doDelete(confirmTargets); setConfirmOpen(false); setConfirmTargets([]); }}
          />
        )}

        {menu && <ContextMenu state={menu} />}
      </div>
    </TreeContext.Provider>
  );
}

// ── Batch Action Bar ────────────────────────────────────────────────────────

function BatchActionBar({ count }: { count: number }) {
  const ctx = useTree();
  const ws = useWorkspace();
  return (
    <div className="batch-bar flex items-center gap-1 px-2 py-1 border-t border-gray-700 bg-gray-900">
      <span className="text-xs text-gray-300 font-medium mr-1">{count} selected</span>
      <button type="button" className="btn btn-sm"
        onClick={() => { for (const p of ctx.selected) { if (p.endsWith(".md")) ws.openTab(p); } }}>Open</button>
      <button type="button" className="btn btn-sm" onClick={() => ctx.setMovePicker([...ctx.selected])}>Move to…</button>
      <button type="button" className="btn btn-sm" onClick={() => ctx.copyText(ctx.selected.join("\n"), "Copy Paths")}>Copy Paths</button>
      <button type="button" className="btn btn-sm btn-danger"
        onClick={() => { ctx.setConfirmTargets([...ctx.selected]); ctx.setConfirmOpen(true); }}>Delete</button>
      <button type="button" className="btn btn-sm btn-ghost" aria-label="Clear selection"
        onClick={() => ctx.setSelected([])}><Icon name="x" className="w-3 h-3" /></button>
    </div>
  );
}

// ── Move Picker ─────────────────────────────────────────────────────────────

function MovePicker({ items, onMove, onCancel }: { items: string[]; onMove: (items: string[], dest: string) => void; onCancel: () => void }) {
  const ctx = useTree();
  const folders: { path: string; depth: number }[] = [];
  collectFolders(ctx.nodes, 0, folders);
  return (
    <>
      <div className="dialog-overlay fixed inset-0 z-40 bg-black/40" onClick={onCancel} />
      <div className="menu move-picker fixed z-50 bg-gray-900 border border-gray-700 rounded shadow-xl py-1" style={{ left: 100, top: 100 }} onClick={(e) => e.stopPropagation()}>
        <div className="px-2 py-1 text-xs font-medium text-gray-400">Move {items.length} item(s) to…</div>
        <MenuItem label="Vault root" icon="folder" onSelect={() => onMove(items, "")} />
        <MenuSeparator />
        {folders.map((f) => (
          <MenuItem key={f.path} label={`${"  ".repeat(f.depth)}${f.path}`} icon="folder" onSelect={() => onMove(items, f.path)} />
        ))}
      </div>
    </>
  );
}

// ── Context Menu ───────────────────────────────────────────────────────────

type ContextMenuAction =
  | "rename" | "new_note" | "new_folder" | "delete" | "duplicate"
  | "move" | "archive" | "copy_path" | "copy_wikilink" | "reveal" | "new_window";

function ContextMenu({ state }: { state: MenuState }) {
  const ctx = useTree();
  const { x, y, path, is_folder } = state;

  const handleSelect = (action: ContextMenuAction) => {
    const parent = is_folder ? path : parentDir(path);
    switch (action) {
      case "rename": ctx.setRenameValue(displayName(path)); ctx.setRenaming(path); ctx.setMenu(null); break;
      case "new_note": ctx.setCreating({ parent, is_folder: false }); ctx.setCreateValue(""); ctx.setExpanded((p) => new Set([...p, parent])); ctx.setMenu(null); break;
      case "new_folder": ctx.setCreating({ parent, is_folder: true }); ctx.setCreateValue(""); ctx.setExpanded((p) => new Set([...p, parent])); ctx.setMenu(null); break;
      case "delete": ctx.setConfirmTargets([path]); ctx.setConfirmOpen(true); ctx.setMenu(null); break;
      case "duplicate": ctx.doDuplicate(path); ctx.setMenu(null); break;
      case "move": ctx.setMovePicker([path]); ctx.setMenu(null); break;
      case "archive": ctx.doArchive(path); ctx.setMenu(null); break;
      case "copy_path": ctx.copyText(path, "Copy Path"); ctx.setMenu(null); break;
      case "copy_wikilink": ctx.copyText(`[[${fileStem(displayName(path))}]]`, "Copy Wikilink"); ctx.setMenu(null); break;
      case "reveal": ctx.revealInFM(path); ctx.setMenu(null); break;
      case "new_window": ctx.setMenu(null); break;
    }
  };

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={() => ctx.setMenu(null)}
        onContextMenu={(e) => e.preventDefault()} />
      <div className="menu fixed z-50 min-w-48 bg-gray-900 border border-gray-700 rounded shadow-xl py-1"
        role="menu" style={{ left: `${x}px`, top: `${y}px` }} onClick={(e) => e.stopPropagation()}>
        <MenuItem label="New Note" onSelect={() => handleSelect("new_note")} />
        <MenuItem label="New Folder" onSelect={() => handleSelect("new_folder")} />
        <MenuSeparator />
        <MenuItem label="Rename" hint="⏎" onSelect={() => handleSelect("rename")} />
        <MenuItem label="Delete" danger onSelect={() => handleSelect("delete")} />
        <MenuItem label="Duplicate" onSelect={() => handleSelect("duplicate")} />
        <MenuItem label="Move to…" onSelect={() => handleSelect("move")} />
        <MenuItem label="Archive" hint="hides from navigation" onSelect={() => handleSelect("archive")} />
        <MenuSeparator />
        <MenuItem label="Copy Path" onSelect={() => handleSelect("copy_path")} />
        <MenuItem label="Copy Wikilink" onSelect={() => handleSelect("copy_wikilink")} />
        <MenuItem label="Reveal in File Manager" onSelect={() => handleSelect("reveal")} />
        <MenuItem label="Open in New Window" onSelect={() => handleSelect("new_window")} />
      </div>
    </>
  );
}

// ── Tree Node View ──────────────────────────────────────────────────────────

interface TreeNodeViewProps { node: TreeNode; depth: number; }

function TreeNodeView({ node, depth }: TreeNodeViewProps) {
  const ctx = useTree();
  const { path, name, is_folder, children } = node;
  const inputRef = useRef<HTMLInputElement>(null);

  const onRowClick = (ev: React.MouseEvent) => {
    if (ev.metaKey || ev.ctrlKey) {
      ctx.setSelected((prev) => prev.includes(path) ? prev.filter((p) => p !== path) : [...prev, path]);
      ctx.setAnchor(path);
      return;
    }
    if (ev.shiftKey) {
      const flat = visiblePaths(ctx.nodes, ctx.expanded);
      if (ctx.anchor && flat.includes(ctx.anchor)) {
        const ai = flat.indexOf(ctx.anchor);
        const bi = flat.indexOf(path);
        if (ai >= 0 && bi >= 0) {
          const lo = Math.min(ai, bi); const hi = Math.max(ai, bi);
          ctx.setSelected(flat.slice(lo, hi + 1));
          return;
        }
      }
      ctx.setSelected([path]);
      ctx.setAnchor(path);
      return;
    }
    ctx.setSelected([path]);
    ctx.setAnchor(path);
    if (is_folder) ctx.setExpanded((prev) => { const n = new Set(prev); if (n.has(path)) n.delete(path); else n.add(path); return n; });
    else ctx.openTab(path);
  };

  const onRowDblClick = (ev: React.MouseEvent) => {
    ev.stopPropagation();
    if (!ctx.renaming) { ctx.setRenameValue(displayName(path)); ctx.setRenaming(path); }
  };

  const onContextMenu = (ev: React.MouseEvent) => {
    ev.preventDefault(); ev.stopPropagation();
    if (!ctx.selected.includes(path)) { ctx.setSelected([path]); ctx.setAnchor(path); }
    ctx.setMenu({ x: ev.clientX, y: ev.clientY, path, is_folder });
  };

  const onDragStart = (ev: React.DragEvent) => {
    ctx.setDragging(path);
    ev.dataTransfer.setData("application/x-nabu-note", path);
    ev.dataTransfer.setData("text/plain", path);
    ev.dataTransfer.effectAllowed = "move";
  };
  const onDragEnd = () => { ctx.setDragging(null); ctx.setDropTarget(null); };

  const onDragOver = (ev: React.DragEvent) => {
    if (!ctx.dragging) return;
    ev.preventDefault(); ev.stopPropagation();
    const dest = is_folder ? path : parentDir(path);
    ctx.setDropTarget(dest);
  };
  const onDrop = (ev: React.DragEvent) => {
    ev.preventDefault(); ev.stopPropagation();
    const src = ctx.dragging;
    if (!src) return;
    ctx.setDragging(null); ctx.setDropTarget(null);
    const dest = is_folder ? path : parentDir(path);
    ctx.doMoveItems([src], dest);
  };

  const isRenamingThisRow = ctx.renaming === path;
  useEffect(() => {
    if (isRenamingThisRow) {
      const timer = setTimeout(() => { inputRef.current?.focus(); inputRef.current?.select(); }, 10);
      return () => clearTimeout(timer);
    }
  }, [isRenamingThisRow, ctx.renaming]);

  const selectedNow = ctx.selected.includes(path);
  const isDraggingNow = ctx.dragging === path;
  const isDropNow = ctx.dropTarget === path;
  const rowClass = [
    "tree-row flex items-center gap-1 px-1 py-0.5 mx-1 rounded cursor-pointer select-none text-sm",
    selectedNow ? "tree-row-selected" : "",
    isDraggingNow ? "tree-row-dragging" : "",
    isDropNow ? "tree-row-drop-target" : "",
  ].filter(Boolean).join(" ");

  const chevronState = is_folder && ctx.expanded.has(path) ? "open" : "closed";
  const hasChildren = is_folder && children.length > 0;
  const indent = depth * 12;

  return (
    <li className="tree-node" draggable="true" onDragStart={onDragStart} onDragEnd={onDragEnd} onDragOver={onDragOver} onDrop={onDrop}>
      <div className={rowClass} onClick={onRowClick} onDoubleClick={onRowDblClick} onContextMenu={onContextMenu}
        role="treeitem" aria-selected={selectedNow}
        aria-expanded={is_folder ? (ctx.expanded.has(path) ? "true" : "false") : undefined}
        style={{ paddingLeft: `${indent + 8}px` }}>
        <span className="tree-chevron w-4 text-center text-xs text-gray-500" aria-hidden="true">
          {is_folder && <Icon name={chevronState === "open" ? "chevronDown" : "chevronRight"} className="w-3 h-3" />}
          {!is_folder && "•"}
        </span>
        <span className="tree-icon" aria-hidden="true">
          <Icon name={is_folder ? "folder" : "fileText"} className="w-4 h-4 text-gray-400" />
        </span>
        {isRenamingThisRow ? (
          <input ref={inputRef} className="input flex-1 text-xs py-0" value={ctx.renameValue}
            onChange={(e) => ctx.setRenameValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") ctx.doRename(path, is_folder);
              else if (e.key === "Escape") ctx.setRenaming(null);
            }}
            onClick={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()} />
        ) : (
          <span className="tree-name truncate flex-1 min-w-0" title={path}>{name}</span>
        )}
      </div>
      {is_folder && hasChildren && chevronState === "open" && (
        <ul className="tree-children ml-3 border-l border-gray-800">
          {children.map((child) => <TreeNodeView key={child.path} node={child} depth={depth + 1} />)}
        </ul>
      )}
    </li>
  );
}
