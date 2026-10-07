//! Opt-in advice on ordinary input; this module never chooses the execution path.
use std::{sync::Arc, time::Duration};

use alan_agent_protocol::{ContentPart, InputIntent, Op, Submission};
use alan_ap::InProcessTransport;
use alan_llm::{ChoiceEvaluationRequest, EvaluationCandidate, EvaluationSelection};
use anyhow::{Context, Result, ensure};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::{
    NamespaceRuntimeEnvironment, model_binding::CallableIdentity, transition::RuntimeLoopState,
};
use crate::agent_machine::AgentMachine;
use crate::agent_machine::evaluation::{
    BypassReason, Candidate, EVENT_TYPE, Identity, Observation, Outcome, Usage,
};
use crate::rollout::{RolloutItem, RolloutRecorder};

/// Client admission surface supplied explicitly by the launching host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationSurface {
    Interactive,
    Redirected,
}

pub(crate) type EvaluationPublisher =
    Arc<dyn Fn(Option<serde_json::Value>) -> BoxFuture<'static, Result<()>> + Send + Sync>;

pub(crate) struct ShadowEvaluation {
    root: InProcessTransport,
    identity: CallableIdentity,
    surface: EvaluationSurface,
    deadline_ms: u64,
}

impl NamespaceRuntimeEnvironment {
    /// Attach a separately captured evaluator without changing generation authority.
    pub fn with_shadow_evaluation(
        mut self,
        root: InProcessTransport,
        identity: CallableIdentity,
        surface: EvaluationSurface,
        deadline_ms: u64,
    ) -> Result<Self> {
        ensure!(
            (1..=30_000).contains(&deadline_ms),
            "invalid evaluation deadline"
        );
        self.shadow_evaluation = Some(Arc::new(ShadowEvaluation {
            root,
            identity,
            surface,
            deadline_ms,
        }));
        Ok(self)
    }

    /// Inject the AgentFS owner's read-only projection publisher; never a file writer.
    pub fn with_evaluation_publisher<F, Fut>(mut self, publish: F) -> Self
    where
        F: Fn(Option<serde_json::Value>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        self.evaluation_publisher = Some(Arc::new(move |value| Box::pin(publish(value))));
        self
    }

    pub(crate) async fn publish_evaluation(
        &self,
        observation: Option<serde_json::Value>,
    ) -> Result<()> {
        if let Some(publish) = &self.evaluation_publisher {
            publish(observation).await?;
        }
        Ok(())
    }
}

impl RuntimeLoopState {
    pub(crate) async fn dispatch_input(
        &mut self,
        submission: &Submission,
        cancel: &CancellationToken,
    ) -> Result<()> {
        dispatch_input(&mut self.machine, &self.environment, submission, cancel).await
    }

    pub(crate) async fn observe_input_shadow(
        &mut self,
        submission: &Submission,
        cancel: &CancellationToken,
    ) -> Result<()> {
        observe_input_shadow(&mut self.machine, &self.environment, submission, cancel).await
    }
}

pub(super) async fn dispatch_input(
    machine: &mut AgentMachine,
    environment: &NamespaceRuntimeEnvironment,
    submission: &Submission,
    cancel: &CancellationToken,
) -> Result<()> {
    observe_input_shadow(machine, environment, submission, cancel).await?;
    machine.dispatch_input(submission).await
}

async fn observe_input_shadow(
    machine: &mut AgentMachine,
    environment: &NamespaceRuntimeEnvironment,
    submission: &Submission,
    cancel: &CancellationToken,
) -> Result<()> {
    let Some(shadow) = environment.shadow_evaluation.clone() else {
        return Ok(());
    };
    // Ordinary input waiting behind an interaction is not response admission.
    if machine.has_pending_interaction() && matches!(submission.op, Op::Input { .. }) {
        return Ok(());
    }
    // Response identity belongs to the request, not its transient delivery UUID.
    let (correlation_id, parts, bypass) = match &submission.op {
        Op::Input { parts, .. } => (
            submission.id.clone(),
            parts,
            match submission.intent {
                InputIntent::Agent => None,
                InputIntent::ForceAgent => Some(BypassReason::ExplicitAgent),
                InputIntent::Command => Some(BypassReason::ExplicitCommand),
            },
        ),
        Op::Resume {
            request_id,
            content,
        } => {
            // Host Mount completion is service-owned control, not client response
            // admission. Unknown/stale responses retain their ordinary rejection.
            if !matches!(
                machine.pending_yield(request_id),
                Some(
                    crate::agent_machine::PendingYield::Confirmation(_)
                        | crate::agent_machine::PendingYield::StructuredInput(_)
                )
            ) {
                return Ok(());
            }
            (
                format!("response:{request_id}"),
                content,
                Some(BypassReason::RequestResponse),
            )
        }
        _ => return Ok(()),
    };
    // This digest describes the actual Machine payload. The qualification
    // collector separately retains the raw client bytes before form/prefix parsing.
    let text = match parts.as_slice() {
        [ContentPart::Text { text }] => text.clone(),
        _ if bypass.is_some() => serde_json::to_string(parts)?,
        _ => anyhow::bail!("shadow evaluation requires one original text body"),
    };
    let started_at = Instant::now();
    let expires = started_at + Duration::from_millis(shadow.deadline_ms);
    let digest = if matches!(submission.op, Op::Resume { .. }) {
        // Include content-part tags so text containing JSON cannot alias a form response.
        hex::encode(Sha256::digest(serde_json::to_vec(parts)?))
    } else {
        hex::encode(Sha256::digest(text.as_bytes()))
    };
    let recorder = machine
        .input_recorder()
        .context("evaluation requires durable rollout storage")?;
    let history = bounded(expires, cancel, async {
        recorder.flush().await?;
        RolloutRecorder::load_history(recorder.path()).await
    })
    .await?;
    // Repeated admission reconciles the same durable attempt before allocation.
    for item in history {
        if let RolloutItem::Event(event) = item
            && event.event_type == EVENT_TYPE
        {
            let prior: Observation = serde_json::from_value(event.payload.clone())?;
            if prior.identity.submission_id == correlation_id {
                ensure!(
                    prior.outcome.bypass_reason() == bypass
                        && prior.identity.input_sha256 == digest
                        && prior.identity.callable == shadow.identity
                        && prior.identity.surface == shadow.surface
                        && prior.identity.deadline_ms == shadow.deadline_ms,
                    "repeated evaluation changed captured input or authority"
                );
                return bounded(expires, cancel, async {
                    machine
                        .persist_evaluation_observation(event.payload)
                        .await?;
                    ensure!(
                        !cancel.is_cancelled(),
                        super::NamespaceEvaluationFailure::Cancelled
                    );
                    environment
                        .publish_evaluation(machine.evaluation_observation.clone())
                        .await
                })
                .await;
            }
        }
    }
    let candidates: Vec<_> = [
        ("command", "The original input is already a literal shell command; do not rewrite natural language into a command."),
        ("agent", "The original input is a natural language request for the Agent."),
        ("ambiguous", "The original input cannot be confidently classified as a literal command or Agent request."),
    ].into_iter().map(|(id, description)| EvaluationCandidate { id: id.into(), description: description.into() }).collect();
    let request = ChoiceEvaluationRequest {
        input: text.clone(),
        candidates,
    };
    let mut observation = Observation {
        identity: Identity {
            source_rollout_id: recorder.rollout_id().into(),
            submission_id: correlation_id,
            input_sha256: digest,
            surface: shadow.surface.clone(),
            operation_id: None,
            callable: shadow.identity.clone(),
            schema: "choice.v1".into(),
            deadline_ms: shadow.deadline_ms,
            candidates: request
                .candidates
                .iter()
                .map(|c| Candidate {
                    id: c.id.clone(),
                    description: c.description.clone(),
                })
                .collect(),
        },
        outcome: Outcome::Started,
        elapsed_ms: None,
        usage: None,
    };
    if let Some(reason) = bypass {
        observation.outcome = Outcome::Bypassed {
            reason,
            evaluator_calls: 0,
        };
        observation.elapsed_ms = Some(started_at.elapsed().as_millis() as u64);
        return bounded(expires, cancel, async {
            machine
                .persist_evaluation_observation(serde_json::to_value(&observation)?)
                .await?;
            ensure!(
                !cancel.is_cancelled(),
                super::NamespaceEvaluationFailure::Cancelled
            );
            environment
                .publish_evaluation(machine.evaluation_observation.clone())
                .await
        })
        .await;
    }
    let remaining = expires
        .saturating_duration_since(Instant::now())
        .as_millis() as u64;
    ensure!(remaining > 0, super::NamespaceEvaluationFailure::TimedOut);
    let evaluation_environment = NamespaceRuntimeEnvironment::new(
        shadow.root.clone(),
        environment.agent_path(),
        shadow.identity.profile.clone(),
    );
    let operation = evaluation_environment
        .allocate_choice_evaluation(shadow.identity.clone(), request.clone(), remaining, cancel)
        .await?;
    observation.identity.operation_id = Some(operation.operation_id().into());
    let acknowledgement = bounded(
        expires,
        cancel,
        machine.persist_evaluation_observation(serde_json::to_value(&observation)?),
    )
    .await;
    if !matches!(acknowledgement, Ok(true)) {
        let aborted = operation.abort().await;
        aborted.context("evaluation start was not acknowledged")?;
        acknowledgement.context("evaluation start acknowledgement uncertain")?;
        anyhow::bail!("evaluation already recorded; commit refused");
    }
    let published = bounded(
        expires,
        cancel,
        environment.publish_evaluation(machine.evaluation_observation.clone()),
    )
    .await;
    if let Err(error) = published {
        operation
            .abort()
            .await
            .context("evaluation start publication failed")?;
        return Err(error);
    }
    let result = match operation.commit(cancel).await {
        Err(error)
            if error
                .downcast_ref::<super::NamespaceEvaluationUncertainty>()
                .is_some() =>
        {
            return Err(error);
        }
        result => result,
    };
    observation.elapsed_ms = Some(started_at.elapsed().as_millis().min(u64::MAX as u128) as u64);
    observation.outcome = match result {
        Ok(result) => {
            observation.usage = result.usage.map(|usage| Usage {
                input_tokens: usage.prompt_tokens as u64,
                output_tokens: usage.completion_tokens as u64,
            });
            if cancel.is_cancelled() {
                Outcome::Cancelled
            } else {
                match result.selection {
                    EvaluationSelection::Selected(candidate_id) => {
                        Outcome::Selected { candidate_id }
                    }
                    EvaluationSelection::NoMatch => Outcome::NoMatch,
                }
            }
        }
        Err(error) => match error.downcast_ref::<super::NamespaceEvaluationFailure>() {
            Some(super::NamespaceEvaluationFailure::Cancelled) => Outcome::Cancelled,
            Some(super::NamespaceEvaluationFailure::TimedOut) => Outcome::TimedOut,
            Some(super::NamespaceEvaluationFailure::Malformed) => Outcome::Malformed,
            _ => Outcome::Unavailable,
        },
    };
    // Timeout/cancel outcomes still get one bounded evidence-write window.
    // A successful result remains cancellable until publication. Lost write
    // acknowledgements stay uncertain; they never permit another model call.
    let settlement_cancel = if matches!(
        observation.outcome,
        Outcome::Selected { .. } | Outcome::NoMatch
    ) {
        cancel.clone()
    } else {
        CancellationToken::new()
    };
    bounded(
        Instant::now() + Duration::from_secs(1),
        &settlement_cancel,
        async {
            machine
                .persist_evaluation_observation(serde_json::to_value(observation)?)
                .await?;
            ensure!(
                !settlement_cancel.is_cancelled(),
                super::NamespaceEvaluationFailure::Cancelled
            );
            environment
                .publish_evaluation(machine.evaluation_observation.clone())
                .await
        },
    )
    .await
}

async fn bounded<T>(
    expires: Instant,
    cancel: &CancellationToken,
    work: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(super::NamespaceEvaluationFailure::Cancelled.into()),
        _ = tokio::time::sleep_until(expires) => Err(super::NamespaceEvaluationFailure::TimedOut.into()),
        result = work => result,
    }
}
