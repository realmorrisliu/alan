//! Retry logic with exponential backoff for LLM requests.
//!
//! This module provides retry functionality inspired by Codex's design:
//! - Exponential backoff with jitter
//! - Configurable retry limits
//! - Error classification (retryable vs non-retryable)

use std::time::Duration;
use tracing::debug;

/// Initial delay for backoff (milliseconds)
const INITIAL_DELAY_MS: u64 = 200;
/// Backoff multiplier factor
const BACKOFF_FACTOR: f64 = 2.0;
/// Maximum backoff delay (seconds)
const MAX_BACKOFF_SECS: u64 = 30;

/// Default number of retries for LLM requests
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// Calculate backoff delay with exponential backoff and jitter
///
/// Formula: delay = INITIAL_DELAY_MS * BACKOFF_FACTOR^(attempt-1) * jitter(0.9..1.1)
pub fn backoff_delay(attempt: u32) -> Duration {
    let exp = BACKOFF_FACTOR.powi(attempt.saturating_sub(1) as i32);
    let base_ms = (INITIAL_DELAY_MS as f64 * exp) as u64;
    let jitter: f64 = rand::random::<f64>() * 0.2 + 0.9; // 0.9..1.1
    let delay_ms = (base_ms as f64 * jitter) as u64;

    // Cap at maximum delay
    let capped_delay = delay_ms.min(MAX_BACKOFF_SECS * 1000);

    debug!(attempt, delay_ms = capped_delay, "Calculated backoff delay");
    Duration::from_millis(capped_delay)
}

/// Check if an error is retryable
///
/// Retryable errors:
/// - Network timeouts
/// - Connection failures
/// - Transient server errors (5xx)
/// - Rate limiting (429)
/// - Stream disconnections
///
/// Non-retryable errors:
/// - Invalid API keys (401)
/// - Permission denied (403)
/// - Bad requests (400)
/// - Content filtered/safety blocked
pub fn is_retryable(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();

    // Check for explicitly retryable conditions
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

    // Check for explicitly non-retryable conditions
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

    // Default: retry on network-related errors
    // Check if the error has a source that might be retryable
    let source_str = error.root_cause().to_string().to_lowercase();
    if source_str.contains("hyper") || source_str.contains("reqwest") || source_str.contains("io") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_delay_increases_with_attempts() {
        let d1 = backoff_delay(1);
        let d2 = backoff_delay(2);
        let d3 = backoff_delay(3);

        // Each delay should generally be longer (with some jitter)
        assert!(d2 >= d1 * 15 / 10 || d2 > d1); // ~2x factor with tolerance
        assert!(d3 >= d2 * 15 / 10 || d3 > d2);
    }

    #[test]
    fn test_backoff_delay_capped() {
        let d10 = backoff_delay(10);
        // Should not exceed MAX_BACKOFF_SECS
        assert!(d10 <= Duration::from_secs(MAX_BACKOFF_SECS));
    }

    #[test]
    fn test_is_retryable_timeout() {
        let err = anyhow::anyhow!("Request timed out after 30s");
        assert!(is_retryable(&err));
    }

    #[test]
    fn test_is_retryable_connection() {
        let err = anyhow::anyhow!("Connection refused");
        assert!(is_retryable(&err));
    }

    #[test]
    fn test_is_not_retryable_auth() {
        let err = anyhow::anyhow!("Invalid API key");
        assert!(!is_retryable(&err));
    }

    #[test]
    fn test_is_not_retryable_safety() {
        let err = anyhow::anyhow!("Content blocked by safety filter");
        assert!(!is_retryable(&err));
    }
}
