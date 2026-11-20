//! Stable per-client session identifiers for provider cache affinity.
//!
//! Provider prompt/KV caches route on a key that must stay constant for the
//! lifetime of one client (one Connection in one Alan invocation). Generating a
//! fresh identifier per request would scatter cache routing, so adapters mint
//! one identifier at construction and reuse it on every request.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Mint a 128-bit, hex-encoded identifier that is unique per call.
///
/// Uses the process-random `RandomState` seed mixed with wall-clock time, the
/// process id and a monotonic sequence, so no extra RNG dependency is needed.
pub fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut words = [0_u64; 2];
    for (salt, word) in words.iter_mut().enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        hasher.write_u64(sequence);
        hasher.write_usize(salt);
        *word = hasher.finish();
    }
    format!("{:016x}{:016x}", words[0], words[1])
}

/// Derive a fixed-length, opaque affinity id from a caller session key.
///
/// The same key always yields the same id within a build, so every request of
/// one Agent Process routes to the same provider cache while the raw key (which
/// may contain host paths) never leaves the process.
pub fn derive_session_id(session_key: &str) -> String {
    let mut words = [0_u64; 2];
    for (salt, word) in words.iter_mut().enumerate() {
        let mut hasher = std::hash::DefaultHasher::new();
        hasher.write(b"alan-session-affinity");
        hasher.write_usize(salt);
        hasher.write(session_key.as_bytes());
        *word = hasher.finish();
    }
    format!("{:016x}{:016x}", words[0], words[1])
}

/// Resolve the affinity id for one request: the caller key when present,
/// otherwise the client's own stable id.
pub fn request_session_id(session_key: Option<&str>, client_session_id: &str) -> String {
    session_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(derive_session_id)
        .unwrap_or_else(|| client_session_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::{derive_session_id, new_session_id, request_session_id};

    #[test]
    fn session_ids_are_unique_hex() {
        let first = new_session_id();
        let second = new_session_id();
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn derived_ids_are_stable_opaque_and_key_specific() {
        let first = derive_session_id("boot-1/agent/root");
        assert_eq!(first, derive_session_id("boot-1/agent/root"));
        assert_ne!(first, derive_session_id("boot-1/agent/7"));
        assert_eq!(first.len(), 32);
        assert!(!first.contains("agent"));
        assert_eq!(request_session_id(None, "client"), "client");
        assert_eq!(request_session_id(Some("  "), "client"), "client");
        assert_eq!(
            request_session_id(Some("boot-1/agent/root"), "client"),
            first
        );
    }
}
