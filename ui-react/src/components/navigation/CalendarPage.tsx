// ──────────────────────────────────────────────────────────────────────────────
// navigation/CalendarPage.tsx — date-based note browsing
//
// Mirrors: ui-react/src/components/navigation/calendar_page.rs
//
// Date-based navigation workspace. Loads notes dated within the selected
// month via the `calendar_notes` command, lays them out on a month grid, and
// lets the user open any note or create/open that day's daily note.
//
// Month navigation (prev / today / next) re-fetches for the new month.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, type ReactNode } from "react";
import { Icon } from "../layout/icons";
import { useNav, useWorkspace } from "../../context";
import { calendarNotes, dailyNoteFor } from "../../ipc";
import type { CalendarEntry } from "../../types";
import type { ViewMode } from "./state";

// ── Load-state lifecycle ────────────────────────────────────────────────────

type LoadState = "loading" | "error" | "loaded";

// ── Date helpers (mirrors Dioxus calendar_page.rs) ────────────────────────

function todayStr(): string {
  return new Date().toISOString().slice(0, 10);
}

function currentMonth(): string {
  // Returns YYYY-MM for today's date
  return todayStr().slice(0, 7);
}

function shiftMonth(month: string, delta: number): string {
  const y = parseInt(month.slice(0, 4), 10);
  const m = parseInt(month.slice(5, 7), 10);
  const total = y * 12 + (m - 1) + delta;
  const ny = Math.floor(total / 12);
  const nm = ((total % 12) + 12) % 12 + 1;
  return `${String(ny).padStart(4, "0")}-${String(nm).padStart(2, "0")}`;
}

function monthLabel(month: string): string {
  const y = parseInt(month.slice(0, 4), 10);
  const m = parseInt(month.slice(5, 7), 10);
  try {
    const dt = new Date(y, m - 1, 1);
    return dt.toLocaleDateString("en-US", { month: "long", year: "numeric" });
  } catch {
    return month;
  }
}

/**
 * Day of week where Sunday = 0, using Tomohiko Sakamoto's algorithm.
 */
function naiveWeekdaySunday(y: number, m: number, d: number): number {
  const dd = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
  const yy = m < 3 ? y - 1 : y;
  return (
    ((yy + Math.floor(yy / 4) - Math.floor(yy / 100) + Math.floor(yy / 400) + dd[m - 1] + d) %
      7 +
    7) %
    7
  );
}

function truncateTitle(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + "…";
}

// ── Open helpers ────────────────────────────────────────────────────────────

function openNote(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
  path: string,
): void {
  ws.openTab(path);
  nav.setRecentNotes([path, ...nav.recentNotes.filter((p) => p !== path)].slice(0, 20));
  nav.setViewMode("Editor" as ViewMode);
}

function openDailyNote(
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
  date: string,
): void {
  dailyNoteFor(date)
    .then((path) => openNote(nav, ws, path))
    .catch(() => {
      /* Non-fatal */
    });
}

// ── Calendar grid computation ───────────────────────────────────────────────

interface DayCellData {
  dayNum: number | null;
  dateStr: string | null;
  isToday: boolean;
  notes: CalendarEntry[];
}

function useCalendarGrid(
  month: string,
  notes: CalendarEntry[],
): { weeks: DayCellData[][]; today: string } {
  const today = todayStr();
  const [grid, setGrid] = useState<DayCellData[][]>([]);

  useEffect(() => {
    const byDate: Record<string, CalendarEntry[]> = {};
    for (const n of notes) {
      if (!byDate[n.date]) byDate[n.date] = [];
      byDate[n.date].push(n);
    }

    const y = parseInt(month.slice(0, 4), 10);
    const mMon = parseInt(month.slice(5, 7), 10);

    const first = new Date(y, mMon - 1, 1);
    const nextFirst = new Date(y, mMon, 1);
    const daysInMonth = Math.round(
      (nextFirst.getTime() - first.getTime()) / (1000 * 60 * 60 * 24),
    );
    const startOffset = naiveWeekdaySunday(y, mMon, 1);
    const totalCells = startOffset + daysInMonth;
    const weekCount = Math.ceil(totalCells / 7);

    const weeks: DayCellData[][] = [];
    for (let week = 0; week < weekCount; week++) {
      const row: DayCellData[] = [];
      for (let col = 0; col < 7; col++) {
        const i = week * 7 + col;
        if (i < startOffset || i >= startOffset + daysInMonth) {
          row.push({
            dayNum: null,
            dateStr: null,
            isToday: false,
            notes: [],
          });
        } else {
          const d = i - startOffset + 1;
          const dateStr = `${month}-${String(d).padStart(2, "0")}`;
          row.push({
            dayNum: d,
            dateStr,
            isToday: dateStr === today,
            notes: byDate[dateStr] ?? [],
          });
        }
      }
      weeks.push(row);
    }
    setGrid(weeks);
  }, [month, notes, today]);

  return { weeks: grid, today };
}

// ── Renderers ───────────────────────────────────────────────────────────────

function renderDayCell(
  cell: DayCellData,
  nav: ReturnType<typeof useNav>,
  ws: ReturnType<typeof useWorkspace>,
): ReactNode {
  if (cell.dayNum === null) {
    return (
      <td className="cal-day cal-day-empty border border-gray-800 p-1 align-top h-20" />
    );
  }

  const { dateStr, isToday, notes } = cell;
  const dayNumber = cell.dayNum;
  const cellClass = isToday
    ? "cal-day cal-day-today border border-blue-500 p-1 align-top text-xs h-20"
    : "cal-day border border-gray-800 p-1 align-top text-xs h-20";

  return (
    <td className={cellClass}>
      <div className="day-header" />
      <div
        className="day-number text-gray-400 cursor-pointer"
        onClick={() => {
          if (dateStr) openDailyNote(nav, ws, dateStr);
        }}
        title={dateStr ?? undefined}
      >
        {dayNumber}
      </div>
      {notes.length > 0 && (
        <div className="day-notes space-y-0.5">
          {notes.map((n) => (
            <div
              key={n.path}
              className="day-note truncate text-blue-400 hover:text-blue-300 cursor-pointer"
              onClick={(e) => {
                e.stopPropagation();
                openNote(nav, ws, n.path);
              }}
              title={n.title}
            >
              {truncateTitle(n.title, 18)}
            </div>
          ))}
        </div>
      )}
    </td>
  );
}

// ── CalendarPage main component ─────────────────────────────────────────────

/** Date-based calendar view with month navigation and daily notes. */
export function CalendarPage(): ReactNode {
  const nav = useNav();
  const ws = useWorkspace();

  const [month, setMonth] = useState(currentMonth);
  const [entries, setEntries] = useState<CalendarEntry[]>([]);
  const [state, setState] = useState<LoadState>("loading");

  const monthStr = month;
  const label = monthLabel(monthStr);
  const prevMonth = shiftMonth(monthStr, -1);
  const nextMonth = shiftMonth(monthStr, 1);

  // Load notes for the current month
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setState("loading");
      try {
        const list = await calendarNotes(monthStr);
        if (!cancelled) {
          setEntries(list);
          setState("loaded");
        }
      } catch {
        if (!cancelled) {
          setEntries([]);
          setState("error");
        }
      }
    };
    load();
    return () => { cancelled = true; };
  }, [monthStr]);

  // Compute calendar grid
  const { weeks } = useCalendarGrid(monthStr, entries);

  // Navigation handlers
  const handlePrev = () => {
    const m = prevMonth;
    setMonth(m);
  };
  const handleToday = () => {
    const m = currentMonth();
    setMonth(m);
  };
  const handleNext = () => {
    const m = nextMonth;
    setMonth(m);
  };

  return (
    <div className="calendar-page h-screen overflow-y-auto bg-gray-950 text-gray-100">
      {/* Toolbar */}
      <div className="calendar-toolbar flex items-center justify-between px-6 py-4 border-b border-gray-700">
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="cal-btn rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50"
            onClick={handlePrev}
          >
            <Icon name="clock" className="w-3 h-3 mr-1 inline" />
            Prev
          </button>
          <h1 className="text-lg font-semibold text-gray-100">{label}</h1>
          <button
            type="button"
            className="cal-btn rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50"
            onClick={handleToday}
          >
            Today
          </button>
          <button
            type="button"
            className="cal-btn rounded px-2 py-1 text-sm text-gray-300 hover:bg-gray-700/50"
            onClick={handleNext}
          >
            Next
          </button>
        </div>
      </div>

      {/* Body */}
      <div className="calendar-body px-6 py-4">
        {state === "loading" && (
          <div className="flex items-center justify-center py-8">
            <div className="flex flex-col items-center gap-2 text-gray-500">
              <div className="w-5 h-5 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
              <span className="text-sm">Loading…</span>
            </div>
          </div>
        )}

        {state === "error" && (
          <div className="text-center py-8 text-gray-500 text-sm">
            <div>Couldn't load notes for this month.</div>
          </div>
        )}

        {state === "loaded" && (
          <>
            {entries.length === 0 && (
              <div className="text-center py-8 text-gray-500">
                <Icon name="calendar" className="w-8 h-8 mx-auto mb-2 opacity-30" />
                <div className="text-lg font-medium text-gray-300">
                  No notes this month
                </div>
                <div className="text-xs mt-1">
                  No dated notes in {label}.
                </div>
              </div>
            )}

            <table className="cal-table w-full border-collapse text-sm">
              <thead className="cal-weekday-h border-b border-gray-700">
                <tr>
                  <th className="text-gray-500">Sun</th>
                  <th className="text-gray-500">Mon</th>
                  <th className="text-gray-500">Tue</th>
                  <th className="text-gray-500">Wed</th>
                  <th className="text-gray-500">Thu</th>
                  <th className="text-gray-500">Fri</th>
                  <th className="text-gray-500">Sat</th>
                </tr>
              </thead>
              <tbody>
                {weeks.map((week, wi) => (
                  <tr key={wi} className="cal-row">
                    {week.map((cell) => (
                      <>
                        {renderDayCell(cell, nav, ws)}
                      </>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}
      </div>
    </div>
  );
}
