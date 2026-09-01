# Pivot to TypeScript + React frontend (Tauri shell over existing Rust core)

Status: decided 2026-08-25. The Dioxus/WASM frontend (`crates/nabu-ui`) is **deleted**
from main (Wave 4 complete). Its source is preserved on branch
`freeze/dioxus-rust-frontend` at commit `6b2a087`. This document is the
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

> ✅ = already deleted by Wave 4 (Agent D).

| Path | Why | Status |
|---|---|---|
| `crates/nabu-ui/` | Entire Dioxus frontend (lib.rs, components/, events/, ipc.rs, etc.) | ✅ Deleted |
| `crates/nabu-ui/dioxus.toml`, `crates/nabu-ui/web/` | dx bundle output + config | ✅ Deleted (with crate) |
| `crates/nabu-ui/src/bin/dx_bundle.rs` | dx bundling entry | ✅ Deleted (with crate) |
| `src-tauri/scripts/build-dioxus.sh` | manual wasm-bindgen pipeline (no longer needed) | ✅ Deleted |
| `src-tauri/scripts/dx-bundle.sh` | dx pipeline | ✅ Deleted |
| `src-tauri/scripts/run-dioxus.sh` | dev server for dx | ✅ Deleted |
| `build-dioxus.sh` (repo root) | manual wasm-bindgen pipeline (no longer needed) | ✅ Deleted |
| `dist/` (repo root) | old wasm output | ✅ Removed |
| `dioxus.toml` (repo root) | root dx config | ✅ Deleted |
| `Trunk.toml` | dead (never existed; leave out) | — |
| `crates/nabu-ui/web/public/index.html` boot-placeholder logic | replaced by Vite `index.html` | ✅ Deleted (with crate) |

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

---

# Actionable migration prompts — 12 parallel agents

> **Status: OPEN.** Executes the Vite 8 + React 19 + TS + Tailwind 4 frontend
> rebuild over the existing Rust core (`nabu-core` + `src-tauri`, 87 IPC commands
> in `src-tauri/src/commands.rs`). Source-of-truth references:
> - `docs/react-migration-plan.md` (this file) — stack + constraints + gotchas.
> - `crates/nabu-ui/` — the existing Dioxus frontend in main (read-only spec of
>   behavior; do NOT port code, read it to learn layout/behavior/which commands
>   each screen calls). It is deleted in Wave 4.
>
> **Headline:** 12 agents across 3 waves + a Wave 4 cleanup. Wave 1 = scaffold (1).
> Wave 2 = IPC+types+context (1) + app shell (1) — run after Wave 1 lands.
> Wave 3 = 10 view clusters, each owning a disjoint set of NEW `ui-react/src/...`
> files, run in parallel (up to ~10 concurrent). Wave 4 = delete the old Dioxus
> WASM frontend from main. Shared git tree → each agent polices ONLY its paths
> (SCOPE FENCE). A closeout agent (CL) runs `tauri build` + typecheck to confirm
> a booting app.

**Execution model:** one agent per prompt. Within a wave, prompts are
INDEPENDENT (disjoint file sets — verify zero overlap). Agents read
`crates/nabu-ui` as spec only; they WRITE only under `ui-react/`.

**Shared facts:**
- Toolchain: Vite 8, React 19, TypeScript ~5, Tailwind 4 (`@tailwindcss/postcss`),
  `@vitejs/plugin-react`, `@tauri-apps/api` ^2.11. pnpm.
- IPC: `import { invoke } from "@tauri-apps/api/core";` — NO `withGlobalTauri`.
- Use `h-screen`/`w-screen`, never `dvh`/`dvw` (WKWebView collapses to 0 height).
- Each new React component maps to a Dioxus file of the same name under
  `crates/nabu-ui/src/components/` — read that file for behavior, then write
  idiomatic React. Delete `crates/nabu-ui` only after Wave 3 closeout.

---

## Shared closeout role

### PROMPT — CL · Migration closeout
```
You are Agent CL, migration closeout for the Nabu React rebuild. Run ONCE after Waves 1-3 land.
VERIFIED CONTEXT: 12 agents build a Vite+React+Tailwind4 frontend (ui-react/) that talks to the
87 existing Tauri IPC commands in src-tauri/src/commands.rs. The old Dioxus frontend (crates/nabu-ui/) is deleted in Wave 4.
SCOPE: build + typecheck + boot smoke. Read-only on src-tauri and nabu-core (do NOT edit backend).
STRATEGY:
1. cd ui-react && pnpm install && pnpm build          # expect dist/ with index.html + assets
2. cargo tauri build --bundles app                     # expect Nabu.app
3. Launch the app headless-ish: timeout 25 <app>/Contents/MacOS/app > /tmp/nabu-cl.log 2>&1
4. Confirm no panic; confirm window paints (if available, screenshot). Any blank/black screen -> report which wave regressed.
GUARDRAILS: READ-ONLY on backend; never lower a typecheck floor; SCOPE FENCE; HONEST REPORT.
DoD: [ ] pnpm build exits 0; [ ] cargo tauri build exits 0; [ ] app boots past the vault check; Report residual (should be empty)
```

---

## WAVE 1 — FOUNDATION (1 prompt, run first)

### PROMPT — S · Scaffold + Tauri wiring
```
You are Agent S, scaffolding the Nabu React frontend (Wave 1).
DOMAIN OWNERSHIP: you own the new ui-react/ workspace root and tauri.conf.json wiring. No other agent touches scaffolding.
VERIFIED CONTEXT: buzz-main (reference) uses Vite 8 + React 19 + TS + Tailwind 4 + @tauri-apps/api v2, tauri.conf devUrl http://localhost:1420, frontendDist ../dist, beforeDevCommand=vite. Our backend (src-tauri) already has 87 IPC commands + capabilities/default.json=["core:default"].
SCOPE (STRICT): create ui-react/ (package.json, vite.config.ts, tsconfig.json, index.html, src/main.tsx, tailwind via @tailwindcss/postcss + postcss.config.js, src/index.css). Edit src-tauri/tauri.conf.json: frontendDist "../ui-react/dist", devUrl "http://localhost:1420", beforeDevCommand { script: "pnpm dev", cwd: ".." } (or vite), beforeBuildCommand "pnpm build". Do NOT edit backend Rust.
FIX STRATEGY / BUILD:
  1. pnpm workspace: deps react@19 react-dom@19 @tauri-apps/api@^2.11; dev: vite@^8 @vitejs/plugin-react typescript @tailwindcss/postcss tailwindcss@^4 @types/react @types/react-dom.
  2. vite.config.ts: react() plugin; server.port 1420 strictPort; clearScreen false; ignore src-tauri.
  3. index.html mounts #root; src/main.tsx renders <App/>.
  4. App.tsx (minimal): window with "Nabu" + a button that calls invoke("check_vault_exists", {}) and prints the result. This proves the IPC path.
  5. src/ipc.ts baseline: export const checkVaultExists = () => invoke("check_vault_exists", {}).
VERIFY:
  cd ui-react && pnpm install && pnpm dev          # app reachable at :1420, button returns a vault path or null
  pnpm build                                        # expect ui-react/dist/index.html
  cargo tauri build --bundles app                   # expect Nabu.app that shows the "Nabu" window
GUARDRAILS: no dvh/dvw; no withGlobalTauri; READ-BEFORE-WRITE on tauri.conf.json; TSC NON-REGRESSION (tsc --noEmit 0 new err); HONEST REPORT.
DoD:
- [ ] ui-react/ builds; dev server serves on :1420
- [ ] tauri.conf.json points at ui-react; cargo tauri build produces Nabu.app
- [ ] Button invoking check_vault_exists works in the running app
- [ ] Report: build + boot evidence
```

---

## WAVE 2 — IPC + CONTEXT + SHELL (2 prompts, run after Wave 1)

### PROMPT — I · IPC layer + shared types + context
```
You are Agent I, building the IPC + types + context foundation (Wave 2).
DOMAIN OWNERSHIP: you own ui-react/src/ipc.ts, ui-react/src/types.ts, ui-react/src/context/ (nav, theme, workspace, toast, history, save-status). No view agent edits these.
VERIFIED CONTEXT: 87 commands in src-tauri/src/commands.rs (vault, settings, notes, inbox, templates, graph, recovery, history, trash, canvas, statistics, dictation, capabilities, diagnostics, threads, acp, streaming, plugin_call). Each command has a Rust arg struct + return type — read commands.rs + nabu-core models to derive TS types.
SCOPE (STRICT): ONLY ui-react/src/ipc.ts, types.ts, context/*. Read crates/nabu-ui/src/ipc.rs + components/contexts.rs + nabu-core for shapes. Do NOT write view components (Wave 3 owns those) — but DO define the context interfaces they will consume.
FIX STRATEGY / BUILD:
  1. types.ts: TS interfaces for every command payload/return used by the UI (SettingsSnapshot, VaultPath, NoteSummary, VersionMeta, Template, GraphData, InboxItem, Statistics, etc). Mirror Rust serde shapes exactly.
  2. ipc.ts: one typed wrapper per command the UI needs (start with the ~40 the shell+views use; add others as Waves need). Signature: invoke<T>("cmd", args): Promise<T>.
  3. context/: NavContext (view_mode, vault_name, show_left_sidebar, show_right_inspector), ThemeContext, WorkspaceContext (active_path), ToastProvider, HistoryProvider, SaveStatusProvider. Use React context + hooks. Match the provider contract in crates/nabu-ui/src/components/contexts.rs (read-only spec).
VERIFY:
  cd ui-react && pnpm exec tsc --noEmit      # 0 errors
  pnpm build                                  # succeeds
GUARDRAILS: types match Rust serde EXACTLY (field names + optionality); never invent commands not in commands.rs; TSC NON-REGRESSION; READ-BEFORE-WRITE; HONEST REPORT.
DoD:
- [ ] ipc.ts covers all 87 commands (or a documented subset + clear TODO list for the rest)
- [ ] types.ts mirrors Rust models
- [ ] context/ providers compile and export the hooks Wave 3 consumes
- [ ] tsc --noEmit clean
```

### PROMPT — SH · App shell (ribbon/sidebars/tab/nav/inspector)
```
You are Agent SH, building the app shell (Wave 2).
DOMAIN OWNERSHIP: you own ui-react/src/components/layout/* (RibbonBar, LeftSidebar, TabBar, NavBar, RightInspector) + App.tsx router (vault-check -> view switch). Depends on Agent I's context/ipc.
VERIFIED CONTEXT: shell spec in crates/nabu-ui/src/components/layout/workspace.rs + app.rs (AppRouter). Uses h-screen root; reads NavContext for view switching; calls check_vault_exists then routes to VaultSetup or MainDashboard.
SCOPE (STRICT): ui-react/src/App.tsx, ui-react/src/components/layout/**. Import from ../ipc and ../context (Agent I). Do NOT build view contents (Wave 3 owns ViewContent children).
FIX STRATEGY / BUILD:
  1. App.tsx: on mount call check_vault_exists; loading spinner (h-screen, NOT dvh); on Ok(path) -> workspace; on null -> VaultSetupWizard; on error -> error text.
  2. WorkspaceLayout: root div "flex h-screen w-screen bg-gray-950 text-gray-100 overflow-hidden". Compose RibbonBar + (LeftSidebar if show) + main(TabBar+NavBar+ViewContent) + (RightInspector if show) + overlays (CommandPalette/QuickSwitcher/ShortcutReference stubs).
  3. LeftSidebar = vault file tree (calls tree_list). TabBar/NavBar drive NavContext.view_mode.
VERIFY:
  cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen only (NO dvh/dvw); theme via data-theme attr; TSC NON-REGRESSION; READ-BEFORE-WRITE; HONEST REPORT.
DoD:
- [ ] Shell renders ribbon + sidebars + tab/nav + inspector from context
- [ ] Vault check routes correctly; shell is visible (not zero-height)
- [ ] tsc --noEmit clean; builds
```

---

## WAVE 3 — VIEW CLUSTERS (10 prompts, parallel)

> Each owns NEW ui-react/src/components/<area>/** files. They all consume
> Agent I's ipc.ts + context. Disjoint areas. Read the matching Dioxus file in
> crates/nabu-ui/src/components/<area> for behavior, then write React. The
> Dioxus source lives in main — that is your only reference.

### PROMPT — V1 · Navigation views (dashboard/home/search/calendar/smart_folders/archive)
```
You are Agent V1 (Wave 3), navigation views.
DOMAIN OWNERSHIP: ui-react/src/components/navigation/** (Dashboard, HomeScreen, SearchPage, CalendarPage, SmartFoldersPage, breadcrumb, view_switcher) + navigation/state.ts (NavContext view-mode enum + helper).
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/navigation/*.rs. Commands used: notes_search, calendar_notes, daily_note_for, smart_folders_list/evaluate/save/delete, graph_data. Consumes NavContext from Agent I.
SCOPE (STRICT): ui-react/src/components/navigation/** + state.ts. Import ipc/context from Agent I. No shell edits.
FIX STRATEGY: port each view to React+TS; data via ipc.ts wrappers; Tailwind classes only; h-screen containers.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen (no dvh); types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] navigation views render + call IPC; [ ] tsc clean; [ ] git diff shows only navigation/**
```

### PROMPT — V2 · Inbox (capture + batch)
```
You are Agent V2 (Wave 3), Inbox.
DOMAIN OWNERSHIP: ui-react/src/components/inbox/** (Inbox, quick-capture, batch handlers, drag/drop, keyboard shortcuts).
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/inbox.rs (~1300 lines). Commands: inbox_subscribe, inbox_get_queue, inbox_quick_capture, inbox_approve/reject/retry/delete, inbox_move, inbox_edit_metadata, inbox_batch_* , capture_file_drop. Consumes NavContext + ToastProvider.
SCOPE (STRICT): ui-react/src/components/inbox/**. Import ipc/context from Agent I.
FIX STRATEGY: port Inbox to React; implement batch approve/reject/delete, drag/drop (dnd-kit), keyboard shortcuts; call IPC wrappers.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] Inbox renders queue + batch ops + capture; [ ] tsc clean; [ ] git diff shows only inbox/**
```

### PROMPT — V3 · Editor + reader + comparison (notes)
```
You are Agent V3 (Wave 3), note editor.
DOMAIN OWNERSHIP: ui-react/src/components/editor/** (NoteEditor, slash_menu) + note_view + reader + comparison + components/shipped/* (reader, comparison).
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/{note_editor,note_view,reader,comparison}.rs + editor/*.rs + shipped/*.rs. Commands: note_create_file, note_daily, notes_diff, note_links, link_mention, mention_ignore(_list), archive_note/restore, daily_note_for. Consumes WorkspaceContext.active_path.
SCOPE (STRICT): ui-react/src/components/editor/** + note_view.tsx + reader.tsx + comparison.tsx + shipped/*.tsx.
FIX STRATEGY: port editor to React; Tiptap for rich text (match buzz stack); slash menu; reader + comparison views.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] editor + reader + comparison render + call IPC; [ ] tsc clean; [ ] git diff shows only editor/**
```

### PROMPT — V4 · Settings (15 tabs)
```
You are Agent V4 (Wave 3), Settings.
DOMAIN OWNERSHIP: ui-react/src/components/settings/** (SettingsPanel + capability_management) + ui-react/src/components/theme_toggle.tsx.
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/settings/*.rs (settings_panel.rs lists 15 tabs; AppSettings struct in nabu-core). Commands: get_settings, settings_get/set/set_all/export/import/reset, capability_enable/disable/list/list_with_state. Consumes ThemeContext + SettingsSnapshot.
SCOPE (STRICT): ui-react/src/components/settings/** + theme_toggle.tsx.
FIX STRATEGY: port 15-tab SettingsPanel to React; wire every control to settings_set/settings_get; theme toggle sets data-theme.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts + AppSettings; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] Settings panel renders 15 tabs + persists; [ ] tsc clean; [ ] git diff shows only settings/**
```

### PROMPT — V5 · Graph view
```
You are Agent V5 (Wave 3), Graph.
DOMAIN OWNERSHIP: ui-react/src/components/graph_view.tsx + graph_layout.tsx.
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/{graph_view,graph_layout}.rs. Commands: graph_data, note_links, link_mention, mention_ignore(_list). Consumes WorkspaceContext.
SCOPE (STRICT): ui-react/src/components/graph_view.tsx + graph_layout.tsx.
FIX STRATEGY: render graph (canvas or svg) from graph_data; node click -> WorkspaceContext.active_path.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] graph renders + interactive; [ ] tsc clean; [ ] git diff shows only graph_*
```

### PROMPT — V6 · Recovery + history (versions/diff/snapshots)
```
You are Agent V6 (Wave 3), Recovery/History.
DOMAIN OWNERSHIP: ui-react/src/components/recovery/** (RecoveryManager, VersionHistory, DiffView, RecoveryBanner) + ui-react/src/components/trash.tsx + history wiring.
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/recovery/*.rs + trash.rs. Commands: versions_all/list/get/diff/restore/duplicate, snapshot_create, recovery_check/discard, trash_list/delete/restore_many/purge_expired/empty, history_undo/redo/clear/set_depth, note_rename/delete/duplicate, items_move, archive_*. Consumes HistoryProvider + SaveStatusProvider.
SCOPE (STRICT): ui-react/src/components/recovery/** + trash.tsx.
FIX STRATEGY: port version history + diff viewer + trash + recovery manager to React; wire undo/redo buttons to history_*.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] recovery + trash render + call IPC; [ ] tsc clean; [ ] git diff shows only recovery/** + trash.tsx
```

### PROMPT — V7 · Templates + canvas + collections
```
You are Agent V7 (Wave 3), Templates/Canvas/Collections.
DOMAIN OWNERSHIP: ui-react/src/components/template_editor.tsx + template_picker.tsx + canvas.tsx + collections/** (gallery_view, board_view, table_view, calendar_view, container, view_switcher, mod).
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/{template_editor,template_picker,canvas}.rs + collections/*.rs. Commands: template_list/save/delete/duplicate/set_favourite, canvas_list/get/save/delete, notes_index, graph_data. Consumes NavContext.
SCOPE (STRICT): ui-react/src/components/{template_editor,template_picker,canvas}.tsx + collections/**.
FIX STRATEGY: port template editor/picker + canvas + collection views (gallery/board/table/calendar) to React.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] templates + canvas + collections render + call IPC; [ ] tsc clean; [ ] git diff shows only these paths
```

### PROMPT — V8 · Statistics + activity + reading-queue
```
You are Agent V8 (Wave 3), Statistics/Activity/ReadingQueue.
DOMAIN OWNERSHIP: ui-react/src/components/statistics.tsx + activity/** (mod, panel) + reading_queue.tsx + dashboard/metrics.
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/{statistics,reading_queue}.rs + activity/*.rs. Commands: statistics_get, notes_index, graph_data. Consumes NavContext.
SCOPE (STRICT): ui-react/src/components/statistics.tsx + activity/** + reading_queue.tsx.
FIX STRATEGY: port statistics dashboard (metrics + growth histogram + tags + recent) + activity panel + reading queue to React.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] statistics + activity + reading queue render; [ ] tsc clean; [ ] git diff shows only these paths
```

### PROMPT — V9 · Dictation pill + overlays (command palette/quick switcher/shortcuts)
```
You are Agent V9 (Wave 3), Dictation + overlays.
DOMAIN OWNERSHIP: ui-react/src/components/dictation_pill.tsx + navigation/{command_palette,quick_switcher,shortcuts}.tsx + sandbox*.tsx.
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/{dictation_pill}.rs + navigation/{command_palette,quick_switcher,shortcuts}.rs + sandbox*.rs. Commands: open_dictation_pill, close_dictation_pill, toggle_dictation_pill, start_dictation, stop_dictation, capture_file_drop. Consumes ToastProvider + NavContext.
SCOPE (STRICT): ui-react/src/components/dictation_pill.tsx + navigation/{command_palette,quick_switcher,shortcuts}.tsx + sandbox*.tsx.
FIX STRATEGY: port dictation pill (opacity, clipboard cache panel, drop zone -> capture_file_drop, copy button) + command palette + quick switcher + shortcut reference to React.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] dictation pill + overlays render + call IPC; [ ] tsc clean; [ ] git diff shows only these paths
```

### PROMPT — V10 · File tree + property/relation editors + misc (pdf/chat/streaming)
```
You are Agent V10 (Wave 3), FileTree + editors + misc.
DOMAIN OWNERSHIP: ui-react/src/components/file_tree.tsx + property_editor.tsx + relation_editor.tsx + pdf_viewer.tsx + chat.tsx + streaming/** + diagnostics.tsx + ui/ primitives if not provided by shell.
VERIFIED CONTEXT: spec in crates/nabu-ui/src/components/{file_tree,property_editor,relation_editor,pdf_viewer,chat,diagnostics}.rs + streaming/*.rs + ui/*.rs. Commands: tree_list, notes_index, note_links, archive_*, canvas_*, stream_cancel, acp_*. Consumes WorkspaceContext + NavContext.
SCOPE (STRICT): ui-react/src/components/{file_tree,property_editor,relation_editor,pdf_viewer,chat,diagnostics}.tsx + streaming/**.
FIX STRATEGY: port file tree + property/relation editors + pdf viewer + chat + streaming surfaces to React; reuse ui/ primitives (button, card, dialog, menu, input, feedback) — create them if Agent SH did not.
VERIFY: cd ui-react && pnpm exec tsc --noEmit && pnpm build
GUARDRAILS: h-screen; types match ipc.ts; TSC NON-REGRESSION; SCOPE FENCE; HONEST REPORT.
DoD: [ ] file tree + editors + misc render + call IPC; [ ] tsc clean; [ ] git diff shows only these paths
```

---

## WAVE 4 — CLEANUP (run only after CL passes)

### PROMPT — D · Delete old Dioxus WASM frontend
```
You are Agent D, deleting the legacy Dioxus WASM frontend from main (Wave 4).
DOMAIN OWNERSHIP: you own the removal of crates/nabu-ui/ (the entire Dioxus frontend) and any references to it in the workspace root (Cargo.toml excludes, build scripts, README, AGENTS.md mentions that point at crates/nabu-ui as the live UI).
VERIFIED CONTEXT: the new React frontend lives in ui-react/ and is the sole frontendDist target (wired in Wave 1). The 87 IPC commands in src-tauri/src/commands.rs and the nabu-core crate are UNAFFECTED and must remain. crates/nabu-ui is no longer referenced by tauri.conf.json.
SCOPE (STRICT): delete crates/nabu-ui/ only. Do NOT touch src-tauri, nabu-core, ui-react, or any Cargo workspace that does not list crates/nabu-ui. Grep the whole repo for "nabu-ui" / "crates/nabu-ui" and remove or repoint every stale reference (docs, scripts, CI). Leave the rest of main intact.
FIX STRATEGY / BUILD:
  1. git rm -r crates/nabu-ui
  2. Update root Cargo.toml workspace members if crates/nabu-ui is listed; remove it.
  3. Grep -r "nabu-ui" . --exclude-dir=.git ; fix each hit (docs, AGENTS.md, build scripts, README) or delete the line if it only described the old UI.
  4. Confirm cargo build (root workspace = nabu-core + src-tauri) still succeeds WITHOUT crates/nabu-ui.
VERIFY:
  git status                                 # only crates/nabu-ui + doc/script references removed
  cargo build                                # root workspace compiles (nabu-core + src-tauri)
  cd ui-react && pnpm build                  # React app still builds
  cargo tauri build --bundles app           # Nabu.app still builds, now React-only
GUARDRAILS: NEVER delete src-tauri, nabu-core, or ui-react; confirm cargo + tauri build GREEN before reporting; SCOPE FENCE; HONEST REPORT. If any command references crates/nabu-ui and is not trivially repointable, STOP and report rather than guess.
DoD:
- [ ] crates/nabu-ui/ removed from main
- [ ] no remaining "nabu-ui" references in build/config/docs (or each repointed)
- [ ] cargo build + cargo tauri build + pnpm build all pass
- [ ] Report: list of files touched + build evidence
```

---

## Execution summary

| Wave | Agents | Runs | Depends on |
|---|---|---|---|
| 1 | S (scaffold) | 1 | — |
| 2 | I (ipc/types/context), SH (shell) | 2 | S |
| 3 | V1–V10 (views) | 10 | I + SH |
| 4 | D (delete Dioxus) | 1 | CL |
| closeout | CL | 1 | all of 1–3 |

Total: **12 agent prompts + 1 closeout + 1 cleanup = 14**. Up to ~10 concurrent in
Wave 3. `D` runs only after `CL` confirms the React app boots. No prompt references
any branch — the Dioxus source in `crates/nabu-ui/` (main) is the sole reference for
the view agents, and `D` removes it from main at the end.
