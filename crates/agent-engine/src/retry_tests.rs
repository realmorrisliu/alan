use super::*;
use alan_ap::ErrorCode;

#[test]
fn typed_permanent_cause_wins_over_transient_context() {
    for reason in [
        "stream_error:authentication",
        "stream_error:http",
        "stream_error:safety",
        "stream_error:recitation",
    ] {
        let error = anyhow::Error::new(ErrorCode::Io).context(GenerationCause::new(reason));
        assert_eq!(
            error.to_string(),
            format!("llmfs generation failed: {reason}")
        );
        let error = error.context("temporary stream connection failure");
        assert!(!is_retryable(&error), "{reason}");
        assert_eq!(error.downcast_ref::<ErrorCode>(), Some(&ErrorCode::Io));
        assert!(error.downcast_ref::<GenerationCause>().is_some());
    }
}

#[test]
fn typed_transient_and_compatibility_causes_keep_retry_behavior() {
    for reason in [
        "stream_error:rate_limit",
        "stream_error:unavailable",
        "stream_error:timeout",
        "stream_error:connect",
        "stream_error:body",
        "stream_error:closed",
        "stream_error:parse",
        "stream_error:unknown",
        "stream_error",
        "untrusted https://private.example",
        "stop",
    ] {
        let error = anyhow::Error::new(ErrorCode::Io).context(GenerationCause::new(reason));
        assert!(
            is_retryable(&error.context("invalid request context")),
            "{reason}"
        );
    }
    let cause = GenerationCause::new("untrusted https://private.example");
    assert_eq!(
        cause.to_string(),
        "llmfs generation failed: stream_error:unknown"
    );
}

#[test]
fn gemini_prompt_block_normalization_and_actual_retry_decision() {
    for (raw, normalized, retry) in [
        (
            "stream_error:prompt_blocked:safety",
            "stream_error:safety",
            false,
        ),
        (
            "stream_error:prompt_blocked:recitation",
            "stream_error:recitation",
            false,
        ),
        ("stream_error:safety", "stream_error:safety", false),
        ("stream_error:recitation", "stream_error:recitation", false),
        ("stream_error:timeout", "stream_error:timeout", true),
        ("stream_error:rate_limit", "stream_error:rate_limit", true),
        (
            "stream_error:prompt_blocked:new_reason",
            "stream_error:unknown",
            true,
        ),
        ("stream_error:unknown", "stream_error:unknown", true),
    ] {
        assert_eq!(alan_llm::safe_finish_reason(raw), normalized, "{raw}");
        let error = anyhow::Error::new(ErrorCode::Io).context(GenerationCause::new(raw));
        assert_eq!(
            is_retryable(&error.context("temporary stream connection")),
            retry,
            "{raw}"
        );
    }
}

#[test]
fn untyped_legacy_heuristics_are_unchanged() {
    for reason in [
        "503 unavailable",
        "stream failure",
        "timeout",
        "connection reset",
    ] {
        assert!(is_retryable(&anyhow::anyhow!(reason)));
    }
    for reason in [
        "unauthorized",
        "invalid request",
        "content filtered",
        "ordinary failure",
    ] {
        assert!(!is_retryable(&anyhow::anyhow!(reason)));
    }
}
