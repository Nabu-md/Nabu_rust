// ──────────────────────────────────────────────────────────────────────────────
// ThemeContext — theme preference (light / dark / system)
//
// Mirrors: ui-react/src/components/contexts::ThemeProvider
// ──────────────────────────────────────────────────────────────────────────────

import { createContext, useContext, useState, type ReactNode } from "react";

export type Theme = "light" | "dark" | "system";

export interface ThemeContextValue {
  /** Current theme preference. */
  theme: Theme;
  /** Set the theme preference. */
  setTheme: (theme: Theme) => void;
  /** The resolved theme after applying system preference (for consumers). */
  resolvedTheme: "light" | "dark";
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

/** Hook to access the theme context. */
export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return ctx;
}

interface ThemeProviderProps {
  children: ReactNode;
  /** Initial theme from backend settings. */
  initialTheme?: Theme;
}

/** Provider component for theme state. */
export function ThemeProvider({
  children,
  initialTheme = "system",
}: ThemeProviderProps) {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  // Resolve "system" to the OS preference.
  const resolvedTheme: "light" | "dark" =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : theme;

  return (
    <ThemeContext.Provider value={{ theme, setTheme, resolvedTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}
