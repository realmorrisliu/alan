//! One mounted typed Decision operation (`decision.v1`) over `/mnt/llm/connections/<conn>/decide`.
//!
//! Mirrors the strict allocate -> commit -> terminal event lifecycle of [`super::evaluation`]
//! without carrying tools or assistant prose. All provider, transport, timeout, cancellation,
//! and malformed-response errors map to distinct [`NamespaceDecisionFailure`] variants so callers
//! can always fail closed.

use std::collections::BTreeMap;
use std::time::Duration;

use alan_ap::{Fid, InProcessTransport, OpenMode};
use alan_llm::{CalibrationKind, DecisionAnswer, DecisionRequest, DecisionResponse};
use anyhow::{Result, ensure};
use serde::Deserialize;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::{
    NamespaceGeneration, NamespaceOperationCleanup, NamespaceRuntimeEnvironment,
    NamespaceToolAuthorization,
    client::{NamespaceClient, NamespaceFidGuard},
};

/// Typed terminal failures for mounted `decision.v1` operations.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum NamespaceDecisionFailure {
    #[error("decision cancelled")]
    Cancelled,
    #[error("decision deadline exceeded")]
    TimedOut,
    #[error("decision result malformed")]
    Malformed,
    #[error("decision unavailable")]
    Unavailable,
}
use NamespaceDecisionFailure::*;

/// Unconfirmed operation cleanup or allocation identity outcomes.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum NamespaceDecisionUncertainty {
    #[error("decision abort unconfirmed")]
    Abort,
    #[error("decision allocation identity unconfirmed")]
    AllocationIdentity,
}

/// Narrow file-native handle for dispatching `decision.v1` requests against one mounted Connection.
#[derive(Clone)]
pub(crate) struct NamespaceDecisionConnection {
    root: InProcessTransport,
    llm_connection: String,
    operation_cleanup: NamespaceOperationCleanup,
}

impl NamespaceDecisionConnection {
    pub(crate) fn new(
        root: InProcessTransport,
        llm_connection: impl Into<String>,
        operation_cleanup: NamespaceOperationCleanup,
    ) -> Self {
        Self {
            root,
            llm_connection: llm_connection.into(),
            operation_cleanup,
        }
    }

    pub(crate) fn connection_name(&self) -> &str {
        &self.llm_connection
    }

    /// Convert this decision handle into an unmetered generation handle for the same Connection
    /// (used for bounded Tier 2 `Uncalibrated` structured-generation rubric fallback).
    pub(crate) fn as_unmetered_generation(&self) -> NamespaceGeneration {
        NamespaceGeneration {
            root: self.root.clone(),
            llm_connection: self.llm_connection.clone(),
            budget: None,
            operation_cleanup: self.operation_cleanup.clone(),
        }
    }

    /// Execute one bounded `decision.v1` request against `/mnt/llm/connections/<conn>/decide`.
    pub(crate) async fn decide(
        &self,
        request: DecisionRequest,
        deadline_ms: u64,
        cancel: &CancellationToken,
    ) -> Result<DecisionResponse> {
        let operation = allocate_decision_inner(
            &self.root,
            &self.llm_connection,
            &self.operation_cleanup,
            request,
            deadline_ms,
            cancel,
        )
        .await?;
        operation.commit(cancel).await
    }
}

/// A single allocated `decision.v1` operation. Consuming [`Self::commit`] prevents redispatch.
pub struct NamespaceDecision {
    client: NamespaceClient,
    llm_connection: String,
    operation_id: String,
    request: DecisionRequest,
    expires: Instant,
    armed: bool,
    operation_cleanup: NamespaceOperationCleanup,
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

impl NamespaceGeneration {
    /// Convert this captured generation handle into a decision handle for the same Connection.
    pub(crate) fn as_decision_connection(&self) -> NamespaceDecisionConnection {
        NamespaceDecisionConnection::new(
            self.root.clone(),
            self.llm_connection.clone(),
            self.operation_cleanup.clone(),
        )
    }
}

impl NamespaceRuntimeEnvironment {
    /// Allocate a `decision.v1` operation on the active Connection under `/mnt/llm`.
    pub async fn allocate_decision(
        &self,
        request: DecisionRequest,
        deadline_ms: u64,
        cancel: &CancellationToken,
    ) -> Result<NamespaceDecision> {
        let (root, connection) = self.active_decision_target();
        allocate_decision_inner(
            &root,
            &connection,
            &self.operation_cleanup,
            request,
            deadline_ms,
            cancel,
        )
        .await
    }

    /// Execute one bounded `decision.v1` operation on the active Connection.
    pub async fn decide(
        &self,
        request: DecisionRequest,
        deadline_ms: u64,
        cancel: &CancellationToken,
    ) -> Result<DecisionResponse> {
        let operation = self.allocate_decision(request, deadline_ms, cancel).await?;
        operation.commit(cancel).await
    }

    fn active_decision_target(&self) -> (InProcessTransport, String) {
        if let Some((_, callable)) = &*self.active_binding.read().expect("active binding snapshot")
        {
            return (callable.root.clone(), callable.connection.clone());
        }
        (self.root.clone(), self.llm_connection.clone())
    }
}

impl NamespaceToolAuthorization {
    /// Capture an independently reachable smart-evaluator Connection for `decision.v1` calls
    /// without mutating the main generation binding.
    pub(crate) async fn capture_smart_decision_connection(
        &self,
        profile: &str,
    ) -> Result<NamespaceDecisionConnection> {
        let authority = self
            .model_bindings
            .lock()
            .await
            .authority
            .clone()
            .ok_or_else(|| anyhow::anyhow!("smart evaluator Connection authority unavailable"))?;
        let captured = authority.capture_smart_evaluator(profile).await?;
        Ok(NamespaceDecisionConnection::new(
            captured.root,
            captured.connection,
            self.operation_cleanup.clone(),
        ))
    }

    /// Capture the dedicated Guardian Connection for `decision.v1` fallback without mutating
    /// main generation bindings.
    pub(crate) async fn capture_guardian_decision_connection(
        &self,
        profile: &str,
    ) -> Result<NamespaceDecisionConnection> {
        let generation = self.capture_guardian_generation(profile).await?;
        Ok(generation.as_decision_connection())
    }
}

async fn allocate_decision_inner(
    root: &InProcessTransport,
    llm_connection: &str,
    operation_cleanup: &NamespaceOperationCleanup,
    request: DecisionRequest,
    deadline_ms: u64,
    cancel: &CancellationToken,
) -> Result<NamespaceDecision> {
    operation_cleanup.check_admission()?;
    request.validate().map_err(|_| Malformed)?;
    ensure!(valid_component(llm_connection), Malformed);
    ensure!((1..=30_000).contains(&deadline_ms), Malformed);
    let expires = Instant::now() + Duration::from_millis(deadline_ms);
    let client = NamespaceClient::new(root.clone());
    let path = format!("/mnt/llm/connections/{llm_connection}/decide");
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
                let ctl = format!("/mnt/llm/connections/{llm_connection}/{id}/ctl");
                if abort(&client, &ctl).await.is_err() {
                    error = error.context(NamespaceDecisionUncertainty::Abort);
                }
            } else if allocation_may_exist {
                error = error.context(NamespaceDecisionUncertainty::AllocationIdentity);
            }
            if let Some(fid) = allocation_fid {
                let _ = tokio::time::timeout(Duration::from_secs(1), fid.close()).await;
            }
            return Err(error);
        }
    };
    Ok(NamespaceDecision {
        client,
        llm_connection: llm_connection.to_owned(),
        operation_id,
        request,
        expires,
        armed: true,
        operation_cleanup: operation_cleanup.clone(),
    })
}

impl NamespaceDecision {
    /// Allocated operation directory identifier under `/mnt/llm/connections/<conn>/`.
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    fn path(&self, file: &str) -> String {
        format!(
            "/mnt/llm/connections/{}/{}/{file}",
            self.llm_connection, self.operation_id
        )
    }

    /// Abort without committing request data.
    pub async fn abort(mut self) -> Result<()> {
        let result = abort(&self.client, &self.path("ctl")).await;
        self.armed = false;
        result.map_err(|error| error.context(NamespaceDecisionUncertainty::Abort))
    }

    /// Commit at most once and validate the returned `decision.v1` response.
    pub async fn commit(mut self, cancel: &CancellationToken) -> Result<DecisionResponse> {
        self.operation_cleanup.check_admission()?;
        let mut data_fid: Option<Fid> = None;
        let mut terminal_observed = false;
        let work = async {
            let remaining = self
                .expires
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(30_000) as u64;
            ensure!(remaining > 0, TimedOut);
            let payload = serde_json::json!({
                "version": 1,
                "schema": "decision.v1",
                "state": self.request.state,
                "questions": self.request.questions,
                "instructions": self.request.instructions,
                "model_override": self.request.model_override,
                "deadline_ms": remaining,
            });
            let bytes = serde_json::to_vec(&payload).map_err(|_| Malformed)?;
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
            if terminal_observed {
                self.armed = false;
                return Err(error);
            }
            let aborted = abort(&self.client, &self.path("ctl")).await;
            if aborted.is_ok()
                && let Some(fid) = data_fid
            {
                let _ = tokio::time::timeout(Duration::from_secs(1), self.client.clunk(fid)).await;
            }
            self.armed = false;
            if aborted.is_err() {
                return Err(error.context(NamespaceDecisionUncertainty::Abort));
            }
            return Err(error);
        }
        self.armed = false;
        result
    }

    async fn read_result(&self, terminal_observed: &mut bool) -> Result<DecisionResponse> {
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
        let event: DecisionEvent = serde_json::from_slice(&bytes).map_err(|_| Malformed)?;
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
        if event.error.as_deref() == Some("decision_timeout") {
            return Err(TimedOut.into());
        }
        if event.rejected == Some(true)
            || matches!(
                event.error.as_deref(),
                Some("invalid_decision_answer" | "malformed_decision_response")
            )
        {
            return Err(Malformed.into());
        }
        ensure!(event.error.is_none(), Unavailable);
        ensure!(event.done == Some(true), Malformed);
        let result = event.decision.ok_or(Malformed)?;
        ensure!(
            result.schema == "decision.v1" && result.cost_microusd.is_null(),
            Malformed
        );
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
        let response = DecisionResponse {
            model: result.model,
            provider: result.provider,
            calibration: result.calibration,
            answers: result.answers,
            usage,
        };
        self.request
            .validate_response(&response)
            .map_err(|_| Malformed)?;
        Ok(response)
    }
}

async fn read_operation_id(client: &NamespaceClient, fid: Fid) -> Result<String> {
    let bytes = client.read_at(fid, 0, 129).await?;
    let id = std::str::from_utf8(&bytes).map_err(|_| Malformed)?.trim();
    ensure!(valid_component(id), Malformed);
    Ok(id.to_owned())
}

fn classify(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<NamespaceDecisionFailure>().is_some() {
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

impl Drop for NamespaceDecision {
    fn drop(&mut self) {
        if self.armed {
            self.operation_cleanup
                .abort_on_drop(self.client.clone(), self.path("ctl"), b"abort");
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionEvent {
    version: u16,
    done: Option<bool>,
    error: Option<String>,
    rejected: Option<bool>,
    aborted: Option<bool>,
    decision: Option<DecisionResult>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionResult {
    schema: String,
    calibration: CalibrationKind,
    provider: String,
    model: String,
    #[serde(deserialize_with = "deserialize_unique_answers")]
    answers: BTreeMap<String, DecisionAnswer>,
    usage: Option<super::generation::LlmEventTokenUsage>,
    cost_microusd: serde_json::Value,
}

fn deserialize_unique_answers<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, DecisionAnswer>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct MapVisitor;

    impl<'de> serde::de::Visitor<'de> for MapVisitor {
        type Value = BTreeMap<String, DecisionAnswer>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map of unique decision answer keys")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, DecisionAnswer>()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate decision answer key"));
                }
            }
            Ok(result)
        }
    }

    deserializer.deserialize_map(MapVisitor)
}

#[cfg(test)]
mod tests;
