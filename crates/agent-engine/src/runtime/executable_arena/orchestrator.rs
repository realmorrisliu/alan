//! Concurrent contender orchestration, independent `read_only` Verifier scoring,
//! immediate losing-clone discard, and exit `.patch` registration for `alan-arena`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use alan_agent_protocol::{SpawnHostMount, SpawnMountAccess, SpawnSpec};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::branch_client::{BranchForkCommand, BranchFsApClient, BranchScoreCommand};
use super::specs::{
    VerifierCandidateSummary, build_executable_contender_spawn_spec,
    build_read_only_contender_spawn_spec, build_verifier_spawn_spec,
};
use super::{
    ArenaExitFlushRegistry, ArenaMode, ArenaWinnerFlushRequest, ArenaWorkspaceBackingKind,
    ArenaWorkspaceCreateRequest, ArenaWorkspacePort, ExecutableArenaError, ExecutableArenaRequest,
    ensure_executable_arena_sandbox, validate_no_raw_host_path, validate_opaque_workspace_ref,
};
use crate::runtime::delegated_child_run::ChildRuntimeStatus;
use crate::tools::{SandboxBackendKind, detect_backend};

/// Normalized subagent execution output consumed by [`ExecutableArenaCoordinator`].
#[derive(Debug, Clone, PartialEq)]
pub struct ArenaSubagentOutput {
    pub completed: bool,
    pub output_text: String,
    pub structured_output: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

impl ArenaSubagentOutput {
    /// Construct a successful completed output.
    pub fn completed(output_text: impl Into<String>) -> Self {
        Self {
            completed: true,
            output_text: output_text.into(),
            structured_output: None,
            error_message: None,
        }
    }

    /// Attach structured JSON output (such as a Verifier scorecard).
    pub fn with_structured_output(mut self, structured_output: serde_json::Value) -> Self {
        self.structured_output = Some(structured_output);
        self
    }

    /// Construct a failed subagent output.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            completed: false,
            output_text: String::new(),
            structured_output: None,
            error_message: Some(message.into()),
        }
    }
}

impl From<crate::runtime::delegated_child_run::ChildRuntimeResult> for ArenaSubagentOutput {
    fn from(res: crate::runtime::delegated_child_run::ChildRuntimeResult) -> Self {
        Self {
            completed: res.status == ChildRuntimeStatus::Completed,
            output_text: res.output_text,
            structured_output: res.structured_output,
            error_message: res.error_message,
        }
    }
}

/// Spawn hook used by [`ExecutableArenaCoordinator`] to run each contender and the Verifier.
pub type ArenaSpawnFn = Arc<
    dyn Fn(
            SpawnSpec,
            CancellationToken,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<ArenaSubagentOutput>> + Send>>
        + Send
        + Sync,
>;

/// Single candidate entry in a Verifier's structured scorecard output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArenaScoreEntry {
    /// Neutral label (`Contender A`, ...) or `candidate_id`.
    pub candidate: String,
    pub score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Structured scorecard returned by the independent `read_only = true` Verifier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArenaVerifierScorecard {
    pub scores: Vec<ArenaScoreEntry>,
    pub winner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Per-candidate outcome produced by [`ExecutableArenaCoordinator::run_bakeoff`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArenaCandidateOutcome {
    pub candidate_id: String,
    pub neutral_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_kind: Option<ArenaWorkspaceBackingKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_summary: Option<String>,
    pub selected: bool,
    pub discarded: bool,
    pub output_text: String,
}

/// Complete adjudicated result of an `alan-arena` run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArenaBakeoffResult {
    pub mode: ArenaMode,
    pub sandbox_backend: String,
    pub selected_candidate_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_workspace_ref: Option<String>,
    pub verifier_rationale: Option<String>,
    pub purged_losers: Vec<String>,
    pub candidates: Vec<ArenaCandidateOutcome>,
}

/// Coordinates `read_only` (default) and `executable` `alan-arena` runs.
pub struct ExecutableArenaCoordinator {
    sandbox_backend: SandboxBackendKind,
    workspace_port: Option<Arc<dyn ArenaWorkspacePort>>,
    branch_client: Option<BranchFsApClient>,
    exit_registry: ArenaExitFlushRegistry,
}

impl ExecutableArenaCoordinator {
    /// Construct a coordinator probing the host's active [`SandboxBackendKind`].
    pub fn new(exit_registry: ArenaExitFlushRegistry) -> Self {
        Self {
            sandbox_backend: detect_backend(),
            workspace_port: None,
            branch_client: None,
            exit_registry,
        }
    }

    /// Override the active [`SandboxBackendKind`] (for deterministic testing or explicit binding).
    pub fn with_sandbox_backend(mut self, sandbox_backend: SandboxBackendKind) -> Self {
        self.sandbox_backend = sandbox_backend;
        self
    }

    /// Attach the host-agnostic CoW workspace port required by `mode = "executable"`.
    pub fn with_workspace_port(mut self, port: Arc<dyn ArenaWorkspacePort>) -> Self {
        self.workspace_port = Some(port);
        self
    }

    /// Attach `/mnt/branch` `aP` client for branch lifecycle coordination.
    pub fn with_branch_client(mut self, client: BranchFsApClient) -> Self {
        self.branch_client = Some(client);
        self
    }

    /// Inspect the attached exit-flush registry.
    pub fn exit_registry(&self) -> &ArenaExitFlushRegistry {
        &self.exit_registry
    }

    /// Execute a bounded `alan-arena` bakeoff using the supplied spawn hook.
    pub async fn run_bakeoff(
        &self,
        request: &ExecutableArenaRequest,
        spawn_fn: ArenaSpawnFn,
        cancel: &CancellationToken,
    ) -> Result<ArenaBakeoffResult, ExecutableArenaError> {
        request.validate()?;

        match request.mode {
            ArenaMode::ReadOnly => self.run_read_only_bakeoff(request, spawn_fn, cancel).await,
            ArenaMode::Executable => self.run_executable_bakeoff(request, spawn_fn, cancel).await,
        }
    }

    async fn run_read_only_bakeoff(
        &self,
        request: &ExecutableArenaRequest,
        spawn_fn: ArenaSpawnFn,
        cancel: &CancellationToken,
    ) -> Result<ArenaBakeoffResult, ExecutableArenaError> {
        let mut specs = Vec::with_capacity(request.contenders.len());
        for (idx, contender) in request.contenders.iter().enumerate() {
            let spec = build_read_only_contender_spawn_spec(
                request,
                contender,
                &request.read_only_mounts,
            )?;
            let label = neutral_contender_label(idx);
            specs.push((
                idx,
                contender.candidate_id.clone(),
                label,
                contender.model.clone(),
                spec,
            ));
        }

        let outputs = execute_contender_specs(specs, spawn_fn.clone(), cancel).await?;
        let verifier_candidates = outputs
            .iter()
            .map(|entry| VerifierCandidateSummary {
                neutral_label: &entry.neutral_label,
                branch_id: &entry.candidate_id,
                workspace_ref: None,
                backing_kind: None,
                diff_summary: None,
                checks_summary: None,
                contender_output: &entry.output_text,
            })
            .collect::<Vec<_>>();

        let verifier_spec =
            build_verifier_spawn_spec(request, &request.read_only_mounts, &verifier_candidates)?;
        let verifier_res = spawn_fn(verifier_spec, cancel.clone())
            .await
            .map_err(|err| ExecutableArenaError::ExecutionFailed(err.to_string()))?;
        let scorecard = parse_verifier_scorecard(&verifier_res, &outputs)?;

        let mut outcomes = Vec::with_capacity(outputs.len());
        for entry in outputs {
            let score_entry = scorecard.scores.get(&entry.candidate_id);
            let selected = scorecard.winner_id == entry.candidate_id;
            outcomes.push(ArenaCandidateOutcome {
                candidate_id: entry.candidate_id,
                neutral_label: entry.neutral_label,
                model: entry.model,
                workspace_ref: None,
                backing_kind: None,
                diff_summary: None,
                checks_summary: None,
                score: score_entry.map(|s| s.score),
                verifier_summary: score_entry.and_then(|s| s.summary.clone()),
                selected,
                discarded: false,
                output_text: entry.output_text,
            });
        }

        Ok(ArenaBakeoffResult {
            mode: ArenaMode::ReadOnly,
            sandbox_backend: self.sandbox_backend.name().to_string(),
            selected_candidate_id: scorecard.winner_id,
            selected_workspace_ref: None,
            verifier_rationale: scorecard.rationale,
            purged_losers: Vec::new(),
            candidates: outcomes,
        })
    }

    async fn run_executable_bakeoff(
        &self,
        request: &ExecutableArenaRequest,
        spawn_fn: ArenaSpawnFn,
        cancel: &CancellationToken,
    ) -> Result<ArenaBakeoffResult, ExecutableArenaError> {
        // 1. Fail closed BEFORE creating any CoW workspace or spawning any contender.
        ensure_executable_arena_sandbox(self.sandbox_backend)?;

        let port = self.workspace_port.as_ref().ok_or_else(|| {
            ExecutableArenaError::WorkspacePort(
                "executable alan-arena requires an attached ArenaWorkspacePort".to_string(),
            )
        })?;
        let source_grant = request.source_grant.as_deref().ok_or_else(|| {
            ExecutableArenaError::InvalidConfiguration(
                "executable alan-arena requires source_grant".to_string(),
            )
        })?;

        // 2. Provision one isolated CoW workspace per contender, rolling back on error.
        let mut provisioned = Vec::with_capacity(request.contenders.len());
        for (idx, contender) in request.contenders.iter().enumerate() {
            let ws = match port
                .create_contender_workspace(&ArenaWorkspaceCreateRequest {
                    candidate_id: contender.candidate_id.clone(),
                    source_grant: source_grant.to_string(),
                    workstream: request.workstream.clone(),
                    rollout_ref: request.rollout_ref.clone(),
                    why: request.why.clone(),
                })
                .await
            {
                Ok(ws) => ws,
                Err(err) => {
                    purge_workspaces(port, &provisioned).await;
                    return Err(ExecutableArenaError::WorkspacePort(err));
                }
            };

            if let Err(err) = validate_opaque_workspace_ref(&ws.workspace_ref) {
                let _ = port.discard_workspace(&ws.workspace_ref).await;
                purge_workspaces(port, &provisioned).await;
                return Err(err);
            }
            if let Some(ro_mount) = ws.verifier_read_only_mount.as_ref()
                && ro_mount.access != SpawnMountAccess::ReadOnly
            {
                let _ = port.discard_workspace(&ws.workspace_ref).await;
                purge_workspaces(port, &provisioned).await;
                return Err(ExecutableArenaError::VerifierNotReadOnly(format!(
                    "port returned non-read_only verifier mount for `{}`",
                    contender.candidate_id
                )));
            }

            let spec = match build_executable_contender_spawn_spec(
                request,
                contender,
                source_grant,
                &ws.workspace_ref,
                &ws.contender_mount,
            ) {
                Ok(spec) => spec,
                Err(err) => {
                    let _ = port.discard_workspace(&ws.workspace_ref).await;
                    purge_workspaces(port, &provisioned).await;
                    return Err(err);
                }
            };

            provisioned.push(ProvisionedContender {
                idx,
                candidate_id: contender.candidate_id.clone(),
                neutral_label: neutral_contender_label(idx),
                model: contender.model.clone(),
                workspace_ref: ws.workspace_ref,
                backing_kind: ws.backing_kind,
                verifier_read_only_mount: ws.verifier_read_only_mount,
                spec,
            });
        }

        // 3. Fork candidate branches in /mnt/branch with opaque workspace_ref metadata.
        if let Some(branch_client) = &self.branch_client {
            for item in &provisioned {
                if let Err(err) = branch_client
                    .fork(BranchForkCommand {
                        id: &item.candidate_id,
                        from: &request.base_branch_id,
                        delta: &format!("arena contender {}", item.candidate_id),
                        workspace_ref: Some(&item.workspace_ref),
                        backing_kind: Some(item.backing_kind),
                        diff_summary: None,
                        checks_summary: None,
                    })
                    .await
                {
                    purge_workspaces(port, &provisioned).await;
                    return Err(err);
                }
            }
        }

        // 4. Execute all contenders concurrently in isolated clones.
        let specs = provisioned
            .iter()
            .map(|item| {
                (
                    item.idx,
                    item.candidate_id.clone(),
                    item.neutral_label.clone(),
                    item.model.clone(),
                    item.spec.clone(),
                )
            })
            .collect::<Vec<_>>();
        let outputs = match execute_contender_specs(specs, spawn_fn.clone(), cancel).await {
            Ok(outputs) => outputs,
            Err(err) => {
                discard_branches_and_workspaces(self.branch_client.as_ref(), port, &provisioned)
                    .await;
                return Err(err);
            }
        };

        // 5. Inspect candidate workspaces for diff_summary & checks_summary without Host paths.
        let mut inspections = HashMap::with_capacity(provisioned.len());
        for item in &provisioned {
            let inspection = match port.inspect_workspace(&item.workspace_ref).await {
                Ok(insp) => insp,
                Err(err) => {
                    discard_branches_and_workspaces(
                        self.branch_client.as_ref(),
                        port,
                        &provisioned,
                    )
                    .await;
                    return Err(ExecutableArenaError::WorkspacePort(err));
                }
            };
            if let Err(err) = validate_no_raw_host_path(&inspection.diff_summary) {
                discard_branches_and_workspaces(self.branch_client.as_ref(), port, &provisioned)
                    .await;
                return Err(err);
            }
            if let Some(checks) = inspection.checks_summary.as_deref()
                && let Err(err) = validate_no_raw_host_path(checks)
            {
                discard_branches_and_workspaces(self.branch_client.as_ref(), port, &provisioned)
                    .await;
                return Err(err);
            }
            if let Some(branch_client) = &self.branch_client
                && let Err(err) = branch_client
                    .update_workspace(
                        &item.candidate_id,
                        Some(&item.workspace_ref),
                        Some(item.backing_kind),
                        Some(&inspection.diff_summary),
                        inspection.checks_summary.as_deref(),
                    )
                    .await
            {
                discard_branches_and_workspaces(self.branch_client.as_ref(), port, &provisioned)
                    .await;
                return Err(err);
            }
            inspections.insert(item.candidate_id.clone(), inspection);
        }

        // 6. Build and invoke the independent `read_only = true` Verifier.
        let verifier_mounts = provisioned
            .iter()
            .filter_map(|item| item.verifier_read_only_mount.clone())
            .collect::<Vec<_>>();
        let output_by_id = outputs
            .iter()
            .map(|entry| (entry.candidate_id.as_str(), entry))
            .collect::<HashMap<_, _>>();
        let verifier_candidates = provisioned
            .iter()
            .map(|item| {
                let inspection = inspections
                    .get(&item.candidate_id)
                    .expect("inspection recorded per contender");
                let contender_output = output_by_id
                    .get(item.candidate_id.as_str())
                    .map(|entry| entry.output_text.as_str())
                    .unwrap_or("");
                VerifierCandidateSummary {
                    neutral_label: &item.neutral_label,
                    branch_id: &item.candidate_id,
                    workspace_ref: Some(&item.workspace_ref),
                    backing_kind: Some(item.backing_kind),
                    diff_summary: Some(&inspection.diff_summary),
                    checks_summary: inspection.checks_summary.as_deref(),
                    contender_output,
                }
            })
            .collect::<Vec<_>>();

        let verifier_spec =
            match build_verifier_spawn_spec(request, &verifier_mounts, &verifier_candidates) {
                Ok(spec) => spec,
                Err(err) => {
                    discard_branches_and_workspaces(
                        self.branch_client.as_ref(),
                        port,
                        &provisioned,
                    )
                    .await;
                    return Err(err);
                }
            };
        let verifier_res = match spawn_fn(verifier_spec, cancel.clone()).await {
            Ok(res) => res,
            Err(err) => {
                discard_branches_and_workspaces(self.branch_client.as_ref(), port, &provisioned)
                    .await;
                return Err(ExecutableArenaError::ExecutionFailed(err.to_string()));
            }
        };
        let scorecard = match parse_verifier_scorecard(&verifier_res, &outputs) {
            Ok(card) => card,
            Err(err) => {
                discard_branches_and_workspaces(self.branch_client.as_ref(), port, &provisioned)
                    .await;
                return Err(err);
            }
        };

        // 7. Publish scores & winner selection to /mnt/branch.
        if let Some(branch_client) = &self.branch_client {
            for item in &provisioned {
                if let Some(entry) = scorecard.scores.get(&item.candidate_id) {
                    let inspection = inspections.get(&item.candidate_id);
                    branch_client
                        .score(BranchScoreCommand {
                            id: &item.candidate_id,
                            score: entry.score,
                            summary: entry.summary.as_deref(),
                            diff_summary: inspection.map(|i| i.diff_summary.as_str()),
                            checks_summary: inspection.and_then(|i| i.checks_summary.as_deref()),
                        })
                        .await?;
                }
            }
            branch_client.select(&scorecard.winner_id).await?;
        }

        // 8. Immediately purge all losing clones and discard losing branches in /mnt/branch.
        let mut purged_losers = Vec::new();
        for item in &provisioned {
            if item.candidate_id != scorecard.winner_id {
                if let Some(branch_client) = &self.branch_client {
                    branch_client.discard(&item.candidate_id).await?;
                }
                port.discard_workspace(&item.workspace_ref)
                    .await
                    .map_err(ExecutableArenaError::WorkspacePort)?;
                purged_losers.push(item.candidate_id.clone());
            }
        }

        // 9. Register unapplied winner for exit-time .patch persistence & clone deletion.
        let (winner_id, winner_workspace_ref) = {
            let winner = provisioned
                .iter()
                .find(|item| item.candidate_id == scorecard.winner_id)
                .expect("winner validated against provisioned contenders");
            let winner_inspection = inspections.get(&winner.candidate_id);
            let winner_score = scorecard.scores.get(&winner.candidate_id);
            self.exit_registry.register_winner(
                Arc::clone(port),
                ArenaWinnerFlushRequest {
                    candidate_id: winner.candidate_id.clone(),
                    workspace_ref: winner.workspace_ref.clone(),
                    backing_kind: winner.backing_kind,
                    workstream: request.workstream.clone(),
                    rollout_ref: request.rollout_ref.clone(),
                    why: request.why.clone(),
                    score: winner_score.map(|s| s.score),
                    verifier_summary: winner_score
                        .and_then(|s| s.summary.clone())
                        .or_else(|| scorecard.rationale.clone()),
                    diff_summary: winner_inspection.map(|i| i.diff_summary.clone()),
                    checks_summary: winner_inspection.and_then(|i| i.checks_summary.clone()),
                },
            );
            (winner.candidate_id.clone(), winner.workspace_ref.clone())
        };

        let outcomes = provisioned
            .into_iter()
            .map(|item| {
                let inspection = inspections.remove(&item.candidate_id);
                let score_entry = scorecard.scores.get(&item.candidate_id);
                let selected = item.candidate_id == scorecard.winner_id;
                let output_text = output_by_id
                    .get(item.candidate_id.as_str())
                    .map(|o| o.output_text.clone())
                    .unwrap_or_default();
                ArenaCandidateOutcome {
                    candidate_id: item.candidate_id,
                    neutral_label: item.neutral_label,
                    model: item.model,
                    workspace_ref: Some(item.workspace_ref),
                    backing_kind: Some(item.backing_kind),
                    diff_summary: inspection.as_ref().map(|i| i.diff_summary.clone()),
                    checks_summary: inspection.and_then(|i| i.checks_summary),
                    score: score_entry.map(|s| s.score),
                    verifier_summary: score_entry.and_then(|s| s.summary.clone()),
                    selected,
                    discarded: !selected,
                    output_text,
                }
            })
            .collect();

        Ok(ArenaBakeoffResult {
            mode: ArenaMode::Executable,
            sandbox_backend: self.sandbox_backend.name().to_string(),
            selected_candidate_id: winner_id,
            selected_workspace_ref: Some(winner_workspace_ref),
            verifier_rationale: scorecard.rationale,
            purged_losers,
            candidates: outcomes,
        })
    }
}

struct ProvisionedContender {
    idx: usize,
    candidate_id: String,
    neutral_label: String,
    model: Option<String>,
    workspace_ref: String,
    backing_kind: ArenaWorkspaceBackingKind,
    verifier_read_only_mount: Option<SpawnHostMount>,
    spec: SpawnSpec,
}

#[derive(Debug)]
pub struct ExecutedContenderOutput {
    pub idx: usize,
    pub candidate_id: String,
    pub neutral_label: String,
    pub model: Option<String>,
    pub output_text: String,
}

/// Parsed and validated Verifier scorecard mapped back to concrete `candidate_id`s.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedScorecard {
    pub scores: HashMap<String, ArenaScoreEntry>,
    pub winner_id: String,
    pub rationale: Option<String>,
}

/// Parse the Verifier's JSON scorecard (from `structured_output` or `output_text`)
/// and map neutral labels (`Contender A`, ...) or `candidate_id`s back to contenders.
pub fn parse_verifier_scorecard(
    verifier_result: &ArenaSubagentOutput,
    contenders: &[ExecutedContenderOutput],
) -> Result<ParsedScorecard, ExecutableArenaError> {
    if !verifier_result.completed {
        let detail = verifier_result
            .error_message
            .as_deref()
            .unwrap_or("verifier did not complete successfully");
        return Err(ExecutableArenaError::ExecutionFailed(format!(
            "arena verifier failed: {detail}"
        )));
    }

    let raw_card: ArenaVerifierScorecard = if let Some(structured) =
        &verifier_result.structured_output
    {
        serde_json::from_value(structured.clone()).map_err(|err| {
            ExecutableArenaError::ExecutionFailed(format!(
                "invalid verifier structured_output scorecard: {err}"
            ))
        })?
    } else {
        let json_slice = extract_json_object(&verifier_result.output_text).ok_or_else(|| {
            ExecutableArenaError::ExecutionFailed(
                "verifier output did not contain a JSON scorecard object".to_string(),
            )
        })?;
        serde_json::from_str(json_slice).map_err(|err| {
            ExecutableArenaError::ExecutionFailed(format!("invalid verifier scorecard JSON: {err}"))
        })?
    };

    let mut label_to_id = HashMap::new();
    let mut known_ids = HashSet::new();
    for contender in contenders {
        label_to_id.insert(
            contender.neutral_label.as_str(),
            contender.candidate_id.as_str(),
        );
        known_ids.insert(contender.candidate_id.as_str());
    }

    let resolve_id = |token: &str| -> Result<String, ExecutableArenaError> {
        let trimmed = token.trim();
        if let Some(&id) = label_to_id.get(trimmed) {
            return Ok(id.to_string());
        }
        if known_ids.contains(trimmed) {
            return Ok(trimmed.to_string());
        }
        Err(ExecutableArenaError::ExecutionFailed(format!(
            "verifier scorecard referenced unknown candidate `{trimmed}`"
        )))
    };

    let mut scores = HashMap::new();
    for entry in raw_card.scores {
        if !entry.score.is_finite() {
            return Err(ExecutableArenaError::ExecutionFailed(format!(
                "verifier scorecard contained non-finite score for `{}`",
                entry.candidate
            )));
        }
        if let Some(summary) = entry.summary.as_deref() {
            validate_no_raw_host_path(summary)?;
        }
        let id = resolve_id(&entry.candidate)?;
        scores.insert(
            id.clone(),
            ArenaScoreEntry {
                candidate: id,
                score: entry.score,
                summary: entry.summary,
            },
        );
    }

    let winner_id = resolve_id(&raw_card.winner)?;
    if let Some(rationale) = raw_card.rationale.as_deref() {
        validate_no_raw_host_path(rationale)?;
    }

    Ok(ParsedScorecard {
        scores,
        winner_id,
        rationale: raw_card.rationale,
    })
}

async fn execute_contender_specs(
    specs: Vec<(usize, String, String, Option<String>, SpawnSpec)>,
    spawn_fn: ArenaSpawnFn,
    cancel: &CancellationToken,
) -> Result<Vec<ExecutedContenderOutput>, ExecutableArenaError> {
    let mut join_set = JoinSet::new();
    for (idx, candidate_id, neutral_label, model, spec) in specs {
        let slot_spawn = spawn_fn.clone();
        let slot_cancel = cancel.child_token();
        join_set.spawn(async move {
            let res = slot_spawn(spec, slot_cancel).await;
            (idx, candidate_id, neutral_label, model, res)
        });
    }

    let mut collected = Vec::new();
    while let Some(joined) = join_set.join_next().await {
        let (idx, candidate_id, neutral_label, model, result) =
            joined.map_err(|err| ExecutableArenaError::ExecutionFailed(err.to_string()))?;
        let child_res = result.map_err(|err| {
            ExecutableArenaError::ExecutionFailed(format!(
                "contender `{candidate_id}` failed to run: {err}"
            ))
        })?;
        if !child_res.completed {
            let detail = child_res
                .error_message
                .as_deref()
                .unwrap_or("contender did not complete");
            return Err(ExecutableArenaError::ExecutionFailed(format!(
                "contender `{candidate_id}` failed: {detail}"
            )));
        }
        validate_no_raw_host_path(&child_res.output_text)?;
        collected.push(ExecutedContenderOutput {
            idx,
            candidate_id,
            neutral_label,
            model,
            output_text: child_res.output_text,
        });
    }

    collected.sort_by_key(|entry| entry.idx);
    Ok(collected)
}

async fn purge_workspaces(
    port: &Arc<dyn ArenaWorkspacePort>,
    provisioned: &[ProvisionedContender],
) {
    for item in provisioned {
        let _ = port.discard_workspace(&item.workspace_ref).await;
    }
}

async fn discard_branches_and_workspaces(
    branch_client: Option<&BranchFsApClient>,
    port: &Arc<dyn ArenaWorkspacePort>,
    provisioned: &[ProvisionedContender],
) {
    for item in provisioned {
        if let Some(client) = branch_client {
            let _ = client.discard(&item.candidate_id).await;
        }
        let _ = port.discard_workspace(&item.workspace_ref).await;
    }
}

fn neutral_contender_label(index: usize) -> String {
    let letter = (b'A' + (index as u8 % 26)) as char;
    if index < 26 {
        format!("Contender {letter}")
    } else {
        format!("Contender {letter}{}", index / 26)
    }
}

fn extract_json_object(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (start < end).then(|| &text[start..=end])
}
