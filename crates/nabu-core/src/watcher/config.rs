//! Configuration for the vault watcher.
//!
//! Tuning knobs are kept in a single, cheaply-`Clone` struct so callers can
//! construct a watcher with sane production defaults and tweak only what they
//! need (or nothing at all, via [`Default`]).

use std::time::Duration;

/// Tunables for [`crate::watcher::VaultWatcher`].
#[derive(Debug, Clone, Copy)]
pub struct VaultWatcherConfig {
    /// How long the watcher waits after the *last* raw OS event for a path
    /// before emitting the coalesced, normalized change. Rapid bursts
    /// (e.g. one editor save producing several modify events) are collapsed
    /// into a single emitted event per path.
    pub debounce: Duration,

    /// How long a registered self-operation remains eligible for suppression
    /// after it is registered. This is a *safety net* only: the primary
    /// suppression mechanism is reservation-based (the next matching event is
    /// consumed), so the TTL only bounds how long a stale registration can
    /// suppress an event if the matching notification never arrives.
    pub self_op_ttl: Duration,

    /// When the buffer is empty, how long the processing thread blocks before
    /// re-checking its shutdown flag. Smaller values make shutdown snappier at
    /// the cost of more idle wake-ups; larger values save wake-ups at the cost
    /// of shutdown latency.
    pub idle_poll: Duration,
}

impl Default for VaultWatcherConfig {
    fn default() -> Self {
        // 200ms debounce comfortably coalesces the burst of OS events that a
        // single save/edit produces (the probe in `notify` shows editor saves
        // fire create + metadata + data events within a few milliseconds),
        // while still emitting changes promptly to users.
        VaultWatcherConfig {
            debounce: Duration::from_millis(200),
            self_op_ttl: Duration::from_secs(2),
            idle_poll: Duration::from_millis(100),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_is_reasonable() {
        let c = VaultWatcherConfig::default();
        assert!(!c.debounce.is_zero());
        assert!(c.debounce <= Duration::from_secs(1));
        assert!(c.self_op_ttl >= c.debounce);
        assert!(!c.idle_poll.is_zero());
    }

    #[test]
    fn config_is_copy() {
        // VaultWatcherConfig is Copy so it can be cheaply moved into the
        // processing thread without Arc/Mutex.
        fn assert_copy<T: Copy>() {}
        assert_copy::<VaultWatcherConfig>();
    }
}
