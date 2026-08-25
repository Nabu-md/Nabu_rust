// ──────────────────────────────────────────────────────────────────────────────
// navigation/breadcrumb.tsx — breadcrumb navigation bar
//
// Mirrors: crates/nabu-ui/src/components/navigation/breadcrumb.rs
//
// Shows the user's current location — vault ▸ folder hierarchy ▸ current
// note — and lets them click any level to navigate:
//
//   - vault crumb → Dashboard
//   - folder crumb → reveals that folder in the sidebar file tree
//   - note crumb → activates the note's tab
//
// Folder-level navigation dispatches a `nabu:reveal-note` window event so
// the LeftSidebar file tree reveals (expands + selects) the path.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav, useWorkspace } from "../../context";
import { viewModeLabel } from "./state";
import type { ViewMode } from "./state";
import type { ReactNode } from "react";

/** One crumb in the breadcrumb trail. */
interface Breadcrumb {
  label: string;
  onClick: (() => void) | null;
}

/**
 * Dispatches a `nabu:reveal-note` window event so the file tree reveals a
 * path (expands its parent folders and selects it).
 */
function revealInSidebar(path: string): void {
  const event = new CustomEvent("nabu:reveal-note", { detail: path });
  window.dispatchEvent(event);
}

/** The breadcrumb bar for the current view. */
export function BreadcrumbBar(): ReactNode {
  const nav = useNav();
  const ws = useWorkspace();

  const mode = nav.viewMode;
  const vault = nav.vaultName || "Vault";

  // Build crumbs
  const crumbs: Breadcrumb[] = [
    {
      label: vault,
      onClick: () => nav.setViewMode("Dashboard" as ViewMode),
    },
  ];

  // Editor view shows folder hierarchy + current note
  if (mode === "Editor") {
    const active = ws.activePath;
    if (active) {
      const lastSlash = active.lastIndexOf("/");
      const folder = lastSlash >= 0 ? active.slice(0, lastSlash) : "";
      const noteName = active.slice(lastSlash + 1);
      const noteDisplay =
        lastSlash >= 0
          ? noteName.replace(/\.md$/, "")
          : active.replace(/\.md$/, "");

      if (folder.length === 0) {
        // No folder — just the note
        crumbs.push({
          label: noteDisplay,
          onClick: () => ws.activateTab(active),
        });
      } else {
        // Folder hierarchy (each level clickable → reveal in sidebar)
        let acc = "";
        for (const part of folder.split("/")) {
          acc = acc ? `${acc}/${part}` : part;
          const folderPath = acc;
          crumbs.push({
            label: part,
            onClick: () => revealInSidebar(folderPath),
          });
        }
        crumbs.push({
          label: noteDisplay,
          onClick: () => ws.activateTab(active),
        });
      }
    } else {
      crumbs.push({
        label: "Home",
        onClick: null,
      });
    }
  } else {
    // All other views show the view label
    crumbs.push({
      label: viewModeLabel(mode),
      onClick: null,
    });
  }

  return (
    <nav
      className="breadcrumb flex items-center gap-1.5 text-xs text-gray-400"
      aria-label="Breadcrumb"
    >
      {crumbs.map((crumb, i) => (
        <span key={i} className="flex items-center gap-1.5">
          {i > 0 && <span className="text-gray-600">›</span>}
          {crumb.onClick ? (
            <button
              type="button"
              onClick={crumb.onClick}
              className="text-gray-400 hover:text-gray-200 hover:underline transition-colors"
            >
              {crumb.label}
            </button>
          ) : (
            <span className="text-gray-500">{crumb.label}</span>
          )}
        </span>
      ))}
    </nav>
  );
}
