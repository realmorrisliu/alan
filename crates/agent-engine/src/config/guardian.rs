//! Dedicated Guardian reviewer configuration.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default aggregate timeout in seconds for a single Guardian escalation review
/// (including retries and optional bounded fallback to the primary connection).
pub const DEFAULT_GUARDIAN_TIMEOUT_SECS: u64 = 30;

/// Default maximum number of Guardian fallback calls to the primary connection
/// allowed in a single logical turn.
pub const DEFAULT_GUARDIAN_MAX_FALLBACKS_PER_TURN: usize = 2;

/// Configuration governing dedicated Guardian review execution and fallback bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardianConfig {
    /// Optional Host-supplied custom policy document supplementing/overriding the
    /// default Guardian reviewer guidance.
    #[serde(default)]
    pub policy_path: Option<PathBuf>,
    /// Aggregate wall-clock budget in seconds across primary Guardian calls and
    /// bounded fallback within one escalation review.
    #[serde(default = "default_guardian_timeout_secs")]
    pub timeout_secs: u64,
    /// Maximum number of fallbacks to the main generation Connection permitted
    /// per logical turn when the dedicated Guardian Connection is unavailable.
    #[serde(default = "default_guardian_max_fallbacks_per_turn")]
    pub max_fallbacks_per_turn: usize,
}

fn default_guardian_timeout_secs() -> u64 {
    DEFAULT_GUARDIAN_TIMEOUT_SECS
}

fn default_guardian_max_fallbacks_per_turn() -> usize {
    DEFAULT_GUARDIAN_MAX_FALLBACKS_PER_TURN
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            policy_path: None,
            timeout_secs: default_guardian_timeout_secs(),
            max_fallbacks_per_turn: default_guardian_max_fallbacks_per_turn(),
        }
    }
}
