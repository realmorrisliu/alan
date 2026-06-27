//! Per-file qid version tracking (§5.1): a [`Qid`](crate::Qid)'s `version` must
//! bump when that file's observable content changes, so a client comparing
//! qid/version pairs detects a change rather than retaining stale cached data.
//!
//! A file server keeps one [`VersionTable`] in the state that owns its mutations
//! and `bump`s a node's key whenever it changes that node's content; qid
//! construction reads the node's current `version`. The key is the server's own
//! choice (a node path, a pid, a name hash) — whatever uniquely identifies the
//! mutable file. Streams need not be tracked here: their freshness is the read
//! offset, not the qid version.

use std::collections::HashMap;

/// Tracks the current qid version of each mutable file, keyed by a server-chosen
/// identifier. Unseen keys are version `0`; [`bump`](Self::bump) increments.
#[derive(Debug, Default, Clone)]
pub struct VersionTable {
    versions: HashMap<u64, u32>,
}

impl VersionTable {
    /// A table in which every key is at version `0`.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current version for `key` (`0` if it has never been bumped).
    pub fn get(&self, key: u64) -> u32 {
        self.versions.get(&key).copied().unwrap_or(0)
    }

    /// Record that the file identified by `key` changed: bump its version. Wraps
    /// on overflow (a version that has changed 2^32 times has long since been
    /// re-read); the contract only needs distinct-on-change, not monotonic.
    pub fn bump(&mut self, key: u64) {
        let v = self.versions.entry(key).or_insert(0);
        *v = v.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unseen_key_is_version_zero() {
        let t = VersionTable::new();
        assert_eq!(t.get(42), 0);
    }

    #[test]
    fn bump_increments_only_that_key() {
        let mut t = VersionTable::new();
        t.bump(1);
        t.bump(1);
        assert_eq!(t.get(1), 2);
        assert_eq!(t.get(2), 0, "other keys are unaffected");
    }
}
