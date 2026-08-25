// ──────────────────────────────────────────────────────────────────────────────
// ViewContent — main content area view switching
//
// Mirrors: crates/nabu-ui/src/components/app.rs (ViewMode match in AppRouter)
//
// Routes the NavContext.viewMode to the appropriate view component.
// Wave 3 owns: Editor, Reader, Comparison, Statistics, Activity, ReadingQueue.
// Other modes (Dashboard, Graph, Search, etc.) remain placeholder until
// subsequent waves.
// ──────────────────────────────────────────────────────────────────────────────

import { useNav } from "../../context";
import { NoteEditor } from "../editor";
import { ReaderView } from "../reader";
import { ComparisonView } from "../comparison";
import { StatisticsView } from "../statistics";
import { ActivityPanel } from "../activity";
import { ReadingQueue } from "../reading_queue";

/** Main content area — renders the active view based on NavContext.viewMode. */
export function ViewContent() {
  const nav = useNav();

  // Wave 3 views
  if (nav.viewMode === "Editor") {
    return (
      <div className="flex-1 overflow-auto bg-gray-950">
        <NoteEditor />
      </div>
    );
  }

  if (nav.viewMode === "Reader") {
    return <ReaderView />;
  }

  if (nav.viewMode === "Comparison") {
    return <ComparisonView />;
  }

  // Wave 3 — Statistics
  if (nav.viewMode === "Statistics") {
    return <StatisticsView />;
  }

  // Wave 3 — Activity
  if (nav.viewMode === "Activity") {
    return <ActivityPanel />;
  }

  // Wave 3 — Reading Queue
  if (nav.viewMode === "ReadingQueue") {
    return <ReadingQueue />;
  }

  // Placeholder for all other views (Dashboard, Graph, Search, History, etc.)
  return (
    <div className="flex-1 overflow-auto p-6 bg-gray-950">
      <div className="text-center py-16 text-gray-500">
        <div className="text-2xl font-medium mb-2">{nav.viewMode}</div>
        <div className="text-sm">View content — coming in a future wave.</div>
      </div>
    </div>
  );
}
