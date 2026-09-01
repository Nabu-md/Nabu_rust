// ──────────────────────────────────────────────────────────────────────────────
// types.ts — TS interfaces mirroring Rust serde shapes EXACTLY
//
// Source of truth: src-tauri/src/commands.rs, settings.rs, recovery.rs,
//                  history.rs, crates/nabu-core/src/models/
//
// Field names, optionality, and types match Rust #[serde] annotations.
// snake_case serde rename is default in Rust, so TS uses camelCase where
// Tauri auto-converts — but we match the *JSON wire format* which Rust
// serialises as snake_case by default.  Verify each struct.
// ──────────────────────────────────────────────────────────────────────────────

// ── AppSettings (src-tauri/src/settings.rs) ─────────────────────────────────

export interface RecentVaultEntry {
  path: string;
  name: string;
}

export interface AppSettings {
  // Appearance
  theme: string;
  main_window_opacity: number;
  floating_pill_opacity: number;
  pill_hover_boost_opacity: boolean;
  sidebar_width: number;
  inspector_width: number;
  font_size: number;
  line_height: number;
  reduced_motion: boolean;
  high_contrast: boolean;

  // Editor
  editor_mode: string;
  auto_pair_brackets: boolean;
  show_line_numbers: boolean;
  convert_pasted_html_to_markdown: boolean;
  enable_notion_slash_menu: boolean;
  auto_format_filler_words: boolean;
  tab_size: number;
  word_wrap: boolean;
  spell_check: boolean;
  auto_save_interval_secs: number;

  // Markdown
  markdown_gfm: boolean;
  markdown_preserve_line_breaks: boolean;
  markdown_smart_quotes: boolean;
  markdown_math_rendering: boolean;
  markdown_diagram_rendering: boolean;

  // Search
  search_index_on_startup: boolean;
  search_max_results: number;
  search_highlight_matches: boolean;
  search_fuzzy_matching: boolean;

  // Graph
  include_folders_in_graph: boolean;
  folder_click_behavior: string;
  graph_node_physics_gravity: number;
  graph_node_physics_spacing: number;
  graph_show_tags_as_badges: boolean;

  // Files & Vaults
  last_vault_path: string;
  recent_vaults: RecentVaultEntry[];
  default_new_note_path: string;
  trash_retention_policy: string;
  enable_daily_notes: boolean;
  confirm_before_delete: boolean;
  show_hidden_files: boolean;
  sort_files_alphabetically: boolean;

  // Import & Export
  default_export_format: string;
  export_include_metadata: boolean;
  export_include_attachments: boolean;
  import_duplicate_strategy: string;

  // OCR
  ocr_language: string;
  ocr_auto_process_scanned_pdfs: boolean;
  ocr_confidence_threshold: number;

  // Accessibility
  screen_reader_support: boolean;
  keyboard_navigation: boolean;
  focus_ring_visible: boolean;

  // Performance
  max_undo_history: number;
  worker_pool_size: number;
  index_on_startup: boolean;
  background_processing: boolean;

  // Privacy
  launch_at_startup: boolean;
  analytics_enabled: boolean;
  crash_reporting_enabled: boolean;
  auto_lock_on_idle: boolean;
  auto_lock_timeout_mins: number;

  // Keyboard Shortcuts
  voice_hotkey: string;
  quick_capture_hotkey: string;
  toggle_sidebar_hotkey: string;

  // Advanced
  force_sandbox_for_web_snippets: boolean;
  debug_mode: boolean;
  developer_tools: boolean;
  experimental_features: boolean;

  // Experimental
  whisper_model: string;
  enable_ai_summarization: boolean;
  enable_semantic_search: boolean;

  // Extras (backwards-compat)
  extra_settings: Record<string, unknown>;
}

// ── File Tree (commands.rs) ─────────────────────────────────────────────────

export interface TreeEntry {
  name: string;
  path: string;
  is_folder: boolean;
  children: TreeEntry[];
}

// ── Inbox (commands.rs) ─────────────────────────────────────────────────────

export type InboxStatus =
  | "pending"
  | "processing"
  | "ready"
  | "approved"
  | "rejected"
  | "failed";

export interface InboxMetadata {
  title: string | null;
  author: string | null;
  language: string | null;
  source_url: string | null;
  tags: string[];
  custom: Record<string, unknown>;
}

export interface DuplicateInfo {
  confidence: string;
  candidate_ids: string[];
  reason: string | null;
  duplicate_source: string | null;
  content_hash: string | null;
}

export interface TimelineInfo {
  document_date: string | null;
  created_date: string | null;
  modified_date: string | null;
  detected_event_date: string | null;
  extraction_confidence: string | null;
}

export interface OcrInfo {
  extracted_text: string | null;
  confidence: number | null;
  recognition_language: string | null;
  page_count: number | null;
  processing_duration_ms: number | null;
  is_scanned: boolean | null;
  warning: string | null;
}

export interface ProcessingHistoryEntry {
  processor_name: string;
  timestamp: string;
  duration_ms: number;
  success: boolean;
  warnings: string[];
  error: string | null;
}

export interface InboxItem {
  id: string;
  title: string;
  object_type: string;
  source: string;
  status: InboxStatus;
  mime_type: string | null;
  source_file: string | null;
  thumbnail: string | null;
  confidence: number | null;
  suggested_folder: string | null;
  metadata: InboxMetadata;
  duplicate_info: DuplicateInfo | null;
  timeline_info: TimelineInfo | null;
  ocr_info: OcrInfo | null;
  processing_history: ProcessingHistoryEntry[];
  warnings: string[];
  selected: boolean;
}

// ── Reading Queue (commands.rs) ─────────────────────────────────────────────

export type QueueStatus = "unread" | "reading" | "completed" | "archived";
export type QueuePriority = "low" | "normal" | "high";

export interface QueueItem {
  id: string;
  title: string;
  object_type: string;
  status: QueueStatus;
  priority: QueuePriority;
  progress: number;
  source: string;
  modified_at: string;
  tags: string[];
  selected: boolean;
}

// ── Notes Index & Search (commands.rs) ──────────────────────────────────────

export interface NoteIndexEntry {
  path: string;
  title: string;
  folder: string;
  modified_at: string;
  pinned: boolean;
}

export interface SearchHit {
  path: string;
  title: string;
  folder: string;
  snippet: string;
  match_start: number;
  match_end: number;
  modified_at: string;
}

// ── Knowledge Graph (commands.rs) ───────────────────────────────────────────

export interface GraphNode {
  path: string;
  title: string;
  folder: string;
  modified_at: string;
  tags: string[];
  backlink_count: number;
  outgoing_count: number;
  degree: number;
}

export interface GraphEdgeData {
  source: string;
  target: string;
  broken: boolean;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdgeData[];
  orphan_count: number;
  cluster_count: number;
}

// ── Note Links (commands.rs) ────────────────────────────────────────────────

export interface BacklinkEntry {
  path: string;
  title: string;
  folder: string;
  snippet: string;
  match_start: number;
  match_end: number;
  count: number;
}

export interface OutgoingLink {
  kind: string;
  target: string;
  path: string | null;
  count: number;
}

export interface MentionEntry {
  title: string;
  path: string;
  snippet: string;
  match_start: number;
  match_end: number;
  score: number;
}

export interface NoteLinks {
  backlinks: BacklinkEntry[];
  outgoing: OutgoingLink[];
  mentions: MentionEntry[];
  tags: string[];
}

// ── Archive (commands.rs) ───────────────────────────────────────────────────

export interface ArchiveEntry {
  archive_path: string;
  original_path: string;
  title: string;
  folder: string;
  modified_at: string;
}

// ── Smart Folders (commands.rs) ─────────────────────────────────────────────

export interface SmartFolder {
  id: string;
  name: string;
  icon: string;
  query: string;
  pinned: boolean;
}

// ── Calendar (commands.rs) ──────────────────────────────────────────────────

export interface CalendarEntry {
  path: string;
  title: string;
  folder: string;
  date: string;
  modified_at: string;
}

// ── Templates (commands.rs) ─────────────────────────────────────────────────

export interface TemplateRecord {
  name: string;
  description: string | null;
  icon: string | null;
  default_folder: string | null;
  category: string | null;
  favourite: boolean;
  frontmatter_defaults: Record<string, string>;
  property_presets: Record<string, unknown>;
  body: string;
  object_type: string | null;
}

// ── Canvas (commands.rs) ────────────────────────────────────────────────────

export interface CanvasNode {
  id: string;
  note_path: string;
  title: string;
  x: number;
  y: number;
  width: number | null;
  height: number | null;
  kind: string;
  source: string;
  text: string;
}

export interface CanvasEdge {
  id: string;
  source: string;
  target: string;
  label: string;
}

export interface CanvasGroup {
  id: string;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
  members: string[];
}

export interface CanvasDef {
  id: string;
  name: string;
  nodes: CanvasNode[];
  edges: CanvasEdge[];
  groups: CanvasGroup[];
  pan_x: number;
  pan_y: number;
  zoom: number;
}

// ── Statistics (commands.rs) ────────────────────────────────────────────────

export interface TagStat {
  tag: string;
  count: number;
}

export interface GrowthPoint {
  date: string;
  count: number;
}

export interface RecentNoteStat {
  path: string;
  title: string;
  folder: string;
  modified_at: string;
  created_at: string | null;
  size: number;
}

export interface VaultStatistics {
  note_count: number;
  folder_count: number;
  tag_count: number;
  total_tags: number;
  graph_nodes: number;
  graph_edges: number;
  graph_orphans: number;
  graph_clusters: number;
  tags: TagStat[];
  recently_created: RecentNoteStat[];
  recently_modified: RecentNoteStat[];
  growth: GrowthPoint[];
  storage_bytes: number;
  writing_streak_days: number;
  active_days_last_30: number;
}

// ── Version History / Recovery (recovery.rs) ────────────────────────────────

export interface VersionMeta {
  id: string;
  created_at: string;
  size: number;
  char_count: number;
  summary: string | null;
  manual: boolean;
  author: string | null;
}

export interface VersionManifest {
  path: string;
  versions: VersionMeta[];
}

export type DiffKind = "same" | "added" | "removed";

export interface DiffRow {
  kind: DiffKind;
  old_line: number | null;
  new_line: number | null;
  text: string;
}

export interface SessionState {
  version: number;
  saved_at: string | null;
  view_mode: string | null;
  active_note: string | null;
  open_tabs: string[];
  split_panes: string[];
  cursor_pos: number | null;
  scroll_top: number | null;
  left_sidebar: boolean | null;
  right_inspector: boolean | null;
  window_layout: string | null;
}

export interface RecoveryStatus {
  crashed: boolean;
  has_session: boolean;
  session: SessionState | null;
}

// ── Snapshot Browser (commands.rs) ──────────────────────────────────────────

export interface NoteSummary {
  path: string;
  version_count: number;
  last_snapshot_at: string | null;
}

// ── History / Undo-Redo (history.rs) ────────────────────────────────────────

export interface HistoryStatus {
  can_undo: boolean;
  can_redo: boolean;
  undo_label: string | null;
  redo_label: string | null;
  undo_len: number;
  redo_len: number;
  max_depth: number;
}

// ── Trash (history.rs) ─────────────────────────────────────────────────────

export interface TrashRecord {
  trash_path: string;
  original_path: string;
  deleted_at: string | null;
  is_folder: boolean;
  /** Number of files this item represents (1 for a plain file; recursive for folders). */
  file_count: number;
  /** A short text preview captured when the item was trashed (text files only). */
  preview: string | null;
}

// ── Capabilities (commands.rs) ──────────────────────────────────────────────

export interface Capability {
  id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  permissions: string[];
  config_schema: Record<string, unknown> | null;
}

export interface CapabilitySummaryWithState {
  /** Flattened from Capability via #[serde(flatten)] */
  id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  permissions: string[];
  config_schema: Record<string, unknown> | null;
  enabled: boolean;
  provider: string;
}

// ── Diagnostics (commands.rs) ───────────────────────────────────────────────

export interface DiagnosticRequest {
  text: string;
  resource_id: string;
  origin: string | null;
}

export interface DiagnosticResponse {
  batch: DiagnosticBatch;
  style_map: DiagnosticStyleMap;
}

export interface DiagnosticBatch {
  origin: string;
  resource_id: string;
  diagnostics: DiagnosticItem[];
}

export interface DiagnosticItem {
  range: { start: number; end: number };
  message: string;
  severity: string;
  code: string | null;
  source: string | null;
  suggestions: DiagnosticSuggestion[];
}

export interface DiagnosticSuggestion {
  title: string;
  range: { start: number; end: number };
  replacement: string;
}

export interface DiagnosticStyleMap {
  /** severity → style mapping */
  [severity: string]: Record<string, unknown>;
}

// ── Plugin (commands.rs) ────────────────────────────────────────────────────

export interface PluginInvocationRequest {
  plugin_id: string;
  capability: string;
  method: string;
  input: unknown;
  metadata: Record<string, unknown> | null;
}

export interface PluginInvocationResponse {
  success: boolean;
  output: unknown;
  error: PluginError | null;
  execution: PluginExecution | null;
}

export interface PluginError {
  code: string;
  message: string;
}

export interface PluginExecution {
  provider: string | null;
  duration_ms: number | null;
}

// ── Threads / Conversation (nabu-core) ─────────────────────────────────────

export interface Thread {
  id: string;
  title: string | null;
  created_at: string;
  updated_at: string | null;
  participants: Participant[];
  messages: Message[];
}

export interface Participant {
  name: string;
  role: string;
  kind: string;
}

export interface Message {
  id: string;
  role: string;
  content: string;
  timestamp: string;
}

// ── ACP (commands.rs) ───────────────────────────────────────────────────────

export interface AcpConnectResult {
  thread_id: string;
  session_id: string;
  protocol_version: ProtocolVersion;
}

export interface ProtocolVersion {
  major: number;
  minor: number;
  patch: number;
}

export interface AcpSessionSummary {
  session_id: string;
  thread_id: string;
  agent_name: string | null;
}

// ── Navigation State (view mode) ────────────────────────────────────────────

export type ViewMode =
  | "Dashboard"
  | "Editor"
  | "Graph"
  | "Inbox"
  | "ReadingQueue"
  | "Templates"
  | "Settings"
  | "Trash"
  | "History"
  | "Recovery"
  | "Search"
  | "Calendar"
  | "Archive"
  | "SmartFolders"
  | "Canvas"
  | "Reader"
  | "Comparison"
  | "Statistics"
  | "Activity"
  | "Streaming"
  | "Chat";

// ── Load State (recovery/mod.rs) ─────────────────────────────────────────────

/**
 * Canonical load-phase enum mirroring `LoadState` in
 * `ui-react/src/components/recovery/mod.rs`.
 *
 * Distinguishes "still loading", "loaded successfully" (which may be empty),
 * and "failed to load" so a blank panel is never confused with an empty one
 * or an error.
 */
export type LoadState = "idle" | "loading" | "loaded" | "failed";

/**
 * Returns `true` when a received result nonce does not match the current
 * nonce (i.e. the async load that produced `received` has been superseded).
 *
 * Mirrors `nonce_is_stale` in `ui-react/src/components/shipped/mod.rs`.
 */
export function nonceIsStale(current: number, received: number): boolean {
  return current !== received;
}

export interface SavedSearch {
  name: string;
  query: string;
}

// ── Health / Metrics (nabu-core registry) ───────────────────────────────────

export interface ServiceHealth {
  status_label: string;
  lifecycle_stage: string | null;
  registered_services: number;
}

export interface RuntimeMetrics {
  timers: MetricTimer[];
  counters: MetricCounter[];
  gauges: MetricGauge[];
  service_count: number;
  error_count: number;
}

export interface MetricTimer {
  name: string;
  count: number;
  min_ms: number;
  max_ms: number;
  avg_ms: number;
  /** 50th percentile latency in ms (mirrors backend MetricTimer.p50_ms). */
  p50_ms: number;
  /** 90th percentile latency in ms (mirrors backend MetricTimer.p90_ms). */
  p90_ms: number;
}

export interface MetricCounter {
  name: string;
  value: number;
}

export interface MetricGauge {
  name: string;
  value: number;
}

export interface PoolHealth {
  worker_count: number;
  active_workers: number;
  pending_jobs: number;
  running_jobs: number;
  is_throttled: boolean;
  is_full: boolean;
  lifecycle_stage: string | null;
  /** Mirrors PoolHealthSnapshot.shutting_down in statistics.rs. */
  shutting_down: boolean;
}

// ── Activity Timeline (activity/mod.rs) ────────────────────────────────────

/** Severity level for an activity item. */
export type ActivitySeverity = "info" | "warning" | "error";

/** Broad category for an activity item. */
export type ActivityCategory =
  | "capture"
  | "processing"
  | "index"
  | "storage"
  | "capability"
  | "plugin"
  | "sync"
  | "agent"
  | "process"
  | "conversation"
  | "stream"
  | "lifecycle"
  | "other";

/** A single entry in the activity timeline. */
export interface ActivityItem {
  id: string;
  title: string;
  description: string | null;
  severity: ActivitySeverity;
  category: ActivityCategory;
  subsystem: string;
  event_kind: string;
  timestamp_ms: number;
  metadata: Record<string, unknown>;
}

// ── Property Editor (nabu-core::models::properties) ─────────────────────────

export type PropertyType =
  | "text"
  | "number"
  | "date"
  | "select"
  | "multi-select"
  | "url";

export type PropertyValue =
  | { type: "text"; value: string }
  | { type: "number"; value: number }
  | { type: "date"; value: string }
  | { type: "select"; value: string }
  | { type: "multi-select"; value: string[] }
  | { type: "url"; value: string };

export interface PropertyDefinition {
  id: string;
  display_name: string;
  property_type: PropertyType;
  description?: string | null;
  default_value?: PropertyValue | null;
  options?: string[] | null;
}

export type ValidationState = "valid" | { invalid: string };

// ── Knowledge Object (nabu-core::models::knowledge_object) ──────────────────

export type ObjectType =
  | "note"
  | "document"
  | "image"
  | "audio"
  | "video"
  | "pdf"
  | "web-page"
  | "spreadsheet"
  | "archive"
  | "unknown";

export interface KnowledgeObject {
  id: string;
  object_type: string;
  content: unknown;
  metadata: Record<string, unknown>;
  custom_properties: Record<string, PropertyValue>;
  tags: string[];
  relations: unknown[];
}

// ── Relation Editor (nabu-core::models::graph) ──────────────────────────────

export type RelationType =
  | "references"
  | "referenced-by"
  | "parent"
  | "child"
  | "attached"
  | "related"
  | { custom: string };

export interface GraphEdge {
  source: string;
  target: string;
  relationship: string;
  weight: number;
  content_derived: boolean;
}

export interface Relation {
  id: string;
  source: string;
  target: string;
  relation_type: string;
}

// ── Streaming (ui-react/src/components/streaming) ─────────────────────

export type StreamId = string;

export type StreamLifeCycle =
  | "created"
  | "active"
  | "completed"
  | "cancelled"
  | "failed";

export interface StreamSession {
  stream_id: StreamId;
  thread_id: string | null;
  agent_name: string | null;
  state: StreamLifeCycle;
  content: string;
  token_count: number;
  total_tokens: number | null;
  error: string | null;
  cancel_reason: string | null;
  last_event: string | null;
  metadata: Record<string, unknown>;
}

export interface StreamingContextValue {
  sessions: Map<StreamId, StreamSession>;
  cancelStream: (streamId: StreamId, reason: string) => Promise<void>;
  pruneTerminal: () => void;
  clearTerminal: () => void;
  clear: () => void;
  len: number;
  is_empty: boolean;
  active_count: number;
  has_active: boolean;
}

// ── ACP Permission (nabu-core::event_bus::events) ───────────────────────────

export type PermissionOutcome =
  | { selected: { option_id: string } }
  | { cancelled: boolean };

export interface PermissionOption {
  option_id: string;
  name: string;
  kind: "allow_once" | "allow_always" | "reject_once" | "reject_always";
  _meta?: unknown | null;
}

export interface AcpPermissionRequestEvent {
  request_id: string;
  thread_id: string;
  session_id: string;
  tool_call_id: string;
  tool_call_title: string | null;
  options: PermissionOption[];
  timestamp: string;
}
