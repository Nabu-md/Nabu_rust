// ──────────────────────────────────────────────────────────────────────────────
// RightInspector — property metadata, backlinks, outgoing links, mentions
//
// Mirrors: ui-react/src/components/layout/right_inspector.rs
//
// Renders a tab bar (Tags / Backlinks / Outgoing / Mentions) backed by the
// `note_links` Tauri command. Link data is re-fetched via useEffect whenever
// the workspace's active path changes.
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useState, type ReactNode } from "react";
import { Icon } from "./icons";
import { noteLinks } from "../../ipc";
import { useWorkspace } from "../../context";
import type {
  NoteLinks,
  BacklinkEntry,
  OutgoingLink,
  MentionEntry,
} from "../../types";

/** Tab definition for the inspector. */
interface TabDef {
  id: string;
  label: string;
  icon: string;
}

/** Load-state classification for the link-data view. */
type LoadPhase = "no-note" | "loading" | "error" | "empty" | "ready";

const INSPECTOR_TABS: TabDef[] = [
  { id: "tags", label: "Tags", icon: "tag" },
  { id: "backlinks", label: "Backlinks", icon: "link" },
  { id: "outgoing", label: "Outgoing", icon: "forward" },
  { id: "mentions", label: "Mentions", icon: "messageCircle" },
];

/** The Right Inspector panel. */
export function RightInspector() {
  const ws = useWorkspace();
  const [activeTab, setActiveTab] = useState("tags");
  const [links, setLinks] = useState<NoteLinks | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch note links on mount and whenever the active path changes.
  useEffect(() => {
    const path = ws.activePath;
    if (!path) {
      setLinks(null);
      setError(null);
      return;
    }

    setLoading(true);
    setError(null);

    noteLinks(path)
      .then((data) => {
        setLinks(data);
        setLoading(false);
      })
      .catch(() => {
        setError("Failed to load link data");
        setLoading(false);
      });
  }, [ws.activePath]);

  // Classify current phase based on signals and active tab.
  const phase: LoadPhase = !ws.activePath
    ? "no-note"
    : loading
      ? "loading"
      : error
        ? "error"
        : links
          ? (() => {
              const isEmpty =
                activeTab === "tags"
                  ? links.tags.length === 0
                  : activeTab === "backlinks"
                    ? links.backlinks.length === 0
                    : activeTab === "outgoing"
                      ? links.outgoing.length === 0
                      : links.mentions.length === 0;
              return isEmpty ? "empty" : "ready";
            })()
          : "no-note";

  // ── Phase renderers ───────────────────────────────────────────────────
  const renderPhaseContent = (): ReactNode => {
    switch (phase) {
      case "loading":
        return (
          <div className="p-4 space-y-2">
            {Array.from({ length: 4 }).map((_, i) => (
              <div
                key={i}
                className="h-3 bg-gray-800 rounded animate-pulse"
                style={{ width: `${60 - i * 10}%` }}
              />
            ))}
          </div>
        );

      case "error":
        return (
          <div className="p-4 text-center text-gray-500 text-sm">
            <div className="mb-2">Could not load note links</div>
            {error && <div className="text-red-400">{error}</div>}
            <button
              type="button"
              onClick={() => {
                if (ws.activePath) {
                  setLinks(null);
                  setError(null);
                }
              }}
              className="mt-2 px-3 py-1 bg-gray-800 rounded text-xs hover:bg-gray-700"
            >
              Retry
            </button>
          </div>
        );

      case "no-note":
        return (
          <div className="p-4 text-center text-gray-500 text-sm">
            Open a note to see its link data.
          </div>
        );

      case "empty":
        return (
          <div className="p-4 text-center text-gray-500 text-sm">
            No data to show for this tab.
          </div>
        );

      case "ready":
        return renderTabContent();

      default:
        return null;
    }
  };

  const renderTabContent = (): ReactNode => {
    if (!links) return null;

    switch (activeTab) {
      case "tags":
        return (
          <div className="p-4 space-y-3">
            <div>
              <div className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-1.5">
                Tags
              </div>
              {links.tags.length === 0 ? (
                <span className="text-xs text-gray-500">No tags</span>
              ) : (
                <div className="flex flex-wrap gap-1">
                  {links.tags.map((tag) => (
                    <span
                      key={tag}
                      className="px-2 py-0.5 text-xs rounded bg-gray-800 text-gray-300"
                    >
                      #{tag}
                    </span>
                  ))}
                </div>
              )}
            </div>
            <div>
              <div className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-1.5">
                Properties
              </div>
              <div className="text-xs text-gray-500">
                Property editor (stubs)
              </div>
            </div>
          </div>
        );

      case "backlinks":
        return renderBacklinks(links.backlinks);

      case "outgoing":
        return renderOutgoing(links.outgoing);

      case "mentions":
        return renderMentions(links.mentions);

      default:
        return null;
    }
  };

  // ── Specific list renderers ───────────────────────────────────────────
  const renderBacklinks = (entries: BacklinkEntry[]): ReactNode => {
    if (entries.length === 0) {
      return (
        <div className="p-4 text-center text-gray-500 text-sm">
          Nothing links to this note yet.
        </div>
      );
    }
    return (
      <div className="space-y-1.5 p-2">
        {entries.map((entry, i) => (
          <div
            key={`${entry.path}-${i}`}
            className="border-b border-gray-800 pb-2 last:border-0"
          >
            <div className="flex items-center gap-1.5">
              <Icon name="fileText" className="w-3 h-3 text-gray-400" />
              <span
                className="text-blue-400 hover:text-blue-300 cursor-pointer text-sm font-medium"
                onClick={() => ws.openTab(entry.path)}
              >
                {entry.title}
              </span>
              <span className="text-xs text-gray-500">{entry.folder}</span>
              {entry.count > 1 && (
                <span className="text-xs bg-gray-800 text-gray-400 rounded px-1.5 py-0.25">
                  x{entry.count}
                </span>
              )}
            </div>
            {entry.snippet && (
              <div className="mt-1 text-xs text-gray-500 line-clamp-2">
                {entry.snippet}
              </div>
            )}
          </div>
        ))}
      </div>
    );
  };

  const renderOutgoing = (links: OutgoingLink[]): ReactNode => {
    if (links.length === 0) {
      return (
        <div className="p-4 text-center text-gray-500 text-sm">
          This note has no outgoing links.
        </div>
      );
    }
    return (
      <div className="space-y-1.5 p-2">
        {links.map((link, i) => {
          const iconName =
            link.kind === "external"
              ? "externalLink"
              : link.kind === "broken"
                ? "x"
                : "link";
          const isInternal = link.kind === "internal" && link.path;
          const kindClass =
            link.kind === "external"
              ? "bg-blue-900/30 text-blue-400"
              : link.kind === "broken"
                ? "bg-red-900/30 text-red-400"
                : "bg-gray-800 text-gray-400";
          return (
            <div
              key={`${link.target}-${i}`}
              className="border-b border-gray-800 pb-2 last:border-0"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-1.5">
                  <Icon name={iconName} className="w-3 h-3 text-gray-400" />
                  {isInternal && link.path ? (
                    <span
                      className="text-blue-400 hover:text-blue-300 cursor-pointer text-sm"
                      onClick={() => ws.openTab(link.path!)}
                    >
                      {link.target}
                    </span>
                  ) : (
                    <span className="text-sm break-all">
                      {link.target}
                    </span>
                  )}
                  <span className={`text-xs px-1.5 py-0.25 rounded ${kindClass}`}>
                    {link.kind}
                  </span>
                  {link.count > 1 && (
                    <span className="text-xs text-gray-500">
                      x{link.count}
                    </span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    );
  };

  const renderMentions = (entries: MentionEntry[]): ReactNode => {
    if (entries.length === 0) {
      return (
        <div className="p-4 text-center text-gray-500 text-sm">
          No unlinked mentions were found.
        </div>
      );
    }
    return (
      <div className="space-y-1.5 p-2">
        {entries.map((entry, i) => (
          <div
            key={`${entry.path}-${i}`}
            className="border-b border-gray-800 pb-2 last:border-0"
          >
            <div className="flex items-center gap-1.5">
              <Icon name="messageCircle" className="w-3 h-3 text-gray-400" />
              {entry.path ? (
                <span
                  className="text-blue-400 hover:text-blue-300 cursor-pointer text-sm font-medium"
                  onClick={() => ws.openTab(entry.path)}
                >
                  {entry.title}
                </span>
              ) : (
                <span className="text-sm">{entry.title}</span>
              )}
              <span className="text-xs text-gray-500">
                score: {entry.score}
              </span>
            </div>
            {entry.snippet && (
              <div className="mt-1 text-xs text-gray-500 line-clamp-2">
                {entry.snippet}
              </div>
            )}
          </div>
        ))}
      </div>
    );
  };

  return (
    <aside className="right-inspector w-64 border-l border-gray-700 bg-gray-900 h-screen flex flex-col min-w-0">
      {/* Tab bar */}
      <div className="flex border-b border-gray-700">
        {INSPECTOR_TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 flex items-center justify-center gap-1 py-2 text-xs transition-colors ${
              activeTab === tab.id
                ? "text-white border-b-2 border-blue-500"
                : "text-gray-500 hover:text-gray-300"
            }`}
            title={tab.label}
            aria-label={tab.label}
          >
            <Icon name={tab.icon} className="w-3 h-3" />
            <span>{tab.label}</span>
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto min-h-0">
        {renderPhaseContent()}
      </div>
    </aside>
  );
}
