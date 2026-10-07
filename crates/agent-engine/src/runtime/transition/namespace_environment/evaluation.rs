//! One mounted finite-choice operation, allocated before the Machine durability barrier.
use std::time::Duration;

use alan_ap::{Fid, OpenMode};
use alan_llm::{ChoiceEvaluationRequest, ChoiceEvaluationResponse, EvaluationSelection};
use anyhow::{Result, ensure};
use serde::Deserialize;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::{
    NamespaceRuntimeEnvironment,
    client::{NamespaceClient, NamespaceFidGuard},
};
use crate::runtime::model_binding::CallableIdentity;

/// Typed terminal failures never select a candidate or request fallback work.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum NamespaceEvaluationFailure {
    #[error("evaluation cancelled")]
    Cancelled,
    #[error("evaluation deadline exceeded")]
    TimedOut,
    #[error("evaluation result malformed")]
    Malformed,
    #[error("evaluation unavailable")]
    Unavailable,
}
use NamespaceEvaluationFailure::*;

/// Cleanup uncertainty must not be relabeled as a confirmed terminal outcome.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum NamespaceEvaluationUncertainty {
    #[error("evaluation abort unconfirmed")]
    Abort,
    #[error("evaluation allocation identity unconfirmed")]
    AllocationIdentity,
}

/// A single allocated operation. Consuming commit prevents accidental redispatch.
/// Allocation performs no paid model call; the Machine must persist its start first.
pub struct NamespaceEvaluation {
    client: NamespaceClient,
    identity: CallableIdentity,
    operation_id: String,
    request: ChoiceEvaluationRequest,
    expires: Instant,
    armed: bool,
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

impl NamespaceRuntimeEnvironment {
    /// Allocate from the explicitly captured Connection before committing a request.
    /// The deadline starts here and includes the caller's intervening durable write.
    pub async fn allocate_choice_evaluation(
        &self,
        identity: CallableIdentity,
        request: ChoiceEvaluationRequest,
        deadline_ms: u64,
        cancel: &CancellationToken,
    ) -> Result<NamespaceEvaluation> {
        request.validate().map_err(|_| Malformed)?;
        ensure!(
            component(&self.llm_connection) && identity.profile == self.llm_connection,
            Malformed
        );
        ensure!((1..=30_000).contains(&deadline_ms), Malformed);
        let expires = Instant::now() + Duration::from_millis(deadline_ms);
        let client = NamespaceClient::new(self.root.clone());
        let path = format!("/mnt/llm/connections/{}/evaluate", self.llm_connection);
        let mut retained_id = None;
        let mut allocation_fid = None;
        let mut allocation_may_exist = false;
        let allocation = async {
            let fid = client.walk_to(&path).await?;
            allocation_fid = Some(NamespaceFidGuard::new(client.clone(), fid));
            allocation_may_exist = true;
            client.open(fid, OpenMode::ReadWrite).await?;
            let id = read_operation_id(&client, fid).await?;
            retained_id = Some(id.clone());
            allocation_fid.take().unwrap().close().await?;
            Ok::<_, anyhow::Error>(id)
        };
        let allocated = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(Cancelled.into()),
            _ = tokio::time::sleep_until(expires) => Err(TimedOut.into()),
            result = allocation => result,
        };
        let operation_id = match allocated {
            Ok(id) => id,
            Err(error) => {
                let mut error = classify(error);
                // Re-read the SAME allocator after an uncertain open/read. This
                // reconciles identity; it never opens another allocation.
                if allocation_may_exist
                    && retained_id.is_none()
                    && let Some(fid) = &allocation_fid
                {
                    retained_id = tokio::time::timeout(
                        Duration::from_secs(1),
                        read_operation_id(&client, fid.fid()),
                    )
                    .await
                    .ok()
                    .and_then(Result::ok);
                }
                if let Some(id) = retained_id {
                    let ctl = format!("/mnt/llm/connections/{}/{id}/ctl", self.llm_connection);
                    if abort(&client, &ctl).await.is_err() {
                        error = error.context(NamespaceEvaluationUncertainty::Abort);
                    }
                } else if allocation_may_exist {
                    error = error.context(NamespaceEvaluationUncertainty::AllocationIdentity);
                }
                if let Some(fid) = allocation_fid {
                    let _ = tokio::time::timeout(Duration::from_secs(1), fid.close()).await;
                }
                return Err(error);
            }
        };
        Ok(NamespaceEvaluation {
            client,
            identity,
            operation_id,
            request,
            expires,
            armed: true,
        })
    }
}

impl NamespaceEvaluation {
    /// Operation identity to include in the Machine's durable started record.
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    fn path(&self, file: &str) -> String {
        format!(
            "/mnt/llm/connections/{}/{}/{file}",
            self.identity.profile, self.operation_id
        )
    }

    /// Abort without committing data, for example after a failed durability barrier.
    pub async fn abort(mut self) -> Result<()> {
        let result = abort(&self.client, &self.path("ctl")).await;
        self.armed = false;
        result.map_err(|error| error.context(NamespaceEvaluationUncertainty::Abort))
    }

    /// Commit at most once, then validate typed advice against captured provenance.
    /// Errors do not retry generation, evaluation, or any effect.
    pub async fn commit(mut self, cancel: &CancellationToken) -> Result<ChoiceEvaluationResponse> {
        let mut data_fid: Option<Fid> = None;
        let mut terminal_observed = false;
        let work = async {
            let remaining = self
                .expires
                .saturating_duration_since(Instant::now())
                .as_millis();
            ensure!(remaining > 0, TimedOut);
            let candidates: Vec<_> = self.request.candidates.iter().map(|candidate| {
                serde_json::json!({"id":candidate.id,"description":candidate.description})
            }).collect();
            let bytes = serde_json::to_vec(&serde_json::json!({
                "version":1,"schema":"choice.v1","input":self.request.input,
                "candidates":candidates,"deadline_ms":remaining,
            }))?;
            ensure!(bytes.len() <= 1 << 20, Malformed);
            let fid = self.client.walk_to(&self.path("data")).await?;
            data_fid = Some(fid);
            self.client.open(fid, OpenMode::Write).await?;
            let mut offset = 0;
            while offset < bytes.len() {
                let written = self
                    .client
                    .write_at(fid, offset as u64, &bytes[offset..])
                    .await?;
                ensure!(written > 0 && written <= bytes.len() - offset, Unavailable);
                offset += written;
            }
            ensure!(!cancel.is_cancelled(), Cancelled);
            ensure!(Instant::now() < self.expires, TimedOut);
            // Taking the descriptor precedes commit: an uncertain clunk is never retried.
            data_fid = None;
            self.client.clunk(fid).await?;
            self.read_result(&mut terminal_observed).await
        };
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(Cancelled.into()),
            _ = tokio::time::sleep_until(self.expires) => Err(TimedOut.into()),
            result = work => result.map_err(classify),
        };
        if let Err(error) = result {
            // A valid terminal receipt already fences this operation. Aborting it
            // is rejected by llmfs and must not turn a settled error into uncertainty.
            if terminal_observed {
                self.armed = false;
                return Err(error);
            }
            let aborted = abort(&self.client, &self.path("ctl")).await;
            // Never clunk a buffered request unless abort was acknowledged: clunk
            // commits data. On uncertain abort retain the fid until server teardown.
            if aborted.is_ok()
                && let Some(fid) = data_fid
            {
                let _ = tokio::time::timeout(Duration::from_secs(1), self.client.clunk(fid)).await;
            }
            self.armed = false;
            if aborted.is_err() {
                return Err(error.context(NamespaceEvaluationUncertainty::Abort));
            }
            return Err(error);
        } else {
            self.armed = false;
        }
        result
    }

    async fn read_result(&self, terminal_observed: &mut bool) -> Result<ChoiceEvaluationResponse> {
        let events = self
            .client
            .open_path_guarded(&self.path("events"), OpenMode::Read)
            .await?;
        let mut bytes = Vec::new();
        loop {
            let part = self
                .client
                .read_at(events.fid(), bytes.len() as u64, 4096)
                .await?;
            ensure!(
                !part.is_empty() && bytes.len() + part.len() <= 1 << 20,
                Malformed
            );
            bytes.extend(part);
            if bytes.contains(&b'\n') {
                break;
            }
        }
        events.close().await?;
        let event: EvaluationEvent = serde_json::from_slice(&bytes).map_err(|_| Malformed)?;
        ensure!(event.version == 1, Malformed);
        let terminals = [
            event.done == Some(true),
            event.aborted == Some(true),
            event.rejected == Some(true),
            event.error.is_some(),
        ];
        ensure!(
            terminals.into_iter().filter(|terminal| *terminal).count() == 1,
            Malformed
        );
        *terminal_observed = true;
        if event.aborted == Some(true) {
            return Err(Cancelled.into());
        }
        if event.error.as_deref() == Some("evaluation_timeout") {
            return Err(TimedOut.into());
        }
        if event.rejected == Some(true)
            || event.error.as_deref() == Some("invalid_evaluation_selection")
        {
            return Err(Malformed.into());
        }
        ensure!(event.error.is_none(), Unavailable);
        ensure!(event.done == Some(true), Malformed);
        let result = event.evaluation.ok_or(Malformed)?;
        ensure!(
            result.schema == "choice.v1"
                && result.provider == self.identity.provider
                && result.model == self.identity.model
                && result.cost_microusd.is_null(),
            Malformed
        );
        let selection = match result.selection {
            Selection::Selected { id } => EvaluationSelection::Selected(id),
            Selection::NoMatch => EvaluationSelection::NoMatch,
        };
        self.request
            .validate_selection(&selection)
            .map_err(|_| Malformed)?;
        let usage: Option<alan_llm::TokenUsage> = result.usage.map(Into::into);
        if let Some(usage) = usage {
            ensure!(
                usage.prompt_tokens >= 0
                    && usage.completion_tokens >= 0
                    && usage.prompt_tokens.checked_add(usage.completion_tokens)
                        == Some(usage.total_tokens)
                    && usage
                        .cached_prompt_tokens
                        .is_none_or(|n| n >= 0 && n <= usage.prompt_tokens)
                    && usage
                        .reasoning_tokens
                        .is_none_or(|n| n >= 0 && n <= usage.completion_tokens),
                Malformed
            );
        }
        Ok(ChoiceEvaluationResponse { selection, usage })
    }
}

async fn read_operation_id(client: &NamespaceClient, fid: Fid) -> Result<String> {
    let bytes = client.read_at(fid, 0, 129).await?;
    let id = std::str::from_utf8(&bytes).map_err(|_| Malformed)?.trim();
    ensure!(component(id), Malformed);
    Ok(id.to_owned())
}

fn classify(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<NamespaceEvaluationFailure>().is_some() {
        error
    } else {
        error.context(Unavailable)
    }
}

async fn abort(client: &NamespaceClient, path: &str) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(1),
        client.write_document(path, b"abort"),
    )
    .await
    .map_err(|_| Unavailable)?
    .map_err(|_| Unavailable.into())
}

impl Drop for NamespaceEvaluation {
    fn drop(&mut self) {
        if self.armed
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let client = self.client.clone();
            let path = self.path("ctl");
            runtime.spawn(async move {
                let _ = abort(&client, &path).await;
            });
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationEvent {
    version: u16,
    done: Option<bool>,
    error: Option<String>,
    rejected: Option<bool>,
    aborted: Option<bool>,
    evaluation: Option<EvaluationResult>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationResult {
    schema: String,
    provider: String,
    model: String,
    selection: Selection,
    usage: Option<super::generation::LlmEventTokenUsage>,
    cost_microusd: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Selection {
    Selected { id: String },
    NoMatch,
}

#[cfg(test)]
mod tests;
