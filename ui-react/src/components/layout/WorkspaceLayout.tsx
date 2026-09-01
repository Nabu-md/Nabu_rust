// ──────────────────────────────────────────────────────────────────────────────
// WorkspaceLayout — the app's structural skeleton
//
// Mirrors: ui-react/src/components/layout/workspace.rs
//
// Composes the ribbon bar, left sidebar, main content area (with tab bar and
// navbar), right inspector, and the overlay surfaces (command palette, quick
// switcher, shortcuts reference) into a single layout.
//
// View switching is driven by NavContext.viewMode; actual view content is
// rendered by the ViewContent placeholder in the main content area.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav } from "../../context";
import { RibbonBar } from "./RibbonBar";
import { LeftSidebar } from "./LeftSidebar";
import { RightInspector } from "./RightInspector";
import { TabBar } from "./TabBar";
import { NavBar } from "./NavBar";
import { ViewContent } from "./ViewContent";
import { CommandPalette } from "./CommandPalette";
import { QuickSwitcher } from "./QuickSwitcher";
import { ShortcutReference } from "./ShortcutReference";

/** The full workspace layout — ribbon, sidebars, content, and overlays. */
export function WorkspaceLayout() {
  const nav = useNav();

  return (
    <div className="flex h-screen w-screen bg-gray-950 text-gray-100 overflow-hidden font-sans select-none">
      {/* ── Left Ribbon Bar ── */}
      <RibbonBar />

      {/* ── Left Sidebar (vault file explorer) ── */}
      {nav.showLeftSidebar && <LeftSidebar />}

      {/* ── Main Content Area ── */}
      <main className="flex-1 flex flex-col h-screen overflow-hidden bg-gray-900">
        {/* Tab Bar */}
        <TabBar />

        {/* Top Nav Bar */}
        <NavBar />

        {/* View Content */}
        <ViewContent />
      </main>

      {/* ── Right Inspector Sidebar ── */}
      {nav.showRightInspector && <RightInspector />}

      {/* ── Overlay surfaces ── */}
      <CommandPalette />
      <QuickSwitcher />
      <ShortcutReference />
    </div>
  );
}
