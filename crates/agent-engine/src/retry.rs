//! Retry logic with exponential backoff for LLM requests.

use std::time::Duration;

use tracing::debug;

const INITIAL_DELAY_MS: u64 = 200;
const BACKOFF_FACTOR: f64 = 2.0;
const MAX_BACKOFF_SECS: u64 = 30;

/// Default number of retries for LLM requests.
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// Calculate an exponential backoff delay with small jitter.
pub fn backoff_delay(attempt: u32) -> Duration {
    let exp = BACKOFF_FACTOR.powi(attempt.saturating_sub(1) as i32);
    let base_ms = (INITIAL_DELAY_MS as f64 * exp) as u64;
    let jitter: f64 = rand::random::<f64>() * 0.2 + 0.9;
    let delay_ms = (base_ms as f64 * jitter) as u64;
    let capped_delay = delay_ms.min(MAX_BACKOFF_SECS * 1000);

    debug!(attempt, delay_ms = capped_delay, "calculated backoff delay");
    Duration::from_millis(capped_delay)
}

/// Check whether an LLM/provider error looks transient enough to retry.
pub fn is_retryable(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();

    let retryable_patterns = [
        "timeout",
        "timed out",
        "connection",
        "stream",
        "disconnected",
        "reset",
        "broken pipe",
        "try again",
        "temporary",
        "unavailable",
        "too many requests",
        "rate limit",
        "429",
        "503",
        "502",
        "500",
        "504",
    ];

    for pattern in &retryable_patterns {
        if error_str.contains(pattern) {
            return true;
        }
    }

    let non_retryable_patterns = [
        "invalid api key",
        "unauthorized",
        "permission denied",
        "forbidden",
        "bad request",
        "invalid request",
        "content filtered",
        "safety",
        "blocked",
        "context length",
        "token limit",
        "quota exceeded",
    ];

    for pattern in &non_retryable_patterns {
        if error_str.contains(pattern) {
            return false;
        }
    }

    let source_str = error.root_cause().to_string().to_lowercase();
    source_str.contains("hyper") || source_str.contains("reqwest") || source_str.contains("io")
}
