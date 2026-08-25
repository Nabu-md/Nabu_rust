// ──────────────────────────────────────────────────────────────────────────────
// navigation/SearchPage.tsx — full-text search
//
// Mirrors: crates/nabu-ui/src/components/navigation/search_page.rs
//
// Full-text search backed by the backend `notes_search` command. Shows
// explicit loading, empty, and error states so the user always knows whether
// results are in-flight, absent, or failed.
//
// Uses the ipc.ts `notesSearch` wrapper so a rejected promise (command not
// registered, backend error, vault not configured) becomes a graceful error
// state instead of a crash.
// ──────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback, type ReactNode } from "react";
import { Icon } from "../layout/icons";
import { useNav, useWorkspace, useToast } from "../../context";
import { notesSearch } from "../../ipc";
import type { SearchHit } from "../../types";

// ── Search lifecycle states ─────────────────────────────────────────────────

type SearchState = "idle" | "loading" | "loaded" | "failed";

// ── Helpers ─────────────────────────────────────────────────────────────────

/** Highlights the matched substring within `text` by wrapping it in <mark>. */
function highlightMatch(
  text: string,
  matchStart: number,
  matchEnd: number,
): ReactNode {
  if (matchStart < 0 || matchEnd > text.length || matchStart >= matchEnd) {
    return text;
  }
  return (
    <>
      {text.slice(0, matchStart)}
      <mark className="bg-yellow-400/20 text-yellow-300">
        {text.slice(matchStart, matchEnd)}
      </mark>
      {text.slice(matchEnd)}
    </>
  );
}

// ── SearchPage main component ───────────────────────────────────────────────

/** Full-text search page with loading, empty, and error states. */
export function SearchPage(): ReactNode {
  const nav = useNav();
  const ws = useWorkspace();
  const toasts = useToast();

  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [searchState, setSearchState] = useState<SearchState>("idle");
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Track whether we've done the initial search from NavContext.searchQuery
  const initializedRef = useRef(false);

  const runSearch = useCallback(
    (q: string) => {
      const trimmed = q.trim();
      if (trimmed.length === 0) {
        setHits([]);
        setSearchState("loaded");
        setError("");
        return;
      }

      setSearchState("loading");
      setError("");

      notesSearch(trimmed)
        .then((results) => {
          setHits(results);
          setSearchState("loaded");
          setError("");
          // Record the query in recent searches
          nav.setRecentSearches([
            trimmed,
            ...nav.recentSearches.filter((s) => s !== trimmed),
          ].slice(0, 10));
        })
        .catch((err: unknown) => {
          const msg =
            err instanceof Error
              ? err.message
              : String(err ?? "Unknown error");
          setError(msg);
          setSearchState("failed");
          toasts.toast("Search failed", { variant: "error" });
        });
    },
    [nav, toasts],
  );

  // One-time initial load from the external query (if any)
  useEffect(() => {
    if (!initializedRef.current) {
      initializedRef.current = true;
      const initial = nav.searchQuery;
      if (initial && initial.trim()) {
        runSearch(initial);
      }
    }
  }, [nav.searchQuery, runSearch]);

  // Focus the input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // ── Handlers ──────────────────────────────────────────────────────────

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setQuery(e.target.value);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      runSearch(query);
    }
  };

  const handleRetry = () => {
    runSearch(query);
  };

  // Pre-compute values for rendering
  const isEmpty = hits.length === 0 && query.trim().length > 0;
  const showIdle = searchState === "idle" && query.trim().length === 0;

  // ── Render helpers ──────────────────────────────────────────────────

  const renderIdle = (): ReactNode => (
    <div className="text-center py-12 text-gray-500">
      <Icon name="search" className="w-12 h-12 mx-auto mb-3 opacity-30" />
      <div className="text-lg font-medium text-gray-300">Search your vault</div>
      <div className="text-xs text-gray-500 mt-1">
        Type a query and press Enter to find notes by title and content.
      </div>
    </div>
  );

  const renderLoading = (): ReactNode => (
    <div className="flex items-center justify-center py-12 text-gray-500">
      <div className="flex flex-col items-center gap-2">
        <div className="w-5 h-5 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
        <span className="text-sm">Searching…</span>
      </div>
    </div>
  );

  const renderError = (): ReactNode => (
    <div className="text-center py-12 text-gray-400">
      <div className="mb-2 font-medium text-red-300">Search failed</div>
      <div className="text-sm text-gray-500 mb-4">
        {error || "Could not search the vault."}
      </div>
      <button
        type="button"
        onClick={handleRetry}
        className="px-3 py-1 text-xs rounded bg-gray-800 hover:bg-gray-700 text-gray-200"
      >
        Retry
      </button>
    </div>
  );

  const renderResults = (): ReactNode => {
    if (hits.length === 0) return null;
    return (
      <div className="space-y-1">
        {hits.map((hit) => {
          const path = hit.path;
          return (
            <div
              key={`${hit.path}-${hit.match_start}`}
              className="group flex items-start gap-3 px-3 py-3 rounded-lg hover:bg-gray-800/50 transition-colors border border-transparent hover:border-gray-700 cursor-pointer"
              onClick={() => ws.openTab(path)}
            >
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium text-gray-200 truncate">
                  {hit.title}
                </div>
                {hit.folder && (
                  <span className="text-xs text-gray-500">{hit.folder}/</span>
                )}
                <div className="mt-1 text-xs text-gray-400 line-clamp-2">
                  {highlightMatch(
                    hit.snippet,
                    hit.match_start,
                    hit.match_end,
                  )}
                </div>
                {hit.modified_at && (
                  <div className="mt-1 text-[10px] text-gray-600">
                    {hit.modified_at}
                  </div>
                )}
              </div>
              <Icon
                name="externalLink"
                className="w-3 h-3 text-gray-600 mt-0.5 shrink-0"
              />
            </div>
          );
        })}
      </div>
    );
  };

  // ── Main render ───────────────────────────────────────────────────────

  return (
    <div className="search-page h-screen flex flex-col bg-gray-950 text-gray-100">
      {/* Search input */}
      <div className="p-4 border-b border-gray-800">
        <div className="relative max-w-2xl mx-auto">
          <input
            ref={inputRef}
            type="text"
            placeholder="Search your vault… (press Enter)"
            className="w-full bg-gray-800 text-gray-100 rounded-lg px-4 py-2.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none pr-10"
            autoComplete="off"
            spellCheck={false}
            value={query}
            onChange={handleInputChange}
            onKeyDown={handleKeyDown}
          />
          <span
            className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-500"
            aria-hidden="true"
          >
            <Icon name="search" className="w-4 h-4" />
          </span>
        </div>
      </div>

      {/* Results area */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="max-w-2xl mx-auto">
          {searchState === "loading" && renderLoading()}
          {searchState === "failed" && renderError()}
          {showIdle && renderIdle()}
          {searchState === "loaded" && isEmpty && (
            <div className="text-center py-12 text-gray-500">
              <Icon name="search" className="w-12 h-12 mx-auto mb-3 opacity-30" />
              <div className="text-lg font-medium text-gray-300">
                No matches found
              </div>
              <div className="text-xs text-gray-500 mt-1">
                Try a different search term or check your spelling.
              </div>
            </div>
          )}
          {searchState === "loaded" && !isEmpty && renderResults()}
        </div>
      </div>
    </div>
  );
}
