//! Explicit invocation-only reviewer capture. Its output never controls an action.

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

use super::*;
use crate::agent_machine::{
    AgentMachine,
    guardian_shadow::{Identity, Observation, Stage},
};
use crate::runtime::model_binding::CapturedCallable;

pub(crate) struct GuardianShadow {
    callable: CapturedCallable,
    policy: String,
}

pub(crate) struct ShadowReviewRequest<'a> {
    pub call_id: &'a str,
    pub context: ReviewContext<'a>,
    pub baseline: &'a ReviewOutcome,
    pub timeout_secs: u64,
}

impl GuardianShadow {
    pub(crate) fn new(callable: CapturedCallable, policy: Option<String>) -> Result<Self> {
        let policy = policy.unwrap_or_else(|| DEFAULT_REVIEWER_POLICY.to_string());
        ensure!(!policy.trim().is_empty(), "Guardian shadow policy is empty");
        ensure!(
            policy.len() <= 65_536,
            "Guardian shadow policy exceeds 64 KiB"
        );
        Ok(Self { callable, policy })
    }

    pub(crate) async fn observe(
        &self,
        machine: &mut AgentMachine,
        generation: &NamespaceGeneration,
        review: ShadowReviewRequest<'_>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let ShadowReviewRequest {
            call_id,
            context,
            baseline,
            timeout_secs,
        } = review;
        let request = build_review_request(&ReviewContext {
            policy: &self.policy,
            transcript: context.transcript,
            approval_request: context.approval_request,
        });
        let task = machine
            .task_budget
            .as_ref()
            .context("Guardian shadow requires a task budget")?;
        let digest = |bytes: &[u8]| hex::encode(Sha256::digest(bytes));
        let identity = Identity {
            task_id: task.snapshot().await.task_id,
            tool_call_id: call_id.to_string(),
            action_digest: digest(&serde_json::to_vec(context.approval_request)?),
            request_digest: NamespaceGeneration::request_fingerprint(&request)?,
            policy_digest: digest(self.policy.as_bytes()),
            baseline_request_digest: NamespaceGeneration::request_fingerprint(
                &build_review_request(&context),
            )?,
            baseline_binding: generation.identity().cloned(),
            reviewer: self.callable.identity.clone(),
        };
        let mut observation = Observation {
            identity,
            baseline: baseline.clone(),
            stage: Stage::Started,
        };
        if !machine.begin_guardian_shadow(observation.clone()).await? {
            return Ok(());
        }
        let candidate = generation.with_callable(&self.callable);
        let started = tokio::time::Instant::now();
        let result = generate_review(&candidate, request, timeout_secs, cancel).await;
        if let Err(error) = &result
            && (error.is::<crate::agent_machine::task_budget::Exhausted>()
                || error.is::<NamespaceOperationUncertainty>()
                || cancel.is_cancelled())
        {
            // Keep the acknowledged start incomplete; neither failure permits a replay.
            return Err(result.unwrap_err());
        }
        ensure!(!cancel.is_cancelled(), "Guardian shadow cancelled");
        let usage = result
            .as_ref()
            .ok()
            .and_then(|response| response.usage)
            .map(|usage| {
                serde_json::json!({
                    "prompt_tokens": usage.prompt_tokens,
                    "cached_prompt_tokens": usage.cached_prompt_tokens,
                    "completion_tokens": usage.completion_tokens,
                    "total_tokens": usage.total_tokens,
                    "reasoning_tokens": usage.reasoning_tokens,
                })
            });
        observation.stage = Stage::Finished {
            candidate: review_generation_result(result),
            elapsed_millis: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            final_response_usage: usage,
            cost_microusd: None,
        };
        machine.persist_guardian_shadow(observation).await
    }
}
