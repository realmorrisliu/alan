//! Bounded Tier 2 `Uncalibrated` structured-generation rubric fallback.
//!
//! Invoked ONLY when native `decide()` returns [`NamespaceDecisionFailure::Unavailable`].
//! Dispatches a single no-tools structured JSON-Schema generation call via
//! [`alan_llm::build_fallback_generation_request`] and validates the output via
//! [`alan_llm::decode_fallback_generation_response`], which strictly stamps
//! [`alan_llm::CalibrationKind::Uncalibrated`]. Any failure, timeout, cancellation,
//! or malformed output fails closed.

use alan_llm::{
    CalibrationKind, DecisionRequest, DecisionResponse, build_fallback_generation_request,
    decode_fallback_generation_response,
};
use anyhow::Result;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::runtime::transition::{
    NamespaceDecisionConnection, NamespaceDecisionFailure, NamespaceGeneration,
};

pub(crate) const MAX_DECISION_DEADLINE_MS: u64 = 30_000;

pub(crate) enum DecisionAttempt {
    Success(DecisionResponse),
    RetryNext {
        reason: String,
        /// Captured connection whose native `decide()` returned `Unavailable`,
        /// eligible for Tier 2 `Uncalibrated` structured-generation fallback.
        fallback_generation: Option<NamespaceGeneration>,
    },
    FailClosed(String),
}

pub(crate) enum Tier2Attempt {
    Success(DecisionResponse),
    Unavailable(String),
    FailClosed(String),
}

pub(crate) fn remaining_duration(deadline: Instant) -> Option<std::time::Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        None
    } else {
        Some(remaining)
    }
}

pub(crate) fn remaining_decision_ms(deadline: Instant) -> Option<u64> {
    let remaining = remaining_duration(deadline)?;
    let ms = (remaining.as_millis() as u64).min(MAX_DECISION_DEADLINE_MS);
    if ms == 0 { None } else { Some(ms) }
}

/// Attempt Tier 1 native `decide()` on one captured Connection.
pub(crate) async fn try_native_decision(
    connection: &NamespaceDecisionConnection,
    request: DecisionRequest,
    deadline: Instant,
    cancel: &CancellationToken,
    label: &str,
) -> DecisionAttempt {
    if cancel.is_cancelled() {
        return DecisionAttempt::FailClosed(format!("{label} cancelled"));
    }
    let Some(deadline_ms) = remaining_decision_ms(deadline) else {
        return DecisionAttempt::FailClosed(format!("{label} aggregate deadline expired"));
    };
    classify_decision_result(
        connection.decide(request, deadline_ms, cancel).await,
        Some(connection.as_unmetered_generation()),
        label,
    )
}

/// Attempt Tier 1 native `decide()` on one Connection, falling back seamlessly to
/// Tier 2 `Uncalibrated` structured generation ONLY on `NamespaceDecisionFailure::Unavailable`.
pub(crate) async fn try_decision_with_tier2(
    connection: &NamespaceDecisionConnection,
    request: DecisionRequest,
    deadline: Instant,
    cancel: &CancellationToken,
    label: &str,
) -> DecisionAttempt {
    match try_native_decision(connection, request.clone(), deadline, cancel, label).await {
        DecisionAttempt::Success(response) => DecisionAttempt::Success(response),
        DecisionAttempt::FailClosed(reason) => DecisionAttempt::FailClosed(reason),
        DecisionAttempt::RetryNext {
            reason,
            fallback_generation: Some(generation),
        } => {
            tracing::debug!(
                connection = connection.connection_name(),
                %reason,
                "Native decide() unavailable; falling back to Tier 2 uncalibrated structured rubric"
            );
            match evaluate_tier2_structured_rubric(&generation, &request, deadline, cancel, label)
                .await
            {
                Tier2Attempt::Success(response) => DecisionAttempt::Success(response),
                Tier2Attempt::FailClosed(t2_reason) => DecisionAttempt::FailClosed(t2_reason),
                Tier2Attempt::Unavailable(t2_reason) => DecisionAttempt::RetryNext {
                    reason: format!("{reason}; {t2_reason}"),
                    fallback_generation: None,
                },
            }
        }
        DecisionAttempt::RetryNext {
            reason,
            fallback_generation: None,
        } => DecisionAttempt::RetryNext {
            reason,
            fallback_generation: None,
        },
    }
}

/// Run a single bounded no-tools structured generation call to evaluate `request`
/// as `CalibrationKind::Uncalibrated`.
pub(crate) async fn evaluate_tier2_structured_rubric(
    generation: &NamespaceGeneration,
    request: &DecisionRequest,
    deadline: Instant,
    cancel: &CancellationToken,
    label: &str,
) -> Tier2Attempt {
    if cancel.is_cancelled() {
        return Tier2Attempt::FailClosed(format!("{label} cancelled"));
    }
    let Some(remaining) = remaining_duration(deadline) else {
        return Tier2Attempt::FailClosed(format!("{label} aggregate deadline expired"));
    };
    let gen_request = match build_fallback_generation_request(request) {
        Ok(req) => req,
        Err(err) => {
            return Tier2Attempt::FailClosed(format!(
                "tier-2 structured rubric request invalid: {err}"
            ));
        }
    };
    debug_assert!(
        gen_request.tools.is_empty(),
        "Tier 2 structured rubric generation request must carry zero tools"
    );

    let child_cancel = cancel.child_token();
    let timeout_secs = remaining.as_secs().max(1);
    let gen_result = match tokio::time::timeout(
        remaining,
        generation.generate_controlled(&gen_request, timeout_secs, &child_cancel),
    )
    .await
    {
        Ok(res) => res,
        Err(_) => {
            child_cancel.cancel();
            return Tier2Attempt::FailClosed(format!("{label} tier-2 structured rubric timed out"));
        }
    };

    let gen_response = match gen_result {
        Ok(resp) => resp,
        Err(error) => {
            if cancel.is_cancelled() {
                return Tier2Attempt::FailClosed(format!("{label} cancelled"));
            }
            return Tier2Attempt::Unavailable(format!(
                "tier-2 structured rubric unavailable: {error:#}"
            ));
        }
    };

    let (provider, model) = match generation.read_llm_connection_profile().await {
        Ok((provider, model)) => (
            provider,
            model.unwrap_or_else(|| generation.connection_name().to_string()),
        ),
        Err(_) => (
            "unknown".to_string(),
            generation.connection_name().to_string(),
        ),
    };

    match decode_fallback_generation_response(&provider, &model, request, gen_response) {
        Ok(response) => {
            debug_assert_eq!(
                response.calibration,
                CalibrationKind::Uncalibrated,
                "Tier 2 structured generation fallback must always stamp Uncalibrated"
            );
            Tier2Attempt::Success(response)
        }
        Err(err) => Tier2Attempt::FailClosed(format!(
            "tier-2 structured rubric returned malformed output: {err}"
        )),
    }
}

fn classify_decision_result(
    result: Result<DecisionResponse>,
    fallback_generation: Option<NamespaceGeneration>,
    label: &str,
) -> DecisionAttempt {
    match result {
        Ok(response) => DecisionAttempt::Success(response),
        Err(error) => match error.downcast_ref::<NamespaceDecisionFailure>() {
            Some(NamespaceDecisionFailure::Cancelled) => {
                DecisionAttempt::FailClosed(format!("{label} cancelled"))
            }
            Some(NamespaceDecisionFailure::TimedOut) => {
                DecisionAttempt::FailClosed(format!("{label} timed out"))
            }
            Some(NamespaceDecisionFailure::Malformed) => {
                DecisionAttempt::FailClosed(format!("{label} returned malformed output"))
            }
            Some(NamespaceDecisionFailure::Unavailable) | None => DecisionAttempt::RetryNext {
                reason: format!("decision call unavailable: {error:#}"),
                fallback_generation,
            },
        },
    }
}
