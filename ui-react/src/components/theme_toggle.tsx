// ──────────────────────────────────────────────────────────────────────────────
// theme_toggle.tsx — theme selector (light / dark / system)
//
// Mirrors: AppSettings.theme field + the Appearance → Theme select in
//          ui-react/src/components/settings/settings_panel.rs
//
// Consumes ThemeContext (resolvedTheme / setTheme) and persists the choice to
// the backend via the `settings_set` IPC command with key "theme".
//
// The active theme is also reflected on the DOM via the `data-theme` attribute
// (applied in App.tsx on every resolvedTheme change), so CSS-based theming
// works in both light and dark modes.
// ──────────────────────────────────────────────────────────────────────────────

import { useTheme } from "../context/ThemeContext";
import { settingsSet } from "../ipc";
import type { Theme } from "../context/ThemeContext";

/**
 * A compact theme toggle button for the shell chrome.
 *
 * Cycles between light → dark → system on each click. The current resolved
 * theme is shown as an icon. Clicking sets the `data-theme` attribute on
 * `<html>` (via ThemeContext + App.tsx) and persists to the backend.
 */
export function ThemeToggle() {
  const { theme, setTheme, resolvedTheme } = useTheme();

  const nextTheme = (): Theme => {
    if (theme === "light") return "dark";
    if (theme === "dark") return "system";
    return "light";
  };

  const handleClick = () => {
    const next = nextTheme();
    setTheme(next);
    void settingsSet("theme", next);
  };

  const icon = resolvedTheme === "dark" ? "🌙" : "☀️";
  const label = `Theme: ${theme} (resolved: ${resolvedTheme})`;

  return (
    <button
      type="button"
      onClick={handleClick}
      className="navbar-action w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-800 transition-colors focus:outline-none focus:ring-1 focus:ring-blue-500/50"
      title={label}
      aria-label={label}
      aria-pressed={false}
    >
      <span aria-hidden="true">
        {icon}
      </span>
    </button>
  );
}
