// ──────────────────────────────────────────────────────────────────────────────
// ipc.ts — Typed IPC invoke wrappers for every Tauri command
//
// Each wrapper calls invoke<T>(cmdName, args) from @tauri-apps/api/core.
// Return types mirror the Rust return type exactly.
//
// Command catalogue sourced from: src-tauri/src/commands.rs, recovery.rs,
// history.rs. Every `#[tauri::command]` in those files has a wrapper here.
// ──────────────────────────────────────────────────────────────────────────────

import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ArchiveEntry,
  AcpConnectResult,
  CalendarEntry,
  CanvasDef,
  Capability,
  CapabilitySummaryWithState,
  DiffRow,
  GraphData,
  HistoryStatus,
  InboxItem,
  NoteIndexEntry,
  NoteLinks,
  NoteSummary,
  PluginInvocationRequest,
  PluginInvocationResponse,
  PoolHealth,
  QueueItem,
  RecoveryStatus,
  RuntimeMetrics,
  SearchHit,
  ServiceHealth,
  SessionState,
  SmartFolder,
  TemplateRecord,
  Thread,
  TrashRecord,
  TreeEntry,
  VaultStatistics,
  VersionMeta,
} from "./types";

// ── Vault ───────────────────────────────────────────────────────────────────

/** Check if a vault exists at the configured path. Returns the path or null. */
export const checkVaultExists = () =>
  invoke<string | null>("check_vault_exists");

/** Get the current vault path (same as check_vault_exists). */
export const getCurrentVault = () =>
  invoke<string | null>("get_current_vault");

/** Open a native dialog to select an existing vault directory. */
export const selectVaultDialog = () =>
  invoke<string | null>("select_vault_dialog");

/** Open a native dialog to create a new vault directory. */
export const createVaultDialog = () =>
  invoke<string | null>("create_vault_dialog");

/** Complete setup after vault selection (materialises application context). */
export const completeSetup = () => invoke<void>("complete_setup");

/** Reveal a vault-relative path in the OS file manager. */
export const revealInFileManager = (path: string) =>
  invoke<void>("reveal_in_file_manager", { path });

/** Reveal the vault root in the OS file manager. */
export const revealVaultInFileManager = () =>
  invoke<void>("reveal_vault_in_file_manager");

// ── Diagnostics (frontend → backend log) ────────────────────────────────────

/** Send a diagnostic state message to the backend log. */
export const diagReport = (state: string) =>
  invoke<void>("diag_report", { state });

// ── Dictation Pill ──────────────────────────────────────────────────────────

/** Open the dictation pill window. */
export const openDictationPill = () => invoke<void>("open_dictation_pill");

/** Close the dictation pill window. */
export const closeDictationPill = () => invoke<void>("close_dictation_pill");

/** Toggle dictation pill visibility. Returns true if visible after toggle. */
export const toggleDictationPill = () =>
  invoke<boolean>("toggle_dictation_pill");

/** Start dictation. Returns the status string. */
export const startDictation = () => invoke<string>("start_dictation");

/** Stop dictation. Returns a status message. */
export const stopDictation = () => invoke<string>("stop_dictation");

// ── Settings ────────────────────────────────────────────────────────────────

/** Get all application settings. */
export const getSettings = () => invoke<AppSettings>("get_settings");

/** Set a single setting by key (JSON value). */
export const settingsSet = (key: string, value: unknown) =>
  invoke<void>("settings_set", { key, value });

/** Get a single setting by key (returns JSON value). */
export const settingsGet = (key: string) =>
  invoke<unknown>("settings_get", { key });

/** Replace all settings with the given AppSettings object. */
export const settingsSetAll = (settings: AppSettings) =>
  invoke<void>("settings_set_all", { settings });

/** Export settings as JSON bytes. */
export const settingsExport = () =>
  invoke<number[]>("settings_export");

/** Import settings from JSON bytes. Returns the imported AppSettings. */
export const settingsImport = (payload: number[]) =>
  invoke<AppSettings>("settings_import", { payload });

/** Reset settings to defaults. Returns the reset AppSettings. */
export const settingsReset = () => invoke<AppSettings>("settings_reset");

/** Open the settings window. */
export const openSettings = () => invoke<void>("open_settings");

// ── Notes ───────────────────────────────────────────────────────────────────

/** Create a new note file at the given vault-relative path. */
export const noteCreateFile = (path: string, content?: string) =>
  invoke<void>("note_create_file", { path, content: content ?? null });

/** Get the daily note filename for today (YYYY-MM-DD.md). */
export const noteDaily = () => invoke<string>("note_daily");

/** Save a note's content to disk. */
export const noteSave = (path: string, content: string) =>
  invoke<void>("note_save", { path, content });

/** Read a note's content (empty string if not found). */
export const noteRead = (path: string) =>
  invoke<string>("note_read", { path });

/** Rename or move a vault-relative path. */
export const noteRename = (from: string, to: string) =>
  invoke<void>("note_rename", { from, to });

/** Delete a note (moves to trash). */
export const noteDelete = (path: string) =>
  invoke<void>("note_delete", { path });

/** Restore a trashed note. */
export const noteRestore = (trashPath: string) =>
  invoke<void>("note_restore", { trash_path: trashPath });

/** Duplicate a note or folder. Returns the vault-relative dest path. */
export const noteDuplicate = (from: string, dest: string) =>
  invoke<string>("note_duplicate", { from, dest });

/** Move multiple items to a destination folder. Returns new paths. */
export const itemsMove = (items: string[], destFolder: string) =>
  invoke<string[]>("items_move", { items, dest_folder: destFolder });

// ── Notes Index & Search ────────────────────────────────────────────────────

/** Get the full note index (sorted by mtime). */
export const notesIndex = () =>
  invoke<NoteIndexEntry[]>("notes_index");

/** Full-text search across note contents. */
export const notesSearch = (query: string) =>
  invoke<SearchHit[]>("notes_search", { query });

/** Compare two notes and return a line diff. */
export const notesDiff = (pathA: string, pathB: string) =>
  invoke<DiffRow[]>("notes_diff", { path_a: pathA, path_b: pathB });

// ── Note Links (Backlinks / Outgoing / Mentions) ────────────────────────────

/** Get backlinks, outgoing links and unlinked mentions for a note. */
export const noteLinks = (path: string, minTitleLen?: number) =>
  invoke<NoteLinks>("note_links", {
    path,
    min_title_len: minTitleLen ?? null,
  });

/** Convert a plain-text mention to a [[wikilink]]. Returns new content. */
export const linkMention = (path: string, title: string) =>
  invoke<string>("link_mention", { path, title });

/** Get the list of ignored mention titles. */
export const mentionIgnoreList = () =>
  invoke<string[]>("mention_ignore_list");

/** Add a title to the mention ignore list. */
export const mentionIgnore = (title: string) =>
  invoke<void>("mention_ignore", { title });

// ── File Tree ───────────────────────────────────────────────────────────────

/** Get the vault file tree (vault-relative paths). */
export const treeList = () => invoke<TreeEntry[]>("tree_list");

/** Create a folder inside the vault. */
export const folderCreate = (path: string) =>
  invoke<void>("folder_create", { path });

/** Rename a folder inside the vault. */
export const folderRename = (from: string, to: string) =>
  invoke<void>("folder_rename", { from, to });

// ── Knowledge Graph ─────────────────────────────────────────────────────────

/** Get the full knowledge graph (nodes + edges). */
export const graphData = () => invoke<GraphData>("graph_data");

// ── Inbox ───────────────────────────────────────────────────────────────────

/** Subscribe to the inbox (alias for get_queue). */
export const inboxSubscribe = () =>
  invoke<InboxItem[]>("inbox_subscribe");

/** Get all inbox items. */
export const inboxGetQueue = () =>
  invoke<InboxItem[]>("inbox_get_queue");

/** Approve an inbox item (file it into the vault). */
export const inboxApprove = (id: string) =>
  invoke<void>("inbox_approve", { id });

/** Reject an inbox item. */
export const inboxReject = (id: string, reason: string) =>
  invoke<void>("inbox_reject", { id, reason });

/** Retry a failed inbox item (resets to pending). */
export const inboxRetry = (id: string) =>
  invoke<void>("inbox_retry", { id });

/** Delete an inbox item permanently. */
export const inboxDelete = (id: string) =>
  invoke<void>("inbox_delete", { id });

/** Batch-approve multiple inbox items. */
export const inboxBatchApprove = (ids: string[]) =>
  invoke<void>("inbox_batch_approve", { ids });

/** Batch-reject multiple inbox items. */
export const inboxBatchReject = (ids: string[], reason: string) =>
  invoke<void>("inbox_batch_reject", { ids, reason });

/** Batch-delete multiple inbox items. */
export const inboxBatchDelete = (ids: string[]) =>
  invoke<void>("inbox_batch_delete", { ids });

/** Batch-retry multiple inbox items. */
export const inboxBatchRetry = (ids: string[]) =>
  invoke<void>("inbox_batch_retry", { ids });

/** Edit metadata of an inbox item. */
export const inboxEditMetadata = (
  id: string,
  opts: {
    title?: string;
    author?: string;
    language?: string;
    tags?: string[];
    custom?: Record<string, unknown>;
  }
) =>
  invoke<void>("inbox_edit_metadata", {
    id,
    title: opts.title ?? null,
    author: opts.author ?? null,
    language: opts.language ?? null,
    tags: opts.tags ?? [],
    custom: opts.custom ?? {},
  });

/** Move an inbox item to a destination folder. */
export const inboxMove = (id: string, destination: string) =>
  invoke<void>("inbox_move", { id, destination });

/** Quick capture a note into the inbox. */
export const inboxQuickCapture = (title: string, content: string) =>
  invoke<void>("inbox_quick_capture", { title, content });

/** Capture a file drop (drag-and-drop). Returns the new object ID. */
export const captureFileDrop = (
  filename: string,
  mimeType: string,
  data: number[]
) =>
  invoke<string>("capture_file_drop", {
    filename,
    mime_type: mimeType,
    data,
  });

// ── Reading Queue ───────────────────────────────────────────────────────────

/** Get all queue items. */
export const queueGetAll = () =>
  invoke<QueueItem[]>("queue_get_all");

/** Set the reading status of a queue item. */
export const queueSetStatus = (id: string, status: string) =>
  invoke<void>("queue_set_status", { id, status });

/** Set the priority of a queue item. */
export const queueSetPriority = (id: string, priority: string) =>
  invoke<void>("queue_set_priority", { id, priority });

/** Set the reading progress of a queue item (0.0–1.0). */
export const queueSetProgress = (id: string, progress: number) =>
  invoke<void>("queue_set_progress", { id, progress });

/** Batch-set status for multiple queue items. */
export const queueBatchSetStatus = (ids: string[], status: string) =>
  invoke<void>("queue_batch_set_status", { ids, status });

/** Archive all completed queue items. Returns count archived. */
export const queueArchiveCompleted = () =>
  invoke<number>("queue_archive_completed");

// ── Archive ─────────────────────────────────────────────────────────────────

/** Move a note to the archive folder. */
export const archiveNote = (path: string) =>
  invoke<void>("archive_note", { path });

/** Restore an archived note to its original location. */
export const archiveRestore = (archivePath: string) =>
  invoke<void>("archive_restore", { archive_path: archivePath });

/** List all archived notes. */
export const archiveList = () =>
  invoke<ArchiveEntry[]>("archive_list");

// ── Smart Folders ───────────────────────────────────────────────────────────

/** List all saved smart folders. */
export const smartFoldersList = () =>
  invoke<SmartFolder[]>("smart_folders_list");

/** Save (create/update) a smart folder definition. */
export const smartFolderSave = (folder: SmartFolder) =>
  invoke<void>("smart_folder_save", { folder });

/** Delete a smart folder by id. */
export const smartFolderDelete = (id: string) =>
  invoke<void>("smart_folder_delete", { id });

/** Evaluate a smart folder query and return matching notes. */
export const smartFolderEvaluate = (query: string) =>
  invoke<NoteIndexEntry[]>("smart_folder_evaluate", { query });

// ── Calendar ────────────────────────────────────────────────────────────────

/** Get notes dated within a month (YYYY-MM). */
export const calendarNotes = (month: string) =>
  invoke<CalendarEntry[]>("calendar_notes", { month });

/** Get (or create) the daily note for a date (YYYY-MM-DD). */
export const dailyNoteFor = (date: string) =>
  invoke<string>("daily_note_for", { date });

// ── Templates ───────────────────────────────────────────────────────────────

/** List all saved templates. */
export const templateList = () =>
  invoke<TemplateRecord[]>("template_list");

/** Save (create/update) a template. */
export const templateSave = (template: TemplateRecord) =>
  invoke<void>("template_save", { template });

/** Delete a template by name. */
export const templateDelete = (name: string) =>
  invoke<void>("template_delete", { name });

/** Duplicate a template. Returns the copy. */
export const templateDuplicate = (name: string) =>
  invoke<TemplateRecord>("template_duplicate", { name });

/** Set/unset a template as favourite. */
export const templateSetFavourite = (name: string, favourite: boolean) =>
  invoke<void>("template_set_favourite", { name, favourite });

// ── Canvas ──────────────────────────────────────────────────────────────────

/** List all saved canvases (includes nodes/edges/groups). */
export const canvasList = () =>
  invoke<CanvasDef[]>("canvas_list");

/** Get a full canvas definition by id. */
export const canvasGet = (id: string) =>
  invoke<CanvasDef | null>("canvas_get", { id });

/** Create or update a canvas (deduped by id). */
export const canvasSave = (canvas: CanvasDef) =>
  invoke<void>("canvas_save", { canvas });

/** Delete a canvas by id. */
export const canvasDelete = (id: string) =>
  invoke<void>("canvas_delete", { id });

// ── Statistics ──────────────────────────────────────────────────────────────

/** Get comprehensive vault statistics. */
export const statisticsGet = () =>
  invoke<VaultStatistics>("statistics_get");

// ── Version History ─────────────────────────────────────────────────────────

/** List snapshot versions for a note. */
export const versionsList = (path: string) =>
  invoke<VersionMeta[]>("versions_list", { path });

/** Get the content of a specific snapshot. */
export const versionsGet = (path: string, id: string) =>
  invoke<string>("versions_get", { path, id });

/** Restore a snapshot over the live note. */
export const versionsRestore = (path: string, id: string) =>
  invoke<void>("versions_restore", { path, id });

/** Copy a snapshot to a new note path. */
export const versionsDuplicate = (path: string, id: string, dest: string) =>
  invoke<void>("versions_duplicate", { path, id, dest });

/** Diff two snapshots (or snapshot vs live). */
export const versionsDiff = (
  path: string,
  idA: string | null,
  idB: string | null
) => invoke<DiffRow[]>("versions_diff", { path, id_a: idA, id_b: idB });

/** Create a manual snapshot. */
export const snapshotCreate = (path: string) =>
  invoke<VersionMeta>("snapshot_create", { path });

/** List every note that has snapshots (snapshot browser). */
export const versionsAll = () =>
  invoke<NoteSummary[]>("versions_all");

// ── Session & Recovery ──────────────────────────────────────────────────────

/** Persist the current workspace session. */
export const sessionSave = (state: SessionState) =>
  invoke<void>("session_save", { state });

/** Load the persisted session. */
export const sessionLoad = () =>
  invoke<SessionState | null>("session_load");

/** Clear the persisted session. */
export const sessionClear = () => invoke<void>("session_clear");

/** Check if the previous run crashed and if a session is available. */
export const recoveryCheck = () =>
  invoke<RecoveryStatus>("recovery_check");

/** Discard the recovery-pending marker. */
export const recoveryDiscard = () => invoke<void>("recovery_discard");

// ── History / Undo-Redo ─────────────────────────────────────────────────────

/** Get current undo/redo state. */
export const historyStatus = () =>
  invoke<HistoryStatus>("history_status");

/** Undo the most recent operation. Returns the label or null. */
export const historyUndo = () =>
  invoke<string | null>("history_undo");

/** Redo the most recently undone operation. Returns the label or null. */
export const historyRedo = () =>
  invoke<string | null>("history_redo");

/** Clear all history. */
export const historyClear = () => invoke<void>("history_clear");

/** Set the maximum undo history depth. */
export const historySetDepth = (depth: number) =>
  invoke<void>("history_set_depth", { depth });

// ── Trash ───────────────────────────────────────────────────────────────────

/** List all trashed items. */
export const trashList = () =>
  invoke<TrashRecord[]>("trash_list");

/** Permanently delete trashed items. Returns count deleted. */
export const trashDelete = (trashPaths: string[]) =>
  invoke<number>("trash_delete", { trash_paths: trashPaths });

/** Restore multiple trashed items. Returns restored paths. */
export const trashRestoreMany = (trashPaths: string[]) =>
  invoke<string[]>("trash_restore_many", { trash_paths: trashPaths });

/** Purge expired trashed items. Returns count purged. */
export const trashPurgeExpired = () =>
  invoke<number>("trash_purge_expired");

/** Empty the entire trash. Returns count removed. */
export const trashEmpty = () => invoke<number>("trash_empty");

// ── Capabilities ────────────────────────────────────────────────────────────

/** Enable a capability by id. */
export const capabilityEnable = (capabilityId: string) =>
  invoke<void>("capability_enable", { capability_id: capabilityId });

/** Disable a capability by id. */
export const capabilityDisable = (capabilityId: string) =>
  invoke<void>("capability_disable", { capability_id: capabilityId });

/** List all registered capabilities. */
export const capabilityList = () =>
  invoke<Capability[]>("capability_list");

/** List capabilities with runtime enabled state. */
export const capabilityListWithState = () =>
  invoke<CapabilitySummaryWithState[]>("capability_list_with_state");

// ── Health / Metrics ────────────────────────────────────────────────────────

/** Get service health report. */
export const healthCheck = () =>
  invoke<ServiceHealth>("health_check");

/** Get runtime metrics (timers, counters, gauges). */
export const metrics = () =>
  invoke<RuntimeMetrics>("metrics");

/** Get worker pool health. */
export const poolHealth = () =>
  invoke<PoolHealth>("pool_health");

// ── Diagnostics ─────────────────────────────────────────────────────────────

/** Run diagnostic analysis on a document. */
export const diagnosticRequested = (request: {
  text: string;
  resource_id: string;
  origin?: string;
}) =>
  invoke<{ batch: unknown; style_map: unknown }>("diagnostic_requested", {
    request,
  });

// ── Plugin ──────────────────────────────────────────────────────────────────

/** Dispatch a plugin capability invocation. */
export const pluginCall = (request: PluginInvocationRequest) =>
  invoke<PluginInvocationResponse>("plugin_call", { request });

// ── Threads / Conversation ─────────────────────────────────────────────────

/** Save a conversation thread. */
export const threadSave = (thread: Thread) =>
  invoke<void>("thread_save", { thread });

/** Load a conversation thread by id. */
export const threadLoad = (id: string) =>
  invoke<Thread | null>("thread_load", { id });

/** List all conversation threads. */
export const threadList = () =>
  invoke<Thread[]>("thread_list");

/** Delete a conversation thread by id. */
export const threadDelete = (id: string) =>
  invoke<void>("thread_delete", { id });

// ── Platform Integration ────────────────────────────────────────────────────

/** Open the app in Finder (macOS only). */
export const openAppInFinder = () =>
  invoke<void>("open_app_in_finder");

/** Show a macOS notification. */
export const showMacOSNotification = (title: string, body: string) =>
  invoke<void>("show_macos_notification", { title, body });

/** Pin to taskbar (Windows only). */
export const pinToTaskbar = () => invoke<void>("pin_to_taskbar");

/** Open in Explorer (Windows only). */
export const openInExplorer = () => invoke<void>("open_in_explorer");

/** Open in file manager (Linux). */
export const openInFileManager = () =>
  invoke<void>("open_in_file_manager");

/** Show a Linux notification. */
export const showLinuxNotification = (title: string, body: string) =>
  invoke<void>("show_linux_notification", { title, body });

/** Install a desktop entry (Linux). */
export const installDesktopEntry = () =>
  invoke<void>("install_desktop_entry");

// ── Streaming ───────────────────────────────────────────────────────────────

/** Cancel an active streaming session. */
export const streamCancel = (streamId: string, reason: string) =>
  invoke<boolean>("stream_cancel", { stream_id: streamId, reason });

// ── ACP (Agent Communication Protocol) ──────────────────────────────────────

/** Connect to an external ACP agent. */
export const acpConnect = (config: {
  agent_command: string[];
  agent_args?: string[];
  agent_env?: Record<string, string>;
  agent_cwd?: string;
}) => invoke<AcpConnectResult>("acp_connect", { config });

/** Send a user message to an ACP session. */
export const acpSendMessage = (threadId: string, message: string) =>
  invoke<void>("acp_send_message", { thread_id: threadId, message });

/** Cancel the active ACP prompt turn. */
export const acpCancel = (threadId: string) =>
  invoke<void>("acp_cancel", { thread_id: threadId });

/** Disconnect from an ACP session. */
export const acpDisconnect = (threadId: string) =>
  invoke<void>("acp_disconnect", { thread_id: threadId });

/** List known ACP session IDs. */
export const acpListSessions = () =>
  invoke<string[]>("acp_list_sessions");

/** Respond to an ACP permission request. */
export const acpPermissionRespond = (
  threadId: string,
  requestId: string,
  outcome: "Approved" | "Denied"
) =>
  invoke<void>("acp_permission_respond", {
    thread_id: threadId,
    request_id: requestId,
    outcome,
  });
