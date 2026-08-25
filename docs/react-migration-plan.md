# Pivot to TypeScript + React frontend (Tauri shell over existing Rust core)

Status: decided 2026-08-25. The Dioxus/WASM frontend (`crates/nabu-ui`) is **frozen**
on branch `freeze/dioxus-rust-frontend` at commit `6b2a087`. This document is the
migration map for rebuilding the UI in TS/React while keeping `nabu-core` +
`src-tauri` (the Rust engine + 87 IPC commands) exactly as-is.

## Target tech stack — copied from buzz-main (proven, popular Rust app)

buzz-main (the reference) uses exactly this for its desktop UI. Match it:

- **Tauri 2** desktop shell (`src-tauri`) — already in place.
- **Vite 8** + **React 19** + **TypeScript** frontend. (`@vitejs/plugin-react`)
- **Tailwind CSS 4** via `@tailwindcss/postcss` (NOT the v3 CLI we used in Dioxus).
- **TanStack Router** (file-based routing) + **TanStack Query** (async state) +
  **TanStack Virtual** (virtualized lists).
- **Radix UI** primitives + shadcn-style `components.json` (accessible blocks).
- **Tiptap** (rich-text editor), **dnd-kit** (drag/drop), **lucide-react**
  (icons), **sonner** (toasts), **zod** (validation).
- **`@tauri-apps/api` v2** for IPC (`invoke` / `listen`).
- **Biome** for lint/format. **Playwright** for e2e. **pnpm** workspace.
- Dev config (from buzz `tauri.conf.json`): `devUrl: http://localhost:1420`,
  `frontendDist: ../dist`, `beforeDevCommand: vite`, `beforeBuildCommand: pnpm build`.
  `withGlobalTauri` left off — use the `@tauri-apps/api` module, not the global.

Hard constraints (do NOT change the backend):

- **All 87 IPC commands stay.** They are the contract (in `src-tauri/src/commands.rs`).
  The React app talks to them via `@tauri-apps/api`'s `invoke`.
- **`nabu-core` stays** — models, processing, storage, indexer, graph, recovery.
- **`capabilities/default.json`** needs `["core:default"]` (already set) so
  `invoke` + `listen` work. Keep it.

## What gets DELETED (Dioxus / WASM world)

| Path | Why |
|---|---|
| `crates/nabu-ui/` | Entire Dioxus frontend (lib.rs, components/, events/, ipc.rs, etc.) |
| `crates/nabu-ui/dioxus.toml`, `crates/nabu-ui/web/` | dx bundle output + config |
| `crates/nabu-ui/src/bin/dx_bundle.rs` | dx bundling entry |
| `src-tauri/scripts/dx-bundle.sh` | dx pipeline |
| `src-tauri/scripts/run-dioxus.sh` | dev server for dx |
| `build-dioxus.sh` | manual wasm-bindgen pipeline (no longer needed) |
| `dist/` (repo root) | old wasm output |
| `dioxus.toml` (repo root) | root dx config |
| `Trunk.toml` | dead (never existed; leave out) |
| `crates/nabu-ui/web/public/index.html` boot-placeholder logic | replaced by Vite `index.html` |

## What gets REUSED (keep)

- `src-tauri/src/commands.rs` — the 87-command API surface (the new frontend's
  entire data layer).
- `src-tauri/src/lib.rs` `run()` — Tauri builder, window config, invoke_handler.
  **Strip** the diagnostic `on_page_load` console-hook eval + `__nabuLogs` poll
  (lines ~690–800) — that was investigation scaffolding, not product.
- `src-tauri/capabilities/default.json` — keep `core:default`.
- `nabu-core` — unchanged.
- `tauri.conf.json` — point `frontendDist` at the Vite build dir (`../ui-react/dist`),
  `devUrl` at the Vite dev server (`http://localhost:1420`), `beforeDevCommand: vite`,
  `beforeBuildCommand: pnpm build` (matching buzz-main).

## What gets REDONE (React/TS)

New folder `ui-react/` (Vite + React + TS + Tailwind). Maps 1:1 to the Dioxus
component tree. Each Dioxus component under `crates/nabu-ui/src/components/**`
has a React equivalent:

- `app.rs` (AppRouter vault-check + view switch) → `App.tsx` + `useVault()`
  (calls `check_vault_exists`, `complete_setup`, `get_current_vault`).
- `layout/workspace.rs`, `ribbon_bar.rs`, `left_sidebar.rs`, `tab_bar.rs`,
  `navbar.rs`, `right_inspector.rs` → layout shell components.
- `navigation/*` (dashboard, search, home, calendar, smart_folders,
  command_palette, quick_switcher, shortcuts, archive_page) → view components.
- `inbox.rs` (biggest, ~1300 lines) → `Inbox.tsx` + hooks.
- `note_editor.rs`, `editor/*`, `reader.rs`, `comparison.rs` → editor views.
- `settings/settings_panel.rs` (15 tabs) → `Settings.tsx`.
- `graph_view.rs`, `graph_layout.rs` → graph view (canvas/svg).
- `recovery/*` (version_history, diff_view, recovery_manager, recovery_banner)
  → history/recovery UI.
- `statistics.rs` → stats dashboard.
- `template_editor.rs` / `template_picker.rs` → template management.
- `dictation_pill.rs` → dictation pill window UI.
- `collections/*`, `canvas.rs`, `chat.rs`, `activity/*`, `streaming/*`,
  `pdf_viewer.rs`, `sandbox*.rs` → remaining views.

### IPC binding layer (write once, reuse everywhere)

Create `ui-react/src/ipc.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
export const getSettings = () => invoke<any>("get_settings", {});
export const settingsGet = (key: string) => invoke("settings_get", { key });
export const settingsSet = (key: string, value: any) =>
  invoke("settings_set", { key, value });
// ... one wrapper per command actually used by the UI
```

Keep the command list faithful to `src-tauri/src/commands.rs` (87 commands).
NOT all 87 need a wrapper on day one — start with the ones the home/dashboard
screen needs, expand per view.

### Known gotchas to avoid (learned from the Dioxus attempt)

1. **Viewport units**: WKWebView mis-handles `dvh`/`dvw`. Use `h-screen`/`w-screen`
   (or `100vh`/`100vw` with a resize listener). The Dioxus `workspace.rs` used
   `h-dvh w-dvw` which collapsed the layout to zero height — do not repeat.
2. **Window doesn't resize with content**: the Tauri `main` window has a known
   WKWebView intrinsic-size quirk. Keep `resizable: true` and let CSS drive
   layout; don't rely on the webview auto-growing.
3. **Boot visibility**: render the real shell immediately; don't gate the
   first paint on an async IPC round-trip. Show a loading state, then swap.
4. **Single pipeline**: use ONE build path (Vite). Remove the old dx/trunk/
   wasm-bindgen scripts so they can't desync again.

## Suggested build order

1. Scaffold `ui-react/` (Vite 8 + React 19 + TS + Tailwind 4), mirroring
   buzz-main's `desktop/` layout: `src/`, `vite.config.ts`, `tailwind` via
   postcss, `package.json` (pnpm). Wire `tauri.conf.json` (devUrl 1420,
   frontendDist `../ui-react/dist`, beforeDevCommand `vite`). Get a blank window
   with "Nabu" + a button calling `check_vault_exists` working.
2. IPC layer `ipc.ts` (`@tauri-apps/api` `invoke`) for the ~10 commands the
   dashboard needs. Keep the wrapper list faithful to `commands.rs`.
3. App shell (ribbon, sidebars, tab bar, navbar) with `h-screen` — **avoid
   `dvh`/`dvw`** (see gotchas).
4. One real view end-to-end (Inbox or Dashboard) to prove the pattern.
5. Port remaining views incrementally; delete `crates/nabu-ui` once parity is
   reached and the React app is the default `frontendDist`.

## Rollback

`freeze/dioxus-rust-frontend` (`6b2a087`) remains the last working-ish Rust
frontend state. If React stalls, that branch is the safe harbor.
