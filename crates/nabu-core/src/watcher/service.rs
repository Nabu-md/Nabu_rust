//! The vault filesystem watcher service.
//!
//! [`VaultWatcher`] wraps the platform `notify` backend and exposes a single,
//! normalized, debounced event stream of [`VaultEvent`]s over a
//! `tokio::sync::mpsc::UnboundedReceiver`.
//!
//! ## Architecture
//!
//! ```text
//!   notify backend (OS kernel events)
//!          │  handler pushes `Result<notify::Event>` into a `std::sync::mpsc`
//!          ▼  channel (notify's handler thread → processing thread)
//!   processing thread: normalize → debounce/coalesce → dedup → self-suppress
//!          │
//!          ▼  `tokio::sync::mpsc::UnboundedSender<VaultEvent>`
//!   consumer (Phase 1B: indexer/graph/UI) reads `UnboundedReceiver<VaultEvent>`
//! ```
//!
//! The processing loop runs on a dedicated POSIX thread (notify already spawns
//! an internal thread for the kernel callback, and a `std`-thread keeps the
//! watcher usable from non-async contexts such as a future Tauri command). It
//! only talks to the async world through an unbounded `tokio` channel whose
//! `send` is safe to call from a non-Tokio thread and which correctly wakes a
//! parked Tokio consumer.
//!
//! ## Normalization
//!
//! Raw OS events are noisy: one logical save can produce create + metadata +
//! data events within milliseconds. The watcher collapses these into the four
//! logical kinds in [`WatcherChangeKind`] and, for renames — which several
//! platforms report as two unrelated `Name` notifications (one for the path
//! that vanished, one for the path that appeared) — pairs them by directory
//! and filesystem existence into a single `Renamed` event carrying both paths.
//!
//! ## Self-event suppression
//!
//! Nabu mutates its own vault (saves, moves, deletes). To avoid reacting to its
//! own writes, callers register expected operations via
//! [`VaultWatcher::expect_self_operation`]. The next matching event for that
//! exact path is suppressed (reservation semantics); a short TTL is kept only as
//! a safety net so a stale registration can never permanently hide an external
//! edit to a *different* path. This is intentionally **not** a broad,
//! time-only suppression window that could swallow genuine external edits.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use tokio::sync::mpsc as tk_mpsc;
use tracing::{debug, info, warn};

use crate::watcher::{Result, VaultEvent, VaultWatcherConfig, WatcherChangeKind, WatcherError};
use notify::Watcher;

/// A single normalized signal for one absolute path.
///
/// Several `RawChange`s may accumulate for the same path inside a debounce
/// window; the coalescer folds them into at most one [`VaultEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawChange {
    Created,
    Modified,
    Deleted,
    Name,
}

/// A change waiting to be emitted after existence-based resolution.
///
/// Paths are absolute (lexically normalized) so rename pairing and self-op
/// matching can compare them without filesystem round-trips.
#[derive(Debug, Clone)]
struct PendingEmit {
    abs: PathBuf,
    /// Previous absolute path, populated for renames whose source is known.
    old_abs: Option<PathBuf>,
    kind: WatcherChangeKind,
}

/// A time-boxed registration of an operation Nabu itself is about to perform
/// on `abs`. The next matching event for that path is suppressed.
#[derive(Debug, Clone)]
struct SelfEventRegistry {
    /// `(normalized_abs_path, expires_at)`
    ops: Vec<(PathBuf, Instant)>,
}

impl SelfEventRegistry {
    fn new() -> Self {
        Self { ops: Vec::new() }
    }

    fn register(&mut self, abs: PathBuf, now: Instant, ttl: std::time::Duration) {
        // Replace any prior registration for the exact same path so callers
        // don't need to drain previous entries.
        self.ops.retain(|(p, _)| *p != abs);
        self.ops.push((abs, now + ttl));
    }

    /// Returns `true` if `abs` is a live self-operation, consuming it.
    fn consume(&mut self, path: &Path, now: Instant) -> bool {
        if let Some(idx) = self
            .ops
            .iter()
            .position(|(p, expires)| *p == path && *expires >= now)
        {
            self.ops.remove(idx);
            true
        } else {
            false
        }
    }

    /// Drop registrations whose TTL has elapsed.
    fn retire_expired(&mut self, now: Instant) {
        self.ops.retain(|(_, expires)| *expires >= now);
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// The vault filesystem watcher.
///
/// Construct with [`VaultWatcher::new`], start with [`VaultWatcher::start`]
/// (which returns the event receiver), register self-operations with
/// [`VaultWatcher::expect_self_operation`], and stop with [`VaultWatcher::stop`].
pub struct VaultWatcher {
    vault_path: PathBuf,
    config: VaultWatcherConfig,
    watcher: Option<notify::RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    /// Shared with the processing thread so either side can observe registrations.
    self_ops: Arc<Mutex<SelfEventRegistry>>,
    /// Canonical absolute paths of files that existed when `start()` was
    /// called. On macOS, FSEvents reports a write to an *existing* file as a
    /// `Create` event (indistinguishable from a genuine new-file create by
    /// signal shape alone). The processing thread consults this set to
    /// downgrade such spurious `Create`s back to `Modify`.
    known_files: Arc<Mutex<HashSet<PathBuf>>>,
}

impl VaultWatcher {
    /// Create a new watcher for `vault_path` with the default configuration.
    ///
    /// The vault path is not opened/validated here; pass a valid vault to
    /// [`VaultWatcher::start`].
    #[must_use]
    pub fn new(vault_path: impl Into<PathBuf>) -> Self {
        Self::with_config(vault_path, VaultWatcherConfig::default())
    }

    /// Create a new watcher with an explicit configuration.
    #[must_use]
    pub fn with_config(vault_path: impl Into<PathBuf>, config: VaultWatcherConfig) -> Self {
        Self {
            vault_path: vault_path.into(),
            config,
            watcher: None,
            thread: None,
            shutdown: Arc::new(AtomicBool::new(false)),
            self_ops: Arc::new(Mutex::new(SelfEventRegistry::new())),
            known_files: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Vault root this watcher monitors.
    #[must_use]
    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }

    /// `true` once [`start`](Self::start) has been called and not yet stopped.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.thread.is_some()
    }

    /// Begin watching the vault recursively.
    ///
    /// Returns the receiver half of the normalized event stream. The watcher
    /// thread owns the matching sender, so the receiver closes cleanly (yielding
    /// `None`) once [`stop`](Self::stop) is called.
    ///
    /// # Errors
    /// - [`WatcherError::AlreadyStarted`] if already running.
    /// - [`WatcherError::VaultNotFound`] if the vault path is not a directory.
    /// - [`WatcherError::Notify`] if the platform backend rejects the watch.
    pub fn start(&mut self) -> Result<tk_mpsc::UnboundedReceiver<VaultEvent>> {
        if self.thread.is_some() {
            return Err(WatcherError::AlreadyStarted);
        }
        if !self.vault_path.is_dir() {
            return Err(WatcherError::VaultNotFound {
                path: self.vault_path.clone(),
            });
        }

        // Canonicalize the vault root once so it aligns with the absolute paths
        // the `notify` backend reports. On macOS `/var` is a symlink to
        // `/private/var` and FSEvents yields canonical `/private/var/...`
        // paths; without this, vault-relative stripping silently fails.
        let canonical =
            self.vault_path
                .canonicalize()
                .map_err(|_| WatcherError::VaultNotFound {
                    path: self.vault_path.clone(),
                })?;
        self.vault_path = canonical;

        // Seed the "known-existing" file set with a one-time recursive scan of
        // the vault. macOS FSEvents reports a write to an *existing* file as a
        // `Create` event (the same signal shape as a genuine new-file create),
        // so the processing thread cannot tell the two apart from raw events
        // alone. A path that was already present at watch-start must therefore
        // be downgraded from `Created` → `Modified`; a path absent from the set
        // is a genuine creation.
        self.known_files = Arc::new(Mutex::new(scan_known_files(&self.vault_path)));

        let (out_tx, out_rx) = tk_mpsc::unbounded_channel::<VaultEvent>();
        let (raw_tx, raw_rx) = std_mpsc::channel::<notify::Result<notify::Event>>();

        let mut watcher = notify::recommended_watcher(raw_tx).map_err(WatcherError::Notify)?;
        watcher
            .watch(&self.vault_path, notify::RecursiveMode::Recursive)
            .map_err(WatcherError::Notify)?;

        self.shutdown = Arc::new(AtomicBool::new(false));
        let shutdown = self.shutdown.clone();
        let self_ops = self.self_ops.clone();
        let known_files = self.known_files.clone();
        let vault = self.vault_path.clone();
        let config = self.config;

        let handle = thread::Builder::new()
            .name("nabu-vault-watcher".into())
            .spawn(move || {
                processor_loop(
                    vault,
                    config,
                    raw_rx,
                    out_tx,
                    shutdown,
                    self_ops,
                    known_files,
                )
            })
            .map_err(|e| {
                WatcherError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;

        self.watcher = Some(watcher);
        self.thread = Some(handle);
        info!(subsystem = "watcher", vault = ?self.vault_path, "watching vault recursively");
        Ok(out_rx)
    }

    /// Register an operation Nabu itself is about to perform on `path`.
    ///
    /// `path` may be absolute or vault-relative; it is resolved against the
    /// vault root and lexically normalized before matching. The next matching
    /// notification for that exact path is suppressed (consumed). A short TTL
    /// (see [`VaultWatcherConfig::self_op_ttl`]) is the safety net only.
    ///
    /// Call this *before* the write/move/delete occurs.
    pub fn expect_self_operation(&self, path: impl AsRef<Path>) {
        let abs = resolve_abs(path.as_ref(), &self.vault_path);
        let mut ops = self.self_ops.lock().expect("self_ops mutex poisoned");
        ops.register(abs, Instant::now(), self.config.self_op_ttl);
    }

    /// Stop watching and release all background threads.
    ///
    /// After `stop` returns the event receiver yields `None`, signalling
    /// consumers that the stream is closed.
    ///
    /// # Errors
    /// - [`WatcherError::NotRunning`] if the watcher was never started.
    pub fn stop(&mut self) -> Result<()> {
        if self.thread.is_none() {
            return Err(WatcherError::NotRunning);
        }
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        // Dropping the native watcher stops its internal kernel thread.
        self.watcher.take();
        info!(subsystem = "watcher", "watcher stopped");
        Ok(())
    }
}

impl Drop for VaultWatcher {
    fn drop(&mut self) {
        if self.thread.is_some() {
            // Best-effort: ensure the background thread and native watcher are
            // cleaned up if the caller forgot to call stop().
            let _ = self.stop();
        }
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Lexically normalize `path` to an absolute path without touching the
/// filesystem. Relative paths are resolved against `vault`. This lets self-op
/// registrations and normalized event paths be compared by value regardless of
/// whether either side could `canonicalize` (e.g. for a path that was just
/// deleted).
fn normalize_abs(path: &Path, vault: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        vault.join(path)
    };
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => {
                out.push("/");
            }
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s),
        }
    }
    out
}

/// Resolve a caller-supplied path (absolute or vault-relative) to a canonicalized
/// absolute path. Canonicalization matters because the `notify` backend reports
/// paths under the OS's canonical root — on macOS `/var` is a symlink to
/// `/private/var`, and FSEvents yields `/private/var/...`. A path that does not
/// currently exist (e.g. a deleted file whose event is being suppressed) falls
/// back to lexical normalization so matching still works.
fn resolve_abs(path: &Path, vault: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        vault.join(path)
    };
    abs.canonicalize()
        .unwrap_or_else(|_| normalize_abs(&abs, vault))
}

/// Convert an absolute path to a vault-relative string (`Inbox/note.md`),
/// or `None` if it isn't under the vault root.
fn to_vault_rel(abs: &Path, vault: &Path) -> Option<String> {
    let rel = abs.strip_prefix(vault).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// True if `path` lives anywhere under `<vault>/.nabu` (Nabu's internal
/// sidecar/index store, which is derived state and must not surface as vault
/// content events).
fn is_nabu_internal(path: &Path, vault: &Path) -> bool {
    let abs = normalize_abs(path, vault);
    let nabu = normalize_abs(&vault.join(".nabu"), vault);
    abs == nabu || abs.starts_with(&nabu)
}

/// Last path component (file name) — used for pairing rename source/dest by
/// directory. Returns the path's final component or the path itself.
fn parent_dir(path: &Path) -> PathBuf {
    path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

/// Recursively collect the canonical absolute paths of every regular file
/// currently living under `vault` (skipping the `.nabu` sidecar dir, which is
/// derived state). The result seeds [`VaultWatcher`]'s "known files" set so
/// spurious `Create` events for existing files (the macOS FSEvents
/// write-to-existing-file quirk) can be downgraded to `Modify`.
fn scan_known_files(vault: &Path) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let nabu = normalize_abs(&vault.join(".nabu"), vault);
    let mut stack = vec![vault.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                debug!(subsystem = "watcher", dir = ?dir, error = ?e, "scan: read_dir failed");
                continue;
            }
        };
        for entry in read {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            let abs = normalize_abs(&path, vault);
            // Skip Nabu's own sidecar store and descend into subdirectories.
            if abs == nabu || abs.starts_with(&nabu) {
                continue;
            }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(path);
            } else if abs.is_file() {
                out.insert(abs);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

/// Translate one raw `notify` event into per-path raw signals.
///
/// Only the four logical change categories are emitted; pure metadata, access,
/// and "imprecise" (`Any`/`Other`) events are dropped as platform noise.
/// `.nabu/`-internal paths are filtered out entirely.
fn normalize_event(vault: &Path, res: notify::Result<notify::Event>) -> Vec<(PathBuf, RawChange)> {
    let ev = match res {
        Ok(e) => e,
        Err(e) => {
            warn!(subsystem = "watcher", error = ?e, "notify backend error");
            return Vec::new();
        }
    };

    if ev.flag() == Some(notify::event::Flag::Rescan) {
        // The backend may have missed events. Downstream (Phase 1B) is
        // responsible for any rescan; the watcher itself emits nothing here to
        // avoid fabricating a flood of synthetic events.
        warn!(subsystem = "watcher", paths = ?ev.paths, "notify rescan requested");
        return Vec::new();
    }

    let mut out = Vec::new();
    let kind = ev.kind;
    for path in &ev.paths {
        if is_nabu_internal(path, vault) {
            continue;
        }
        // Skip the vault root itself.
        if normalize_abs(path, vault) == normalize_abs(vault, vault) {
            continue;
        }
        let abs = normalize_abs(path, vault);
        let change = match kind {
            notify::EventKind::Create(_) => Some(RawChange::Created),
            notify::EventKind::Remove(_) => Some(RawChange::Deleted),
            notify::EventKind::Modify(m) => match m {
                notify::event::ModifyKind::Data(_) => Some(RawChange::Modified),
                notify::event::ModifyKind::Name(_) => Some(RawChange::Name),
                // Metadata-only / Other modify: platform noise for our purposes.
                _ => None,
            },
            // Access / Any / Other: too imprecise to act on safely.
            _ => None,
        };
        if let Some(ch) = change {
            // Deduplicate identical (path, change) pairs within a single raw
            // event without requiring an Ord impl on PathBuf/RawChange.
            if !out.iter().any(|(p, c)| *p == abs && *c == ch) {
                out.push((abs, ch));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Coalesce
// ---------------------------------------------------------------------------

/// Collapse per-path signal sets into concrete changes, resolving renames via
/// filesystem existence.
///
/// `exists` is a closure so this function can be unit-tested without touching
/// the disk.
fn coalesce(
    signals: HashMap<PathBuf, Vec<RawChange>>,
    exists: impl Fn(&Path) -> bool,
) -> Vec<PendingEmit> {
    let mut emits = Vec::new();
    let mut sources: Vec<(PathBuf, PathBuf)> = Vec::new(); // (abs, parent) gone + Name
    let mut dests: Vec<PathBuf> = Vec::new(); // abs exists + Name
    let mut consumed_dests: Vec<PathBuf> = Vec::new();

    let has = |chs: &[RawChange], c: RawChange| chs.contains(&c);

    for (abs, chs) in &signals {
        let exists_now = exists(abs);
        let has_name = has(chs, RawChange::Name);
        if !exists_now {
            if has_name {
                // Vanished path that was involved in a rename: the "from" side.
                sources.push((abs.clone(), parent_dir(abs)));
            } else {
                // No rename signal: it was deleted (or created+deleted).
                emits.push(PendingEmit {
                    abs: abs.clone(),
                    old_abs: None,
                    kind: WatcherChangeKind::Deleted,
                });
            }
        } else if has_name {
            // Present path involved in a rename: the "to" side (await pairing).
            dests.push(abs.clone());
        } else if has(chs, RawChange::Created) {
            emits.push(PendingEmit {
                abs: abs.clone(),
                old_abs: None,
                kind: WatcherChangeKind::Created,
            });
        } else if has(chs, RawChange::Modified) {
            emits.push(PendingEmit {
                abs: abs.clone(),
                old_abs: None,
                kind: WatcherChangeKind::Modified,
            });
        } else {
            // Metadata-only change: nothing of interest to consumers.
        }
    }

    // Pair rename sources with destinations in the same parent directory.
    for (src, parent) in sources {
        let matched = dests
            .iter()
            .find(|d| parent_dir(d) == parent && !consumed_dests.contains(*d))
            .cloned();
        if let Some(dest) = matched {
            consumed_dests.push(dest.clone());
            emits.push(PendingEmit {
                abs: dest,
                old_abs: Some(src),
                kind: WatcherChangeKind::Renamed,
            });
        } else {
            // Rename source with no destination observed — emit as deletion.
            emits.push(PendingEmit {
                abs: src,
                old_abs: None,
                kind: WatcherChangeKind::Deleted,
            });
        }
    }

    // Unpaired destinations (rename target the source of which we never saw,
    // e.g. a file moved into the vault from outside) become creations.
    for dest in dests {
        if !consumed_dests.contains(&dest) {
            emits.push(PendingEmit {
                abs: dest,
                old_abs: None,
                kind: WatcherChangeKind::Created,
            });
        }
    }

    emits
}

/// Apply self-event suppression, map to vault-relative [`VaultEvent`]s, and
/// forward to the consumer channel.
fn emit(
    emits: Vec<PendingEmit>,
    vault: &Path,
    self_ops: &Arc<Mutex<SelfEventRegistry>>,
    out_tx: &tk_mpsc::UnboundedSender<VaultEvent>,
) {
    let now = Instant::now();
    let mut registry = self_ops.lock().expect("self_ops mutex poisoned");
    registry.retire_expired(now);

    for pe in emits {
        // Suppress if either side of the change was a registered self-op.
        let suppressed = pe
            .old_abs
            .as_deref()
            .map_or(false, |o| registry.consume(o, now))
            || registry.consume(&pe.abs, now);
        if suppressed {
            debug!(
                subsystem = "watcher",
                path = ?pe.abs,
                "suppressed self-generated event"
            );
            continue;
        }
        let path = match to_vault_rel(&pe.abs, vault) {
            Some(p) => p,
            None => continue,
        };
        let old_path = pe.old_abs.as_deref().and_then(|o| to_vault_rel(o, vault));
        let ev = match pe.kind {
            WatcherChangeKind::Renamed => VaultEvent::renamed(path, old_path.unwrap_or_default()),
            WatcherChangeKind::Created => VaultEvent::created(path),
            WatcherChangeKind::Modified => VaultEvent::modified(path),
            WatcherChangeKind::Deleted => VaultEvent::deleted(path),
        };
        if out_tx.send(ev).is_err() {
            // Consumer gone / channel closed. Stop processing.
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Processing thread
// ---------------------------------------------------------------------------

/// Drain `signals`, coalescing + emitting, returning the number emitted.
fn flush(
    signals: &mut HashMap<PathBuf, Vec<RawChange>>,
    vault: &Path,
    self_ops: &Arc<Mutex<SelfEventRegistry>>,
    out_tx: &tk_mpsc::UnboundedSender<VaultEvent>,
) -> usize {
    let pending = std::mem::take(signals);
    if pending.is_empty() {
        return 0;
    }
    let emits = coalesce(pending, |p| p.exists());
    let n = emits.len();
    emit(emits, vault, self_ops, out_tx);
    n
}

fn processor_loop(
    vault: PathBuf,
    config: VaultWatcherConfig,
    raw_rx: std_mpsc::Receiver<notify::Result<notify::Event>>,
    out_tx: tk_mpsc::UnboundedSender<VaultEvent>,
    shutdown: Arc<AtomicBool>,
    self_ops: Arc<Mutex<SelfEventRegistry>>,
    known_files: Arc<Mutex<HashSet<PathBuf>>>,
) {
    let mut signals: HashMap<PathBuf, Vec<RawChange>> = HashMap::new();
    let mut flush_deadline: Option<Instant> = None;

    info!(subsystem = "watcher", "processing thread started");

    loop {
        if shutdown.load(Ordering::Acquire) {
            debug!(subsystem = "watcher", "shutdown flag observed");
            flush(&mut signals, &vault, &self_ops, &out_tx);
            break;
        }

        let timeout = match &flush_deadline {
            Some(dl) => dl
                .checked_duration_since(Instant::now())
                .unwrap_or_default()
                .max(std::time::Duration::from_millis(10)),
            None => config.idle_poll,
        };

        match raw_rx.recv_timeout(timeout) {
            Ok(res) => {
                let changes = normalize_event(&vault, res);
                for (abs, mut ch) in changes {
                    // macOS FSEvents emits a `Create` for writes to existing
                    // files (truncate+write). If the path was known to exist
                    // at watch-start, treat the `Create` as a `Modify` of an
                    // existing file instead. Genuine new files are absent from
                    // the set and stay `Created`.
                    if ch == RawChange::Created {
                        let mut known = known_files.lock().expect("known_files mutex poisoned");
                        if known.contains(&abs) {
                            ch = RawChange::Modified;
                        } else {
                            known.insert(abs.clone());
                        }
                    } else if ch == RawChange::Deleted {
                        let mut known = known_files.lock().expect("known_files mutex poisoned");
                        known.remove(&abs);
                    }
                    let entry = signals.entry(abs).or_default();
                    if !entry.contains(&ch) {
                        entry.push(ch);
                    }
                }
                flush_deadline = Some(Instant::now() + config.debounce);
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                if flush_deadline.take().is_some() {
                    flush(&mut signals, &vault, &self_ops, &out_tx);
                }
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                // The native watcher was dropped (stop) — flush anything
                // still pending, then exit.
                flush(&mut signals, &vault, &self_ops, &out_tx);
                break;
            }
        }
    }

    info!(subsystem = "watcher", "processing thread stopped");
}

// ===========================================================================
// Pure-logic unit tests (deterministic, no filesystem)
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn exists_always_true(_: &Path) -> bool {
        true
    }
    fn exists_always_false(_: &Path) -> bool {
        false
    }

    fn rel(abs: &str, vault: &str) -> PathBuf {
        PathBuf::from(vault).join(abs)
    }

    #[test]
    fn normalize_abs_makes_paths_absolute_and_lexically_resolves() {
        let vault = Path::new("/tmp/vault");
        assert_eq!(
            normalize_abs(&PathBuf::from("Inbox/a.md"), vault),
            PathBuf::from("/tmp/vault/Inbox/a.md")
        );
        // relative with ./ and .. collapses without fs access
        let p = normalize_abs(&PathBuf::from("Inbox/./sub/../a.md"), vault);
        assert_eq!(p, PathBuf::from("/tmp/vault/Inbox/a.md"));
    }

    #[test]
    fn to_vault_rel_strips_vault_prefix() {
        let vault = Path::new("/tmp/vault");
        assert_eq!(
            to_vault_rel(&PathBuf::from("/tmp/vault/Inbox/a.md"), vault),
            Some("Inbox/a.md".to_string())
        );
        assert_eq!(to_vault_rel(&PathBuf::from("/tmp/vault"), vault), None);
        assert_eq!(to_vault_rel(&PathBuf::from("/elsewhere/a.md"), vault), None);
    }

    #[test]
    fn is_nabu_internal_filters_dotnabu_subtree() {
        let vault = Path::new("/tmp/vault");
        assert!(is_nabu_internal(
            &PathBuf::from("/tmp/vault/.nabu/x.json"),
            vault
        ));
        assert!(is_nabu_internal(
            &PathBuf::from("/tmp/vault/.nabu/sub/y.json"),
            vault
        ));
        assert!(!is_nabu_internal(
            &PathBuf::from("/tmp/vault/Inbox/a.md"),
            vault
        ));
        assert!(!is_nabu_internal(
            &PathBuf::from("/tmp/vault/.nabu_other/a.md"),
            vault
        ));
    }

    fn sig(path: &str, changes: &[RawChange]) -> (PathBuf, Vec<RawChange>) {
        (PathBuf::from(path), changes.to_vec())
    }

    #[test]
    fn coalesce_dedups_duplicate_signals_per_path() {
        // Test 6 (duplicate suppression) at the logic level: many identical
        // modify signals for one path collapse to a single Modified event.
        let signals: HashMap<_, _> = [sig(
            "/tmp/vault/note.md",
            &[
                RawChange::Modified,
                RawChange::Modified,
                RawChange::Modified,
            ],
        )]
        .into_iter()
        .collect();
        let emit = coalesce(signals, exists_always_true);
        assert_eq!(emit.len(), 1);
        assert_eq!(emit[0].kind, WatcherChangeKind::Modified);
        assert_eq!(emit[0].abs, PathBuf::from("/tmp/vault/note.md"));
    }

    #[test]
    fn coalesce_create_and_modify_on_existing_path_emits_created() {
        let signals: HashMap<_, _> = [sig(
            "/tmp/vault/new.md",
            &[RawChange::Created, RawChange::Modified, RawChange::Modified],
        )]
        .into_iter()
        .collect();
        let emit = coalesce(signals, exists_always_true);
        assert_eq!(emit.len(), 1);
        assert_eq!(emit[0].kind, WatcherChangeKind::Created);
    }

    #[test]
    fn coalesce_deleted_when_path_gone_and_no_name_event() {
        let signals: HashMap<_, _> = [sig("/tmp/vault/gone.md", &[RawChange::Deleted])]
            .into_iter()
            .collect();
        let emit = coalesce(signals, exists_always_false);
        assert_eq!(emit.len(), 1);
        assert_eq!(emit[0].kind, WatcherChangeKind::Deleted);
    }

    #[test]
    fn coalesce_pairs_rename_source_and_dest_in_same_dir() {
        // macOS-style rename: a vanished source + an existing destination, both
        // emitting Name signals in the same debounce window.
        let signals: HashMap<_, _> = [
            sig("/tmp/vault/A/note.md", &[RawChange::Name]),
            sig("/tmp/vault/A/note2.md", &[RawChange::Name]),
        ]
        .into_iter()
        .collect();
        let exists = |p: &Path| p == Path::new("/tmp/vault/A/note2.md");
        let emit = coalesce(signals, exists);
        assert_eq!(emit.len(), 1);
        assert_eq!(emit[0].kind, WatcherChangeKind::Renamed);
        assert_eq!(emit[0].abs, PathBuf::from("/tmp/vault/A/note2.md"));
        assert_eq!(emit[0].old_abs, Some(PathBuf::from("/tmp/vault/A/note.md")));
    }

    #[test]
    fn coalesce_unpaired_rename_source_emits_deleted() {
        let signals: HashMap<_, _> = [sig("/tmp/vault/A/gone.md", &[RawChange::Name])]
            .into_iter()
            .collect();
        let emit = coalesce(signals, exists_always_false);
        assert_eq!(emit.len(), 1);
        assert_eq!(emit[0].kind, WatcherChangeKind::Deleted);
    }

    #[test]
    fn coalesce_unpaired_rename_dest_emits_created() {
        let signals: HashMap<_, _> = [sig("/tmp/vault/A/arrived.md", &[RawChange::Name])]
            .into_iter()
            .collect();
        let emit = coalesce(signals, exists_always_true);
        assert_eq!(emit.len(), 1);
        assert_eq!(emit[0].kind, WatcherChangeKind::Created);
    }

    #[test]
    fn coalesce_mixed_unordered_signals_for_one_path() {
        // create + metadata + data + name, all on one existing path
        let signals: HashMap<_, _> = [sig(
            "/tmp/vault/m.md",
            &[RawChange::Created, RawChange::Name, RawChange::Modified],
        )]
        .into_iter()
        .collect();
        let emit = coalesce(signals, exists_always_true);
        assert_eq!(emit.len(), 1, "one path -> one event");
        assert_eq!(emit[0].kind, WatcherChangeKind::Created); // Name + exists => dest, paired with nothing => Created
    }

    #[test]
    fn self_event_registry_consume_matches_exact_path() {
        let mut reg = SelfEventRegistry::new();
        let now = Instant::now();
        reg.register(
            PathBuf::from("/v/a.md"),
            now,
            std::time::Duration::from_secs(10),
        );
        assert!(reg.consume(&PathBuf::from("/v/a.md"), now));
        assert!(!reg.consume(&PathBuf::from("/v/a.md"), now)); // consumed
        assert!(!reg.consume(&PathBuf::from("/v/other.md"), now)); // different path
    }

    #[test]
    fn self_event_registry_retire_expired() {
        let mut reg = SelfEventRegistry::new();
        let now = Instant::now();
        reg.register(
            PathBuf::from("/v/a.md"),
            now,
            std::time::Duration::from_nanos(1),
        );
        reg.register(
            PathBuf::from("/v/b.md"),
            now,
            std::time::Duration::from_secs(60),
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        let later = Instant::now();
        reg.retire_expired(later);
        assert!(!reg.consume(&PathBuf::from("/v/a.md"), later)); // expired -> not consumed
        assert!(reg.consume(&PathBuf::from("/v/b.md"), later)); // still live
    }

    #[test]
    fn emit_suppresses_registered_self_operations() {
        let (out_tx, mut out_rx) = tk_mpsc::unbounded_channel::<VaultEvent>();
        let self_ops = Arc::new(Mutex::new(SelfEventRegistry::new()));
        let vault = Path::new("/tmp/vault");

        // Register a self-op on the modified path.
        {
            let mut r = self_ops.lock().unwrap();
            r.register(
                PathBuf::from("/tmp/vault/note.md"),
                Instant::now(),
                std::time::Duration::from_secs(2),
            );
        }

        let emits = vec![PendingEmit {
            abs: PathBuf::from("/tmp/vault/note.md"),
            old_abs: None,
            kind: WatcherChangeKind::Modified,
        }];
        emit(emits, vault, &self_ops, &out_tx);
        // Nothing should have been emitted.
        assert!(out_rx.try_recv().is_err());
        assert!(
            self_ops.lock().unwrap().is_empty(),
            "self-op consumed on suppress"
        );
    }

    #[test]
    fn emit_passes_through_external_events_unsuppressed() {
        let (out_tx, mut out_rx) = tk_mpsc::unbounded_channel::<VaultEvent>();
        let self_ops = Arc::new(Mutex::new(SelfEventRegistry::new()));
        let vault = Path::new("/tmp/vault");

        let emits = vec![PendingEmit {
            abs: PathBuf::from("/tmp/vault/note.md"),
            old_abs: None,
            kind: WatcherChangeKind::Modified,
        }];
        emit(emits, vault, &self_ops, &out_tx);
        let got = out_rx.try_recv().expect("external event should be emitted");
        assert_eq!(got.kind, WatcherChangeKind::Modified);
        assert_eq!(got.path, "note.md");
    }

    #[test]
    fn emit_pairs_rename_with_vault_relative_paths() {
        let (out_tx, mut out_rx) = tk_mpsc::unbounded_channel::<VaultEvent>();
        let self_ops = Arc::new(Mutex::new(SelfEventRegistry::new()));
        let vault = Path::new("/tmp/vault");
        let emits = vec![PendingEmit {
            abs: PathBuf::from("/tmp/vault/Inbox/new.md"),
            old_abs: Some(PathBuf::from("/tmp/vault/Inbox/old.md")),
            kind: WatcherChangeKind::Renamed,
        }];
        emit(emits, vault, &self_ops, &out_tx);
        let got = out_rx.try_recv().expect("rename event should be emitted");
        assert_eq!(got.kind, WatcherChangeKind::Renamed);
        assert_eq!(got.path, "Inbox/new.md");
        assert_eq!(got.old_path.as_deref(), Some("Inbox/old.md"));
    }
}

// ===========================================================================
// Filesystem integration tests (Test 1-8 from the spec)
// ===========================================================================
#[cfg(test)]
mod fs_tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use tempfile::tempdir;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, timeout};

    use super::{VaultEvent, VaultWatcher, VaultWatcherConfig, WatcherChangeKind, WatcherError};

    /// FSEvents needs a moment to arm after `watch()` returns.
    const ARM: Duration = Duration::from_millis(100);
    /// Collection window — comfortably larger than the 200ms debounce.
    const WINDOW: u64 = 600;

    fn test_config() -> VaultWatcherConfig {
        VaultWatcherConfig {
            debounce: Duration::from_millis(200),
            self_op_ttl: Duration::from_secs(3),
            idle_poll: Duration::from_millis(50),
        }
    }

    fn p(vault: &Path, rel: &str) -> PathBuf {
        vault.join(rel)
    }

    /// Collect every event received within `ms`, then stop collecting.
    async fn collect(rx: &mut mpsc::UnboundedReceiver<VaultEvent>, ms: u64) -> Vec<VaultEvent> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        let mut out = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match timeout(remaining, rx.recv()).await {
                Ok(Some(e)) => out.push(e),
                _ => break,
            }
        }
        out
    }

    /// Test 1 — Create
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_create() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        let mut rx = w.start().unwrap();
        sleep(ARM).await;
        fs::write(p(vault, "note.md"), "# hello").unwrap();
        let evs = collect(&mut rx, WINDOW).await;
        w.stop().unwrap();
        assert!(
            evs.iter()
                .any(|e| e.kind == WatcherChangeKind::Created && e.path == "note.md"),
            "expected Created(note.md), got: {:?}",
            evs
        );
    }

    /// Test 2 — Modify
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_modify() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let f = p(vault, "note.md");
        fs::write(&f, "v1").unwrap();
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        let mut rx = w.start().unwrap();
        sleep(ARM).await;
        fs::write(&f, "v2").unwrap();
        let evs = collect(&mut rx, WINDOW).await;
        w.stop().unwrap();
        assert!(
            evs.iter()
                .any(|e| e.kind == WatcherChangeKind::Modified && e.path == "note.md"),
            "expected Modified(note.md), got: {:?}",
            evs
        );
    }

    /// Test 3 — Delete
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_delete() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let f = p(vault, "note.md");
        fs::write(&f, "bye").unwrap();
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        let mut rx = w.start().unwrap();
        sleep(ARM).await;
        fs::remove_file(&f).unwrap();
        let evs = collect(&mut rx, WINDOW).await;
        w.stop().unwrap();
        assert!(
            evs.iter()
                .any(|e| e.kind == WatcherChangeKind::Deleted && e.path == "note.md"),
            "expected Deleted(note.md), got: {:?}",
            evs
        );
    }

    /// Test 4 — Rename/move (asserts old *and* new paths where the platform
    /// reports them; macOS FSEvents reports both Name halves, which the
    /// coalescer pairs into a single Renamed event).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_rename() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let f = p(vault, "note.md");
        fs::write(&f, "content").unwrap();
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        let mut rx = w.start().unwrap();
        sleep(ARM).await;
        // Drain any startup/initial burst so it doesn't pollute the assertion.
        let _ = collect(&mut rx, 400).await;
        fs::rename(&f, p(vault, "note2.md")).unwrap();
        let evs = collect(&mut rx, WINDOW).await;
        w.stop().unwrap();
        let renamed = evs.iter().find(|e| {
            e.kind == WatcherChangeKind::Renamed
                && e.path == "note2.md"
                && e.old_path.as_deref() == Some("note.md")
        });
        assert!(
            renamed.is_some(),
            "expected a Renamed event note.md -> note2.md, got: {:?}",
            evs
        );
    }

    /// Test 5 — Debounce: several rapid modifications coalesce to a single
    /// normalized event instead of one per raw OS notification.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_debounce() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let f = p(vault, "note.md");
        fs::write(&f, "base").unwrap(); // exists before watching -> no Create
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        let mut rx = w.start().unwrap();
        sleep(ARM).await;
        let _ = collect(&mut rx, 400).await;
        for i in 0..5 {
            fs::write(&f, format!("v{i}")).unwrap();
            sleep(Duration::from_millis(20)).await;
        }
        let evs = collect(&mut rx, WINDOW).await;
        w.stop().unwrap();
        let mods: Vec<_> = evs
            .iter()
            .filter(|e| e.kind == WatcherChangeKind::Modified && e.path == "note.md")
            .collect();
        assert_eq!(
            mods.len(),
            1,
            "expected exactly 1 coalesced Modified, got {} events: {:?}",
            mods.len(),
            evs
        );
    }

    /// Test 6 — Duplicate suppression: equivalent rapid writes collapse to a
    /// single event. (The pure-logic dedup is covered by `coalesce_*` unit
    /// tests; this validates it end-to-end on the real filesystem.)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_duplicate_suppression() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let f = p(vault, "note.md");
        fs::write(&f, "a").unwrap();
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        let mut rx = w.start().unwrap();
        sleep(ARM).await;
        let _ = collect(&mut rx, 400).await;
        fs::write(&f, "b").unwrap();
        sleep(Duration::from_millis(10)).await;
        fs::write(&f, "b").unwrap();
        let evs = collect(&mut rx, WINDOW).await;
        w.stop().unwrap();
        let mods: Vec<_> = evs
            .iter()
            .filter(|e| e.kind == WatcherChangeKind::Modified && e.path == "note.md")
            .collect();
        assert_eq!(
            mods.len(),
            1,
            "expected 1 event for duplicate writes, got: {:?}",
            evs
        );
    }

    /// Test 7 — Self-event suppression.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_self_event_suppression() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let f = p(vault, "note.md");
        fs::write(&f, "base").unwrap();
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        let mut rx = w.start().unwrap();
        sleep(ARM).await;
        let _ = collect(&mut rx, 500).await; // drain

        // Registered self-operation -> suppressed.
        w.expect_self_operation(&f);
        fs::write(&f, "self-write").unwrap();
        let suppressed = collect(&mut rx, 500).await;
        assert!(
            !suppressed.iter().any(|e| e.path == "note.md"),
            "self-op should be suppressed, got: {:?}",
            suppressed
        );

        // Unregistered external change -> detected normally.
        fs::write(&f, "external-write").unwrap();
        let external = collect(&mut rx, WINDOW).await;
        assert!(
            external
                .iter()
                .any(|e| e.kind == WatcherChangeKind::Modified && e.path == "note.md"),
            "external change should be detected, got: {:?}",
            external
        );
        w.stop().unwrap();
    }

    /// Test 8 — Lifecycle: clean start/stop, no leaked threads, closed channel.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fs_test_lifecycle() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let mut w = VaultWatcher::with_config(vault.to_path_buf(), test_config());
        assert!(!w.is_running());
        let mut rx = w.start().unwrap();
        assert!(w.is_running());
        sleep(ARM).await;
        let _ = collect(&mut rx, 300).await;

        let stop_result = timeout(Duration::from_secs(2), async { w.stop() }).await;
        assert!(stop_result.is_ok(), "stop did not complete within 2s");
        assert!(stop_result.unwrap().is_ok(), "stop returned an error");
        assert!(!w.is_running());

        // The event stream must close cleanly after stop.
        let closed = timeout(Duration::from_millis(500), rx.recv()).await;
        assert_eq!(closed, Ok(None), "receiver should close after stop");
    }

    /// start() must reject a non-existent vault.
    #[test]
    fn fs_test_start_rejects_nonexistent_vault() {
        let mut w = VaultWatcher::new("/this/vault/should/not/exist");
        let res = w.start();
        assert!(
            matches!(res, Err(WatcherError::VaultNotFound { .. })),
            "expected VaultNotFound, got: {:?}",
            res
        );
    }
}
