use nabu_core::conversations::ConversationStore;
use nabu_core::models::conversation::Thread;
use nabu_core::models::{CustomPropertyValue, KnowledgeObject};
use nabu_core::registry::context::ApplicationContext;
use nabu_core::storage::StorageManager;
use nabu_core::streaming::StreamManager;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::settings::{AppSettings, SettingsStore};

// ── Capability Management Types ─────────────────────────────────────

/// Backend-side DTO that pairs a `Capability` with its runtime state.
///
/// Returned by the `capability_list_with_state` IPC command so the frontend
/// can render enable/disable toggles with the correct initial state without
/// issuing separate per-capability probes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilitySummaryWithState {
    #[serde(flatten)]
    pub capability: nabu_core::plugin::capability::Capability,
    pub enabled: bool,
    pub provider: String,
}

// ── Security Utilities ──────────────────────────────────────

/// Validates that a file path is within the vault directory.
/// Prevents path traversal attacks by ensuring the resolved path
/// is a descendant of the vault path.
pub(crate) fn validate_path_within_vault(
    vault_path: &Path,
    user_path: &str,
) -> Result<PathBuf, String> {
    let resolved = vault_path.join(user_path);
    let canonical = resolved
        .canonicalize()
        .map_err(|e| format!("Invalid path: {}", e))?;
    let canonical_vault = vault_path
        .canonicalize()
        .map_err(|e| format!("Invalid vault path: {}", e))?;

    if !canonical.starts_with(&canonical_vault) {
        return Err(format!(
            "Path traversal detected: {} is outside vault directory {}",
            user_path,
            canonical_vault.display()
        ));
    }

    Ok(canonical)
}

/// Validates that a file path is within the vault directory, without
/// requiring the target to exist on disk. Used by commands that create
/// new files (note_create_file, daily_note_for).
pub(crate) fn validate_path_within_vault_unchecked(
    vault_path: &Path,
    user_path: &str,
) -> Result<PathBuf, String> {
    let resolved = vault_path.join(user_path);

    for component in resolved.components() {
        use std::path::Component;
        if matches!(component, Component::ParentDir) {
            return Err(format!(
                "Path traversal detected: {} is outside vault directory {}",
                user_path,
                vault_path.display()
            ));
        }
    }

    Ok(resolved)
}

/// Validates that a string input does not contain dangerous characters
/// or patterns that could be used for injection attacks.
fn validate_input_safe(input: &str, max_length: usize) -> Result<(), String> {
    if input.len() > max_length {
        return Err(format!(
            "Input exceeds maximum length of {} characters",
            max_length
        ));
    }

    // Check for null bytes
    if input.contains('\0') {
        return Err("Input contains null bytes".to_string());
    }

    // Check for path traversal patterns
    if input.contains("..") {
        return Err("Input contains path traversal pattern '..'".to_string());
    }

    // Check for null byte injection
    if input.contains('%') && input.contains('0') {
        return Err("Input contains potential URL encoding injection".to_string());
    }

    Ok(())
}

// ── Queue Types ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Unread,
    Reading,
    Completed,
    Archived,
}

impl Default for QueueStatus {
    fn default() -> Self {
        Self::Unread
    }
}

impl QueueStatus {
    fn label(self) -> &'static str {
        match self {
            QueueStatus::Unread => "unread",
            QueueStatus::Reading => "reading",
            QueueStatus::Completed => "completed",
            QueueStatus::Archived => "archived",
        }
    }

    fn from_label(label: &str) -> Self {
        match label {
            "reading" => QueueStatus::Reading,
            "completed" => QueueStatus::Completed,
            "archived" => QueueStatus::Archived,
            _ => QueueStatus::Unread,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuePriority {
    Low,
    Normal,
    High,
}

impl Default for QueuePriority {
    fn default() -> Self {
        Self::Normal
    }
}

impl QueuePriority {
    fn label(self) -> &'static str {
        match self {
            QueuePriority::Low => "low",
            QueuePriority::Normal => "normal",
            QueuePriority::High => "high",
        }
    }

    fn from_label(label: &str) -> Self {
        match label {
            "low" => QueuePriority::Low,
            "high" => QueuePriority::High,
            _ => QueuePriority::Normal,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub object_type: String,
    pub status: QueueStatus,
    pub priority: QueuePriority,
    pub progress: f32,
    pub source: String,
    pub modified_at: String,
    pub tags: Vec<String>,
    pub selected: bool,
}

// ── Inbox Types ──────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxStatus {
    Pending,
    Processing,
    Ready,
    Approved,
    Rejected,
    Failed,
}

impl Default for InboxStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: String,
    pub title: String,
    pub object_type: String,
    pub source: String,
    pub status: InboxStatus,
    pub mime_type: Option<String>,
    pub source_file: Option<String>,
    /// Icon name (Lucide) suitable for use as a thumbnail in list/grid views.
    /// Derived from the object type and MIME type when the object carries no
    /// inline binary preview.
    pub thumbnail: Option<String>,
    /// Confidence score (0.0–1.0) from the ContentClassifier, if classified.
    pub confidence: Option<f64>,
    /// Suggested vault folder from the AutoFiler, if a classification matches.
    pub suggested_folder: Option<String>,
    pub metadata: InboxMetadata,
    pub duplicate_info: Option<DuplicateInfo>,
    pub timeline_info: Option<TimelineInfo>,
    pub ocr_info: Option<OcrInfo>,
    pub processing_history: Vec<ProcessingHistoryEntry>,
    pub warnings: Vec<String>,
    pub selected: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct InboxMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub source_url: Option<String>,
    pub tags: Vec<String>,
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DuplicateInfo {
    pub confidence: String,
    pub candidate_ids: Vec<String>,
    pub reason: Option<String>,
    pub duplicate_source: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct TimelineInfo {
    pub document_date: Option<String>,
    pub created_date: Option<String>,
    pub modified_date: Option<String>,
    pub detected_event_date: Option<String>,
    pub extraction_confidence: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct OcrInfo {
    pub extracted_text: Option<String>,
    pub confidence: Option<f64>,
    pub recognition_language: Option<String>,
    pub page_count: Option<u32>,
    pub processing_duration_ms: Option<u64>,
    pub is_scanned: Option<bool>,
    pub warning: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ProcessingHistoryEntry {
    pub processor_name: String,
    pub timestamp: String,
    pub duration_ms: u64,
    pub success: bool,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

// ── Canonical storage access ────────────────────────────────────────

/// Resolves the single canonical StorageManager from the ApplicationContext.
///
/// R1: nothing constructs its own StorageManager / EventBus — every command
/// resolves the one registered at startup through dependency injection.
fn get_storage_manager(ctx: &ApplicationContext) -> Result<Arc<StorageManager>, String> {
    ctx.storage_manager()
        .ok_or_else(|| "StorageManager is not registered in the application context".to_string())
}

/// Reads a plain-text custom property from the canonical model.
fn custom_text(obj: &KnowledgeObject, key: &str) -> Option<String> {
    match obj.custom_properties.get(key) {
        Some(CustomPropertyValue::Text(s))
        | Some(CustomPropertyValue::Select(s))
        | Some(CustomPropertyValue::Url(s))
        | Some(CustomPropertyValue::Date(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Reads a JSON-encoded custom property (stored as a serialized Text value).
fn custom_json(obj: &KnowledgeObject, key: &str) -> Option<serde_json::Value> {
    custom_text(obj, key).and_then(|s| serde_json::from_str(&s).ok())
}

/// Writes an arbitrary JSON value into a custom property as serialized Text.
fn set_custom_json(obj: &mut KnowledgeObject, key: &str, value: &serde_json::Value) {
    obj.custom_properties.insert(
        key.to_string(),
        CustomPropertyValue::Text(serde_json::to_string(value).unwrap_or_default()),
    );
}

/// Sets a plain text custom property.
fn set_custom_text(obj: &mut KnowledgeObject, key: &str, value: &str) {
    obj.custom_properties.insert(
        key.to_string(),
        CustomPropertyValue::Text(value.to_string()),
    );
}

/// Reads a numeric custom property.
fn custom_number(obj: &KnowledgeObject, key: &str) -> Option<f64> {
    match obj.custom_properties.get(key) {
        Some(CustomPropertyValue::Number(n)) => Some(*n),
        _ => None,
    }
}

// ── File Tree Commands ────────────────────────────────────────────

/// One entry of the vault file tree returned to the frontend. `path` is
/// vault-relative (forward slashes) so the frontend can round-trip it to the
/// other file commands; hidden entries (`.nabu`, dotfiles) are skipped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub is_folder: bool,
    pub children: Vec<TreeEntry>,
}

/// Recursively scans `dir`, producing vault-relative [`TreeEntry`]s.
/// Hidden entries (leading `.`) are skipped so metadata dirs like `.nabu`
/// never appear in the user-facing tree.
fn scan_tree(dir: &Path, prefix: &str) -> Vec<TreeEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut items: Vec<TreeEntry> = entries
        .flatten()
        .filter(|e| {
            let file_name = e.file_name();
            let name_lossy = file_name.to_string_lossy();
            // The reserved `archive/` folder is hidden from normal navigation.
            !name_lossy.starts_with('.') && !(prefix.is_empty() && name_lossy == ARCHIVE_FOLDER)
        })
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let is_folder = e.file_type().map(|t| t.is_dir()).unwrap_or_else(|_| {
                std::fs::metadata(e.path()).map(|m| m.is_dir()).unwrap_or(false)
            });
            let children = if is_folder {
                scan_tree(&e.path(), &path)
            } else {
                Vec::new()
            };
            TreeEntry {
                name,
                path,
                is_folder,
                children,
            }
        })
        .collect();
    // Folders first, then notes, each alphabetically — matches Finder/Explorer.
    items.sort_by(|a, b| {
        b.is_folder
            .cmp(&a.is_folder)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    items
}

/// Returns the vault's file tree (vault-relative paths). The left sidebar
/// renders this and drives drag-and-drop, context menus and inline rename.
#[tauri::command]
pub fn tree_list(store: State<'_, SettingsStore>) -> Result<Vec<TreeEntry>, String> {
    tree_list_impl(&store)
}

pub(crate) fn tree_list_impl(store: &SettingsStore) -> Result<Vec<TreeEntry>, String> {
    let settings = store.get();
    let vault_path = PathBuf::from(&settings.last_vault_path);
    if vault_path.as_os_str().is_empty() || !vault_path.exists() {
        return Ok(Vec::new());
    }
    Ok(scan_tree(&vault_path, ""))
}

/// Reveals a vault-relative path in the operating system's file manager
/// (Finder / Explorer / the default file manager).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn reveal_in_file_manager(
    path: String,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    let settings = store.get();
    let vault_path = PathBuf::from(&settings.last_vault_path);
    let full = validate_path_within_vault(&vault_path, &path)?;
    std::process::Command::new("open")
        .arg("-R")
        .arg(&full)
        .status()
        .map_err(|e| format!("Could not reveal in Finder: {e}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn reveal_in_file_manager(
    path: String,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    let settings = store.get();
    let vault_path = PathBuf::from(&settings.last_vault_path);
    let full = validate_path_within_vault(&vault_path, &path)?;
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(&full)
        .spawn()
        .map_err(|e| format!("Could not reveal in Explorer: {e}"))?;
    Ok(())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
#[tauri::command]
pub fn reveal_in_file_manager(
    path: String,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    use std::path::Path as P;
    let settings = store.get();
    let vault_path = PathBuf::from(&settings.last_vault_path);
    let full = validate_path_within_vault(&vault_path, &path)?;
    // `xdg-open` opens the parent directory for folders; for files, open the
    // containing directory (there is no portable "select" flag).
    let target = if full.is_dir() {
        full
    } else {
        full.parent().map(P::to_path_buf).unwrap_or(vault_path)
    };
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map_err(|e| format!("Could not open file manager: {e}"))?;
    Ok(())
}

// ── Vault Commands ─────────────────────────────────────────────

#[tauri::command]
pub fn check_vault_exists(store: State<'_, SettingsStore>) -> Result<Option<String>, String> {
    check_vault_exists_impl(&store)
}

pub(crate) fn check_vault_exists_impl(store: &SettingsStore) -> Result<Option<String>, String> {
    eprintln!("[IPC] check_vault_exists invoked");
    let settings = store.get();
    let path = settings.last_vault_path.trim();
    // A path is only a usable vault if it exists AND is a real Nabu vault
    // (contains a `.nabu` directory). This prevents the app from silently
    // treating arbitrary directories — e.g. the user's Desktop — as the vault
    // and materialising `.nabu/` there without the setup wizard. If it isn't a
    // real vault, we report no vault so the wizard launches first.
    if !path.is_empty()
        && std::path::Path::new(path).is_dir()
        && std::path::Path::new(path).join(".nabu").is_dir()
    {
        let _ = crate::history::trash_purge_expired_impl(store);
        eprintln!("[IPC] check_vault_exists returning Ok(Some({}))", path);
        Ok(Some(path.to_string()))
    } else {
        eprintln!("[IPC] check_vault_exists returning Ok(None)");
        Ok(None)
    }
}

#[tauri::command]
pub fn diag_report(state: String) {
    eprintln!("[DIAG] {}", state);
    tracing::info!(target: "nabu_frontend", "[DIAG] {}", state);
}

#[tauri::command]
pub fn get_current_vault(store: State<'_, SettingsStore>) -> Result<Option<String>, String> {
    check_vault_exists(store)
}

#[tauri::command]
pub async fn select_vault_dialog(
    store: State<'_, SettingsStore>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    // macOS NSOpenPanel must be presented from the main thread, otherwise it can
    // fail to appear and leave the wizard stuck on its "Opening system dialog…" spinner.
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    app.run_on_main_thread(move || {
        let folder = rfd::FileDialog::new()
            .set_title("Select Vault Directory")
            .pick_folder();
        let _ = tx.send(folder);
    })
    .map_err(|e| e.to_string())?;
    let folder = rx.recv().map_err(|e| e.to_string())?;

    if let Some(path) = folder {
        let path_str = path.display().to_string();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        store
            .update(|s| {
                s.last_vault_path = path_str.clone();
                crate::settings::update_recent_vaults(s, path_str.clone(), name);
            })
            .map_err(|e| e.to_string())?;
        // Retention runs as soon as a vault becomes active, so expired trashed
        // items are purged even if the user never opens the Trash screen.
        let _ = crate::history::trash_purge_expired(store.clone());
        Ok(Some(path_str))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn create_vault_dialog(
    store: State<'_, SettingsStore>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    app.run_on_main_thread(move || {
        let folder = rfd::FileDialog::new()
            .set_title("Select Directory for New Vault")
            .pick_folder();
        let _ = tx.send(folder);
    })
    .map_err(|e| e.to_string())?;
    let folder = rx.recv().map_err(|e| e.to_string())?;

    if let Some(path) = folder {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        let path_str = path.display().to_string();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        store
            .update(|s| {
                s.last_vault_path = path_str.clone();
                crate::settings::update_recent_vaults(s, path_str.clone(), name);
            })
            .map_err(|e| e.to_string())?;
        let _ = crate::history::trash_purge_expired(store.clone());
        Ok(Some(path_str))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn open_dictation_pill(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("dictation-pill") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        tauri::WebviewWindowBuilder::new(
            &app,
            "dictation-pill",
            tauri::WebviewUrl::App("dictation-pill.html".into()),
        )
        .title("Dictation Pill")
        .inner_size(260.0, 64.0)
        .resizable(false)
        .decorations(false)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn close_dictation_pill(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("dictation-pill") {
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_dictation_pill(app: AppHandle) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("dictation-pill") {
        let is_visible = window.is_visible().unwrap_or(false);
        if is_visible {
            let _ = window.hide();
            Ok(false)
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            Ok(true)
        }
    } else {
        open_dictation_pill(app)?;
        Ok(true)
    }
}

#[tauri::command]
pub fn start_dictation(
    dictation: State<'_, crate::dictation::DictationService>,
) -> Result<String, String> {
    dictation
        .start()
        .map_err(|e| e.to_string())?;
    Ok(dictation.status().to_string())
}

#[tauri::command]
pub async fn stop_dictation(
    dictation: State<'_, crate::dictation::DictationService>,
) -> Result<String, String> {
    dictation.stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_setup(app: AppHandle) -> Result<(), String> {
    // The canonical application context is normally constructed during
    // `setup` — but only when a vault was already configured. On first launch
    // (no vault chosen yet) setup defers to here: this command runs after the
    // wizard's `select_vault_dialog` / `create_vault_dialog` has persisted
    // `last_vault_path`, so we can now materialise the full service graph
    // against the user's chosen vault directory.
    if app.try_state::<ApplicationContext>().is_some() {
        if let Some(main_window) = app.get_webview_window("main") {
            let _ = main_window.show();
            let _ = main_window.set_focus();
        }
        return Ok(());
    }

    let vault_path = {
        let settings = app.state::<crate::settings::SettingsStore>().get();
        let path = settings.last_vault_path.trim().to_string();
        if path.is_empty() || !std::path::Path::new(&path).exists() {
            return Err("No vault path has been configured".to_string());
        }
        PathBuf::from(path)
    };

    crate::recovery::mark_running(&vault_path);

    let app_handle = app.clone();
    let ctx = tauri::async_runtime::block_on(async move {
        crate::build_application_context(vault_path, app_handle)
    })?;
    app.manage(ctx);

    if let Some(main_window) = app.get_webview_window("main") {
        let _ = main_window.show();
        let _ = main_window.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(settings_window) = app.get_webview_window("settings") {
        let _ = settings_window.show();
    }
    Ok(())
}

#[tauri::command]
pub fn note_create_file(
    path: String,
    content: Option<String>,
    store: State<'_, SettingsStore>,
    ctx: State<'_, ApplicationContext>,
) -> Result<(), String> {
    note_create_file_impl(&ctx, &store, &path, content.as_deref().unwrap_or_default())
}

pub(crate) fn note_create_file_impl(
    ctx: &ApplicationContext,
    store: &SettingsStore,
    path: &str,
    content: &str,
) -> Result<(), String> {
    let settings = store.get();
    let vault_path = PathBuf::from(&settings.last_vault_path);

    // Validate path is within vault
    let safe_path = validate_path_within_vault_unchecked(&vault_path, path)?;

    // Validate input safety
    validate_input_safe(path, 500)?;
    validate_input_safe(content, 1_000_000)?; // 1MB max content

    // Phase 11.3: if this write overwrites an existing note (import / bulk
    // edit), snapshot the previous content first so it is never lost.
    let _ = crate::recovery::snapshot_note(&vault_path, path);

    // Route through the canonical StorageManager -- the single persistence
    // gateway. This publishes ITEM_STORED, which drives the Indexer and
    // VaultGraph subscribers downstream via the Capability Platform.
    let manager = get_storage_manager(ctx)?;
    manager
        .save_note_content(path, content)
        .map_err(|e| e.to_string())?;

    // Register an undoable history entry so creation can be reversed.
    let undo_path = safe_path.clone();
    let redo_path = safe_path.clone();
    let redo_content = content.to_string();
    crate::history::push_history(
        ctx,
        nabu_core::history::HistoryOp::NoteCreate,
        format!("Create Note '{}'", path),
        vec![path.to_string()],
        serde_json::json!({ "path": path, "exists": false }),
        serde_json::json!({ "path": path, "exists": true }),
        std::sync::Arc::new(move || {
            if undo_path.exists() {
                std::fs::remove_file(&undo_path).map_err(|e| e.to_string())?;
            }
            Ok(())
        }),
        std::sync::Arc::new(move || {
            if let Some(parent) = redo_path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            std::fs::write(&redo_path, &redo_content).map_err(|e| e.to_string())?;
            Ok(())
        }),
    )?;
    Ok(())
}

#[tauri::command]
pub fn note_daily() -> Result<String, String> {
    let date_name = chrono::Local::now().format("%Y-%m-%d").to_string();
    Ok(format!("{}.md", date_name))
}

#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Result<AppSettings, String> {
    get_settings_impl(&store)
}

pub(crate) fn get_settings_impl(store: &SettingsStore) -> Result<AppSettings, String> {
    Ok(store.get())
}

#[tauri::command]
pub fn settings_set(
    key: String,
    value: serde_json::Value,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    settings_set_impl(&store, &key, value)
}

pub(crate) fn settings_set_impl(
    store: &SettingsStore,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    store
        .update(|s| {
            s.extra_settings.insert(key.to_string(), value);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn settings_get(
    key: String,
    store: State<'_, SettingsStore>,
) -> Result<serde_json::Value, String> {
    settings_get_impl(&store, &key)
}

pub(crate) fn settings_get_impl(store: &SettingsStore, key: &str) -> Result<serde_json::Value, String> {
    Ok(store.get_value(key))
}

#[tauri::command]
pub fn settings_set_all(
    settings: AppSettings,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    settings_set_all_impl(&store, &settings)
}

pub(crate) fn settings_set_all_impl(store: &SettingsStore, settings: &AppSettings) -> Result<(), String> {
    store.save(settings).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Settings Import / Export ─────────────────────────────────────────

#[tauri::command]
pub fn settings_export(store: State<'_, SettingsStore>) -> Result<Vec<u8>, String> {
    settings_export_impl(&store)
}

pub(crate) fn settings_export_impl(store: &SettingsStore) -> Result<Vec<u8>, String> {
    let export = store
        .export_settings()
        .map_err(|e| e.to_string())?;
    serde_json::to_vec(&export).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_import(
    payload: Vec<u8>,
    store: State<'_, SettingsStore>,
) -> Result<AppSettings, String> {
    settings_import_impl(&store, &payload)
}

pub(crate) fn settings_import_impl(
    store: &SettingsStore,
    payload: &[u8],
) -> Result<AppSettings, String> {
    store
        .import_settings(payload)
        .map(|s| s.clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_reset(store: State<'_, SettingsStore>) -> Result<AppSettings, String> {
    settings_reset_impl(&store)
}

pub(crate) fn settings_reset_impl(store: &SettingsStore) -> Result<AppSettings, String> {
    store.reset().map(|s| s.clone()).map_err(|e| e.to_string())
}

// ── Platform Integration Commands ───────────────────────────────────

/// macOS: Open the app's location in Finder.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn open_app_in_finder() -> Result<(), String> {
    let app_path = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new("open")
        .arg("-R")
        .arg(&app_path)
        .status()
        .map_err(|e| format!("Failed to open Finder: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn open_app_in_finder() -> Result<(), String> {
    Err("open_app_in_finder is only available on macOS".to_string())
}

/// macOS: Show a notification using the native `terminal-notifier` if available.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn show_macos_notification(title: String, body: String) -> Result<(), String> {
    let _ = std::process::Command::new("terminal-notifier")
        .arg("-title")
        .arg(&title)
        .arg("-message")
        .arg(&body)
        .arg("-sound")
        .arg("Glass")
        .status();
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn show_macos_notification(_title: String, _body: String) -> Result<(), String> {
    Err("show_macos_notification is only available on macOS".to_string())
}

/// Windows: Pin app to taskbar (Jump List).
#[cfg(target_os = "windows")]
#[tauri::command]
pub fn pin_to_taskbar() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dirs = known_folder::get_shell_dirs();
    let dest = dirs
        .taskbar_pins
        .join(format!("{}.lnk", env!("CARGO_PKG_NAME")));
    let _ = std::os::windows::fs::symlink_file(&exe, &dest);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn pin_to_taskbar() -> Result<(), String> {
    Err("pin_to_taskbar is only available on Windows".to_string())
}

/// Windows: Open the app's location in Explorer.
#[cfg(target_os = "windows")]
#[tauri::command]
pub fn open_in_explorer() -> Result<(), String> {
    let app_path = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(&app_path)
        .spawn()
        .map_err(|e| format!("Failed to open Explorer: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn open_in_explorer() -> Result<(), String> {
    Err("open_in_explorer is only available on Windows".to_string())
}

/// Linux: Open the app's location in the default file manager.
#[tauri::command]
pub fn open_in_file_manager() -> Result<(), String> {
    let app_path = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new("xdg-open")
        .arg(&app_path)
        .status()
        .map_err(|e| format!("Failed to open file manager: {}", e))?;
    Ok(())
}

/// Linux: Show a desktop notification using `notify-send` or a portal.
#[tauri::command]
pub fn show_linux_notification(title: String, body: String) -> Result<(), String> {
    let _ = std::process::Command::new("notify-send")
        .arg(&title)
        .arg(&body)
        .status();
    Ok(())
}

/// Linux: Install a desktop entry for the app (portal integration).
#[tauri::command]
pub fn install_desktop_entry() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let home = std::env::var("HOME").map_err(|e| e.to_string())?;
        let apps_dir = std::path::PathBuf::from(home).join(".local").join("share").join("applications");
        std::fs::create_dir_all(&apps_dir).map_err(|e| e.to_string())?;
        let desktop = apps_dir.join(format!("{}.desktop", env!("CARGO_PKG_NAME")));
        let content = format!(
            r#"[Desktop Entry]
Type=Application
Name=Nabu
Exec={} %U
Icon=nabu
Terminal=false
Categories=Office;Utility;
MimeType=text/markdown;text/plain;
"#,
            exe.display()
        );
        std::fs::write(&desktop, content).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err("install_desktop_entry is only available on Linux".to_string())
}

/// Generic: reveal vault path in the OS file manager.
#[tauri::command]
pub fn reveal_vault_in_file_manager(store: State<'_, SettingsStore>) -> Result<(), String> {
    let settings = store.get();
    let vault_path = std::path::PathBuf::from(&settings.last_vault_path);
    if !vault_path.exists() {
        return Err("Vault path does not exist".to_string());
    }
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("-R")
        .arg(&vault_path)
        .status()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&vault_path)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    std::process::Command::new("xdg-open")
        .arg(&vault_path)
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Inbox Commands ──────────────────────────────────────────────────

/// Helper: convert a canonical KnowledgeObject to the frontend InboxItem.
fn knowledge_object_to_inbox_item(obj: &KnowledgeObject) -> InboxItem {
    let inbox_status_str =
        custom_text(obj, "inbox_status").unwrap_or_else(|| "pending".to_string());
    let status = match inbox_status_str.as_str() {
        "processing" => InboxStatus::Processing,
        "ready" => InboxStatus::Ready,
        "approved" => InboxStatus::Approved,
        "rejected" => InboxStatus::Rejected,
        "failed" => InboxStatus::Failed,
        _ => InboxStatus::Pending,
    };

    let duplicate_info = custom_json(obj, "duplicate_info")
        .and_then(|v| serde_json::from_value::<DuplicateInfo>(v).ok());

    let timeline_info = custom_json(obj, "timeline_info")
        .and_then(|v| serde_json::from_value::<TimelineInfo>(v).ok());

    let ocr_info =
        custom_json(obj, "ocr_info").and_then(|v| serde_json::from_value::<OcrInfo>(v).ok());

    let processing_history = custom_json(obj, "processing_history")
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value::<ProcessingHistoryEntry>(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    let warnings = custom_json(obj, "processing_warnings")
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let custom: std::collections::HashMap<String, serde_json::Value> = obj
        .custom_properties
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
        .collect();

    // Derive thumbnail icon from the object type + MIME type.
    let thumbnail = Some(thumbnail_for_object(&obj.object_type, obj.metadata.mime_type.as_deref()).to_string());

    // Confidence score (0.0–1.0) stored by the ContentClassifier.
    let confidence = custom_number(obj, "classification_confidence");

    // Suggested destination folder from the AutoFiler.
    let suggested_folder = custom_text(obj, "suggested_folder");

    InboxItem {
        id: obj.id.to_string(),
        title: obj.metadata.title.clone().unwrap_or_default(),
        object_type: obj.object_type.variant_name().to_string(),
        source: obj.metadata.source_url.clone().unwrap_or_default(),
        status,
        mime_type: obj.metadata.mime_type.clone(),
        source_file: obj.metadata.original_filename.clone(),
        thumbnail,
        confidence,
        suggested_folder,
        metadata: InboxMetadata {
            title: obj.metadata.title.clone(),
            author: obj.metadata.authors.first().cloned(),
            language: obj.metadata.language.clone(),
            source_url: obj.metadata.source_url.clone(),
            tags: obj.tags.clone(),
            custom,
        },
        duplicate_info,
        timeline_info,
        ocr_info,
        processing_history,
        warnings,
        selected: false,
    }
}

/// Maps an object type (+ optional MIME type) to a Lucide icon name suitable
/// for display as a 16×16 thumbnail in the inbox list.
fn thumbnail_for_object(object_type: &nabu_core::models::ObjectType, mime_type: Option<&str>) -> &'static str {
    use nabu_core::models::ObjectType;
    match object_type {
        ObjectType::Image | ObjectType::Screenshot | ObjectType::Scan => "image",
        ObjectType::AudioRecording => "music-3",
        ObjectType::VideoRecording | ObjectType::YouTubeVideo => "play",
        ObjectType::CodeSnippet | ObjectType::Repository => "code-block",
        ObjectType::Email => "mail",
        ObjectType::Bookmark => "bookmark",
        ObjectType::Article => "book-text",
        ObjectType::Document => {
            if let Some(mime) = mime_type {
                if mime.starts_with("application/pdf") {
                    return "file-text";
                }
            }
            "file-text"
        }
        ObjectType::Whiteboard => "pen-line",
        ObjectType::Contact => "user",
        ObjectType::Event => "calendar",
        ObjectType::Task => "list-checks",
        ObjectType::Project => "folder",
        ObjectType::Template => "file-pen",
        ObjectType::Note => "sticky-note",
        ObjectType::Collection => "folder-tree",
        ObjectType::Dashboard => "layout-dashboard",
        ObjectType::Attachment => {
            if let Some(mime) = mime_type {
                if mime.starts_with("image/") {
                    return "image";
                }
                if mime.starts_with("video/") {
                    return "play";
                }
                if mime.starts_with("audio/") {
                    return "music-3";
                }
                if mime.starts_with("text/") {
                    return "file-text";
                }
            }
            "file"
        }
    }
}

#[tauri::command]
pub fn inbox_subscribe(ctx: State<'_, ApplicationContext>) -> Result<Vec<InboxItem>, String> {
    inbox_get_queue(ctx)
}

#[tauri::command]
pub fn inbox_get_queue(ctx: State<'_, ApplicationContext>) -> Result<Vec<InboxItem>, String> {
    inbox_get_queue_impl(&ctx)
}

fn inbox_get_queue_impl(ctx: &ApplicationContext) -> Result<Vec<InboxItem>, String> {
    let manager = get_storage_manager(ctx)?;
    let objects = manager
        .list_objects("", None, 1000)
        .map_err(|e| e.to_string())?;

    let inbox_items: Vec<InboxItem> = objects
        .into_iter()
        .filter(|obj| {
            obj.custom_properties.contains_key("inbox_status")
                || obj.custom_properties.contains_key("suggested_folder")
                || obj.custom_properties.contains_key("classification")
        })
        .map(|obj| knowledge_object_to_inbox_item(&obj))
        .collect();

    Ok(inbox_items)
}

#[tauri::command]
pub fn inbox_approve(ctx: State<'_, ApplicationContext>, id: String) -> Result<(), String> {
    inbox_approve_impl(&ctx, &id)
}

/// Approves a single inbox item (extracted from the Tauri command so it can be
/// unit-tested against a real `&ApplicationContext`).
fn inbox_approve_impl(ctx: &ApplicationContext, id: &str) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Inbox item not found: {}", id))?;

    let previous = custom_text(&obj, "inbox_status").unwrap_or_else(|| "pending".to_string());

    // Approve == file the capture into a real vault artifact (a Markdown note
    // for text captures, a native binary file otherwise).  `FilingService::file_object`
    // stamps the vault path / content / hash / processing state and persists;
    // the resulting `ITEM_STORED` event is handed off to the indexer/graph by
    // the canonical pipeline subscriber wired up in `src/lib.rs`.
    let service = nabu_core::inbox::FilingService::new(manager.clone());
    service
        .file_object(&mut obj)
        .map_err(|e| e.to_string())?;

    // Undo flips back to the previous status; redo re-approves.
    let manager_undo = manager.clone();
    let manager_redo = manager.clone();
    crate::history::push_history(
        ctx,
        nabu_core::history::HistoryOp::Metadata,
        "Approve Inbox Item".to_string(),
        vec![id.to_string()],
        serde_json::json!({ "inbox_status": previous }),
        serde_json::json!({ "inbox_status": "approved" }),
        std::sync::Arc::new(move || {
            let mut o = manager_undo
                .load(object_id)
                .ok_or_else(|| "Object not found during undo".to_string())?;
            set_custom_text(&mut o, "inbox_status", &previous);
            manager_undo.save(&o).map_err(|e| e.to_string())?;
            Ok(())
        }),
        std::sync::Arc::new(move || {
            let mut o = manager_redo
                .load(object_id)
                .ok_or_else(|| "Object not found during redo".to_string())?;
            set_custom_text(&mut o, "inbox_status", "approved");
            manager_redo.save(&o).map_err(|e| e.to_string())?;
            Ok(())
        }),
    )?;
    Ok(())
}

#[tauri::command]
pub fn inbox_reject(
    ctx: State<'_, ApplicationContext>,
    id: String,
    reason: String,
) -> Result<(), String> {
    inbox_reject_impl(&ctx, &id, &reason)
}

fn inbox_reject_impl(ctx: &ApplicationContext, id: &str, reason: &str) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Inbox item not found: {}", id))?;

    let previous_status =
        custom_text(&obj, "inbox_status").unwrap_or_else(|| "pending".to_string());
    let previous_reason = custom_text(&obj, "rejection_reason").unwrap_or_default();
    set_custom_text(&mut obj, "inbox_status", "rejected");
    set_custom_text(&mut obj, "rejection_reason", reason);
    manager.save(&obj).map_err(|e| e.to_string())?;

    let manager_undo = manager.clone();
    let manager_redo = manager.clone();
    crate::history::push_history(
        ctx,
        nabu_core::history::HistoryOp::Metadata,
        "Reject Inbox Item".to_string(),
        vec![id.to_string()],
        serde_json::json!({ "inbox_status": previous_status }),
        serde_json::json!({ "inbox_status": "rejected", "rejection_reason": reason }),
        std::sync::Arc::new(move || {
            let mut o = manager_undo
                .load(object_id)
                .ok_or_else(|| "Object not found during undo".to_string())?;
            set_custom_text(&mut o, "inbox_status", &previous_status);
            set_custom_text(&mut o, "rejection_reason", &previous_reason);
            manager_undo.save(&o).map_err(|e| e.to_string())?;
            Ok(())
        }),
        std::sync::Arc::new(move || {
            let mut o = manager_redo
                .load(object_id)
                .ok_or_else(|| "Object not found during redo".to_string())?;
            set_custom_text(&mut o, "inbox_status", "rejected");
            manager_redo.save(&o).map_err(|e| e.to_string())?;
            Ok(())
        }),
    )?;
    Ok(())
}

#[tauri::command]
pub fn inbox_retry(ctx: State<'_, ApplicationContext>, id: String) -> Result<(), String> {
    inbox_retry_impl(&ctx, &id)
}

fn inbox_retry_impl(ctx: &ApplicationContext, id: &str) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Inbox item not found: {}", id))?;

    set_custom_text(&mut obj, "inbox_status", "pending");
    manager.save(&obj).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn inbox_delete(ctx: State<'_, ApplicationContext>, id: String) -> Result<(), String> {
    inbox_delete_impl(&ctx, &id)
}

fn inbox_delete_impl(ctx: &ApplicationContext, id: &str) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    manager.delete(object_id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn inbox_batch_approve(
    ctx: State<'_, ApplicationContext>,
    ids: Vec<String>,
) -> Result<(), String> {
    inbox_batch_approve_impl(&ctx, &ids)
}

/// Approves every item, propagating the first real error instead of silently
/// swallowing it (extracted for unit-testing against `&ApplicationContext`).
fn inbox_batch_approve_impl(ctx: &ApplicationContext, ids: &[String]) -> Result<(), String> {
    for id in ids {
        inbox_approve_impl(ctx, id)?;
    }
    Ok(())
}

#[tauri::command]
pub fn inbox_batch_reject(
    ctx: State<'_, ApplicationContext>,
    ids: Vec<String>,
    reason: String,
) -> Result<(), String> {
    inbox_batch_reject_impl(&ctx, &ids, &reason)
}

fn inbox_batch_reject_impl(ctx: &ApplicationContext, ids: &[String], reason: &str) -> Result<(), String> {
    for id in ids {
        inbox_reject_impl(ctx, id, reason)?;
    }
    Ok(())
}

#[tauri::command]
pub fn inbox_batch_delete(
    ctx: State<'_, ApplicationContext>,
    ids: Vec<String>,
) -> Result<(), String> {
    inbox_batch_delete_impl(&ctx, &ids)
}

fn inbox_batch_delete_impl(ctx: &ApplicationContext, ids: &[String]) -> Result<(), String> {
    for id in ids {
        inbox_delete_impl(ctx, id)?;
    }
    Ok(())
}

#[tauri::command]
pub fn inbox_batch_retry(
    ctx: State<'_, ApplicationContext>,
    ids: Vec<String>,
) -> Result<(), String> {
    inbox_batch_retry_impl(&ctx, &ids)
}

fn inbox_batch_retry_impl(ctx: &ApplicationContext, ids: &[String]) -> Result<(), String> {
    for id in ids {
        inbox_retry_impl(ctx, id)?;
    }
    Ok(())
}

#[tauri::command]
pub fn inbox_edit_metadata(
    ctx: State<'_, ApplicationContext>,
    id: String,
    title: Option<String>,
    author: Option<String>,
    language: Option<String>,
    tags: Vec<String>,
    custom: std::collections::HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    inbox_edit_metadata_impl(&ctx, &id, title, author, language, tags, custom)
}

fn inbox_edit_metadata_impl(
    ctx: &ApplicationContext,
    id: &str,
    title: Option<String>,
    author: Option<String>,
    language: Option<String>,
    tags: Vec<String>,
    custom: std::collections::HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Inbox item not found: {}", id))?;

    if let Some(t) = title {
        obj.metadata.title = Some(t);
    }
    if let Some(a) = author {
        obj.metadata.authors = vec![a];
    }
    if let Some(l) = language {
        obj.metadata.language = Some(l);
    }
    if !tags.is_empty() {
        obj.tags = tags;
    }
    for (key, value) in custom {
        set_custom_json(&mut obj, &key, &value);
    }

    manager.save(&obj).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn inbox_move(
    ctx: State<'_, ApplicationContext>,
    id: String,
    destination: String,
) -> Result<(), String> {
    inbox_move_impl(&ctx, &id, &destination)
}

fn inbox_move_impl(ctx: &ApplicationContext, id: &str, destination: &str) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Inbox item not found: {}", id))?;

    set_custom_text(&mut obj, "destination_folder", destination);
    manager.save(&obj).map_err(|e| e.to_string())?;
    Ok(())
}

/// Drag-and-drop (or file-open) capture entry point.
///
/// Routes the file bytes through the canonical `CaptureEngine` → `FileDropHandler`
/// → `CaptureData::Binary`, exactly the same path as every other capture source.
/// The returned object ID will appear in the Knowledge Inbox once processing
/// completes.
#[tauri::command]
pub async fn capture_file_drop(
    ctx: State<'_, ApplicationContext>,
    filename: String,
    mime_type: String,
    data: Vec<u8>,
) -> Result<String, String> {
    capture_file_drop_impl(&ctx, filename, mime_type, data).await
}

pub(crate) async fn capture_file_drop_impl(
    ctx: &ApplicationContext,
    filename: String,
    mime_type: String,
    data: Vec<u8>,
) -> Result<String, String> {
    let engine = ctx
        .capture_engine()
        .ok_or_else(|| "CaptureEngine is not registered in the application context".to_string())?;

    let request = nabu_core::capture::CaptureRequest::new(
        nabu_core::capture::CaptureData::Binary {
            mime_type: mime_type.clone(),
            data,
            filename: Some(filename.clone()),
        },
    )
    .with_mime_type(mime_type)
    .with_title(filename.clone());

    let object_id = engine.ingest(request).await.map_err(|e| e.to_string())?;
    match object_id {
        Some(id) => Ok(id.to_string()),
        None => Err("FileDropHandler could not process the dropped file".to_string()),
    }
}

// ── Reading Queue Commands ────────────────────────────────────────

/// Helper: convert a canonical KnowledgeObject to the frontend QueueItem.
fn knowledge_object_to_queue_item(obj: &KnowledgeObject) -> QueueItem {
    let status = custom_text(obj, "reading_status")
        .map(|s| QueueStatus::from_label(&s))
        .unwrap_or_default();
    let priority = custom_text(obj, "reading_priority")
        .map(|s| QueuePriority::from_label(&s))
        .unwrap_or_default();
    let progress = custom_number(obj, "reading_progress").unwrap_or(0.0) as f32;

    QueueItem {
        id: obj.id.to_string(),
        title: obj.metadata.title.clone().unwrap_or_default(),
        object_type: obj.object_type.variant_name().to_string(),
        status,
        priority,
        progress,
        source: obj.metadata.source_url.clone().unwrap_or_default(),
        modified_at: obj.updated_at.to_rfc3339(),
        tags: obj.tags.clone(),
        selected: false,
    }
}

#[tauri::command]
pub fn queue_get_all(ctx: State<'_, ApplicationContext>) -> Result<Vec<QueueItem>, String> {
    queue_get_all_impl(&ctx)
}

fn queue_get_all_impl(ctx: &ApplicationContext) -> Result<Vec<QueueItem>, String> {
    let manager = get_storage_manager(ctx)?;
    let objects = manager
        .list_objects("", None, 1000)
        .map_err(|e| e.to_string())?;

    let queue_items = objects
        .into_iter()
        .map(|obj| knowledge_object_to_queue_item(&obj))
        .collect();

    Ok(queue_items)
}

#[tauri::command]
pub fn queue_set_status(
    ctx: State<'_, ApplicationContext>,
    id: String,
    status: String,
) -> Result<(), String> {
    queue_set_status_impl(&ctx, &id, &status)
}

fn queue_set_status_impl(ctx: &ApplicationContext, id: &str, status: &str) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Object not found: {}", id))?;

    let status = QueueStatus::from_label(status);
    let previous =
        custom_text(&obj, "reading_status").unwrap_or_else(|| QueueStatus::Unread.label().to_string());
    let new_label = status.label().to_string();
    set_custom_text(&mut obj, "reading_status", &new_label);
    manager.save(&obj).map_err(|e| e.to_string())?;

    let manager_undo = manager.clone();
    let manager_redo = manager.clone();
    crate::history::push_history(
        ctx,
        nabu_core::history::HistoryOp::Metadata,
        format!("Mark '{}' {}", obj.metadata.title.as_deref().unwrap_or("item"), new_label),
        vec![id.to_string()],
        serde_json::json!({ "reading_status": previous }),
        serde_json::json!({ "reading_status": new_label }),
        std::sync::Arc::new(move || {
            let mut o = manager_undo
                .load(object_id)
                .ok_or_else(|| "Object not found during undo".to_string())?;
            set_custom_text(&mut o, "reading_status", &previous);
            manager_undo.save(&o).map_err(|e| e.to_string())?;
            Ok(())
        }),
        std::sync::Arc::new(move || {
            let mut o = manager_redo
                .load(object_id)
                .ok_or_else(|| "Object not found during redo".to_string())?;
            set_custom_text(&mut o, "reading_status", &new_label);
            manager_redo.save(&o).map_err(|e| e.to_string())?;
            Ok(())
        }),
    )?;
    Ok(())
}

#[tauri::command]
pub fn queue_set_priority(
    ctx: State<'_, ApplicationContext>,
    id: String,
    priority: String,
) -> Result<(), String> {
    queue_set_priority_impl(&ctx, &id, &priority)
}

fn queue_set_priority_impl(ctx: &ApplicationContext, id: &str, priority: &str) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Object not found: {}", id))?;

    let priority = QueuePriority::from_label(priority);
    set_custom_text(&mut obj, "reading_priority", priority.label());
    manager.save(&obj).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn queue_set_progress(
    ctx: State<'_, ApplicationContext>,
    id: String,
    progress: f32,
) -> Result<(), String> {
    queue_set_progress_impl(&ctx, &id, progress)
}

fn queue_set_progress_impl(ctx: &ApplicationContext, id: &str, progress: f32) -> Result<(), String> {
    let manager = get_storage_manager(ctx)?;
    let object_id = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid object id: {}", e))?;
    let mut obj = manager
        .load(object_id)
        .ok_or_else(|| format!("Object not found: {}", id))?;

    obj.custom_properties.insert(
        "reading_progress".to_string(),
        CustomPropertyValue::Number(progress.clamp(0.0, 1.0) as f64),
    );
    manager.save(&obj).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn queue_batch_set_status(
    ctx: State<'_, ApplicationContext>,
    ids: Vec<String>,
    status: String,
) -> Result<(), String> {
    queue_batch_set_status_impl(&ctx, &ids, &status)
}

fn queue_batch_set_status_impl(ctx: &ApplicationContext, ids: &[String], status: &str) -> Result<(), String> {
    for id in ids {
        queue_set_status_impl(ctx, id, status)?;
    }
    Ok(())
}

#[tauri::command]
pub fn queue_archive_completed(ctx: State<'_, ApplicationContext>) -> Result<usize, String> {
    queue_archive_completed_impl(&ctx)
}

fn queue_archive_completed_impl(ctx: &ApplicationContext) -> Result<usize, String> {
    let manager = get_storage_manager(ctx)?;
    let objects = manager
        .list_objects("", None, 1000)
        .map_err(|e| e.to_string())?;

    let mut archived = 0;
    for mut obj in objects {
        let status = custom_text(&obj, "reading_status")
            .map(|s| QueueStatus::from_label(&s))
            .unwrap_or_default();
        if status == QueueStatus::Completed {
            set_custom_text(&mut obj, "reading_status", "archived");
            if manager.save(&obj).is_ok() {
                archived += 1;
            }
        }
    }
    Ok(archived)
}

// ── Navigation & Discovery Commands ────────────────────────────────

/// One note in the vault index — powers the dashboard (recently modified),
/// the quick switcher and the search page's folder filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteIndexEntry {
    /// Vault-relative path (forward slashes).
    pub path: String,
    /// Display title (file name without `.md`).
    pub title: String,
    /// Parent folder ("" for the vault root).
    pub folder: String,
    /// Last modification time as an RFC 3339 string.
    pub modified_at: String,
    /// Whether the note is pinned (reserved for future use).
    #[serde(default)]
    pub pinned: bool,
}

/// Returns the parent folder of a vault-relative path ("" for the root).
fn note_folder_of(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

/// Returns the file name without its `.md` extension.
fn file_stem(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
        .to_string()
}

/// Builds the flat note index from the canonical StorageManager.
///
/// This is the real indexing path: every note is a persisted
/// `KnowledgeObject` enumerated through the single storage owner (which is
/// kept in sync with the real `Indexer` via the `ITEM_STORED` pipeline). No
/// ad-hoc filesystem walk, no second index implementation.
fn notes_index_impl(ctx: &ApplicationContext) -> Result<Vec<NoteIndexEntry>, String> {
    let manager = get_storage_manager(ctx)?;
    let objects = manager
        .list_objects("", None, 100_000)
        .map_err(|e| e.to_string())?;

    let mut notes = Vec::new();
    for obj in objects {
        if obj.object_type != nabu_core::models::ObjectType::Note {
            continue;
        }
        let Some(path) = obj.metadata.vault_path.clone() else { continue };
        if !path.ends_with(".md") {
            continue;
        }
        let title = obj
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| file_stem(&path));
        notes.push(NoteIndexEntry {
            path: path.clone(),
            title,
            folder: note_folder_of(&path),
            modified_at: obj.updated_at.to_rfc3339(),
            pinned: false,
        });
    }
    // Most recently modified first.
    notes.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(notes)
}

/// Returns every note as a flat, sorted index, sourced from the real storage.
///
/// The index is used by the dashboard's "Recently Modified" section, the
/// Quick Switcher's note list and the Search page's folder filter.
#[tauri::command]
pub fn notes_index(ctx: State<'_, ApplicationContext>) -> Result<Vec<NoteIndexEntry>, String> {
    notes_index_impl(&ctx)
}

/// One full-text search hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// Vault-relative path of the matching note.
    pub path: String,
    /// Display title.
    pub title: String,
    /// Parent folder ("" for vault root).
    pub folder: String,
    /// Text snippet around the first match.
    pub snippet: String,
    /// Character offset of the first match within `snippet`.
    pub match_start: usize,
    /// Character offset one past the end of the first match in `snippet`.
    pub match_end: usize,
    /// Last modification time (RFC 3339) for sorting.
    pub modified_at: String,
}

/// Returns a case-insensitive byte index of the first occurrence of `needle`
/// in `haystack`, or `None`.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let lower = haystack.to_lowercase();
    let needle_lower = needle.to_lowercase();
    lower.find(&needle_lower)
}

/// Builds a short context snippet around a byte offset, returning the snippet
/// and the (recomputed) match range within it.
fn make_snippet(content: &str, byte_idx: usize, match_len: usize) -> (String, usize, usize) {
    const CONTEXT: usize = 60;
    let chars: Vec<char> = content.chars().collect();
    let char_idx = content[..byte_idx].chars().count();
    let match_chars = content[byte_idx..byte_idx + match_len.min(content.len() - byte_idx)]
        .chars()
        .count();
    let start = char_idx.saturating_sub(CONTEXT);
    let end = (char_idx + match_chars + CONTEXT).min(chars.len());
    let mut snippet: String = chars[start..end].iter().collect();
    snippet = snippet.replace('\n', " ");
    snippet = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    // Recompute the match range inside the whitespace-collapsed snippet. We
    // reconstruct by scanning the original slice for the query's character
    // count — simpler: return offsets relative to the collapsed string by
    // locating the match again.
    (snippet, char_idx - start, char_idx - start + match_chars)
}

/// Full-text search across note contents, backed by the real persistent
/// `Indexer`.
///
/// The set of matching object IDs comes from the canonical Indexer (the same
/// inverted index used for persistence and the `ITEM_STORED` pipeline). The
/// returned `SearchHit`s are hydrated from the StorageManager to recover the
/// vault path / title / snippet for the frontend.
#[tauri::command]
pub fn notes_search(
    query: String,
    ctx: State<'_, ApplicationContext>,
) -> Result<Vec<SearchHit>, String> {
    notes_search_impl(&ctx, &query)
}

fn notes_search_impl(ctx: &ApplicationContext, query: &str) -> Result<Vec<SearchHit>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let manager = get_storage_manager(ctx)?;
    let indexer = ctx
        .indexer()
        .ok_or_else(|| "Indexer is not registered in the application context".to_string())?;

    // Query the real Indexer. The Indexer is the single search index — no
    // ad-hoc scan, no metadata-only search, no second implementation.
    let ids = {
        let idx = indexer
            .lock()
            .map_err(|_| "Indexer lock poisoned".to_string())?;
        idx.search(q)
    };

    let mut hits = Vec::new();
    for id_str in ids {
        let Ok(id) = uuid::Uuid::parse_str(&id_str) else { continue };
        let Some(obj) = manager.load(id) else { continue };
        let Some(path) = obj.metadata.vault_path.clone() else { continue };
        let title = obj
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| file_stem(&path));
        let folder = note_folder_of(&path);
        let content = nabu_core::graph::content_as_str(&obj.content).to_string();

        // Snippet around the first match (title or body), matching the old
        // presentation conventions. The search *decision* is the Indexer's;
        // snippet generation is presentation only.
        let (snippet, s, e) = match find_ci(&content, q) {
            Some(idx) => make_snippet(&content, idx, q.len()),
            None => make_snippet(&content, 0, 0),
        };

        hits.push(SearchHit {
            path,
            title,
            folder,
            snippet,
            match_start: s,
            match_end: e,
            modified_at: obj.updated_at.to_rfc3339(),
        });
    }

    Ok(hits)
}

// ── Knowledge Graph & Connected Knowledge ────────────────────────────
//
// Phase 13.1: the UI-facing knowledge graph. Wikilinks (`[[Title]]`) are
// extracted from note markdown on demand and resolved against the vault's
// note titles — reusing the existing tree/scan conventions (hidden entries
// skipped, vault-relative forward-slash paths). No new indexing system is
// introduced; the graph data is derived state, rebuilt on each call.

/// One note collected from the vault with its raw content.
#[derive(Clone)]
struct NoteEntry {
    path: String,
    title: String,
    folder: String,
    modified_at: String,
    content: String,
}

/// Scans the vault for `.md` notes (hidden entries skipped, matching
/// `tree_list` / `notes_index`).
fn collect_notes(vault_path: &Path) -> Vec<NoteEntry> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<NoteEntry>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            // Archived content stays searchable but is hidden from navigation.
            if prefix.is_empty() && name == ARCHIVE_FOLDER {
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let full = entry.path();
            if full.is_dir() {
                walk(&full, &path, out);
            } else if name.ends_with(".md") {
                if let Ok(content) = std::fs::read_to_string(&full) {
                    let title = name.trim_end_matches(".md").to_string();
                    let folder = match path.rfind('/') {
                        Some(i) => path[..i].to_string(),
                        None => String::new(),
                    };
                    let modified = std::fs::metadata(&full)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| {
                            t.duration_since(std::time::UNIX_EPOCH)
                                .ok()
                                .map(|d| d.as_secs() as i64)
                        })
                        .and_then(|secs| {
                            chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
                        })
                        .unwrap_or_default();
                    out.push(NoteEntry {
                        path,
                        title,
                        folder,
                        modified_at: modified,
                        content,
                    });
                }
            }
        }
    }
    let mut notes = Vec::new();
    walk(vault_path, "", &mut notes);
    notes
}

/// Extracts `[[...]]` wikilink targets from markdown, handling the alias
/// (`[[Title|Alias]]`), heading (`[[Title#Heading]]`) and block (`[[Title^id]]`)
/// suffixes. Returns the resolved target name (alias/heading/block stripped).
fn extract_wikilinks(content: &str) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(rel) = content[i + 2..].find("]]") {
                let raw = &content[i + 2..i + 2 + rel];
                let target = raw.split('|').next().unwrap_or(raw);
                let target = target.split('#').next().unwrap_or(target);
                let target = target.split('^').next().unwrap_or(target).trim();
                if !target.is_empty() {
                    out.push(target.to_string());
                }
                i += 2 + rel + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Builds a `lowercased title → paths` index for link resolution.
fn build_title_index(notes: &[NoteEntry]) -> std::collections::HashMap<String, Vec<String>> {
    let mut index: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for note in notes {
        index
            .entry(note.title.to_lowercase())
            .or_default()
            .push(note.path.clone());
        // Also index the full path (folder/title) so `[[Folder/Note]]` works.
        index
            .entry(note.path.to_lowercase())
            .or_default()
            .push(note.path.clone());
    }
    index
}

/// Resolves a wikilink target to a note path, or `None` (broken link).
fn resolve_note(
    index: &std::collections::HashMap<String, Vec<String>>,
    target: &str,
) -> Option<String> {
    let key = target.trim().to_lowercase();
    index
        .get(&key)
        .and_then(|paths| paths.first().cloned())
        .or_else(|| {
            // Allow trailing `.md` in the link.
            index
                .get(&format!("{key}.md"))
                .and_then(|paths| paths.first().cloned())
        })
}

/// Extracts `tags:` from YAML frontmatter — inline array (`tags: [a, b]`),
/// comma list (`tags: a, b`) or block list (`tags:\n  - a\n  - b`).
fn extract_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(rest) = content.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let lines: Vec<&str> = fm.lines().collect();
            let mut in_block = false;
            for line in lines {
                let trimmed = line.trim();
                if in_block {
                    if let Some(item) = trimmed.strip_prefix("-") {
                        let p = item.trim().trim_matches('"').trim_matches('\'').to_string();
                        if !p.is_empty() {
                            tags.push(p);
                        }
                        continue;
                    }
                    in_block = false;
                }
                if let Some(value) = trimmed.strip_prefix("tags:") {
                    let inner = value.trim();
                    if inner.is_empty() {
                        in_block = true;
                        continue;
                    }
                    let inner = inner.trim_start_matches('[').trim_end_matches(']');
                    for part in inner.split(',') {
                        let p = part.trim().trim_matches('"').trim_matches('\'').to_string();
                        if !p.is_empty() {
                            tags.push(p);
                        }
                    }
                }
            }
        }
    }
    tags
}

/// One node in the knowledge graph (a markdown note).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Vault-relative path.
    pub path: String,
    /// Display title (file name without `.md`).
    pub title: String,
    /// Parent folder ("" for the vault root).
    pub folder: String,
    /// Last modification time (RFC 3339).
    pub modified_at: String,
    /// Tags from frontmatter.
    pub tags: Vec<String>,
    /// Incoming link count (other notes linking to this one).
    pub backlink_count: usize,
    /// Outgoing link count.
    pub outgoing_count: usize,
    /// Total degree (backlinks + outgoing).
    pub degree: usize,
}

/// One edge in the knowledge graph (a resolved wikilink).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdgeData {
    /// Path of the note containing the link.
    pub source: String,
    /// Resolved target path (or the raw link text when `broken`).
    pub target: String,
    /// True when the target does not resolve to a note.
    pub broken: bool,
}

/// Full graph payload for the graph view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdgeData>,
    /// Notes with zero connections (orphans).
    pub orphan_count: usize,
    /// Number of disconnected components.
    pub cluster_count: usize,
}

/// Returns the full knowledge graph from the real `VaultGraph`.
///
/// Nodes and edges come from the canonical VaultGraph (the single relationship
/// graph, kept in sync via the `ITEM_STORED` / `GRAPH_UPDATED` pipeline). No
/// graph relationships are reconstructed here — this command only reads the
/// shared graph and shapes it into the frontend DTO.
#[tauri::command]
pub fn graph_data(ctx: State<'_, ApplicationContext>) -> Result<GraphData, String> {
    graph_data_impl(&ctx)
}

fn graph_data_impl(ctx: &ApplicationContext) -> Result<GraphData, String> {
    let graph = ctx
        .vault_graph()
        .ok_or_else(|| "VaultGraph is not registered in the application context".to_string())?;
    let g = graph
        .read()
        .map_err(|_| "VaultGraph lock poisoned".to_string())?;

    let node_objs = g.all_nodes();
    let edges = g.edges();

    // Map object id → vault-relative path so edges can be expressed as paths.
    let mut id_to_path: std::collections::HashMap<uuid::Uuid, String> =
        std::collections::HashMap::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    for obj in &node_objs {
        let Some(path) = obj.metadata.vault_path.clone() else { continue };
        id_to_path.insert(obj.id, path.clone());
        nodes.push(GraphNode {
            path: path.clone(),
            title: obj
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| file_stem(&path)),
            folder: note_folder_of(&path),
            modified_at: obj.updated_at.to_rfc3339(),
            tags: obj.tags.clone(),
            backlink_count: 0,
            outgoing_count: 0,
            degree: 0,
        });
    }

    // Translate graph edges (content-derived wiki-links + explicit relations)
    // into path-based edge DTOs. Every edge here comes from the real graph.
    let mut edges_data: Vec<GraphEdgeData> = Vec::new();
    for edge in &edges {
        let (Some(source), Some(target)) = (
            id_to_path.get(&edge.source),
            id_to_path.get(&edge.target),
        ) else {
            continue;
        };
        edges_data.push(GraphEdgeData {
            source: source.clone(),
            target: target.clone(),
            broken: false,
        });
    }

    // Degree / backlink counts.
    let mut node_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        node_index.insert(n.path.clone(), i);
    }
    for edge in &edges_data {
        if let Some(&si) = node_index.get(&edge.source) {
            nodes[si].outgoing_count += 1;
            nodes[si].degree += 1;
        }
        if let Some(&di) = node_index.get(&edge.target) {
            nodes[di].backlink_count += 1;
            nodes[di].degree += 1;
        }
    }

    let orphan_count = nodes.iter().filter(|n| n.degree == 0).count();

    // Cluster count via union-find over the resolved edges.
    let mut parent: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    fn find(parent: &mut std::collections::HashMap<String, String>, x: &str) -> String {
        let px = parent.get(x).cloned().unwrap_or_else(|| x.to_string());
        if px != x {
            let root = find(parent, &px);
            parent.insert(x.to_string(), root.clone());
            root
        } else {
            px
        }
    }
    for edge in &edges_data {
        let a = find(&mut parent, &edge.source);
        let b = find(&mut parent, &edge.target);
        if a != b {
            parent.insert(a, b);
        }
    }
    let mut roots = std::collections::HashSet::new();
    for n in &nodes {
        roots.insert(find(&mut parent, &n.path));
    }

    Ok(GraphData {
        nodes,
        edges: edges_data,
        orphan_count,
        cluster_count: roots.len(),
    })
}

/// One backlink hit: another note linking to the inspected note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklinkEntry {
    /// Path of the linking note.
    pub path: String,
    /// Title of the linking note.
    pub title: String,
    /// Parent folder of the linking note.
    pub folder: String,
    /// Context snippet around the first link.
    pub snippet: String,
    /// Character offset of the match within the snippet.
    pub match_start: usize,
    /// Character offset one past the match within the snippet.
    pub match_end: usize,
    /// How many times the linking note references this note.
    pub count: usize,
}

/// One outgoing link from the inspected note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingLink {
    /// `internal` (resolves to a note), `broken`, or `external` (URL).
    pub kind: String,
    /// Raw link text or URL.
    pub target: String,
    /// Resolved note path when `internal`.
    pub path: Option<String>,
    /// How many times this target is linked.
    pub count: usize,
}

/// One unlinked mention: another note's title appearing as plain text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionEntry {
    /// The note that would be linked (matched by title).
    pub title: String,
    /// Path of the matched note.
    pub path: String,
    /// Context snippet around the first occurrence.
    pub snippet: String,
    /// Character offset of the match within the snippet.
    pub match_start: usize,
    /// Character offset one past the match within the snippet.
    pub match_end: usize,
    /// Match strength (longer titles rank higher).
    pub score: u32,
}

/// Backlinks, outgoing links and unlinked mentions for one note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteLinks {
    pub backlinks: Vec<BacklinkEntry>,
    pub outgoing: Vec<OutgoingLink>,
    pub mentions: Vec<MentionEntry>,
    /// Frontmatter tags of the inspected note.
    pub tags: Vec<String>,
}

/// Lowercased chars of `content` with their original byte offsets. Each char
/// maps to its first lowercase form (1:1 for all common scripts; multi-char
/// lowercase expansions like `İ` are not handled).
fn lc_chars(content: &str) -> Vec<(usize, char)> {
    content
        .char_indices()
        .map(|(i, c)| (i, c.to_lowercase().next().unwrap_or(c)))
        .collect()
}

/// Returns every word-boundary, case-insensitive occurrence of `needle` in
/// `content` as a byte range into the ORIGINAL `content` (char-safe).
///
/// Word boundaries: the chars immediately before/after must not be
/// alphanumeric or `_`. Reuses the canonical snippet builder (`make_snippet`)
/// semantics — this is the single place plain-text mention matching happens.
fn ci_word_ranges(content: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    ci_word_ranges_in_lc(content, &lc_chars(content), needle)
}

/// Like [`ci_word_ranges`], but matches against a precomputed lowercase char
/// list so callers that scan many needles (unlinked mentions) only build the
/// lowercase list once — O(content) setup + O(needle × content) per needle
/// instead of O(content) per needle.
fn ci_word_ranges_in_lc(
    content: &str,
    lc: &[(usize, char)],
    needle: &str,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let needle_lc: Vec<char> = needle
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    if needle_lc.is_empty() || needle_lc.len() > lc.len() {
        return out;
    }
    let mut i = 0usize;
    while i + needle_lc.len() <= lc.len() {
        let matched = needle_lc
            .iter()
            .enumerate()
            .all(|(k, &c)| lc[i + k].1 == c);
        if matched {
            let before_ok = i == 0 || {
                let p = lc[i - 1].1;
                !(p.is_alphanumeric() || p == '_')
            };
            let after_idx = i + needle_lc.len();
            let after_ok = after_idx >= lc.len() || {
                let p = lc[after_idx].1;
                !(p.is_alphanumeric() || p == '_')
            };
            if before_ok && after_ok {
                let start = lc[i].0;
                let end = if after_idx < lc.len() {
                    lc[after_idx].0
                } else {
                    content.len()
                };
                out.push((start, end));
            }
        }
        i += 1;
    }
    out
}

/// Returns backlinks, outgoing links and unlinked mentions for a note.
///
/// `min_title_len` (default 3) is the mention-detection sensitivity: titles
/// shorter than this are never suggested as unlinked mentions.
#[tauri::command]
pub fn note_links(
    path: String,
    min_title_len: Option<usize>,
    store: State<'_, SettingsStore>,
) -> Result<NoteLinks, String> {
    note_links_impl(&store, &path, min_title_len)
}

pub(crate) fn note_links_impl(
    store: &SettingsStore,
    path: &str,
    min_title_len: Option<usize>,
) -> Result<NoteLinks, String> {
    let min_len = min_title_len.unwrap_or(3).max(2);
    if path.trim().is_empty() {
        return Ok(NoteLinks {
            backlinks: Vec::new(),
            outgoing: Vec::new(),
            mentions: Vec::new(),
            tags: Vec::new(),
        });
    }
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    if vault_path.as_os_str().is_empty() || !vault_path.is_dir() {
        return Ok(NoteLinks {
            backlinks: Vec::new(),
            outgoing: Vec::new(),
            mentions: Vec::new(),
            tags: Vec::new(),
        });
    }

    let notes = collect_notes(&vault_path);
    let index = build_title_index(&notes);
    let this = notes
        .iter()
        .find(|n| n.path == path)
        .cloned()
        .ok_or_else(|| "Note not found".to_string())?;

    // Titles the user has chosen to ignore (persisted under `nabu.mention_ignored`).
    let ignored: std::collections::HashSet<String> = store
        .get_value("nabu.mention_ignored")
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default();

    // Hoisted outside the per-note loops (they were re-extracted per note).
    let this_links = extract_wikilinks(&this.content);

    // ── Backlinks ──
    let mut backlinks = Vec::new();
    for note in &notes {
        if note.path == this.path {
            continue;
        }
        let links = extract_wikilinks(&note.content);
        let mut count = 0usize;
        let mut first: Option<(usize, usize)> = None;
        let lc = lc_chars(&note.content);
        for target in links {
            let resolved = resolve_note(&index, &target)
                .or_else(|| (target.eq_ignore_ascii_case(&this.title)).then(|| this.path.clone()));
            if resolved.as_deref() == Some(this.path.as_str()) {
                count += 1;
                if first.is_none() {
                    // Locate the first `[[target` span char-safely.
                    for (start, end) in ci_word_ranges_in_lc(&note.content, &lc, &target) {
                        if start > 0 && note.content[..start].ends_with('[') {
                            first = Some((start, end - start));
                            break;
                        }
                    }
                }
            }
        }
        if count > 0 {
            let (snippet, s, e) = first
                .map(|(off, len)| make_snippet(&note.content, off, len))
                .unwrap_or_else(|| ("…".to_string(), 0, 0));
            backlinks.push(BacklinkEntry {
                path: note.path.clone(),
                title: note.title.clone(),
                folder: note.folder.clone(),
                snippet,
                match_start: s,
                match_end: e,
                count,
            });
        }
    }
    backlinks.sort_by(|a, b| b.count.cmp(&a.count));

    // ── Outgoing ──
    let mut outgoing: Vec<OutgoingLink> = Vec::new();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for target in &this_links {
        let kind;
        let resolved_path;
        if target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("www.")
        {
            kind = "external";
            resolved_path = None;
        } else if let Some(tpath) = resolve_note(&index, target) {
            kind = "internal";
            resolved_path = Some(tpath);
        } else {
            kind = "broken";
            resolved_path = None;
        }
        let key = resolved_path
            .clone()
            .unwrap_or_else(|| target.to_lowercase());
        let entry = seen.entry(key).or_insert(0);
        *entry += 1;
        if *entry == 1 {
            outgoing.push(OutgoingLink {
                kind: kind.to_string(),
                target: target.clone(),
                path: resolved_path,
                count: 1,
            });
        }
    }
    // Update counts for duplicates.
    for link in outgoing.iter_mut() {
        let key = link
            .path
            .clone()
            .unwrap_or_else(|| link.target.to_lowercase());
        link.count = seen.get(&key).copied().unwrap_or(1);
    }
    outgoing.sort_by(|a, b| b.count.cmp(&a.count));

    // ── Unlinked mentions ──
    // Only notes with a title at least `min_len` chars count (avoids noise
    // from short common words). Longer titles rank higher. Word-boundary
    // matching is char-safe; the lowercase list is built ONCE for this note so
    // the scan is O(needles × content) rather than rebuilding per needle.
    let this_lc = lc_chars(&this.content);
    // Paths already linked from this note (O(1) lookup per candidate).
    let linked_paths: std::collections::HashSet<String> = this_links
        .iter()
        .filter_map(|t| resolve_note(&index, t))
        .collect();
    let mut mentions: Vec<MentionEntry> = Vec::new();
    for note in &notes {
        if note.path == this.path {
            continue;
        }
        let title = note.title.trim();
        if title.len() < min_len {
            continue;
        }
        let title_lower = title.to_lowercase();
        if ignored.contains(&title_lower) {
            continue;
        }
        // Skip titles that are already wikilinked anywhere in this note.
        if linked_paths.contains(&note.path) {
            continue;
        }
        // Word-boundary occurrence count in the plain text.
        let ranges = ci_word_ranges_in_lc(&this.content, &this_lc, title);
        if ranges.is_empty() {
            continue;
        }
        // Exclude matches inside an existing `[[...]]` span (rare: a title
        // could match the link text itself).
        let mut count = 0usize;
        let mut first: Option<(usize, usize)> = None;
        for (start, end) in ranges {
            let inside_link = this.content[..start]
                .rfind("[[")
                .map(|open| {
                    this.content[open..]
                        .find("]]")
                        .map(|close| open + close + 2 > start)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if inside_link {
                continue;
            }
            count += 1;
            if first.is_none() {
                first = Some((start, end - start));
            }
        }
        if count > 0 {
            if let Some((off, len)) = first {
                let (snippet, s, e) = make_snippet(&this.content, off, len);
                mentions.push(MentionEntry {
                    title: title.to_string(),
                    path: note.path.clone(),
                    snippet,
                    match_start: s,
                    match_end: e,
                    score: (title.len() as u32) * (count as u32),
                });
            }
        }
    }
    mentions.sort_by(|a, b| b.score.cmp(&a.score));

    Ok(NoteLinks {
        backlinks,
        outgoing,
        mentions,
        tags: extract_tags(&this.content),
    })
}

/// Converts the first plain-text occurrence of `title` in the note at `path`
/// into a `[[wikilink]]` and writes the note back. Returns the new content.
#[tauri::command]
pub fn link_mention(
    path: String,
    title: String,
    store: State<'_, SettingsStore>,
) -> Result<String, String> {
    link_mention_impl(&store, &path, &title)
}

pub(crate) fn link_mention_impl(
    store: &SettingsStore,
    path: &str,
    title: &str,
) -> Result<String, String> {
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    let abs = validate_path_within_vault(&vault_path, path)?;
    let content = std::fs::read_to_string(&abs).map_err(|e| e.to_string())?;
    let mut replacement: Option<(usize, usize)> = None;
    for (start, end) in ci_word_ranges(&content, title) {
        let inside_link = content[..start]
            .rfind("[[")
            .map(|open| {
                content[open..]
                    .find("]]")
                    .map(|close| open + close + 2 > start)
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if !inside_link {
            replacement = Some((start, end - start));
            break;
        }
    }
    let Some((idx, len)) = replacement else {
        return Err("No matching plain-text mention found".to_string());
    };
    let new_content =
        format!("{}[[{}]]{}", &content[..idx], title, &content[idx + len..]);
    std::fs::write(&abs, &new_content).map_err(|e| e.to_string())?;
    Ok(new_content)
}

/// Reads the persisted list of mention titles the user chose to ignore.
#[tauri::command]
pub fn mention_ignore_list(store: State<'_, SettingsStore>) -> Result<Vec<String>, String> {
    mention_ignore_list_impl(&store)
}

pub(crate) fn mention_ignore_list_impl(store: &SettingsStore) -> Result<Vec<String>, String> {
    let value = store.get_value("nabu.mention_ignored");
    if value.is_null() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Adds a mention title to the ignore list (it stops appearing in the
/// unlinked-mentions panel).
#[tauri::command]
pub fn mention_ignore(title: String, store: State<'_, SettingsStore>) -> Result<(), String> {
    mention_ignore_impl(&store, &title)
}

pub(crate) fn mention_ignore_impl(store: &SettingsStore, title: &str) -> Result<(), String> {
    store
        .update(|s| {
            let mut list: Vec<String> = s
                .extra_settings
                .get("nabu.mention_ignored")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            if !list.iter().any(|t| t == title) {
                list.push(title.to_string());
            }
            s.extra_settings
                .insert("nabu.mention_ignored".to_string(), serde_json::json!(list));
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Knowledge Organisation & Workflow (Phase 13.2) ──────────────────────
//
// Virtual organisational overlays that leave the filesystem untouched:
// - Archive: notes move into a reserved `archive/` folder at the vault root.
//   They stay searchable (notes_search scans everything) but are hidden from
//   normal navigation (tree / index / graph skip the folder) until the
//   Archive view explicitly lists them for restore.
// - Smart Folders: persisted query definitions evaluated on demand against
//   the vault (tags / folders / dates / full text).
// - Calendar: date-indexed note listing for the calendar workspace.
// - Templates: CRUD persisted in settings (browse / edit / duplicate /
//   favourite).
// - Quick capture: create an inbox KnowledgeObject from the palette.

// ── Archive ─────────────────────────────────────────────────────────────

/// One archived note (its path inside `archive/` plus the original location).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// Path inside the archive folder (vault-relative, `archive/...`).
    pub archive_path: String,
    /// Original vault-relative path (where restore puts it back).
    pub original_path: String,
    /// Display title (file name without `.md`).
    pub title: String,
    /// Original parent folder ("" for vault root).
    pub folder: String,
    /// Last modification time (RFC 3339).
    pub modified_at: String,
}

/// The reserved archive folder name at the vault root. Notes moved here are
/// hidden from normal navigation but remain full-text searchable.
pub const ARCHIVE_FOLDER: &str = "archive";

fn archive_dir(vault_path: &Path) -> PathBuf {
    vault_path.join(ARCHIVE_FOLDER)
}

/// Moves a note (or folder) into the reserved `archive/` folder, preserving
/// its relative layout, and returns the new archive-relative path. Non-
/// destructive: the original content is untouched and restore is trivial.
#[tauri::command]
pub fn archive_note(
    path: String,
    ctx: State<'_, ApplicationContext>,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    archive_note_impl(&ctx, &store, &path)
}

pub(crate) fn archive_note_impl(
    ctx: &ApplicationContext,
    store: &SettingsStore,
    path: &str,
) -> Result<(), String> {
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    if path.trim().is_empty() || path == ARCHIVE_FOLDER || path.starts_with("archive/") {
        return Err("Invalid path for archiving".to_string());
    }
    let full = validate_path_within_vault(&vault_path, path)?;
    if !full.exists() {
        return Err(format!("Not found: {path}"));
    }
    let dest = archive_dir(&vault_path).join(path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&full, &dest).map_err(|e| format!("Could not archive: {e}"))?;
    let src_arc = full.clone();
    let dst_arc = dest.clone();
    let src_rename = full.clone();
    let dst_rename = dest.clone();
    let _ = crate::history::push_history(
        ctx,
        nabu_core::history::HistoryOp::Metadata,
        format!("Archive '{path}'"),
        vec![path.to_string()],
        serde_json::json!({ "archived": false }),
        serde_json::json!({ "archived": true }),
        std::sync::Arc::new(move || {
            std::fs::rename(&dst_rename, &src_rename).map_err(|e| e.to_string())
        }),
        std::sync::Arc::new(move || {
            if let Some(parent) = dst_arc.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&src_arc, &dst_arc).map_err(|e| e.to_string())
        }),
    );
    Ok(())
}

/// Restores an archived note to its original location.
#[tauri::command]
pub fn archive_restore(
    archive_path: String,
    ctx: State<'_, ApplicationContext>,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    archive_restore_impl(&ctx, &store, &archive_path)
}

pub(crate) fn archive_restore_impl(
    ctx: &ApplicationContext,
    store: &SettingsStore,
    archive_path: &str,
) -> Result<(), String> {
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    if !archive_path.starts_with("archive/") {
        return Err("Not an archived path".to_string());
    }
    let full = validate_path_within_vault(&vault_path, archive_path)?;
    if !full.exists() {
        return Err(format!("Not found: {}", archive_path));
    }
    let original_rel = archive_path
        .strip_prefix("archive/")
        .map(|s| s.to_string())
        .unwrap_or_default();
    let original = vault_path.join(&original_rel);
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&full, &original).map_err(|e| format!("Could not restore: {e}"))?;
    let src_restore = full.clone();
    let dst_restore = original.clone();
    let src_undo = full.clone();
    let dst_undo = original.clone();
    let _ = crate::history::push_history(
        ctx,
        nabu_core::history::HistoryOp::Metadata,
        format!("Restore '{original_rel}'"),
        vec![original_rel.to_string()],
        serde_json::json!({ "archived": true }),
        serde_json::json!({ "archived": false }),
        std::sync::Arc::new(move || {
            if let Some(parent) = src_undo.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&dst_undo, &src_undo).map_err(|e| e.to_string())
        }),
        std::sync::Arc::new(move || {
            std::fs::rename(&src_restore, &dst_restore).map_err(|e| e.to_string())
        }),
    );
    Ok(())
}

/// Lists every note inside the reserved `archive/` folder with its original
/// location, so the Archive view can offer restore.
#[tauri::command]
pub fn archive_list(store: State<'_, SettingsStore>) -> Result<Vec<ArchiveEntry>, String> {
    archive_list_impl(&store)
}

pub(crate) fn archive_list_impl(store: &SettingsStore) -> Result<Vec<ArchiveEntry>, String> {
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    let dir = archive_dir(&vault_path);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<ArchiveEntry>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let archive_rel = format!("archive/{path}");
            let full = entry.path();
            if full.is_dir() {
                walk(&full, &path, out);
            } else if name.ends_with(".md") {
                let original_path = path.clone();
                let title = name.trim_end_matches(".md").to_string();
                let folder = match path.rfind('/') {
                    Some(i) => path[..i].to_string(),
                    None => String::new(),
                };
                let modified = std::fs::metadata(&full)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs() as i64)
                    })
                    .and_then(|secs| {
                        chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
                    })
                    .unwrap_or_default();
                out.push(ArchiveEntry {
                    archive_path: archive_rel,
                    original_path,
                    title,
                    folder,
                    modified_at: modified,
                });
            }
        }
    }
    walk(&dir, "", &mut out);
    out.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(out)
}

// ── Smart Folders ────────────────────────────────────────────────────────

/// A persisted smart-folder definition (a named query shown in the sidebar).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartFolder {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub query: String,
    #[serde(default)]
    pub pinned: bool,
}

const K_SMART_FOLDERS: &str = "nabu.smart_folders";

/// Lists all saved smart folders (persisted in settings).
#[tauri::command]
pub fn smart_folders_list(store: State<'_, SettingsStore>) -> Result<Vec<SmartFolder>, String> {
    smart_folders_list_impl(&store)
}

pub(crate) fn smart_folders_list_impl(store: &SettingsStore) -> Result<Vec<SmartFolder>, String> {
    Ok(store
        .get_value(K_SMART_FOLDERS)
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value::<SmartFolder>(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default())
}

/// Saves (creates or updates) a smart folder definition.
#[tauri::command]
pub fn smart_folder_save(
    folder: SmartFolder,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    smart_folder_save_impl(&store, folder)
}

pub(crate) fn smart_folder_save_impl(store: &SettingsStore, folder: SmartFolder) -> Result<(), String> {
            let mut list = smart_folders_list_impl(store)?;
    if let Some(existing) = list.iter_mut().find(|f| f.id == folder.id) {
        *existing = folder.clone();
    } else {
        list.push(folder.clone());
    }
    store
        .update(|s| {
            s.extra_settings.insert(
                K_SMART_FOLDERS.to_string(),
                serde_json::to_value(&list).unwrap(),
            );
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes a smart folder by id.
#[tauri::command]
pub fn smart_folder_delete(id: String, store: State<'_, SettingsStore>) -> Result<(), String> {
    smart_folder_delete_impl(&store, &id)
}

pub(crate) fn smart_folder_delete_impl(store: &SettingsStore, id: &str) -> Result<(), String> {
    let list = smart_folders_list_impl(store)?;
    let filtered: Vec<SmartFolder> = list.into_iter().filter(|f| f.id != id).collect();
    store
        .update(|s| {
            s.extra_settings.insert(K_SMART_FOLDERS.to_string(), serde_json::to_value(&filtered).unwrap());
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Reads `date:` / `created:` / `modified:` from YAML frontmatter (YYYY-MM-DD).
fn frontmatter_date(content: &str) -> Option<String> {
    let rest = content.strip_prefix("---")?;
    let fm = rest.split("\n---").next()?;
    for line in fm.lines() {
        let trimmed = line.trim();
        for key in ["date:", "created:", "modified:"] {
            if let Some(value) = trimmed.strip_prefix(key) {
                let v = value.trim().trim_matches('"').trim_matches('\'');
                if v.len() >= 10 {
                    let d: String = v.chars().take(10).collect();
                    if d.chars().filter(|c| *c == '-').count() == 2 {
                        return Some(d);
                    }
                }
            }
        }
    }
    None
}

/// Evaluates a smart-folder query against the vault and returns matching
/// notes. Mini query language (whitespace-separated, ANDed):
///   `tag:name`     — frontmatter tag contains `name`
///   `folder:path`  — note lives in `path` (or a subfolder of it)
///   `date:YYYY-MM-DD` / `before:...` / `after:...` — frontmatter date or mtime
///   anything else  — case-insensitive full-text search (title + content)
#[tauri::command]
/// Evaluate a smart-folder query against the vault and return matching notes.
///
/// Query syntax (space-separated tokens, all optional):
/// - `tag:<name>`       — note must carry the tag (substring match, case-insensitive)
/// - `folder:<path>`    — note must live in the folder or a subfolder (case-insensitive)
/// - `date:YYYY-MM-DD`  — frontmatter date must equal the value
/// - `before:YYYY-MM-DD`— frontmatter date must be earlier (notes without a date pass)
/// - `after:YYYY-MM-DD` — frontmatter date must be later (notes without a date pass)
/// - any other token    — full-text term, must appear in title or body (case-insensitive)
///
/// An empty query matches every note in the vault.
pub fn smart_folder_evaluate(
    query: String,
    store: State<'_, SettingsStore>,
) -> Result<Vec<NoteIndexEntry>, String> {
    smart_folder_evaluate_impl(&store, query.trim())
}

pub(crate) fn smart_folder_evaluate_impl(
    store: &SettingsStore,
    query: &str,
) -> Result<Vec<NoteIndexEntry>, String> {
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    if vault_path.as_os_str().is_empty() || !vault_path.is_dir() {
        return Ok(Vec::new());
    }
    let notes = collect_notes(&vault_path);
    let q = query.trim();
    if q.is_empty() {
        return Ok(notes
            .into_iter()
            .map(|n| NoteIndexEntry {
                path: n.path,
                title: n.title,
                folder: n.folder,
                modified_at: n.modified_at,
                pinned: false,
            })
            .collect());
    }

    let mut tag_filters: Vec<String> = Vec::new();
    let mut folder_filters: Vec<String> = Vec::new();
    let mut before: Option<String> = None;
    let mut after: Option<String> = None;
    let mut exact_date: Option<String> = None;
    let mut text_terms: Vec<String> = Vec::new();

    for token in q.split_whitespace() {
        if let Some(t) = token.strip_prefix("tag:") {
            tag_filters.push(t.to_lowercase());
        } else if let Some(f) = token.strip_prefix("folder:") {
            folder_filters.push(f.trim_end_matches('/').to_lowercase());
        } else if let Some(d) = token.strip_prefix("date:") {
            exact_date = Some(d.to_string());
        } else if let Some(d) = token.strip_prefix("before:") {
            before = Some(d.to_string());
        } else if let Some(d) = token.strip_prefix("after:") {
            after = Some(d.to_string());
        } else {
            text_terms.push(token.to_lowercase());
        }
    }

    let mut out = Vec::new();
    for note in notes {
        let tags = extract_tags(&note.content);
        if !tag_filters.is_empty()
            && !tag_filters.iter().all(|f| tags.iter().any(|t| t.to_lowercase().contains(f)))
        {
            continue;
        }
        let folder_lc = note.folder.to_lowercase();
        if !folder_filters.is_empty()
            && !folder_filters.iter().all(|f| folder_lc == *f || folder_lc.starts_with(&format!("{f}/")))
        {
            continue;
        }
        let date = frontmatter_date(&note.content)
            .or_else(|| note.modified_at.chars().take(10).collect::<String>().into())
            .unwrap_or_default();
        if let Some(d) = &exact_date {
            if date != *d {
                continue;
            }
        }
        if let Some(b) = &before {
            if !date.is_empty() && date >= *b {
                continue;
            }
        }
        if let Some(a) = &after {
            if !date.is_empty() && date <= *a {
                continue;
            }
        }
        if !text_terms.is_empty() {
            let hay = format!("{} {}", note.title.to_lowercase(), note.content.to_lowercase());
            if !text_terms.iter().all(|t| hay.contains(t)) {
                continue;
            }
        }
        out.push(NoteIndexEntry {
            path: note.path.clone(),
            title: note.title.clone(),
            folder: note.folder.clone(),
            modified_at: note.modified_at.clone(),
            pinned: false,
        });
    }
    Ok(out)
}

// ── Calendar ─────────────────────────────────────────────────────────────

/// One dated note for the calendar workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEntry {
    pub path: String,
    pub title: String,
    pub folder: String,
    /// The date this note is shown under (YYYY-MM-DD, frontmatter date or mtime).
    pub date: String,
    pub modified_at: String,
}

/// Returns notes dated within `month` ("YYYY-MM"), using the frontmatter
/// `date:`/`created:` when present, else the file's modification date.
#[tauri::command]
pub fn calendar_notes(
    month: String,
    store: State<'_, SettingsStore>,
) -> Result<Vec<CalendarEntry>, String> {
    calendar_notes_impl(&store, &month)
}

pub(crate) fn calendar_notes_impl(store: &SettingsStore, month: &str) -> Result<Vec<CalendarEntry>, String> {
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    if vault_path.as_os_str().is_empty() || !vault_path.is_dir() {
        return Ok(Vec::new());
    }
    let month = month.trim().to_string();
    let notes = collect_notes(&vault_path);
    let mut out = Vec::new();
    for note in notes {
        let mtime: String = note.modified_at.chars().take(10).collect();
        let date = frontmatter_date(&note.content).unwrap_or_else(|| mtime.clone());
        if !month.is_empty() && !date.starts_with(&month) {
            continue;
        }
        out.push(CalendarEntry {
            path: note.path.clone(),
            title: note.title.clone(),
            folder: note.folder.clone(),
            date,
            modified_at: note.modified_at.clone(),
        });
    }
    out.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(out)
}

/// Returns the vault-relative path of the daily note for `date` (YYYY-MM-DD),
/// opening an existing one or creating it on first write.
#[tauri::command]
pub fn daily_note_for(date: String, store: State<'_, SettingsStore>) -> Result<String, String> {
    daily_note_for_impl(&store, &date)
}

pub(crate) fn daily_note_for_impl(store: &SettingsStore, date: &str) -> Result<String, String> {
    let d = date.trim();
    if d.len() < 10 || d.chars().filter(|c| *c == '-').count() != 2 {
        return Err("Invalid date (expected YYYY-MM-DD)".to_string());
    }
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    let path = format!("{d}.md");
    let full = validate_path_within_vault_unchecked(&vault_path, &path)?;
    if !full.exists() {
        let content = format!("# {d}\n");
        std::fs::write(&full, content).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

// ── Templates ────────────────────────────────────────────────────────────

/// A persisted note template (mirrors the UI `Template` model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateRecord {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub default_folder: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub favourite: bool,
    pub frontmatter_defaults: std::collections::HashMap<String, String>,
    pub property_presets: std::collections::HashMap<String, serde_json::Value>,
    pub body: String,
    pub object_type: Option<String>,
}

const K_TEMPLATES: &str = "nabu.templates";

#[tauri::command]
pub fn template_list(store: State<'_, SettingsStore>) -> Result<Vec<TemplateRecord>, String> {
    template_list_impl(&store)
}

pub(crate) fn template_list_impl(store: &SettingsStore) -> Result<Vec<TemplateRecord>, String> {
    Ok(store
        .get_value(K_TEMPLATES)
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value::<TemplateRecord>(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default())
}

fn template_persist(store: &SettingsStore, list: &[TemplateRecord]) -> Result<(), String> {
    store
        .update(|s| {
            s.extra_settings.insert(K_TEMPLATES.to_string(), serde_json::to_value(list).unwrap());
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn template_save(template: TemplateRecord, store: State<'_, SettingsStore>) -> Result<(), String> {
    template_save_impl(&store, template)
}

pub(crate) fn template_save_impl(store: &SettingsStore, template: TemplateRecord) -> Result<(), String> {
    let mut list = template_list_impl(store)?;
    if let Some(existing) = list.iter_mut().find(|t| t.name == template.name) {
        *existing = template;
    } else {
        list.push(template);
    }
    template_persist(store, &list)
}

#[tauri::command]
pub fn template_delete(name: String, store: State<'_, SettingsStore>) -> Result<(), String> {
    template_delete_impl(&store, &name)
}

pub(crate) fn template_delete_impl(store: &SettingsStore, name: &str) -> Result<(), String> {
    let list = template_list_impl(store)?;
    let filtered: Vec<TemplateRecord> = list.into_iter().filter(|t| t.name != name).collect();
    template_persist(store, &filtered)
}

#[tauri::command]
pub fn template_duplicate(name: String, store: State<'_, SettingsStore>) -> Result<TemplateRecord, String> {
    template_duplicate_impl(&store, &name)
}

pub(crate) fn template_duplicate_impl(store: &SettingsStore, name: &str) -> Result<TemplateRecord, String> {
    let mut list = template_list_impl(store)?;
    let source = list
        .iter()
        .find(|t| t.name == name)
        .cloned()
        .ok_or_else(|| "Template not found".to_string())?;
    let mut copy = source;
    let base = format!("{} Copy", copy.name);
    copy.favourite = false;
    // Ensure a unique name if a copy already exists (…Copy, …Copy 1, …Copy 2, …).
    let mut n = 1;
    let mut candidate = base.clone();
    while list.iter().any(|t| t.name == candidate) {
        candidate = format!("{base} {n}");
        n += 1;
    }
    copy.name = candidate;
    list.push(copy.clone());
    template_persist(store, &list)?;
    Ok(copy)
}

#[tauri::command]
pub fn template_set_favourite(
    name: String,
    favourite: bool,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    template_set_favourite_impl(&store, &name, favourite)
}

pub(crate) fn template_set_favourite_impl(store: &SettingsStore, name: &str, favourite: bool) -> Result<(), String> {
    let mut list = template_list_impl(store)?;
    if let Some(t) = list.iter_mut().find(|t| t.name == name) {
        t.favourite = favourite;
    }
    template_persist(store, &list)
}

// ── Quick capture ────────────────────────────────────────────────────────

/// Captures a quick note into the Inbox (a pending KnowledgeObject) from the
/// command palette or navbar, without touching the filesystem.
#[tauri::command]
pub fn inbox_quick_capture(
    ctx: State<'_, ApplicationContext>,
    title: String,
    content: String,
) -> Result<(), String> {
    inbox_quick_capture_impl(&ctx, &title, &content)
}

fn inbox_quick_capture_impl(ctx: &ApplicationContext, title: &str, content: &str) -> Result<(), String> {
    use nabu_core::models::knowledge_object::{KnowledgeObject, ObjectContent, ObjectMetadata, ObjectType};
    let manager = get_storage_manager(ctx)?;
    let mut obj = KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown(content.to_string()));
    let mut metadata = ObjectMetadata::default();
    metadata.title = Some(if title.trim().is_empty() {
        "Quick capture".to_string()
    } else {
        title.to_string()
    });
    metadata.description = Some("Captured via Quick Capture".to_string());
    obj.metadata = metadata;
    set_custom_text(&mut obj, "inbox_status", "pending");
    set_custom_text(&mut obj, "source", "quick_capture");
    manager.save(&obj).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Canvas Commands (Phase 13.3) ───────────────────────────────────
//
// Canvases are infinite visual workspaces that *reference* existing notes
// rather than duplicating content. A canvas definition is a JSON document
// stored in the settings store under `nabu.canvases` — no proprietary
// storage format, no content duplication. Each node carries a vault-relative
// note path plus an (x, y) position; edges are visual connectors between
// nodes. Groups are bounding boxes that label a region of the canvas.

/// One positioned node on a canvas. The `note_path` references an existing
/// note — the canvas never owns content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNode {
    /// Stable client-generated id (uuid).
    pub id: String,
    /// Vault-relative path of the referenced note.
    pub note_path: String,
    /// Display title (cached from the note for the sidebar list).
    pub title: String,
    /// X position in canvas coordinates.
    pub x: f64,
    /// Y position in canvas coordinates.
    pub y: f64,
    /// Optional width override (px). `None` = default card width.
    #[serde(default)]
    pub width: Option<f64>,
    /// Optional height override (px).
    #[serde(default)]
    pub height: Option<f64>,
    /// Node kind: note, image, pdf, link, group, annotation.
    #[serde(default = "default_node_kind")]
    pub kind: String,
    /// For image/pdf/link nodes: the source URL or vault-relative path.
    #[serde(default)]
    pub source: String,
    /// For annotation nodes: the text content.
    #[serde(default)]
    pub text: String,
}

fn default_node_kind() -> String {
    "note".to_string()
}

/// One visual connector between two canvas nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    /// Optional label on the connector (e.g. "references").
    #[serde(default)]
    pub label: String,
}

/// One labelled group (bounding box) on a canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasGroup {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Node ids contained in this group.
    #[serde(default)]
    pub members: Vec<String>,
}

/// A complete canvas definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub edges: Vec<CanvasEdge>,
    #[serde(default)]
    pub groups: Vec<CanvasGroup>,
    /// Pan offset (x, y) — the canvas viewport origin.
    #[serde(default)]
    pub pan_x: f64,
    #[serde(default)]
    pub pan_y: f64,
    /// Zoom level (1.0 = 100%).
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}

fn default_zoom() -> f64 {
    1.0
}

const CANVAS_KEY: &str = "nabu.canvases";

fn load_canvases(store: &SettingsStore) -> Vec<CanvasDef> {
    let value = store.get_value(CANVAS_KEY);
    serde_json::from_value::<Vec<CanvasDef>>(value).unwrap_or_default()
}

fn save_canvases(store: &SettingsStore, canvases: &[CanvasDef]) -> Result<(), String> {
    store
        .update(|s| {
            s.extra_settings
                .insert(CANVAS_KEY.to_string(), serde_json::to_value(canvases).unwrap());
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Lists every saved canvas (id + name only — nodes/edges omitted for speed).
#[tauri::command]
pub fn canvas_list(store: State<'_, SettingsStore>) -> Result<Vec<CanvasDef>, String> {
    canvas_list_impl(&store)
}

pub(crate) fn canvas_list_impl(store: &SettingsStore) -> Result<Vec<CanvasDef>, String> {
    Ok(load_canvases(store))
}

/// Returns the full canvas definition (nodes, edges, groups).
#[tauri::command]
pub fn canvas_get(
    id: String,
    store: State<'_, SettingsStore>,
) -> Result<Option<CanvasDef>, String> {
    canvas_get_impl(&store, &id)
}

pub(crate) fn canvas_get_impl(store: &SettingsStore, id: &str) -> Result<Option<CanvasDef>, String> {
    Ok(load_canvases(store).into_iter().find(|c| c.id == id))
}

/// Creates or updates a canvas (deduped by id) and persists it.
#[tauri::command]
pub fn canvas_save(
    canvas: CanvasDef,
    store: State<'_, SettingsStore>,
) -> Result<(), String> {
    canvas_save_impl(&store, canvas)
}

pub(crate) fn canvas_save_impl(store: &SettingsStore, canvas: CanvasDef) -> Result<(), String> {
    let mut canvases = load_canvases(store);
    if let Some(existing) = canvases.iter_mut().find(|c| c.id == canvas.id) {
        *existing = canvas;
    } else {
        canvases.push(canvas);
    }
    save_canvases(store, &canvases)
}

/// Deletes a canvas by id.
#[tauri::command]
pub fn canvas_delete(id: String, store: State<'_, SettingsStore>) -> Result<(), String> {
    canvas_delete_impl(&store, &id)
}

pub(crate) fn canvas_delete_impl(store: &SettingsStore, id: &str) -> Result<(), String> {
    let mut canvases = load_canvases(store);
    canvases.retain(|c| c.id != id);
    save_canvases(store, &canvases)
}

// ── Comparison View (Phase 13.3) ───────────────────────────────────

/// Computes a line diff between two arbitrary notes (by vault-relative path).
/// Reuses the same LCS diff engine as `versions_diff` so the Comparison View
/// can compare any two notes — not just revisions of the same note.
#[tauri::command]
pub fn notes_diff(
    path_a: String,
    path_b: String,
    store: State<'_, SettingsStore>,
) -> Result<Vec<crate::recovery::DiffRow>, String> {
    notes_diff_impl(&store, &path_a, &path_b)
}

pub(crate) fn notes_diff_impl(
    store: &SettingsStore,
    path_a: &str,
    path_b: &str,
) -> Result<Vec<crate::recovery::DiffRow>, String> {
    let vault = crate::recovery::vault_path_pub(store);
    let read_note = |p: &str| -> Result<String, String> {
        let abs = crate::recovery::resolve_in_vault_pub(&vault, p)?;
        if !abs.is_file() {
            return Ok(String::new());
        }
        std::fs::read_to_string(&abs).map_err(|e| e.to_string())
    };
    let a = read_note(path_a)?;
    let b = read_note(path_b)?;
    Ok(crate::recovery::line_diff_pub(&a, &b))
}

// ── Statistics & Insights (Phase 13.3) ─────────────────────────────

/// One tag with its usage count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagStat {
    pub tag: String,
    pub count: usize,
}

/// One day in the vault-growth histogram (notes created that day).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthPoint {
    pub date: String,
    pub count: usize,
}

/// One recently-active note (created or modified).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentNoteStat {
    pub path: String,
    pub title: String,
    pub folder: String,
    pub modified_at: String,
    pub created_at: Option<String>,
    pub size: usize,
}

/// The complete vault statistics payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatistics {
    pub note_count: usize,
    pub folder_count: usize,
    pub tag_count: usize,
    pub total_tags: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub graph_orphans: usize,
    pub graph_clusters: usize,
    pub tags: Vec<TagStat>,
    pub recently_created: Vec<RecentNoteStat>,
    pub recently_modified: Vec<RecentNoteStat>,
    pub growth: Vec<GrowthPoint>,
    pub storage_bytes: u64,
    pub writing_streak_days: usize,
    pub active_days_last_30: usize,
}

/// Recursively counts folders (directories) in the vault, skipping hidden
/// entries and the reserved `archive/` folder.
fn count_folders(vault_path: &Path) -> usize {
    fn walk(dir: &Path, prefix: &str) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut count = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if prefix.is_empty() && name == ARCHIVE_FOLDER {
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                count += 1 + walk(&entry.path(), &path);
            }
        }
        count
    }
    walk(vault_path, "")
}

/// Computes the total size of all `.md` files in the vault (bytes).
fn vault_storage_usage(vault_path: &Path) -> u64 {
    fn walk(dir: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut total = 0u64;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                total += walk(&path);
            } else if name.ends_with(".md") {
                total += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            }
        }
        total
    }
    walk(vault_path)
}

/// Computes the writing streak: the number of consecutive days (ending today
/// or yesterday) on which at least one note was modified. Also returns the
/// count of active days in the last 30 days.
fn writing_streak(vault_path: &Path) -> (usize, usize) {
    let notes = collect_notes(vault_path);
    let today = chrono::Local::now().date_naive();
    let mut active_days: std::collections::HashSet<chrono::NaiveDate> = notes
        .iter()
        .filter_map(|n| {
            chrono::DateTime::parse_from_rfc3339(&n.modified_at)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
        })
        .collect();

    let streak = {
        let mut streak = 0;
        let mut cursor = today;
        // Allow today to be empty (streak counts from yesterday if today has
        // no edits yet).
        if !active_days.contains(&cursor) {
            cursor = cursor.pred_opt().unwrap_or(cursor);
        }
        while active_days.contains(&cursor) {
            streak += 1;
            cursor = match cursor.pred_opt() {
                Some(d) => d,
                None => break,
            };
        }
        streak
    };

    // Active days in the last 30 days.
    let cutoff = today - chrono::Duration::days(30);
    active_days.retain(|d| *d >= cutoff);
    (streak, active_days.len())
}

/// Builds a 30-day vault-growth histogram (notes modified per day).
fn vault_growth(vault_path: &Path) -> Vec<GrowthPoint> {
    let notes = collect_notes(vault_path);
    let today = chrono::Local::now().date_naive();
    let mut buckets: std::collections::BTreeMap<chrono::NaiveDate, usize> =
        std::collections::BTreeMap::new();
    for d in 0..30 {
        let date = today - chrono::Duration::days(d);
        buckets.insert(date, 0);
    }
    for note in &notes {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&note.modified_at) {
            let date = dt.with_timezone(&chrono::Local).date_naive();
            if let Some(count) = buckets.get_mut(&date) {
                *count += 1;
            }
        }
    }
    buckets
        .into_iter()
        .rev()
        .map(|(date, count)| GrowthPoint {
            date: date.format("%Y-%m-%d").to_string(),
            count,
        })
        .collect()
}

/// Returns comprehensive vault statistics for the Statistics dashboard.
#[tauri::command]
pub fn statistics_get(
    store: State<'_, SettingsStore>,
    ctx: State<'_, ApplicationContext>,
) -> Result<VaultStatistics, String> {
    statistics_get_impl(&ctx, &store)
}

pub(crate) fn statistics_get_impl(
    ctx: &ApplicationContext,
    store: &SettingsStore,
) -> Result<VaultStatistics, String> {
    let settings = store.get();
    let vault_path = PathBuf::from(settings.last_vault_path.trim());
    if vault_path.as_os_str().is_empty() || !vault_path.is_dir() {
        return Ok(VaultStatistics {
            note_count: 0,
            folder_count: 0,
            tag_count: 0,
            total_tags: 0,
            graph_nodes: 0,
            graph_edges: 0,
            graph_orphans: 0,
            graph_clusters: 0,
            tags: vec![],
            recently_created: vec![],
            recently_modified: vec![],
            growth: vec![],
            storage_bytes: 0,
            writing_streak_days: 0,
            active_days_last_30: 0,
        });
    }

    let notes = collect_notes(&vault_path);

    // Tag aggregation.
    let mut tag_map: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for note in &notes {
        for tag in extract_tags(&note.content) {
            *tag_map.entry(tag).or_insert(0) += 1;
        }
    }
    let mut tags: Vec<TagStat> = tag_map
        .into_iter()
        .map(|(tag, count)| TagStat { tag, count })
        .collect();
    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    let total_tags: usize = tags.iter().map(|t| t.count).sum();
    let tag_count = tags.len();

    // Graph data from the real VaultGraph (via the shared context).
    let graph = graph_data_impl(ctx)?;

    // Recently modified (top 10).
    let mut recent: Vec<RecentNoteStat> = notes
        .iter()
        .map(|n| {
            let size = n.content.len();
            RecentNoteStat {
                path: n.path.clone(),
                title: n.title.clone(),
                folder: n.folder.clone(),
                modified_at: n.modified_at.clone(),
                created_at: None,
                size,
            }
        })
        .collect();
    recent.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    let recently_modified: Vec<RecentNoteStat> = recent.iter().take(10).cloned().collect();
    // Recently "created" — approximated by the oldest modifications reversed.
    let mut by_oldest = recent.clone();
    by_oldest.sort_by(|a, b| a.modified_at.cmp(&b.modified_at));
    let recently_created: Vec<RecentNoteStat> = by_oldest.into_iter().take(10).collect();

    let growth = vault_growth(&vault_path);
    let storage_bytes = vault_storage_usage(&vault_path);
    let (writing_streak_days, active_days_last_30) = writing_streak(&vault_path);
    let folder_count = count_folders(&vault_path);

    Ok(VaultStatistics {
        note_count: notes.len(),
        folder_count,
        tag_count,
        total_tags,
        graph_nodes: graph.nodes.len(),
        graph_edges: graph.edges.len(),
        graph_orphans: graph.orphan_count,
        graph_clusters: graph.cluster_count,
        tags,
        recently_created,
        recently_modified,
        growth,
        storage_bytes,
        writing_streak_days,
        active_days_last_30,
    })
}

#[tauri::command]
pub fn capability_enable(
    ctx: State<'_, ApplicationContext>,
    capability_id: String,
) -> Result<(), String> {
    capability_enable_impl(&ctx, &capability_id)
}

pub(crate) fn capability_enable_impl(ctx: &ApplicationContext, capability_id: &str) -> Result<(), String> {
    if capability_id.is_empty() {
        return Err("capability_id must not be empty".to_string());
    }
    ctx.enable_capability(capability_id)
}

#[tauri::command]
pub fn capability_disable(
    ctx: State<'_, ApplicationContext>,
    capability_id: String,
) -> Result<(), String> {
    capability_disable_impl(&ctx, &capability_id)
}

pub(crate) fn capability_disable_impl(ctx: &ApplicationContext, capability_id: &str) -> Result<(), String> {
    if capability_id.is_empty() {
        return Err("capability_id must not be empty".to_string());
    }
    ctx.disable_capability(capability_id)
}

/// Returns every registered capability as a JSON array.
///
/// This is the canonical IPC API for frontend capability discovery. It
/// queries the [`nabu_core::plugin::capability::CapabilityRegistry`] directly
/// (the single source of truth for what the platform can do) — no manual
/// response construction and no second serialization format. The
/// [`Capability`] type's existing Serde implementation (P1.2.1) is reused
/// verbatim.
///
/// Runtime enabled/disabled state is **not** part of this snapshot; it is
/// communicated reactively to the frontend via `CapabilityStateChanged` events
/// on the `nabu-event` channel.
#[tauri::command]
pub fn capability_list(
    ctx: State<'_, ApplicationContext>,
) -> Result<Vec<nabu_core::plugin::capability::Capability>, String> {
    capability_list_impl(&ctx)
}

pub(crate) fn capability_list_impl(
    ctx: &ApplicationContext,
) -> Result<Vec<nabu_core::plugin::capability::Capability>, String> {
    let registry = ctx.capability_registry();
    let caps: Vec<nabu_core::plugin::capability::Capability> = registry
        .list()
        .iter()
        .filter_map(|id| registry.get(id))
        .cloned()
        .collect();
    let count = caps.len();
    tracing::info!(count, "Capabilities returned");
    Ok(caps)
}

/// Returns every registered capability enriched with its runtime enabled state
/// and provider name.
///
/// This is the canonical IPC API for the Capability Management UI. It queries
/// the [`nabu_core::plugin::capability::CapabilityRegistry`] directly and
/// projects each `Capability` definition into a `CapabilitySummaryWithState`
/// that carries the fields the bare `Capability` type does not expose:
///
/// - `enabled` — whether the capability is currently enabled.
/// - `provider` — which plugin or the host application provides this capability.
///
/// This command is intentionally separate from [`capability_list`] (which
/// returns the bare definition snapshot) so that forward-compatible extensions
/// to the summary payload do not perturb the canonical list endpoint.
#[tauri::command]
pub fn capability_list_with_state(
    ctx: State<'_, ApplicationContext>,
) -> Result<Vec<crate::commands::CapabilitySummaryWithState>, String> {
    capability_list_with_state_impl(&ctx)
}

pub(crate) fn capability_list_with_state_impl(
    ctx: &ApplicationContext,
) -> Result<Vec<crate::commands::CapabilitySummaryWithState>, String> {
    let registry = ctx.capability_registry();
    let ids = registry.list();
    let enabled = registry.list_enabled();
    let enabled_set: std::collections::HashSet<&String> = enabled.iter().collect();

    let summaries: Vec<crate::commands::CapabilitySummaryWithState> = ids
        .iter()
        .filter_map(|id| {
            let cap = registry.get(id)?;
            let provider = registry.provider(id).map(|s| s.to_string()).unwrap_or_default();
            Some(crate::commands::CapabilitySummaryWithState {
                capability: cap.clone(),
                enabled: enabled_set.contains(id),
                provider,
            })
        })
        .collect();

    tracing::info!(count = summaries.len(), "Capability summaries with state returned");
    Ok(summaries)
}

/// Returns a structured [`ServiceHealth`] report describing the current
/// operational state of the application's core services.
///
/// This is the canonical health endpoint for the Capability Platform. It
/// queries the `LifecycleManager` (the single source of truth for lifecycle
/// state) and the `ServiceRegistry` — no duplicate state is maintained.
///
/// The returned [`ServiceHealth`] struct is fully serializable and designed
/// for forward-compatible extension: future phases can add fields
/// (uptime, version, performance metrics, plugin status, etc.) without
/// breaking existing consumers.
///
/// [`ServiceHealth`]: nabu_core::registry::ServiceHealth
#[tauri::command]
pub fn health_check(
    ctx: State<'_, ApplicationContext>,
) -> Result<nabu_core::registry::ServiceHealth, String> {
    health_check_impl(&ctx)
}

pub(crate) fn health_check_impl(
    ctx: &ApplicationContext,
) -> Result<nabu_core::registry::ServiceHealth, String> {
    tracing::info!("Health check IPC requested");
    let health = ctx.health_check();
    tracing::info!(
        status = health.status_label(),
        lifecycle_stage = ?health.lifecycle_stage,
        registered_services = health.registered_services,
        "Health check IPC completed"
    );
    Ok(health)
}

/// Returns a unified runtime metrics snapshot for the Capability Platform.
///
/// This is the canonical metrics IPC endpoint. It collects metrics from:
///
/// - The [`PerformanceMonitor`] (timing and count instrumentation for all
///   subsystems: capture, queue, worker, processing, storage, indexer, graph,
///   event bus, pipeline migration)
/// - Every registered service that implements [`MetricsAggregator`]:
///   `PerformanceMonitor`, `WorkerPool`, `DurableJobQueue`, `StorageManager`,
///   `ConversationStore`, `CaptureEngine`, `Indexer`, `VaultGraph`
///
/// The response contains three metric categories:
///
/// - **Timers** — operation durations with min/max/avg/p50/p90/p99 statistics
/// - **Counters** — monotonically increasing counts (documents processed,
///   events published, IPC requests, completed operations)
/// - **Gauges** — point-in-time values (active workers, queued tasks,
///   connected services, running processes)
///
/// # Performance
///
/// Metrics collection is lightweight — no blocking I/O, no expensive
/// computation. Designed for frequent polling by future monitoring tools.
///
/// # Error Handling
///
/// Unavailable or partially-initialized services are skipped gracefully.
/// The response always includes whatever metrics could be collected, with
/// any errors recorded in the `errors` field.
///
/// # Future Compatibility
///
/// The [`RuntimeMetrics`] struct uses `#[serde(default)]` on all fields,
/// so future phases can add new metric types without breaking deserialization.
///
/// [`PerformanceMonitor`]: nabu_core::diagnostics::PerformanceMonitor
/// [`MetricsAggregator`]: nabu_core::registry::MetricsAggregator
/// [`RuntimeMetrics`]: nabu_core::registry::RuntimeMetrics
#[tauri::command]
pub fn metrics(
    ctx: State<'_, ApplicationContext>,
) -> Result<nabu_core::registry::RuntimeMetrics, String> {
    metrics_impl(&ctx)
}

pub(crate) fn metrics_impl(
    ctx: &ApplicationContext,
) -> Result<nabu_core::registry::RuntimeMetrics, String> {
    tracing::debug!("Metrics IPC requested");
    let metrics = ctx.metrics();
    tracing::debug!(
        timers = metrics.timers.len(),
        counters = metrics.counters.len(),
        gauges = metrics.gauges.len(),
        services = metrics.service_count,
        errors = metrics.error_count,
        "Metrics IPC completed"
    );
    Ok(metrics)
}

/// Returns a snapshot of the WorkerPool's health metrics.
///
/// Includes worker count, pending/running job counts, active worker count,
/// throttling and full status, and lifecycle stage.
#[tauri::command]
pub fn pool_health(
    ctx: State<'_, ApplicationContext>,
) -> Result<nabu_core::jobs::workers::PoolHealth, String> {
    pool_health_impl(&ctx)
}

pub(crate) fn pool_health_impl(
    ctx: &ApplicationContext,
) -> Result<nabu_core::jobs::workers::PoolHealth, String> {
    tracing::debug!("Pool health IPC requested");
    let pool = ctx
        .worker_pool()
        .ok_or_else(|| "WorkerPool not registered".to_string())?;
    let health = pool.health();
    tracing::debug!(
        worker_count = health.worker_count,
        active_workers = health.active_workers,
        pending_jobs = health.pending_jobs,
        running_jobs = health.running_jobs,
        "Pool health IPC completed"
    );
    Ok(health)
}

/// ── Diagnostic IPC ──────────────────────────────────────────────

/// Request payload for the `diagnostic_requested` IPC command.
///
/// The editor (or any frontend consumer) calls `diagnostic_requested` when it
/// needs diagnostics for a specific document. The backend routes the request
/// through the [`DiagnosticPlatform`], which dispatches to the registered
/// [`DiagnosticProvider`](nabu_core::diagnostic::DiagnosticProvider) for the
/// requested origin and returns a standardized [`DiagnosticBatch`].
///
/// The editor never calls Harper, LSP, or any specific analysis engine directly —
/// it only specifies the `origin` (which identifies the provider) and the `text`
/// to analyze. Adding a new analysis engine requires only registering a new
/// provider with the platform; no IPC or editor changes are needed.
///
/// `text` is the full document text to analyze. `resource_id` is a stable
/// identifier (typically a vault path like `"vault:notes/example.md"`) used by
/// subscribers to correlate diagnostics with the right document. `origin` is
/// an optional label for the analysis engine (defaults to `"harper"` when
/// `None`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRequest {
    /// Document text to analyze.
    pub text: String,
    /// Stable resource identifier (e.g. `"vault:notes/example.md"`).
    pub resource_id: String,
    /// Optional origin/producer label. If `None`, the platform defaults to
    /// `"harper"`.
    pub origin: Option<String>,
}

impl Default for DiagnosticRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            resource_id: String::new(),
            origin: Some("harper".to_string()),
        }
    }
}

/// Response payload for the `diagnostic_requested` IPC command.
///
/// Carries the full [`DiagnosticBatch`] plus the canonical
/// [`DiagnosticStyleMap`](nabu_core::diagnostic::DiagnosticStyleMap) so the
/// editor can render each diagnostic without a second round-trip. The style map
/// is the canonical severity→style mapping from
/// [`nabu_core::diagnostic::mapping`].
///
/// Editors should use the [`EditorDiagnosticBridge`](crate::diagnostics::EditorDiagnosticBridge)
/// to resolve the batch into per-diagnostic style-resolved structures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticResponse {
    /// The diagnostic batch containing all diagnostics for the resource.
    pub batch: nabu_core::diagnostic::DiagnosticBatch,
    /// The canonical severity→style mapping for rendering.
    pub style_map: nabu_core::diagnostic::DiagnosticStyleMap,
}

/// Runs on-demand diagnostic analysis on a document and returns the resulting
/// diagnostics with their presentation styles.
///
/// This is the **canonical IPC entry point** for the editor's diagnostic bridge.
/// It delegates to the [`DiagnosticPlatform`] (resolved from the
/// [`ApplicationContext`]), which routes the request to the registered
/// [`DiagnosticProvider`](nabu_core::diagnostic::DiagnosticProvider) for the
/// requested origin.
///
/// The returned [`DiagnosticResponse`] contains a [`DiagnosticBatch`] with all
/// diagnostics and the canonical [`DiagnosticStyleMap`] so the editor can render
/// them without a second IPC call.
///
/// ## Provider Independence
///
/// The command does **not** call Harper directly. It resolves the
/// `DiagnosticPlatform` from the application context and delegates to whatever
/// provider is registered under the requested origin. Adding a new analysis
/// engine (AI, LSP, plugin) requires only registering a new provider — no IPC
/// or command changes.
///
/// ## EventBus Integration
///
/// Every successful retrieval publishes a `DiagnosticEvent::BatchPublished`
/// event through the EventBus (via the platform), enabling asynchronous
/// subscribers (e.g. background panels, lint lists) to receive diagnostic
/// updates without an explicit IPC request. The IPC response is independent of
/// the EventBus — editors get their results directly from the return value.
///
/// ## Error Handling
///
/// Returns structured [`DiagnosticPlatformError`] values (serialized as JSON via
/// Tauri's error handling) — no panics. Errors cover:
/// - Invalid input (empty text or resource_id)
/// - Provider not found for the requested origin
/// - Provider analysis failure
/// - Invalid diagnostic produced by a provider
///
/// [`DiagnosticPlatform`]: nabu_core::diagnostic::DiagnosticPlatform
/// [`DiagnosticProvider`]: nabu_core::diagnostic::DiagnosticProvider
/// [`DiagnosticPlatformError`]: nabu_core::diagnostic::DiagnosticPlatformError
#[tauri::command]
pub async fn diagnostic_requested(
    ctx: State<'_, ApplicationContext>,
    request: DiagnosticRequest,
) -> Result<DiagnosticResponse, nabu_core::diagnostic::DiagnosticPlatformError> {
    diagnostic_requested_impl(&ctx, request).await
}

pub(crate) async fn diagnostic_requested_impl(
    ctx: &ApplicationContext,
    request: DiagnosticRequest,
) -> Result<DiagnosticResponse, nabu_core::diagnostic::DiagnosticPlatformError> {
    let origin = request.origin.as_ref().map(|s| s.as_str()).unwrap_or("harper");

    tracing::info!(
        origin = %origin,
        resource_id = %request.resource_id,
        text_len = request.text.len(),
        "Diagnostic analysis requested via IPC"
    );

    // Resolve the DiagnosticPlatform from the application context.
    let platform = ctx
        .diagnostic_platform()
        .ok_or(nabu_core::diagnostic::DiagnosticPlatformError::ProviderNotFound {
            origin: "diagnostic_platform".to_string(),
        })?;

    // Run analysis on a blocking thread — providers (e.g. Harper) may use
    // non-Send types internally. The platform's `retrieve` method handles
    // validation, EventBus publication, and error mapping.
    let text = request.text.clone();
    let resource_id = request.resource_id.clone();
    let origin_owned = request.origin.clone();

    let batch = tokio::task::spawn_blocking(move || {
        platform.retrieve(&text, &resource_id, origin_owned.as_deref())
    })
    .await
    .map_err(|e| nabu_core::diagnostic::DiagnosticPlatformError::ProviderError {
        origin: origin.to_string(),
        detail: format!("analysis task failed: {}", e),
    })??;

    let style_map = nabu_core::diagnostic::DiagnosticStyleMap::default();

    tracing::info!(
        origin = %batch.origin,
        resource_id = %batch.resource_id,
        diagnostic_count = batch.diagnostic_count(),
        "Diagnostic analysis completed"
    );

    Ok(DiagnosticResponse { batch, style_map })
}

// ---------------------------------------------------------------------------
// Plugin IPC — plugin_call
// ---------------------------------------------------------------------------

/// Frontend-facing IPC command that dispatches a plugin capability invocation.
///
/// This is the **canonical** entry point for frontend-to-plugin communication.
/// The frontend serializes a [`PluginInvocationRequest`] as JSON (with
/// `serde_json::Value` for the `input` field), and the host dispatches it
/// through the `PluginManager` to the appropriate `CapabilityProvider::invoke`.
///
/// ## Workflow
///
/// 1. The frontend sends a JSON object with `plugin_id`, `capability`,
///    `method`, optional `input`, and optional `metadata`.
/// 2. Tauri deserializes it into a [`PluginInvocationRequest`].
/// 3. The `ApplicationContext` acquires a read lock on the `PluginManager`.
/// 4. The `PluginManager` locates the provider, validates the capability,
///    and calls `CapabilityProvider::invoke`.
/// 5. The structured [`PluginInvocationResponse`] is returned to the
///    frontend.
///
/// The frontend never holds a reference to the provider or the
/// `PluginManager` — all access goes through this single command.
///
/// [`PluginInvocationRequest`]: nabu_core::plugin::PluginInvocationRequest
/// [`PluginInvocationResponse`]: nabu_core::plugin::PluginInvocationResponse
#[tauri::command]
pub fn plugin_call(
    ctx: State<'_, ApplicationContext>,
    request: nabu_core::plugin::PluginInvocationRequest,
) -> Result<nabu_core::plugin::PluginInvocationResponse, String> {
    plugin_call_impl(&ctx, request)
}

pub(crate) fn plugin_call_impl(
    ctx: &ApplicationContext,
    request: nabu_core::plugin::PluginInvocationRequest,
) -> Result<nabu_core::plugin::PluginInvocationResponse, String> {
    tracing::debug!(
        plugin_id = %request.plugin_id,
        capability = %request.capability,
        method = %request.method,
        "plugin_call IPC requested"
    );

    let plugin_id = request.plugin_id.clone();
    let capability = request.capability.clone();

    let response = ctx.invoke_plugin(request);

    if response.success {
        tracing::debug!(
            provider = response.execution.as_ref().and_then(|e| e.provider.as_deref()),
            duration_ms = response.execution.as_ref().and_then(|e| e.duration_ms),
            "plugin_call IPC completed successfully"
        );
    } else if let Some(err) = &response.error {
        tracing::warn!(
            plugin_id = %plugin_id,
            capability = %capability,
            error_code = %err.code,
            "plugin_call IPC failed"
        );
    }

    Ok(response)
}

// ── Conversation Thread IPC Commands ────────────────────────────────
//
// These commands expose the ConversationStore through the Tauri IPC layer.
// They resolve the single canonical ConversationStore from the
// ApplicationContext (registered at startup) — no command constructs its
// own store instance.

/// Resolves the single canonical ConversationStore from the ApplicationContext.
fn get_conversation_store(ctx: &ApplicationContext) -> Result<Arc<ConversationStore>, String> {
    ctx.conversation_store()
        .ok_or_else(|| "ConversationStore is not registered in the application context".to_string())
}

/// Saves a thread to persistent storage.
///
/// The thread is serialized atomically to `.nabu/conversations/<uuid>.json`.
/// If a thread with the same ID already exists, it is replaced.
#[tauri::command]
pub fn thread_save(
    thread: Thread,
    ctx: State<'_, ApplicationContext>,
) -> Result<(), String> {
    thread_save_impl(&ctx, thread)
}

pub(crate) fn thread_save_impl(ctx: &ApplicationContext, thread: Thread) -> Result<(), String> {
    let store = get_conversation_store(ctx)?;
    store.save(&thread).map_err(|e| e.to_string())
}

/// Loads a single thread by ID from persistent storage.
#[tauri::command]
pub fn thread_load(
    id: String,
    ctx: State<'_, ApplicationContext>,
) -> Result<Option<Thread>, String> {
    thread_load_impl(&ctx, &id)
}

pub(crate) fn thread_load_impl(ctx: &ApplicationContext, id: &str) -> Result<Option<Thread>, String> {
    let store = get_conversation_store(ctx)?;
    let uuid = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid thread ID: {}", e))?;
    match store.load(uuid) {
        Ok(thread) => Ok(Some(thread)),
        Err(e) => match &e {
            nabu_core::conversations::PersistenceError::ThreadNotFound { .. } => Ok(None),
            _ => Err(e.to_string()),
        },
    }
}

/// Loads all persisted threads from disk.
#[tauri::command]
pub fn thread_list(ctx: State<'_, ApplicationContext>) -> Result<Vec<Thread>, String> {
    thread_list_impl(&ctx)
}

pub(crate) fn thread_list_impl(ctx: &ApplicationContext) -> Result<Vec<Thread>, String> {
    let store = get_conversation_store(ctx)?;
    let threads = store.list();
    if threads.is_empty() {
        store.load_all().map_err(|e| e.to_string())
    } else {
        Ok(threads)
    }
}

/// Deletes a thread from persistent storage.
#[tauri::command]
pub fn thread_delete(
    id: String,
    ctx: State<'_, ApplicationContext>,
) -> Result<(), String> {
    thread_delete_impl(&ctx, &id)
}

pub(crate) fn thread_delete_impl(ctx: &ApplicationContext, id: &str) -> Result<(), String> {
    let store = get_conversation_store(ctx)?;
    let uuid = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid thread ID: {}", e))?;
    store.delete(uuid).map_err(|e| e.to_string())
}

/// Updates an existing thread in place.
#[tauri::command]
pub fn thread_update(
    thread: Thread,
    ctx: State<'_, ApplicationContext>,
) -> Result<(), String> {
    thread_update_impl(&ctx, thread)
}

pub(crate) fn thread_update_impl(ctx: &ApplicationContext, mut thread: Thread) -> Result<(), String> {
    let store = get_conversation_store(ctx)?;
    store.update(&mut thread).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nabu_core::diagnostic::{
        Diagnostic, DiagnosticBatch, DiagnosticPlatformError, DiagnosticSeverity, TextPosition,
        TextRange,
    };
    use nabu_core::graph::VaultGraph;
    use nabu_core::indexer::Indexer;

    #[test]
    fn diagnostic_request_serializes_with_explicit_origin() {
        let req = DiagnosticRequest {
            text: "hello world".to_string(),
            resource_id: "buffer://1".to_string(),
            origin: Some("harper".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: DiagnosticRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text, "hello world");
        assert_eq!(back.resource_id, "buffer://1");
        assert_eq!(back.origin.as_deref(), Some("harper"));
    }

    #[test]
    fn diagnostic_request_serializes_with_none_origin() {
        let req = DiagnosticRequest {
            text: "test".to_string(),
            resource_id: "note://abc".to_string(),
            origin: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"origin\""));
        let back: DiagnosticRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.origin, None);
    }

    #[test]
    fn diagnostic_request_deserializes_missing_origin_as_none() {
        let json = r#"{"text":"hi","resource_id":"buf://1"}"#;
        let back: DiagnosticRequest = serde_json::from_str(json).unwrap();
        assert_eq!(back.text, "hi");
        assert_eq!(back.resource_id, "buf://1");
        assert_eq!(back.origin, None);
    }

    #[test]
    fn diagnostic_response_serializes_and_roundtrips() {
        let diag = Diagnostic::new(
            DiagnosticSeverity::Warning,
            TextRange::new(
                TextPosition::new(0, 0),
                TextPosition::new(0, 5),
            ),
            "found an issue".to_string(),
        );
        let batch = DiagnosticBatch::new("harper", "buf://1", vec![diag]);
        let style_map = nabu_core::diagnostic::DiagnosticStyleMap::default();
        let resp = DiagnosticResponse { batch, style_map };
        let json = serde_json::to_string(&resp).unwrap();
        let back: DiagnosticResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.batch.origin, "harper");
        assert_eq!(back.batch.diagnostic_count(), 1);
    }

    #[test]
    fn diagnostic_platform_error_serializes() {
        let err = DiagnosticPlatformError::ProviderNotFound {
            origin: "custom".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: DiagnosticPlatformError = serde_json::from_str(&json).unwrap();
        match back {
            DiagnosticPlatformError::ProviderNotFound { origin } => {
                assert_eq!(origin, "custom");
            }
            _ => panic!("expected ProviderNotFound"),
        }
    }

    #[test]
    fn diagnostic_request_default_has_none_origin() {
        let req: DiagnosticRequest = DiagnosticRequest {
            text: String::new(),
            resource_id: String::new(),
            origin: None,
        };
        assert_eq!(req.origin, None);
    }

    // ── Phase 1B-1 smoke tests: real Indexer / VaultGraph / StorageManager ──

    /// Creates a unique throwaway vault directory.
    fn temp_vault() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nabu-smoke-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Builds a minimal real ApplicationContext (EventBus + StorageManager +
    /// Indexer + VaultGraph) with the production ITEM_STORED / INDEX_UPDATED
    /// subscribers wired, mirroring `build_application_context` in lib.rs.
    fn test_context(vault: &Path) -> ApplicationContext {
        let event_bus: Arc<nabu_core::event_bus::EventBus<
            nabu_core::event_bus::PipelineEvent,
        >> = Arc::new(nabu_core::event_bus::EventBus::new());

        // Register built-in capabilities so capability_list tests pass.
        let mut capability_registry = nabu_core::plugin::capability::CapabilityRegistry::new();
        capability_registry.register_builtin();

        let ctx = ApplicationContext::builder()
            .with_event_bus(event_bus.clone())
            .with_capability_registry(capability_registry)
            .build();

        let storage = Arc::new(StorageManager::with_event_bus(
            vault.to_path_buf(),
            (*event_bus).clone(),
        ));
        ctx.register("storage_manager", storage.clone());

        let indexer = Arc::new(std::sync::Mutex::new(Indexer::with_event_bus(
            (*event_bus).clone(),
        )));
        ctx.register("indexer", indexer.clone());

        let graph = Arc::new(std::sync::RwLock::new(
            VaultGraph::with_persistence(Some((*event_bus).clone()), vault.to_path_buf())
                .unwrap(),
        ));
        ctx.register("vault_graph", graph.clone());

        let history_mgr = Arc::new(std::sync::RwLock::new(nabu_core::history::HistoryManager::new()));
        ctx.register("history_manager", history_mgr);

        let conv_store = Arc::new(ConversationStore::new(vault.join(".nabu").join("conversations")));
        ctx.register("conversation_store", conv_store);

        let diag_platform = Arc::new(nabu_core::diagnostic::DiagnosticPlatform::new());
        ctx.register("diagnostic_platform", diag_platform);

        let perf_monitor = Arc::new(nabu_core::diagnostics::PerformanceMonitor::new());
        ctx.register("performance_monitor", perf_monitor);

        let stream_manager = Arc::new(StreamManager::new(event_bus.clone()));
        ctx.register("stream_manager", stream_manager);

        let executors = Arc::new(nabu_core::jobs::workers::ExecutorRegistry::new());
        let queue = Arc::new(
            nabu_core::jobs::DurableJobQueue::new(vault.join(".nabu").join("jobs"))
                .expect("job queue"),
        );
        let pool = Arc::new(nabu_core::jobs::workers::WorkerPool::new(2, queue, executors));
        ctx.register("worker_pool", pool);

        // ITEM_STORED → index + graph (mirrors the production subscriber).
        {
            let s = storage.clone();
            let i = indexer.clone();
            let g = graph.clone();
            event_bus.subscribe(
                nabu_core::event_bus::kinds::ITEM_STORED,
                move |event: &nabu_core::event_bus::PipelineEvent| {
                    if let nabu_core::event_bus::PipelineEvent::ItemStored(stored) = event {
                        if let Some(obj) = s.load(stored.object_id) {
                            if let Ok(idx) = i.lock() {
                                let _ = idx.index_object(&obj);
                            }
                            if let Ok(gr) = g.write() {
                                let _ = gr.update_node(&obj);
                            }
                        }
                    }
                },
            );
        }
        // INDEX_UPDATED(Removed) → remove from index + graph.
        {
            let i = indexer.clone();
            let g = graph.clone();
            event_bus.subscribe(
                nabu_core::event_bus::kinds::INDEX_UPDATED,
                move |event: &nabu_core::event_bus::PipelineEvent| {
                    if let nabu_core::event_bus::PipelineEvent::IndexUpdated(updated) = event {
                        if matches!(
                            updated.operation,
                            nabu_core::event_bus::IndexOperation::Removed
                        ) {
                            if let Ok(idx) = i.lock() {
                                let _ = idx.remove_object(updated.object_id);
                            }
                            if let Ok(gr) = g.write() {
                                let _ = gr.remove_node(updated.object_id);
                            }
                        }
                    }
                },
            );
        }

        ctx
    }

    /// Creates a throwaway settings file path in a temp directory.
    fn temp_settings_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nabu-settings-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    /// Creates a SettingsStore pointed at the given vault path.
    fn test_settings_store(vault: &Path) -> SettingsStore {
        let store = SettingsStore::new(temp_settings_path());
        store.set(AppSettings {
            last_vault_path: vault.to_string_lossy().to_string(),
            ..AppSettings::default()
        });
        store
    }

    #[test]
    fn notes_search_finds_body_only_term_via_real_indexer() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();

        manager
            .save_note_content("alpha.md", "hello distinctive body")
            .unwrap();

        // A body-only, non-title term must be found through the real Indexer.
        let hits = notes_search_impl(&ctx, "distinctive").unwrap();
        assert!(
            hits.iter().any(|h| h.path == "alpha.md"),
            "expected hit for alpha.md, got {:?}",
            hits.iter().map(|h| &h.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn graph_data_includes_wikilink_edge() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();

        manager.save_note_content("B.md", "note b").unwrap();
        manager.save_note_content("A.md", "link to [[B]]").unwrap();

        // The A→B content-derived edge must surface from the real VaultGraph.
        let data = graph_data_impl(&ctx).unwrap();
        assert!(
            data.edges
                .iter()
                .any(|e| e.source == "A.md" && e.target == "B.md"),
            "expected A.md→B.md edge, got {:?}",
            data.edges
                .iter()
                .map(|e| (e.source.clone(), e.target.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn storage_move_and_delete_keep_index_and_graph_consistent() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();

        manager
            .save_note_content("x.md", "rename-me marker")
            .unwrap();
        let id = manager.find_by_path("x.md").unwrap().id;

        // Move through the canonical StorageManager; index/graph follow via
        // ITEM_STORED so the content term resolves under the new path.
        manager.move_object(id, "y.md").unwrap();
        assert!(manager.find_by_path("y.md").is_some());
        assert!(manager.find_by_path("x.md").is_none());
        let hits = notes_search_impl(&ctx, "marker").unwrap();
        assert!(hits.iter().any(|h| h.path == "y.md"));

        // Delete through the canonical StorageManager; INDEX_UPDATED(Removed)
        // removes it from both the search index and the graph.
        manager.delete(id).unwrap();
        let hits = notes_search_impl(&ctx, "marker").unwrap();
        assert!(!hits.iter().any(|h| h.path == "y.md"));
        let data = graph_data_impl(&ctx).unwrap();
        assert!(!data.nodes.iter().any(|n| n.path == "y.md"));
    }

    #[test]
    fn inbox_approve_files_capture_to_real_markdown() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();

        // A freshly captured inbox item (pending status).
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("captured body".to_string()),
            Some("capture"),
        );
        manager.save(&obj).unwrap();
        let id = obj.id;

        // FilingService is exactly what `inbox_approve` delegates to.
        let service = nabu_core::inbox::FilingService::new(manager.clone());
        service.approve(id).unwrap();

        // Approving files the capture into a real `.md` artifact on disk.
        let filed = manager.load(id).unwrap();
        let path = filed.metadata.vault_path.as_deref().expect("vault path");
        assert!(path.ends_with(".md"), "expected a .md artifact, got {}", path);
        assert!(dir.join(path).exists(), "filed markdown missing on disk");
    }

    #[test]
    fn batch_approve_propagates_first_real_error() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();

        // A genuinely valid item.
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("ok".to_string()),
            Some("ok"),
        );
        manager.save(&obj).unwrap();

        // A bogus id that does not exist → the batch must propagate the error
        // instead of silently swallowing it and reporting success.
        let bogus = uuid::Uuid::new_v4().to_string();
        let ids = vec![obj.id.to_string(), bogus];
        let result = inbox_batch_approve_impl(&ctx, &ids);
        assert!(result.is_err(), "batch should surface the failing item");
    }

    // ── Settings command tests ───────────────────────────────────────

    #[test]
    fn settings_set_then_get_returns_value() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        settings_set_impl(&store, "theme", serde_json::json!("dark")).unwrap();
        let val = settings_get_impl(&store, "theme").unwrap();
        assert_eq!(val, serde_json::json!("dark"));
    }

    #[test]
    fn settings_set_all_overwrites() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let new_settings = AppSettings {
            last_vault_path: vault.to_string_lossy().to_string(),
            font_size: 18.0,
            ..AppSettings::default()
        };
        settings_set_all_impl(&store, &new_settings).unwrap();
        let got = get_settings_impl(&store).unwrap();
        assert_eq!(got.font_size, 18.0);
    }

    #[test]
    fn settings_export_import_roundtrip() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        settings_set_impl(&store, "custom_key", serde_json::json!(42)).unwrap();
        let exported = settings_export_impl(&store).unwrap();
        let new_store = SettingsStore::new(temp_settings_path());
        let imported = settings_import_impl(&new_store, &exported).unwrap();
        assert_eq!(
            settings_get_impl(&new_store, "custom_key").unwrap(),
            serde_json::json!(42)
        );
        assert_eq!(imported.extra_settings.get("custom_key"), Some(&serde_json::json!(42)));
    }

    #[test]
    fn settings_reset_clears_extra_settings() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        settings_set_impl(&store, "to_delete", serde_json::json!(true)).unwrap();
        settings_reset_impl(&store).unwrap();
        let after = get_settings_impl(&store).unwrap();
        assert!(!after.extra_settings.contains_key("to_delete"));
    }

    #[test]
    fn settings_get_missing_key_returns_null() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let val = settings_get_impl(&store, "nonexistent").unwrap();
        assert!(val.is_null());
    }

    // ── Template command tests ──

    #[test]
    fn template_crud_lifecycle() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let tmpl = TemplateRecord {
            name: "Meeting Notes".to_string(),
            description: Some("For meetings".to_string()),
            icon: None,
            default_folder: None,
            category: None,
            favourite: false,
            frontmatter_defaults: Default::default(),
            property_presets: Default::default(),
            body: "# {{title}}\n\n## Agenda\n\n## Notes\n".to_string(),
            object_type: None,
        };
        template_save_impl(&store, tmpl.clone()).unwrap();
        let list = template_list_impl(&store).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Meeting Notes");

        let mut updated = tmpl.clone();
        updated.description = Some("Updated".to_string());
        template_save_impl(&store, updated).unwrap();
        let list = template_list_impl(&store).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].description.as_deref(), Some("Updated"));

        template_delete_impl(&store, "Meeting Notes").unwrap();
        let list = template_list_impl(&store).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn template_duplicate_creates_unique_copy() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        template_save_impl(&store, TemplateRecord {
            name: "Template A".to_string(),
            description: None,
            icon: None,
            default_folder: None,
            category: None,
            favourite: true,
            frontmatter_defaults: Default::default(),
            property_presets: Default::default(),
            body: "Original".to_string(),
            object_type: None,
        }).unwrap();
        let cloned = template_duplicate_impl(&store, "Template A").unwrap();
        assert!(cloned.name.starts_with("Template A Copy"));
        assert!(!cloned.favourite);
        assert_eq!(cloned.body, "Original");

        let cloned2 = template_duplicate_impl(&store, "Template A").unwrap();
        assert_ne!(cloned.name, cloned2.name);
    }

    #[test]
    fn template_set_favourite_toggles() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        template_save_impl(&store, TemplateRecord {
            name: "Task".to_string(),
            description: None,
            icon: None,
            default_folder: None,
            category: None,
            favourite: false,
            frontmatter_defaults: Default::default(),
            property_presets: Default::default(),
            body: "{{content}}".to_string(),
            object_type: None,
        }).unwrap();
        template_set_favourite_impl(&store, "Task", true).unwrap();
        let list = template_list_impl(&store).unwrap();
        assert!(list.iter().any(|t| t.name == "Task" && t.favourite));
    }

    // ── Smart Folder tests ──

    #[test]
    fn smart_folder_crud_lifecycle() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let folder = SmartFolder {
            id: "inbox-important".to_string(),
            name: "Important".to_string(),
            icon: "tag".to_string(),
            query: "tag:important".to_string(),
            pinned: false,
        };
        smart_folder_save_impl(&store, folder.clone()).unwrap();
        let list = smart_folders_list_impl(&store).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "inbox-important");

        let mut updated = folder.clone();
        updated.query = "tag:important tag:review".to_string();
        smart_folder_save_impl(&store, updated).unwrap();
        let list = smart_folders_list_impl(&store).unwrap();
        assert_eq!(list[0].query, "tag:important tag:review");

        smart_folder_delete_impl(&store, "inbox-important").unwrap();
        let list = smart_folders_list_impl(&store).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn smart_folder_evaluate_empty_query_returns_all() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        std::fs::write(vault.join("a.md"), "# Note A\ntag:work").unwrap();
        std::fs::write(vault.join("b.md"), "# Note B\ntag:personal").unwrap();
        let results = smart_folder_evaluate_impl(&store, "").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn smart_folder_evaluate_tag_filter() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        std::fs::write(
            vault.join("a.md"),
            "---\ntags: [work]\n---\n# Note A\n",
        ).unwrap();
        std::fs::write(
            vault.join("b.md"),
            "---\ntags: [personal]\n---\n# Note B\n",
        ).unwrap();
        std::fs::write(
            vault.join("c.md"),
            "---\ntags: [Work, project]\n---\n# Note C\n",
        ).unwrap();
        let results = smart_folder_evaluate_impl(&store, "tag:work").unwrap();
        assert_eq!(results.len(), 2);
    }

    // ── Canvas tests ──

    #[test]
    fn canvas_crud_lifecycle() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let canvas = CanvasDef {
            id: "canvas-1".to_string(),
            name: "My Canvas".to_string(),
            nodes: vec![],
            edges: vec![],
            groups: vec![],
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        };
        canvas_save_impl(&store, canvas.clone()).unwrap();
        let list = canvas_list_impl(&store).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "My Canvas");

        let got = canvas_get_impl(&store, "canvas-1").unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().name, "My Canvas");

        let mut updated = canvas.clone();
        updated.name = "Renamed Canvas".to_string();
        canvas_save_impl(&store, updated).unwrap();
        let list = canvas_list_impl(&store).unwrap();
        assert_eq!(list[0].name, "Renamed Canvas");

        canvas_delete_impl(&store, "canvas-1").unwrap();
        let list = canvas_list_impl(&store).unwrap();
        assert!(list.is_empty());
    }

    // ── Archive tests ──

    #[test]
    fn archive_note_moves_to_archive_folder() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        std::fs::write(dir.join("original.md"), "# Hello").unwrap();
        archive_note_impl(&ctx, &store, "original.md").unwrap();
        assert!(!dir.join("original.md").exists());
        assert!(dir.join("archive").join("original.md").exists());
    }

    #[test]
    fn archive_note_rejects_path_traversal() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        let result = archive_note_impl(&ctx, &store, "../escape.md");
        assert!(result.is_err());
    }

    #[test]
    fn archive_note_rejects_archive_folder_path() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        let result = archive_note_impl(&ctx, &store, "archive/note.md");
        assert!(result.is_err());
    }

    #[test]
    fn archive_restore_returns_to_original_location() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        std::fs::write(dir.join("to_archive.md"), "# Restore Me").unwrap();
        archive_note_impl(&ctx, &store, "to_archive.md").unwrap();
        archive_restore_impl(&ctx, &store, "archive/to_archive.md").unwrap();
        assert!(dir.join("to_archive.md").exists());
        assert!(!dir.join("archive").join("to_archive.md").exists());
    }

    #[test]
    fn archive_list_returns_archived_notes() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        std::fs::write(dir.join("archived1.md"), "# Note 1").unwrap();
        archive_note_impl(&ctx, &store, "archived1.md").unwrap();
        let list = archive_list_impl(&store).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].original_path, "archived1.md");
    }

    // ── Calendar & Daily Notes tests ──

    #[test]
    fn daily_note_creates_with_default_content() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let path = daily_note_for_impl(&store, "2024-01-15").unwrap();
        assert_eq!(path, "2024-01-15.md");
        assert!(vault.join("2024-01-15.md").exists());
    }

    #[test]
    fn daily_note_returns_existing_without_overwriting() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        std::fs::write(vault.join("2024-01-15.md"), "# My Notes").unwrap();
        daily_note_for_impl(&store, "2024-01-15").unwrap();
        let content = std::fs::read_to_string(vault.join("2024-01-15.md")).unwrap();
        assert_eq!(content, "# My Notes");
    }

    #[test]
    fn daily_note_rejects_invalid_date() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let result = daily_note_for_impl(&store, "invalid");
        assert!(result.is_err());
    }

    #[test]
    fn calendar_notes_filters_by_month() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        std::fs::write(
            vault.join("2024-01-15.md"),
            "---\ncreated: 2024-01-15\n---\n# Jan Note\n",
        ).unwrap();
        std::fs::write(
            vault.join("2024-02-20.md"),
            "---\ncreated: 2024-02-20\n---\n# Feb Note\n",
        ).unwrap();
        let results = calendar_notes_impl(&store, "2024-01").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "2024-01-15.md");
    }

    #[test]
    fn calendar_notes_empty_for_empty_vault() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let results = calendar_notes_impl(&store, "2024-01").unwrap();
        assert!(results.is_empty());
    }

    // ── Notes diff tests ──

    #[test]
    fn notes_diff_shows_changes() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        std::fs::write(vault.join("a.md"), "# Hello\nWorld\n").unwrap();
        std::fs::write(vault.join("b.md"), "# Hello\nWorld\nNew Line\n").unwrap();
        let diff = notes_diff_impl(&store, "a.md", "b.md").unwrap();
        assert!(!diff.is_empty());
    }

    #[test]
    fn notes_diff_empty_for_identical_notes() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        std::fs::write(vault.join("a.md"), "# Same\n").unwrap();
        std::fs::write(vault.join("b.md"), "# Same\n").unwrap();
        let diff = notes_diff_impl(&store, "a.md", "b.md").unwrap();
        assert!(diff.iter().all(|r| matches!(r.kind, crate::recovery::DiffKind::Same)));
    }

    // ── Tree list tests ──

    #[test]
    fn tree_list_returns_markdown_files() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        std::fs::write(vault.join("note1.md"), "# Note 1").unwrap();
        std::fs::create_dir_all(vault.join("subfolder")).unwrap();
        std::fs::write(vault.join("subfolder").join("note2.md"), "# Note 2").unwrap();
        let entries = tree_list_impl(&store).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.path == "note1.md"));
        let subfolder = entries.iter().find(|e| e.path == "subfolder").unwrap();
        assert!(subfolder.is_folder);
        assert!(subfolder.children.iter().any(|c| c.path == "subfolder/note2.md"));
    }

    #[test]
    fn tree_list_empty_when_no_vault() {
        let store = SettingsStore::new(temp_settings_path());
        let entries = tree_list_impl(&store).unwrap();
        assert!(entries.is_empty());
    }

    // ── Check vault exists tests ──

    #[test]
    fn check_vault_exists_returns_path_when_valid() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let result = check_vault_exists_impl(&store).unwrap();
        assert_eq!(result.as_deref(), Some(vault.to_str().unwrap()));
    }

    #[test]
    fn check_vault_exists_returns_none_for_empty_path() {
        let store = SettingsStore::new(temp_settings_path());
        let result = check_vault_exists_impl(&store).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn check_vault_exists_returns_none_for_nonexistent_path() {
        let store = SettingsStore::new(temp_settings_path());
        store.update(|s| {
            s.last_vault_path = "/nonexistent/path/that/should/not/exist".to_string();
        }).unwrap();
        let result = check_vault_exists_impl(&store).unwrap();
        assert!(result.is_none());
    }

    // ── Capability command tests ──

    #[test]
    fn capability_list_returns_built_in_capabilities() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let caps = capability_list_impl(&ctx).unwrap();
        assert!(!caps.is_empty());
    }

    #[test]
    fn capability_list_with_state_returns_summaries() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let summaries = capability_list_with_state_impl(&ctx).unwrap();
        assert!(!summaries.is_empty());
    }

    #[test]
    fn capability_enable_empty_id_rejected() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let result = capability_enable_impl(&ctx, "");
        assert!(result.is_err());
    }

    #[test]
    fn capability_disable_empty_id_rejected() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let result = capability_disable_impl(&ctx, "");
        assert!(result.is_err());
    }

    #[test]
    fn capability_enable_unknown_id_errors() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let result = capability_enable_impl(&ctx, "nonexistent:capability");
        assert!(result.is_err());
    }

    // ── Health / Metrics / Pool tests ──

    #[test]
    fn health_check_returns_service_health() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let health = health_check_impl(&ctx).unwrap();
        assert!(health.registered_services > 0);
    }

    #[test]
    fn metrics_returns_runtime_metrics() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let metrics = metrics_impl(&ctx).unwrap();
        assert!(metrics.timers.len() >= 0);
    }

    #[test]
    fn pool_health_returns_worker_pool_info() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let health = pool_health_impl(&ctx).unwrap();
        assert_eq!(health.worker_count, 2);
    }

    // ── Thread command tests ──

    #[test]
    fn thread_save_and_load_roundtrip() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let mut thread = Thread::new();
        thread.title = Some("Test Thread".to_string());
        thread_save_impl(&ctx, thread.clone()).unwrap();
        let loaded = thread_load_impl(&ctx, &thread.id.to_string()).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().title.as_deref(), Some("Test Thread"));
    }

    #[test]
    fn thread_list_returns_saved_threads() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let mut t1 = Thread::new();
        t1.title = Some("Thread 1".to_string());
        let mut t2 = Thread::new();
        t2.title = Some("Thread 2".to_string());
        thread_save_impl(&ctx, t1).unwrap();
        thread_save_impl(&ctx, t2).unwrap();
        let list = thread_list_impl(&ctx).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn thread_delete_removes_thread() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let mut thread = Thread::new();
        thread.title = Some("To Delete".to_string());
        thread_save_impl(&ctx, thread.clone()).unwrap();
        thread_delete_impl(&ctx, &thread.id.to_string()).unwrap();
        let loaded = thread_load_impl(&ctx, &thread.id.to_string()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn thread_load_nonexistent_returns_none() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let bogus = uuid::Uuid::new_v4().to_string();
        let result = thread_load_impl(&ctx, &bogus);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn thread_load_invalid_id_returns_error() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let result = thread_load_impl(&ctx, "not-a-uuid");
        assert!(result.is_err());
    }

    // ── History command tests ──

    #[test]
    fn history_status_empty_when_no_entries() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let status = crate::history::history_status_impl(&ctx).unwrap();
        assert!(!status.can_undo);
        assert!(!status.can_redo);
        assert_eq!(status.undo_len, 0);
    }

    #[test]
    fn history_undo_redo_roundtrip() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        crate::history::push_history(
            &ctx,
            nabu_core::history::HistoryOp::NoteCreate,
            "Test Op".to_string(),
            vec!["note.md".to_string()],
            serde_json::json!({}),
            serde_json::json!({}),
            std::sync::Arc::new(|| Ok(())),
            std::sync::Arc::new(|| Ok(())),
        ).unwrap();
        let status = crate::history::history_status_impl(&ctx).unwrap();
        assert!(status.can_undo);
        assert_eq!(status.undo_len, 1);
        let undo_label = crate::history::history_undo_impl(&ctx).unwrap();
        assert_eq!(undo_label.as_deref(), Some("Test Op"));
        let status = crate::history::history_status_impl(&ctx).unwrap();
        assert!(status.can_redo);
        let redo_label = crate::history::history_redo_impl(&ctx).unwrap();
        assert_eq!(redo_label.as_deref(), Some("Test Op"));
    }

    #[test]
    fn history_clear_resets_stacks() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        crate::history::push_history(
            &ctx,
            nabu_core::history::HistoryOp::NoteCreate,
            "Op".to_string(),
            vec![],
            serde_json::json!({}),
            serde_json::json!({}),
            std::sync::Arc::new(|| Ok(())),
            std::sync::Arc::new(|| Ok(())),
        ).unwrap();
        crate::history::history_clear_impl(&ctx).unwrap();
        let status = crate::history::history_status_impl(&ctx).unwrap();
        assert!(!status.can_undo);
    }

    #[test]
    fn history_set_depth_limits() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        crate::history::history_set_depth_impl(&ctx, 3).unwrap();
        let status = crate::history::history_status_impl(&ctx).unwrap();
        assert_eq!(status.max_depth, 3);
    }

    // ── Inbox reject/retry/delete tests ──

    #[test]
    fn inbox_reject_marks_item_rejected() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("reject me".to_string()),
            Some("capture"),
        );
        manager.save(&obj).unwrap();
        let id = obj.id.to_string();
        inbox_reject_impl(&ctx, &id, "spam").unwrap();
        let loaded = manager.load(obj.id).unwrap();
        assert_eq!(custom_text(&loaded, "inbox_status").as_deref(), Some("rejected"));
    }

    #[test]
    fn inbox_retry_resets_to_pending() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("retry me".to_string()),
            Some("capture"),
        );
        manager.save(&obj).unwrap();
        let id = obj.id.to_string();
        inbox_reject_impl(&ctx, &id, "bad").unwrap();
        inbox_retry_impl(&ctx, &id).unwrap();
        let loaded = manager.load(obj.id).unwrap();
        assert_eq!(custom_text(&loaded, "inbox_status").as_deref(), Some("pending"));
    }

    #[test]
    fn inbox_delete_removes_object() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("delete me".to_string()),
            Some("capture"),
        );
        manager.save(&obj).unwrap();
        inbox_delete_impl(&ctx, &obj.id.to_string()).unwrap();
        assert!(manager.load(obj.id).is_none());
    }

    #[test]
    fn inbox_batch_reject_applies_to_all() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        let mut ids = Vec::new();
        for i in 0..3 {
            let title = format!("capture{i}");
            let obj = nabu_core::inbox::model::build_inbox_object(
                nabu_core::models::ObjectContent::Markdown(format!("batch {i}")),
                Some(&title),
            );
            manager.save(&obj).unwrap();
            ids.push(obj.id.to_string());
        }
        inbox_batch_reject_impl(&ctx, &ids, "batch reason").unwrap();
        for id_str in &ids {
            let uuid = uuid::Uuid::parse_str(id_str).unwrap();
            let loaded = manager.load(uuid).unwrap();
            assert_eq!(custom_text(&loaded, "inbox_status").as_deref(), Some("rejected"));
        }
    }

    // ── Queue command tests ──

    #[test]
    fn queue_set_status_changes_reading_status() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("queue item".to_string()),
            Some("capture"),
        );
        manager.save(&obj).unwrap();
        queue_set_status_impl(&ctx, &obj.id.to_string(), "completed").unwrap();
        let loaded = manager.load(obj.id).unwrap();
        assert_eq!(
            custom_text(&loaded, "reading_status").as_deref(),
            Some("completed")
        );
    }

    #[test]
    fn queue_set_priority_changes_priority() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("priority item".to_string()),
            Some("capture"),
        );
        manager.save(&obj).unwrap();
        queue_set_priority_impl(&ctx, &obj.id.to_string(), "high").unwrap();
        let loaded = manager.load(obj.id).unwrap();
        assert_eq!(
            custom_text(&loaded, "reading_priority").as_deref(),
            Some("high")
        );
    }

    #[test]
    fn queue_set_progress_clamps_to_range() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        let obj = nabu_core::inbox::model::build_inbox_object(
            nabu_core::models::ObjectContent::Markdown("progress item".to_string()),
            Some("capture"),
        );
        manager.save(&obj).unwrap();
        queue_set_progress_impl(&ctx, &obj.id.to_string(), 150.0).unwrap();
        let loaded = manager.load(obj.id).unwrap();
        let progress = loaded
            .custom_properties
            .get("reading_progress")
            .and_then(|v| match v {
                CustomPropertyValue::Number(n) => Some(*n),
                _ => None,
            });
        assert_eq!(progress, Some(1.0_f64));
    }

    // ── Mention ignore tests ──

    #[test]
    fn mention_ignore_adds_to_list() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        mention_ignore_impl(&store, "Some Note").unwrap();
        let list = mention_ignore_list_impl(&store).unwrap();
        assert!(list.iter().any(|t| *t == "Some Note"));
    }

    #[test]
    fn mention_ignore_deduplicates() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        mention_ignore_impl(&store, "Dup Note").unwrap();
        mention_ignore_impl(&store, "Dup Note").unwrap();
        let list = mention_ignore_list_impl(&store).unwrap();
        assert_eq!(list.iter().filter(|t| **t == "Dup Note").count(), 1);
    }

    #[test]
    fn mention_ignore_list_empty_by_default() {
        let vault = temp_vault();
        let store = test_settings_store(&vault);
        let list = mention_ignore_list_impl(&store).unwrap();
        assert!(list.is_empty());
    }

    // ── note_create_file tests ──

    #[test]
    fn note_create_file_writes_content() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        note_create_file_impl(&ctx, &store, "new_note.md", "# Hello World").unwrap();
        let content = std::fs::read_to_string(dir.join("new_note.md")).unwrap();
        assert_eq!(content, "# Hello World");
    }

    #[test]
    fn note_create_file_rejects_traversal() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        let result = note_create_file_impl(&ctx, &store, "../escape.md", "content");
        assert!(result.is_err());
    }

    #[test]
    fn note_create_file_creates_subdirectories() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        note_create_file_impl(&ctx, &store, "sub/deep/note.md", "deep content").unwrap();
        assert!(dir.join("sub/deep/note.md").exists());
    }

    #[test]
    fn note_create_file_can_be_undone() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let store = test_settings_store(&dir);
        note_create_file_impl(&ctx, &store, "undoable.md", "content").unwrap();
        assert!(dir.join("undoable.md").exists());
        crate::history::history_undo_impl(&ctx).unwrap();
        assert!(!dir.join("undoable.md").exists());
    }

    // ── notes_index tests ──

    #[test]
    fn notes_index_includes_saved_notes() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let manager = ctx.storage_manager().unwrap();
        manager.save_note_content("a.md", "alpha").unwrap();
        manager.save_note_content("b.md", "beta").unwrap();
        let entries = notes_index_impl(&ctx).unwrap();
        assert!(entries.iter().any(|e| e.path == "a.md"));
        assert!(entries.iter().any(|e| e.path == "b.md"));
    }

    #[test]
    fn notes_index_empty_for_empty_vault() {
        let dir = temp_vault();
        let ctx = test_context(&dir);
        let entries = notes_index_impl(&ctx).unwrap();
        assert!(entries.is_empty());
    }
}

// ── Streaming IPC ───────────────────────────────────────────────────────────

/// Cancels an active streaming session by ID.
///
/// Delegates to [`StreamManager::cancel_stream`], which publishes a
/// `StreamEvent::Cancelled` and `StreamSessionEvent::SessionCancelled`
/// through the EventBus. The frontend `StreamingProvider` picks up the
/// terminal event and transitions the session UI to the `Cancelled` state.
///
/// Returns `Ok(true)` if the stream was found and cancelled, `Ok(false)` if
/// no stream with the given ID exists.
#[tauri::command]
pub async fn stream_cancel(
    ctx: State<'_, ApplicationContext>,
    stream_id: String,
    reason: String,
) -> Result<bool, String> {
    stream_cancel_impl(&ctx, &stream_id, reason).await
}

pub(crate) async fn stream_cancel_impl(
    ctx: &ApplicationContext,
    stream_id: &str,
    reason: String,
) -> Result<bool, String> {
    let manager: Arc<StreamManager> = ctx
        .resolve("stream_manager")
        .ok_or_else(|| "StreamManager is not registered in the application context".to_string())?;

    let id = uuid::Uuid::parse_str(stream_id)
        .map_err(|e| format!("Invalid stream id: {e}"))?;

    match manager.cancel_stream(&id, reason) {
        Ok(()) => Ok(true),
        Err(nabu_core::streaming::StreamManagerError::StreamNotFound(_)) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

// ── ACP Session IPC ─────────────────────────────────────────────────────────

/// Response returned by the `acp_connect` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConnectResult {
    /// The Nabu thread UUID for this session.
    thread_id: Uuid,
    /// The ACP session ID assigned by the agent.
    session_id: String,
    /// The negotiated protocol version.
    protocol_version: nabu_core::acp::types::ProtocolVersion,
}

/// Connects to an external ACP agent via stdio.
///
/// Spawns the agent subprocess, initialises the ACP protocol, and creates a
/// new session.  Token chunks from the agent are streamed to the frontend
/// through Nabu's existing `StreamingPipeline` → EventBus → EventBridge
/// pipeline, so the `StreamingProvider` picks them up automatically.
#[tauri::command]
pub async fn acp_connect(
    ctx: State<'_, ApplicationContext>,
    config: nabu_core::agent::AcpConnectConfig,
) -> Result<AcpConnectResult, String> {
    acp_connect_impl(&ctx, &config).await
}

pub(crate) async fn acp_connect_impl(
    ctx: &ApplicationContext,
    config: &nabu_core::agent::AcpConnectConfig,
) -> Result<AcpConnectResult, String> {
    let manager = ctx
        .resolve::<nabu_core::agent::AcpSessionManager>("acp_session_manager")
        .ok_or_else(|| "AcpSessionManager is not registered".to_string())?;

    let (thread_id, session_id) = manager.connect(config).await.map_err(|e| e.to_string())?;

    Ok(AcpConnectResult {
        thread_id,
        session_id,
        protocol_version: nabu_core::acp::types::SUPPORTED_PROTOCOL_VERSION,
    })
}

/// Sends a user message to the active ACP session.
#[tauri::command]
pub async fn acp_send_message(
    ctx: State<'_, ApplicationContext>,
    thread_id: String,
    message: String,
) -> Result<(), String> {
    acp_send_message_impl(&ctx, thread_id, message).await
}

pub(crate) async fn acp_send_message_impl(
    ctx: &ApplicationContext,
    thread_id: String,
    message: String,
) -> Result<(), String> {
    let manager = ctx
        .resolve::<nabu_core::agent::AcpSessionManager>("acp_session_manager")
        .ok_or_else(|| "AcpSessionManager is not registered".to_string())?;

    let thread_uuid = uuid::Uuid::parse_str(&thread_id)
        .map_err(|e| format!("Invalid thread id: {e}"))?;

    manager.send_message(thread_uuid, message).await.map_err(|e| e.to_string())
}

/// Cancels the active prompt turn.
#[tauri::command]
pub async fn acp_cancel(
    ctx: State<'_, ApplicationContext>,
    thread_id: String,
) -> Result<(), String> {
    acp_cancel_impl(&ctx, thread_id).await
}

pub(crate) async fn acp_cancel_impl(
    ctx: &ApplicationContext,
    thread_id: String,
) -> Result<(), String> {
    let manager = ctx
        .resolve::<nabu_core::agent::AcpSessionManager>("acp_session_manager")
        .ok_or_else(|| "AcpSessionManager is not registered".to_string())?;

    let thread_uuid = uuid::Uuid::parse_str(&thread_id)
        .map_err(|e| format!("Invalid thread id: {e}"))?;

    manager.cancel(thread_uuid).await.map_err(|e| e.to_string())
}

/// Disconnects from an ACP session and terminates the agent process.
#[tauri::command]
pub async fn acp_disconnect(
    ctx: State<'_, ApplicationContext>,
    thread_id: String,
) -> Result<(), String> {
    acp_disconnect_impl(&ctx, thread_id).await
}

pub(crate) async fn acp_disconnect_impl(
    ctx: &ApplicationContext,
    thread_id: String,
) -> Result<(), String> {
    let manager = ctx
        .resolve::<nabu_core::agent::AcpSessionManager>("acp_session_manager")
        .ok_or_else(|| "AcpSessionManager is not registered".to_string())?;

    let thread_uuid = uuid::Uuid::parse_str(&thread_id)
        .map_err(|e| format!("Invalid thread id: {e}"))?;

    manager.disconnect(thread_uuid).await.map_err(|e| e.to_string())
}

/// Lists the session IDs known to the ACP session manager.
#[tauri::command]
pub async fn acp_list_sessions(
    ctx: State<'_, ApplicationContext>,
) -> Result<Vec<String>, String> {
    acp_list_sessions_impl(&ctx).await
}

pub(crate) async fn acp_list_sessions_impl(ctx: &ApplicationContext) -> Result<Vec<String>, String> {
    let manager = ctx
        .resolve::<nabu_core::agent::AcpSessionManager>("acp_session_manager")
        .ok_or_else(|| "AcpSessionManager is not registered".to_string())?;

    manager.list_sessions().await.map_err(|e| e.to_string())
}

/// Delivers a user's permission response to the awaiting ACP handler.
///
/// Called from the frontend after the user approves or denies a permission
/// request shown via an `AcpPermissionRequested` platform event. The
/// `thread_id` identifies the session; `request_id` matches the pending
/// oneshot in `NabuAcpHandler::request_permission`.
#[tauri::command]
pub async fn acp_permission_respond(
    ctx: State<'_, ApplicationContext>,
    thread_id: String,
    request_id: String,
    outcome: nabu_core::acp::types::PermissionOutcome,
) -> Result<(), String> {
    let manager = ctx
        .resolve::<nabu_core::agent::AcpSessionManager>("acp_session_manager")
        .ok_or_else(|| "AcpSessionManager is not registered".to_string())?;

    let thread_uuid = uuid::Uuid::parse_str(&thread_id)
        .map_err(|e| format!("Invalid thread id: {e}"))?;
    let request_uuid = uuid::Uuid::parse_str(&request_id)
        .map_err(|e| format!("Invalid request id: {e}"))?;

    manager
        .respond_to_permission(thread_uuid, request_uuid, outcome)
        .await
        .map_err(|e| e.to_string())
}

/// Response for the `acp_list_sessions` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpSessionSummary {
    pub session_id: String,
    pub thread_id: Uuid,
    pub agent_name: Option<String>,
}
