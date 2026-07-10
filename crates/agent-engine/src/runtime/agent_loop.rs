//! Core agent loop implementation.
//!
//! This module contains the main agent execution logic.

mod namespace_environment;

pub(crate) use namespace_environment::NamespaceProcessContext;
#[cfg(test)]
pub(super) use namespace_environment::NamespaceRequestRecord;
pub use namespace_environment::{
    ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator,
    MountGrantApplicatorFactory, NamespaceActionRecord, NamespaceMountApplication,
    NamespaceRuntimeEnvironment, NamespaceToolActionOutput, NamespaceTurnOutput,
    NamespaceTurnRuntime, NamespaceTurnRuntimeConfig,
};

use alan_agent_protocol::{Event, Submission, ToolCapability};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::{config::Config, retry, runtime::RuntimeConfig, session::Session, tools::ToolRegistry};

use super::submission_handlers::{RuntimeOpAction, handle_runtime_op_with_cancel};
use super::tool_orchestrator::{
    ToolBatchOrchestratorOutcome, ToolOrchestratorInputs, replay_approved_tool_batch_with_cancel,
    replay_approved_tool_call_with_cancel,
};
use super::turn_driver::TurnInputBroker;
pub(super) use super::turn_executor::run_turn_with_cancel;
use super::turn_executor::{TurnExecutionOutcome, TurnRunKind};
use super::turn_state::{TurnActivityState, TurnState};
#[allow(unused_imports)]
use super::turn_support::{
    cancel_current_task, emit_streaming_chunks, normalize_tool_calls, split_text_for_typing,
};
/// Normalized tool call with guaranteed ID
#[derive(Debug, Clone)]
pub struct NormalizedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(crate) enum DeferredRuntimeAction {
    TurnMemoryPromotion(super::memory_promotion::TurnMemoryPromotionJob),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeferredRuntimeActionExit {
    Completed,
    Cancelled,
}

/// Runtime environment available to the Agent Execution Engine.
///
/// Generation, tools, and state are reached by walking files under one aP root.
pub enum RuntimeEnvironment {
    #[allow(dead_code)]
    Namespace {
        namespace: NamespaceRuntimeEnvironment,
        tool_definitions: Vec<crate::llm::ToolDefinition>,
    },
}

impl RuntimeEnvironment {
    #[allow(dead_code)]
    pub fn namespace(namespace: NamespaceRuntimeEnvironment) -> Self {
        Self::namespace_with_tool_definitions(namespace, Vec::new())
    }

    pub fn namespace_with_tool_definitions(
        namespace: NamespaceRuntimeEnvironment,
        tool_definitions: Vec<crate::llm::ToolDefinition>,
    ) -> Self {
        Self::Namespace {
            namespace,
            tool_definitions,
        }
    }
}

/// Agent state for the execution loop
pub struct RuntimeLoopState {
    pub workspace_id: String,
    pub workspace_root_dir: Option<std::path::PathBuf>,
    pub session: Session,
    pub current_submission_id: Option<String>,
    pub environment: RuntimeEnvironment,
    pub tool_catalog: ToolRegistry,
    pub core_config: Config,
    pub runtime_config: RuntimeConfig,
    pub workspace_persona_dirs: Vec<std::path::PathBuf>,
    pub prompt_cache: super::prompt_cache::PromptAssemblyCache,
    pub turn_state: TurnState,
}

impl RuntimeLoopState {
    pub(crate) fn namespace_environment(&self) -> &NamespaceRuntimeEnvironment {
        match &self.environment {
            RuntimeEnvironment::Namespace { namespace, .. } => namespace,
        }
    }

    pub(crate) async fn write_namespace_confirmation_request(
        &self,
        pending: &crate::approval::PendingConfirmation,
    ) -> Result<Option<String>> {
        let kind = crate::approval::runtime_confirmation_control_kind(&pending.checkpoint_type)
            .unwrap_or("confirmation");
        let options = serde_json::to_string(&serde_json::json!({
            "checkpoint_id": pending.checkpoint_id.clone(),
            "checkpoint_type": pending.checkpoint_type.clone(),
            "details": pending.details.clone(),
            "options": pending.options.clone(),
        }))?;
        let request_id = self
            .namespace_environment()
            .write_request(
                namespace_environment::NamespaceRequestRecord::new(kind, pending.summary.clone())
                    .with_options(options),
            )
            .await?;
        Ok(Some(request_id))
    }

    pub(crate) async fn write_namespace_structured_input_request(
        &self,
        pending: &crate::approval::PendingStructuredInputRequest,
    ) -> Result<Option<String>> {
        let options = serde_json::to_string(&serde_json::json!({
            "request_id": pending.request_id.clone(),
            "title": pending.title.clone(),
            "questions": pending.questions.clone(),
        }))?;
        let request_id = self
            .namespace_environment()
            .write_request(
                namespace_environment::NamespaceRequestRecord::new(
                    "structured_input",
                    pending.prompt.clone(),
                )
                .with_options(options),
            )
            .await?;
        Ok(Some(request_id))
    }

    pub(crate) async fn write_namespace_dynamic_tool_request(
        &self,
        pending: &crate::approval::PendingDynamicToolCall,
    ) -> Result<Option<String>> {
        let prompt = format!("Resolve dynamic tool: {}", pending.tool_name);
        let options = serde_json::to_string(&serde_json::json!({
            "call_id": pending.call_id.clone(),
            "tool_name": pending.tool_name.clone(),
            "arguments": pending.arguments.clone(),
        }))?;
        let request_id = self
            .namespace_environment()
            .write_request(
                namespace_environment::NamespaceRequestRecord::new("dynamic_tool", prompt)
                    .with_options(options),
            )
            .await?;
        Ok(Some(request_id))
    }

    pub(crate) fn project_generation_messages(
        &self,
        messages: &[crate::session::Message],
    ) -> Vec<crate::llm::Message> {
        super::turn_support::project_messages_for_namespace(messages)
    }

    pub(crate) async fn generate_once_with_cancel(
        &mut self,
        request: crate::llm::GenerationRequest,
        cancel: &CancellationToken,
        cancel_message: &'static str,
    ) -> Result<crate::llm::GenerationResponse> {
        let namespace = self.namespace_environment().clone();
        match namespace.generate_controlled(&request, 0, cancel).await {
            Err(_) if cancel.is_cancelled() => Err(anyhow::anyhow!(cancel_message)),
            result => result,
        }
    }

    pub(crate) async fn generate_response_with_retry(
        &mut self,
        request: crate::llm::GenerationRequest,
        timeout_secs: u64,
        cancel: &CancellationToken,
    ) -> Result<crate::llm::GenerationResponse> {
        let max_retries = retry::DEFAULT_MAX_RETRIES;
        let mut last_error = None;

        for attempt in 0..=max_retries {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("LLM request cancelled"));
            }

            let namespace = self.namespace_environment().clone();
            let attempt_request = request.clone();
            let result = namespace
                .generate_controlled(&attempt_request, timeout_secs, cancel)
                .await;

            match result {
                Ok(response) => return Ok(response),
                Err(error) => {
                    if !retry::is_retryable(&error) || attempt >= max_retries {
                        return Err(error);
                    }
                    last_error = Some(error);
                    let delay = retry::backoff_delay(attempt + 1);
                    tokio::select! {
                        _ = cancel.cancelled() => return Err(anyhow::anyhow!("LLM request cancelled")),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
    }

    pub(crate) fn tool_catalog(&self) -> &ToolRegistry {
        &self.tool_catalog
    }

    pub(crate) fn static_tool_definitions(&self) -> Vec<crate::llm::ToolDefinition> {
        self.tool_catalog.get_tool_definitions()
    }

    pub(crate) fn static_tool_names(&self) -> Vec<String> {
        self.tool_catalog
            .list_tools()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    pub(crate) fn static_tool_capability(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Option<ToolCapability> {
        self.tool_catalog.capability_for_tool(tool_name, arguments)
    }

    pub(crate) fn static_tool_locality(
        &self,
        tool_name: &str,
    ) -> Option<crate::tools::ToolLocality> {
        self.tool_catalog.tool_locality(tool_name)
    }

    pub(crate) fn default_tool_cwd(&self) -> Option<std::path::PathBuf> {
        self.tool_catalog.default_cwd()
    }

    #[cfg(test)]
    pub(crate) fn tool_catalog_mut_for_test(&mut self) -> &mut ToolRegistry {
        &mut self.tool_catalog
    }
}

/// Handle a single submission
#[cfg_attr(not(test), allow(dead_code))]
pub async fn handle_submission<E, F>(
    state: &mut RuntimeLoopState,
    submission: Submission,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let cancel = CancellationToken::new();
    handle_submission_with_cancel(state, submission, emit, &cancel).await
}

pub(crate) async fn handle_submission_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    submission: Submission,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    handle_submission_with_cancel_and_steering(state, submission, emit, cancel, None).await
}

pub(crate) async fn handle_submission_with_cancel_and_steering<E, F>(
    state: &mut RuntimeLoopState,
    submission: Submission,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let op = submission.op;

    match handle_runtime_op_with_cancel(state, op, emit, cancel).await? {
        RuntimeOpAction::NoTurn => Ok(()),
        RuntimeOpAction::RunTurn {
            turn_kind,
            user_input,
            activate_task,
        } => {
            state
                .turn_state
                .set_turn_activity(TurnActivityState::Running);
            let turn_outcome = match run_turn_with_cancel(
                state,
                turn_kind,
                user_input,
                emit,
                cancel,
                steering_broker,
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(err) => {
                    state.turn_state.set_turn_activity(TurnActivityState::Idle);
                    return Err(err);
                }
            };
            state.turn_state.set_turn_activity(
                if matches!(turn_outcome, TurnExecutionOutcome::Paused) {
                    TurnActivityState::Paused
                } else {
                    TurnActivityState::Idle
                },
            );
            if activate_task {
                state.session.has_active_task = true;
            }
            Ok(())
        }
        RuntimeOpAction::ReplayApprovedToolCall {
            tool_call,
            approved_unknown_effect_call_id,
            approved_tool_escalation_call_id,
        } => {
            state
                .turn_state
                .set_turn_activity(TurnActivityState::Running);
            match replay_approved_tool_call_with_cancel(
                state,
                &tool_call,
                approved_unknown_effect_call_id.as_deref(),
                approved_tool_escalation_call_id.as_deref(),
                ToolOrchestratorInputs {
                    cancel,
                    steering_broker,
                },
                emit,
            )
            .await
            {
                Ok(outcome) => match outcome {
                    ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                        let turn_outcome = match run_turn_with_cancel(
                            state,
                            TurnRunKind::ResumeTurn,
                            None,
                            emit,
                            cancel,
                            steering_broker,
                        )
                        .await
                        {
                            Ok(outcome) => outcome,
                            Err(err) => {
                                state.turn_state.set_turn_activity(TurnActivityState::Idle);
                                return Err(err);
                            }
                        };
                        state.turn_state.set_turn_activity(
                            if matches!(turn_outcome, TurnExecutionOutcome::Paused) {
                                TurnActivityState::Paused
                            } else {
                                TurnActivityState::Idle
                            },
                        );
                    }
                    ToolBatchOrchestratorOutcome::PauseTurn => {
                        state
                            .turn_state
                            .set_turn_activity(TurnActivityState::Paused);
                    }
                    ToolBatchOrchestratorOutcome::EndTurn { surfaces_refreshed } => {
                        finalize_replayed_tool_end_turn_best_effort(
                            state,
                            cancel,
                            surfaces_refreshed,
                            "approved-tool-replay-ended-turn",
                            "after approved tool replay call",
                        )
                        .await;
                    }
                },
                Err(err) => {
                    state.turn_state.set_turn_activity(TurnActivityState::Idle);
                    return Err(err);
                }
            };
            Ok(())
        }
        RuntimeOpAction::ReplayApprovedToolBatch {
            tool_calls,
            approved_unknown_effect_call_id,
            approved_tool_escalation_call_id,
        } => {
            state
                .turn_state
                .set_turn_activity(TurnActivityState::Running);
            match replay_approved_tool_batch_with_cancel(
                state,
                &tool_calls,
                approved_unknown_effect_call_id.as_deref(),
                approved_tool_escalation_call_id.as_deref(),
                ToolOrchestratorInputs {
                    cancel,
                    steering_broker,
                },
                emit,
            )
            .await
            {
                Ok(outcome) => match outcome {
                    ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                        let turn_outcome = match run_turn_with_cancel(
                            state,
                            TurnRunKind::ResumeTurn,
                            None,
                            emit,
                            cancel,
                            steering_broker,
                        )
                        .await
                        {
                            Ok(outcome) => outcome,
                            Err(err) => {
                                state.turn_state.set_turn_activity(TurnActivityState::Idle);
                                return Err(err);
                            }
                        };
                        state.turn_state.set_turn_activity(
                            if matches!(turn_outcome, TurnExecutionOutcome::Paused) {
                                TurnActivityState::Paused
                            } else {
                                TurnActivityState::Idle
                            },
                        );
                    }
                    ToolBatchOrchestratorOutcome::PauseTurn => {
                        state
                            .turn_state
                            .set_turn_activity(TurnActivityState::Paused);
                    }
                    ToolBatchOrchestratorOutcome::EndTurn { surfaces_refreshed } => {
                        finalize_replayed_tool_end_turn_best_effort(
                            state,
                            cancel,
                            surfaces_refreshed,
                            "approved-tool-replay-ended-turn",
                            "after approved tool replay batch",
                        )
                        .await;
                    }
                },
                Err(err) => {
                    state.turn_state.set_turn_activity(TurnActivityState::Idle);
                    return Err(err);
                }
            };
            Ok(())
        }
    }
}

async fn finalize_replayed_tool_end_turn_best_effort(
    state: &mut RuntimeLoopState,
    cancel: &CancellationToken,
    surfaces_refreshed: bool,
    surfaces_context: &'static str,
    promotion_context: &'static str,
) {
    if !cancel.is_cancelled() {
        if !surfaces_refreshed {
            super::memory_surfaces::refresh_turn_memory_surfaces_best_effort(
                state,
                surfaces_context,
            )
            .await;
        }
        if let Some(job) =
            super::memory_promotion::build_turn_memory_promotion_job(state, promotion_context)
        {
            state
                .turn_state
                .push_deferred_runtime_action(DeferredRuntimeAction::TurnMemoryPromotion(job));
        }
    }

    state.turn_state.set_turn_activity(TurnActivityState::Idle);
}

pub(super) async fn run_deferred_runtime_action_with_cancel(
    state: &mut RuntimeLoopState,
    action: DeferredRuntimeAction,
    cancel: &CancellationToken,
) -> DeferredRuntimeActionExit {
    match action {
        DeferredRuntimeAction::TurnMemoryPromotion(job) => {
            match super::memory_promotion::run_turn_memory_promotion_job_for_runtime_with_cancel(
                state, &job, cancel,
            )
            .await
            {
                Ok(()) => DeferredRuntimeActionExit::Completed,
                Err(_) if cancel.is_cancelled() => DeferredRuntimeActionExit::Cancelled,
                Err(err) => {
                    warn!(
                        error = %err,
                        context = job.warning_context,
                        "Failed to capture confirmed turn memory"
                    );
                    DeferredRuntimeActionExit::Completed
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::compaction::{
        COMPACTION_TOOL_OUTPUT_CHAR_LIMIT, CompactionRequest,
        DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS, DEGRADED_COMPACTION_SUMMARY_MAX_CHARS,
        build_degraded_compaction_summary, maybe_compact_context_for_request,
        sanitize_tool_text_for_compaction,
    };
    use super::*;

    use crate::approval::{PendingConfirmation, TOOL_ESCALATION_CHECKPOINT_TYPE};
    use crate::config::Config;
    use crate::llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk, ToolCall};
    use crate::rollout::{RolloutItem, RolloutRecorder};
    use alan_agent_protocol::{
        CompactionOutcome, CompactionPressureLevel, CompactionReason, CompactionResult,
        CompactionTrigger, MemoryFlushResult,
    };
    use alan_ap::{Fid, FileServer, OpenMode};
    use alan_shell::Shell;
    use serde_json::json;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    fn maybe_memory_promotion_response(request: &GenerationRequest) -> Option<GenerationResponse> {
        let system_prompt = request.system_prompt.as_deref()?;
        if system_prompt != crate::prompts::MEMORY_PROMOTION_PROMPT {
            return None;
        }

        let joined_user_text = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let content = if joined_user_text.contains("My name is Morris.") {
            serde_json::json!({
                "writes": [{
                    "kind": "user_identity",
                    "target": "USER.md",
                    "confidence": "high",
                    "disposition": "promote_now",
                    "observation": "Name: Morris",
                    "evidence": ["My name is Morris."],
                    "promotion_rationale": "Direct user-stated stable identity detail."
                }]
            })
            .to_string()
        } else {
            serde_json::json!({ "writes": [] }).to_string()
        };

        Some(GenerationResponse {
            content,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    fn generation_response(content: impl Into<String>) -> GenerationResponse {
        GenerationResponse {
            content: content.into(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        }
    }

    fn finished_stream(response: GenerationResponse) -> tokio::sync::mpsc::Receiver<StreamChunk> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _ = tx
                .send(StreamChunk {
                    text: Some(response.content),
                    thinking: response.thinking,
                    thinking_signature: response.thinking_signature,
                    redacted_thinking: response.redacted_thinking.into_iter().next(),
                    usage: response.usage,
                    provider_response_id: response.provider_response_id,
                    provider_response_status: response.provider_response_status,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: response.finish_reason.or_else(|| Some("stop".to_string())),
                })
                .await;
        });
        rx
    }

    fn namespace_environment_with_provider(
        provider: impl LlmProvider + 'static,
    ) -> RuntimeEnvironment {
        let (root, _procfs) = namespace_root_with_provider(provider);
        RuntimeEnvironment::namespace(NamespaceRuntimeEnvironment::new(
            root, "/agent/1", "default",
        ))
    }

    fn runtime_state_with_provider(provider: impl LlmProvider + 'static) -> RuntimeLoopState {
        runtime_state_with_environment(namespace_environment_with_provider(provider))
    }

    fn runtime_state_with_environment(environment: RuntimeEnvironment) -> RuntimeLoopState {
        RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session: Session::new(),
            current_submission_id: None,
            environment,
            tool_catalog: ToolRegistry::new(),
            core_config: Config::default(),
            runtime_config: super::RuntimeConfig::default(),
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        }
    }

    async fn namespace_environment_with_live_process(
        provider: impl LlmProvider + 'static,
    ) -> RuntimeEnvironment {
        let (root, procfs) = namespace_root_with_provider(provider);
        let pid = spawn_test_process(&procfs).await;
        assert_eq!(pid, "1");
        RuntimeEnvironment::namespace(NamespaceRuntimeEnvironment::new(
            root, "/agent/1", "default",
        ))
    }

    fn namespace_root_with_provider(
        provider: impl LlmProvider + 'static,
    ) -> (alan_ap::InProcessTransport, Arc<alan_kernel::ProcFs>) {
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(provider));
        let procfs = Arc::new(alan_kernel::ProcFs::new());

        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/proc",
            alan_ap::InProcessTransport::new(procfs.clone()),
            alan_kernel::Access::ReadWrite,
        );
        namespace.mount(
            "/agent/1",
            alan_ap::InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        namespace.mount(
            "/mnt/llm",
            alan_ap::InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
        (root, procfs)
    }

    async fn spawn_test_process(procfs: &alan_kernel::ProcFs) -> String {
        let clone_fid = Fid(80_000);
        procfs
            .walk(Fid::ROOT, clone_fid, &["clone".to_string()])
            .await
            .expect("walk /proc/clone");
        procfs
            .open(clone_fid, OpenMode::ReadWrite)
            .await
            .expect("open /proc/clone");
        let pid = String::from_utf8(
            procfs
                .read(clone_fid, 0, 64)
                .await
                .expect("read pending pid"),
        )
        .expect("pending pid is utf8");
        procfs
            .write(clone_fid, 0, br#"{"executable":"/bin/agent","args":[]}"#)
            .await
            .expect("write exec spec");
        procfs.clunk(clone_fid).await.expect("commit process");

        let list_fid = Fid(80_001);
        procfs
            .walk(Fid::ROOT, list_fid, &[])
            .await
            .expect("walk /proc");
        procfs
            .open(list_fid, OpenMode::Read)
            .await
            .expect("open /proc");
        let listing = String::from_utf8(
            procfs
                .read(list_fid, 0, 4096)
                .await
                .expect("read /proc listing"),
        )
        .expect("/proc listing is utf8");
        assert!(
            listing.lines().any(|line| line == pid),
            "spawned process {pid} should be visible in /proc: {listing:?}"
        );
        pid
    }

    struct DelayedMockProvider {
        delay: tokio::time::Duration,
        response_text: String,
    }

    impl DelayedMockProvider {
        fn new(delay: tokio::time::Duration, response_text: impl Into<String>) -> Self {
            Self {
                delay,
                response_text: response_text.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for DelayedMockProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            tokio::time::sleep(self.delay).await;
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            Ok(generation_response(self.response_text.clone()))
        }

        async fn chat(&mut self, _system: Option<&str>, user: &str) -> anyhow::Result<String> {
            Ok(format!("mock: {}", user))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            tokio::time::sleep(self.delay).await;
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(finished_stream(response));
            }
            Ok(finished_stream(generation_response(
                self.response_text.clone(),
            )))
        }

        fn provider_name(&self) -> &'static str {
            "mock"
        }
    }

    struct TimeoutThenSucceedStreamProvider {
        attempts: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl TimeoutThenSucceedStreamProvider {
        fn new(attempts: Arc<std::sync::atomic::AtomicUsize>) -> Self {
            Self { attempts }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for TimeoutThenSucceedStreamProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Err(anyhow::anyhow!(
                "timeout-then-succeed provider uses streaming"
            ))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("timeout-then-succeed stream provider".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let attempt = self
                .attempts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if attempt == 0 {
                let (tx, rx) = tokio::sync::mpsc::channel(1);
                tokio::spawn(async move {
                    let _hold = tx;
                    std::future::pending::<()>().await;
                });
                return Ok(rx);
            }

            Ok(finished_stream(generation_response("recovered")))
        }

        fn provider_name(&self) -> &'static str {
            "timeout_then_succeed_stream"
        }
    }

    fn create_replay_memory_test_state(
        memory_dir: std::path::PathBuf,
        turn_state: TurnState,
        session: Session,
    ) -> RuntimeLoopState {
        RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: {
                let mut config = Config::default();
                config.memory.workspace_dir = Some(memory_dir);
                config.memory.enabled = true;
                config
            },
            runtime_config: super::RuntimeConfig::default(),
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state,
        }
    }

    async fn run_deferred_runtime_actions(state: &mut RuntimeLoopState) -> usize {
        let cancel = CancellationToken::new();
        let actions = state.turn_state.drain_deferred_runtime_actions();
        let count = actions.len();
        for action in actions {
            assert_eq!(
                run_deferred_runtime_action_with_cancel(state, action, &cancel).await,
                DeferredRuntimeActionExit::Completed,
                "run deferred runtime action"
            );
        }
        count
    }

    // Test provider that returns errors
    struct ErrorMockProvider {
        error_message: String,
    }

    impl ErrorMockProvider {
        fn new(error_message: impl Into<String>) -> Self {
            Self {
                error_message: error_message.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for ErrorMockProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Err(anyhow::anyhow!("{}", self.error_message))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Err(anyhow::anyhow!("{}", self.error_message))
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            Err(anyhow::anyhow!("{}", self.error_message))
        }

        fn provider_name(&self) -> &'static str {
            "error_mock"
        }
    }

    struct FailThenSucceedMockProvider {
        failures_remaining: usize,
        response_text: String,
    }

    impl FailThenSucceedMockProvider {
        fn new(failures_remaining: usize, response_text: impl Into<String>) -> Self {
            Self {
                failures_remaining,
                response_text: response_text.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for FailThenSucceedMockProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            if self.failures_remaining > 0 {
                self.failures_remaining -= 1;
                return Err(anyhow::anyhow!("synthetic retryable compaction failure"));
            }

            Ok(generation_response(self.response_text.clone()))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Err(anyhow::anyhow!(
                "FailThenSucceedMockProvider does not implement chat"
            ))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let response = self.generate(request).await?;
            Ok(finished_stream(response))
        }

        fn provider_name(&self) -> &'static str {
            "fail_then_succeed_mock"
        }
    }

    #[derive(Clone)]
    enum SequencedStep {
        Success(String),
        Error(String),
    }

    struct SequencedMockProvider {
        steps: Arc<Mutex<VecDeque<SequencedStep>>>,
    }

    impl SequencedMockProvider {
        fn new(steps: Vec<SequencedStep>) -> Self {
            Self {
                steps: Arc::new(Mutex::new(steps.into())),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for SequencedMockProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            match self.steps.lock().unwrap().pop_front() {
                Some(SequencedStep::Success(content)) => Ok(generation_response(content)),
                Some(SequencedStep::Error(message)) => Err(anyhow::anyhow!(message)),
                None => Err(anyhow::anyhow!("sequenced mock provider exhausted")),
            }
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Err(anyhow::anyhow!(
                "SequencedMockProvider does not implement chat"
            ))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let response = self.generate(request).await?;
            Ok(finished_stream(response))
        }

        fn provider_name(&self) -> &'static str {
            "sequenced_mock"
        }
    }

    #[tokio::test]
    async fn namespace_generation_retries_transient_llmfs_errors() {
        let mut state = runtime_state_with_provider(SequencedMockProvider::new(vec![
            SequencedStep::Error("503 unavailable".to_string()),
            SequencedStep::Success("recovered".to_string()),
        ]));
        let cancel = CancellationToken::new();

        let response = state
            .generate_response_with_retry(
                GenerationRequest::new().with_user_message("hello"),
                0,
                &cancel,
            )
            .await
            .unwrap();

        assert_eq!(response.content, "recovered");
    }

    #[tokio::test]
    async fn namespace_generation_aborts_timed_out_generation_before_retry() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (root, _procfs) = namespace_root_with_provider(TimeoutThenSucceedStreamProvider::new(
            Arc::clone(&attempts),
        ));
        let shell = Shell::new(root.clone());
        let mut state = runtime_state_with_environment(RuntimeEnvironment::namespace(
            NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        ));
        let cancel = CancellationToken::new();

        let response = state
            .generate_response_with_retry(
                GenerationRequest::new().with_user_message("hello"),
                1,
                &cancel,
            )
            .await
            .unwrap();

        assert_eq!(response.content, "recovered");
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);

        let status = String::from_utf8(
            shell
                .cat("/mnt/llm/connections/default/g0/status")
                .await
                .unwrap(),
        )
        .unwrap();
        let status: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(status["status"], "aborted");
    }

    fn memory_flush_json_response() -> String {
        serde_json::json!({
            "why": "retain durable blockers before compaction",
            "key_decisions": ["Keep pre-compaction memory flush linked to the compaction attempt"],
            "constraints": ["Do not lose replay metadata"],
            "next_steps": ["Land the runtime coordinator PR"],
            "important_refs": ["crates/agent-engine/src/runtime/compaction.rs"],
        })
        .to_string()
    }

    #[test]
    fn test_sanitize_tool_text_for_compaction_preserves_identifiers_and_trims_noise() {
        let mut tool_output = String::new();
        tool_output.push_str("DEBUG starting noisy stream\n");
        tool_output.push_str("command: cargo test -p alan-agent-engine compact\n");
        tool_output.push_str("path: crates/agent-engine/src/tape.rs\n");
        tool_output.push_str("tool_call_id: call_123\n");
        for idx in 0..200 {
            tool_output.push_str(&format!("DEBUG noisy line {idx}\n"));
        }
        tool_output.push_str("final status: ok\n");

        let sanitized = sanitize_tool_text_for_compaction(&tool_output);
        assert!(sanitized.contains("cargo test -p alan-agent-engine compact"));
        assert!(sanitized.contains("crates/agent-engine/src/tape.rs"));
        assert!(sanitized.contains("call_123"));
        assert!(sanitized.contains("lines omitted"));
        assert!(sanitized.chars().count() < tool_output.chars().count());
    }

    #[test]
    fn test_sanitize_tool_text_for_compaction_enforces_hard_char_cap() {
        let tool_output = "x".repeat(COMPACTION_TOOL_OUTPUT_CHAR_LIMIT * 2);

        let sanitized = sanitize_tool_text_for_compaction(&tool_output);

        assert!(sanitized.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT);
        assert!(sanitized.ends_with("[truncated for compaction]"));
    }

    #[test]
    fn test_sanitize_tool_text_for_compaction_preserves_tail_identifiers_under_hard_cap() {
        let long_noise = "x".repeat(COMPACTION_TOOL_OUTPUT_CHAR_LIMIT);
        let tool_output = format!(
            "{long_noise}\n{long_noise}\n{long_noise}\npath: crates/agent-engine/src/runtime/agent_loop.rs\ntool_call_id: call_tail_123\nfinal status: failed"
        );

        let sanitized = sanitize_tool_text_for_compaction(&tool_output);

        assert!(sanitized.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT);
        assert!(sanitized.contains("crates/agent-engine/src/runtime/agent_loop.rs"));
        assert!(sanitized.contains("call_tail_123"));
        assert!(sanitized.contains("final status: failed"));
    }

    #[test]
    fn test_normalize_tool_calls_with_ids() {
        let tool_calls = vec![
            ToolCall {
                id: Some("call_1".to_string()),
                name: "search".to_string(),
                arguments: json!({"query": "test"}),
            },
            ToolCall {
                id: Some("call_2".to_string()),
                name: "memory_write".to_string(),
                arguments: json!({"content": "data"}),
            },
        ];

        let normalized = normalize_tool_calls(tool_calls);

        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].id, "call_1");
        assert_eq!(normalized[0].name, "search");
        assert_eq!(normalized[1].id, "call_2");
        assert_eq!(normalized[1].name, "memory_write");
    }

    #[test]
    fn test_normalize_tool_calls_missing_ids() {
        let tool_calls = vec![
            ToolCall {
                id: None,
                name: "search".to_string(),
                arguments: json!({}),
            },
            ToolCall {
                id: Some("".to_string()),
                name: "write".to_string(),
                arguments: json!({}),
            },
            ToolCall {
                id: Some("  ".to_string()),
                name: "read".to_string(),
                arguments: json!({}),
            },
        ];

        let normalized = normalize_tool_calls(tool_calls);

        assert_eq!(normalized.len(), 3);
        // All should have generated IDs
        assert!(!normalized[0].id.is_empty());
        assert!(!normalized[1].id.is_empty());
        assert!(!normalized[2].id.is_empty());
        // IDs should be different
        assert_ne!(normalized[0].id, normalized[1].id);
    }

    #[test]
    fn test_normalize_tool_calls_empty() {
        let tool_calls: Vec<ToolCall> = vec![];
        let normalized = normalize_tool_calls(tool_calls);
        assert!(normalized.is_empty());
    }

    #[test]
    fn test_split_text_for_typing() {
        let text = "Hello";
        let chunks = split_text_for_typing(text);

        assert_eq!(chunks, vec!["Hello".to_string()]);
    }

    #[test]
    fn test_split_text_for_typing_empty() {
        let chunks = split_text_for_typing("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_split_text_for_typing_unicode() {
        let text = "你好";
        let chunks = split_text_for_typing(text);

        assert_eq!(chunks, vec!["你好".to_string()]);
    }

    #[test]
    fn test_split_text_for_typing_long_text_chunks_preserve_content() {
        let text = "This is a longer sentence that should be chunked near whitespace boundaries for streaming.";
        let chunks = split_text_for_typing(text);

        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| !c.is_empty()));
        assert_eq!(chunks.concat(), text);
    }

    #[tokio::test]
    async fn test_cancel_current_task() {
        let config = Config::default();
        let session = Session::new();
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: {
                let mut turn_state = TurnState::default();
                turn_state.set_confirmation(PendingConfirmation {
                    checkpoint_id: "cp_123".to_string(),
                    checkpoint_type: "test_checkpoint".to_string(),
                    summary: "Test".to_string(),
                    details: json!({}),
                    options: vec!["approve".to_string()],
                });
                turn_state
            },
        };
        state.session.add_user_message("existing history");
        state.session.has_active_task = true;

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = cancel_current_task(&mut state, &mut emit).await;

        assert!(result.is_ok());
        assert!(state.turn_state.pending_confirmation().is_none());
        assert!(!state.session.has_active_task);
        assert_eq!(state.session.tape.messages().len(), 1);
        assert_eq!(
            state.session.tape.messages()[0].text_content(),
            "existing history"
        );

        // Check events
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::TurnCompleted { summary } => {
                assert_eq!(summary.as_deref(), Some("Task cancelled by user"));
            }
            _ => panic!("Expected TurnCompleted event"),
        }
    }

    #[tokio::test]
    async fn test_handle_submission_promotes_direct_user_fact_when_replayed_tool_call_ends_turn() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");

        let checkpoint_id = "tool_escalation_call-1";
        let mut session = Session::new();
        session.id = "sess-replay-call".to_string();
        session.add_user_message("My name is Morris.");

        let mut turn_state = TurnState::default();
        turn_state.begin_turn(0);
        turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: checkpoint_id.to_string(),
            checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
            summary: "Replay tool call".to_string(),
            details: json!({
                "replay_tool_call": {
                    "call_id": "call-1",
                    "tool_name": "request_confirmation",
                    "arguments": {}
                }
            }),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let mut state = create_replay_memory_test_state(memory_dir.clone(), turn_state, session);
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let result = handle_submission_with_cancel(
            &mut state,
            Submission::new(alan_agent_protocol::Op::Resume {
                request_id: checkpoint_id.to_string(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    json!({"choice": "approve"}),
                )],
            }),
            &mut emit,
            &cancel,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.turn_state.turn_activity(), TurnActivityState::Idle);
        assert_eq!(run_deferred_runtime_actions(&mut state).await, 1);

        let user_memory = std::fs::read_to_string(memory_dir.join("USER.md")).unwrap();
        assert!(user_memory.contains("Name: Morris"));
    }

    #[tokio::test]
    async fn test_handle_submission_promotes_direct_user_fact_when_replayed_tool_batch_ends_turn() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");

        let checkpoint_id = "tool_escalation_batch-1";
        let mut session = Session::new();
        session.id = "sess-replay-batch".to_string();
        session.add_user_message("My name is Morris.");

        let mut turn_state = TurnState::default();
        turn_state.begin_turn(0);
        turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: checkpoint_id.to_string(),
            checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
            summary: "Replay tool batch".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });
        turn_state.set_tool_replay_batch(
            checkpoint_id,
            vec![NormalizedToolCall {
                id: "call-1".to_string(),
                name: "request_confirmation".to_string(),
                arguments: json!({}),
            }],
        );

        let mut state = create_replay_memory_test_state(memory_dir.clone(), turn_state, session);
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let result = handle_submission_with_cancel(
            &mut state,
            Submission::new(alan_agent_protocol::Op::Resume {
                request_id: checkpoint_id.to_string(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    json!({"choice": "approve"}),
                )],
            }),
            &mut emit,
            &cancel,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.turn_state.turn_activity(), TurnActivityState::Idle);
        assert_eq!(run_deferred_runtime_actions(&mut state).await, 1);

        let user_memory = std::fs::read_to_string(memory_dir.join("USER.md")).unwrap();
        assert!(user_memory.contains("Name: Morris"));
    }

    #[tokio::test]
    async fn test_emit_streaming_chunks() {
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        emit_streaming_chunks(&mut emit, "Hi").await;

        // Should have: TextDelta content chunk, TextDelta final
        assert_eq!(events.len(), 2);

        match &events[0] {
            Event::TextDelta { chunk, is_final } => {
                assert_eq!(chunk, "Hi");
                assert!(!is_final);
            }
            _ => panic!("Expected TextDelta"),
        }

        match &events[1] {
            Event::TextDelta { chunk, is_final } => {
                assert!(chunk.is_empty());
                assert!(*is_final);
            }
            _ => panic!("Expected final TextDelta"),
        }
    }

    #[test]
    fn test_agent_loop_state_creation() {
        let config = Config::default();
        let session = Session::new();
        let runtime_config = super::RuntimeConfig::default();

        let state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        assert!(state.turn_state.pending_confirmation().is_none());
    }

    #[test]
    fn test_pending_confirmation_clone() {
        let pending = PendingConfirmation {
            checkpoint_id: "cp_123".to_string(),
            checkpoint_type: "test_checkpoint".to_string(),
            summary: "Test summary".to_string(),
            details: json!({"key": "value"}),
            options: vec!["approve".to_string(), "reject".to_string()],
        };

        let cloned = pending.clone();
        assert_eq!(pending.checkpoint_id, cloned.checkpoint_id);
        assert_eq!(pending.checkpoint_type, cloned.checkpoint_type);
        assert_eq!(pending.summary, cloned.summary);
    }

    #[test]
    fn test_normalized_tool_call_creation() {
        let call = NormalizedToolCall {
            id: "call_1".to_string(),
            name: "search".to_string(),
            arguments: json!({"query": "test"}),
        };

        assert_eq!(call.id, "call_1");
        assert_eq!(call.name, "search");
    }

    // Tests for maybe_compact_context
    #[tokio::test]
    async fn test_maybe_compact_context_no_compaction_needed() {
        let config = Config::default();
        let session = Session::new();
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        // Session is empty, no compaction needed
        let result = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await;

        assert!(result.is_ok());
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_maybe_compact_context_with_mock_llm() {
        let config = Config::default();
        let mut session = Session::new();

        // Add enough messages to trigger compaction
        for i in 0..65 {
            session.add_user_message(&format!("Message {}", i));
        }

        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "Summary",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await;

        // Should succeed or fail gracefully
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_maybe_compact_context_triggers_on_estimated_token_budget() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message(&"x".repeat(1200));
        session.add_assistant_message(&"y".repeat(1200), None);

        let mut runtime_config = super::RuntimeConfig::default();
        runtime_config.compaction_trigger_messages = 100; // avoid message-count trigger
        runtime_config.compaction_keep_last = 1;
        runtime_config.context_window_tokens = 256;
        runtime_config.compaction_trigger_ratio = 0.8;

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "Summary from token-triggered compaction",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut emit = |_event: Event| async {};
        let result = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.session.tape.len(), 1);
        let prompt_messages = state.session.tape.messages_for_prompt();
        assert!(prompt_messages.iter().any(|m| {
            m.is_context()
                && m.text_content()
                    .contains("Summary from token-triggered compaction")
        }));
        assert_eq!(
            state.session.tape.messages()[0].text_content(),
            "y".repeat(1200)
        );
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_maybe_compact_context_triggers_immediately_when_ratio_is_zero() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message(&"x".repeat(1200));
        session.add_assistant_message(&"y".repeat(1200), None);

        let mut runtime_config = super::RuntimeConfig::default();
        runtime_config.compaction_trigger_messages = 100; // avoid message-count trigger
        runtime_config.compaction_keep_last = 1;
        runtime_config.context_window_tokens = 16_384;
        runtime_config.compaction_trigger_ratio = 0.0;

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "Summary from zero-ratio compaction",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut emit = |_event: Event| async {};
        let result = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.session.tape.len(), 1);
        let prompt_messages = state.session.tape.messages_for_prompt();
        assert!(prompt_messages.iter().any(|m| {
            m.is_context()
                && m.text_content()
                    .contains("Summary from zero-ratio compaction")
        }));
        assert_eq!(
            state.session.tape.messages()[0].text_content(),
            "y".repeat(1200)
        );
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_maybe_compact_context_skips_when_context_window_budget_has_room() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message(&"x".repeat(1200));
        session.add_assistant_message(&"y".repeat(1200), None);

        let mut runtime_config = super::RuntimeConfig::default();
        runtime_config.compaction_trigger_messages = 100; // avoid message-count trigger
        runtime_config.compaction_keep_last = 1;
        runtime_config.context_window_tokens = 16_384;
        runtime_config.compaction_trigger_ratio = 0.8;

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "Should not compact",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let original_len = state.session.tape.len();
        let mut emit = |_event: Event| async {};
        let result = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.session.tape.len(), original_len);
        assert!(state.session.tape.summary().is_none());
    }

    #[tokio::test]
    async fn test_auto_pre_turn_soft_compaction_flushes_memory_before_compaction() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let memory_dir = temp_dir.path().join(".alan").join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

        let mut config = Config::default();
        config.memory.workspace_dir = Some(memory_dir.clone());

        let mut session = Session::new();
        for i in 0..6 {
            session.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
            session.add_assistant_message(
                &format!("Need to preserve file paths and next steps for blocker {i}."),
                None,
            );
        }

        let estimated_prompt_tokens = session.tape.estimated_prompt_tokens();
        let runtime_config = super::RuntimeConfig {
            compaction_trigger_messages: 100,
            compaction_keep_last: 1,
            context_window_tokens: ((estimated_prompt_tokens as f64) / 0.75).ceil() as u32,
            compaction_trigger_ratio: 0.85,
            compaction_soft_trigger_ratio: 0.70,
            compaction_hard_trigger_ratio: 0.85,
            ..super::RuntimeConfig::default()
        };

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
                SequencedStep::Success(memory_flush_json_response()),
                SequencedStep::Success("Summary after soft-threshold compaction".to_string()),
            ])),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, CompactionOutcome::Applied(_)));

        let flush_attempt = events.iter().find_map(|event| match event {
            Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });
        let compaction_attempt = events.iter().find_map(|event| match event {
            Event::CompactionObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });

        let flush_attempt = flush_attempt.expect("expected memory flush attempt");
        let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
        assert_eq!(flush_attempt.result, MemoryFlushResult::Success);
        assert_eq!(flush_attempt.pressure_level, CompactionPressureLevel::Soft);
        assert_eq!(
            compaction_attempt.pressure_level,
            Some(CompactionPressureLevel::Soft)
        );
        assert_eq!(
            compaction_attempt.memory_flush_attempt_id.as_deref(),
            Some(flush_attempt.attempt_id.as_str())
        );

        let note_path = memory_dir
            .join(crate::prompts::MEMORY_DAILY_DIRNAME)
            .join(format!("{}.md", chrono::Utc::now().format("%F")));
        let note = tokio::fs::read_to_string(note_path).await.unwrap();
        assert!(note.contains("attempt_id"));
        assert!(note.contains("crates/agent-engine/src/runtime/compaction.rs"));
        assert_eq!(
            state.session.latest_memory_flush_attempt(),
            Some(&flush_attempt)
        );
    }

    #[tokio::test]
    async fn test_auto_pre_turn_soft_compaction_continues_after_memory_flush_failure() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let memory_dir = temp_dir.path().join(".alan").join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

        let mut config = Config::default();
        config.memory.workspace_dir = Some(memory_dir.clone());

        let mut session = Session::new();
        for i in 0..6 {
            session.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
            session.add_assistant_message(
                &format!("Need to preserve file paths and next steps for blocker {i}."),
                None,
            );
        }

        let estimated_prompt_tokens = session.tape.estimated_prompt_tokens();
        let runtime_config = super::RuntimeConfig {
            compaction_trigger_messages: 100,
            compaction_keep_last: 1,
            context_window_tokens: ((estimated_prompt_tokens as f64) / 0.75).ceil() as u32,
            compaction_trigger_ratio: 0.85,
            compaction_soft_trigger_ratio: 0.70,
            compaction_hard_trigger_ratio: 0.85,
            ..super::RuntimeConfig::default()
        };

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
                SequencedStep::Error("synthetic memory flush failure".to_string()),
                SequencedStep::Success("Summary after failed memory flush".to_string()),
            ])),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, CompactionOutcome::Applied(_)));

        let flush_attempt = events.iter().find_map(|event| match event {
            Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });
        let compaction_attempt = events.iter().find_map(|event| match event {
            Event::CompactionObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });
        let warnings: Vec<String> = events
            .iter()
            .filter_map(|event| match event {
                Event::Warning { message } => Some(message.clone()),
                _ => None,
            })
            .collect();

        let flush_attempt = flush_attempt.expect("expected memory flush attempt");
        let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
        assert_eq!(flush_attempt.result, MemoryFlushResult::Failure);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("Silent memory flush failed"))
        );
        assert_eq!(
            compaction_attempt.memory_flush_attempt_id.as_deref(),
            Some(flush_attempt.attempt_id.as_str())
        );
        assert!(
            !memory_dir
                .join(crate::prompts::MEMORY_DAILY_DIRNAME)
                .join(format!("{}.md", chrono::Utc::now().format("%F")))
                .exists(),
            "failed memory flush should not write a daily note"
        );
    }

    #[tokio::test]
    async fn test_auto_pre_turn_soft_compaction_skips_memory_flush_when_nothing_is_durable() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let memory_dir = temp_dir.path().join(".alan").join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

        let mut config = Config::default();
        config.memory.workspace_dir = Some(memory_dir.clone());

        let mut session = Session::new();
        for i in 0..6 {
            session.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
            session.add_assistant_message(
                &format!("Need to preserve file paths and next steps for blocker {i}."),
                None,
            );
        }

        let estimated_prompt_tokens = session.tape.estimated_prompt_tokens();
        let runtime_config = super::RuntimeConfig {
            compaction_trigger_messages: 100,
            compaction_keep_last: 1,
            context_window_tokens: ((estimated_prompt_tokens as f64) / 0.75).ceil() as u32,
            compaction_trigger_ratio: 0.85,
            compaction_soft_trigger_ratio: 0.70,
            compaction_hard_trigger_ratio: 0.85,
            ..super::RuntimeConfig::default()
        };

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
                SequencedStep::Success(
                    "{\"why\":\"\",\"key_decisions\":[],\"constraints\":[],\"next_steps\":[],\"important_refs\":[]}"
                        .to_string(),
                ),
                SequencedStep::Success("Summary after noop memory flush".to_string()),
            ])),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, CompactionOutcome::Applied(_)));

        let flush_attempt = events.iter().find_map(|event| match event {
            Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });
        let compaction_attempt = events.iter().find_map(|event| match event {
            Event::CompactionObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });

        let flush_attempt = flush_attempt.expect("expected memory flush attempt");
        let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
        assert_eq!(flush_attempt.result, MemoryFlushResult::Skipped);
        assert_eq!(
            flush_attempt.skip_reason,
            Some(alan_agent_protocol::MemoryFlushSkipReason::NoDurableContent)
        );
        assert!(flush_attempt.warning_message.is_none());
        assert!(flush_attempt.error_message.is_none());
        assert_eq!(
            compaction_attempt.memory_flush_attempt_id.as_deref(),
            Some(flush_attempt.attempt_id.as_str())
        );
        assert!(
            !memory_dir
                .join(crate::prompts::MEMORY_DAILY_DIRNAME)
                .join(format!("{}.md", chrono::Utc::now().format("%F")))
                .exists(),
            "noop memory flush should not write a daily note"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Warning { .. })),
            "noop memory flush should not emit warnings"
        );
    }

    #[tokio::test]
    async fn test_auto_pre_turn_soft_compaction_records_already_flushed_cycle_skip() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let memory_dir = temp_dir.path().join(".alan").join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

        let mut config = Config::default();
        config.memory.workspace_dir = Some(memory_dir.clone());

        let mut session = Session::new();
        for i in 0..6 {
            session.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
            session.add_assistant_message(
                &format!("Need to preserve file paths and next steps for blocker {i}."),
                None,
            );
        }
        session.note_auto_memory_flush_attempt();

        let estimated_prompt_tokens = session.tape.estimated_prompt_tokens();
        let runtime_config = super::RuntimeConfig {
            compaction_trigger_messages: 100,
            compaction_keep_last: 1,
            context_window_tokens: ((estimated_prompt_tokens as f64) / 0.75).ceil() as u32,
            compaction_trigger_ratio: 0.85,
            compaction_soft_trigger_ratio: 0.70,
            compaction_hard_trigger_ratio: 0.85,
            ..super::RuntimeConfig::default()
        };

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
                SequencedStep::Success("Summary after already-flushed-cycle skip".to_string()),
            ])),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, CompactionOutcome::Applied(_)));

        let flush_attempt = events.iter().find_map(|event| match event {
            Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });
        let compaction_attempt = events.iter().find_map(|event| match event {
            Event::CompactionObserved { attempt } => Some(attempt.clone()),
            _ => None,
        });

        let flush_attempt = flush_attempt.expect("expected memory flush attempt");
        let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
        assert_eq!(flush_attempt.result, MemoryFlushResult::Skipped);
        assert_eq!(
            flush_attempt.skip_reason,
            Some(alan_agent_protocol::MemoryFlushSkipReason::AlreadyFlushedThisCycle)
        );
        assert_eq!(
            compaction_attempt.memory_flush_attempt_id.as_deref(),
            Some(flush_attempt.attempt_id.as_str())
        );
        assert!(
            !memory_dir
                .join(crate::prompts::MEMORY_DAILY_DIRNAME)
                .join(format!("{}.md", chrono::Utc::now().format("%F")))
                .exists(),
            "already-flushed-cycle skip should not write a daily note"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Warning { .. })),
            "already-flushed-cycle skip should not emit warnings"
        );
    }

    #[tokio::test]
    async fn test_auto_pre_turn_hard_compaction_skips_memory_flush() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let memory_dir = temp_dir.path().join(".alan").join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

        let mut config = Config::default();
        config.memory.workspace_dir = Some(memory_dir);

        let mut session = Session::new();
        for i in 0..6 {
            session.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
            session.add_assistant_message(
                &format!("Need to preserve file paths and next steps for blocker {i}."),
                None,
            );
        }

        let estimated_prompt_tokens = session.tape.estimated_prompt_tokens();
        let runtime_config = super::RuntimeConfig {
            compaction_trigger_messages: 100,
            compaction_keep_last: 1,
            context_window_tokens: ((estimated_prompt_tokens as f64) / 0.95).ceil() as u32,
            compaction_trigger_ratio: 0.80,
            compaction_soft_trigger_ratio: 0.70,
            compaction_hard_trigger_ratio: 0.80,
            ..super::RuntimeConfig::default()
        };

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
                SequencedStep::Success("Summary at hard threshold".to_string()),
            ])),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_pre_turn(),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, CompactionOutcome::Applied(_)));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::MemoryFlushObserved { .. }))
        );
        let compaction_attempt = events.iter().find_map(|event| match event {
            Event::CompactionObserved { attempt } => Some(attempt),
            _ => None,
        });
        assert_eq!(
            compaction_attempt.and_then(|attempt| attempt.pressure_level),
            Some(CompactionPressureLevel::Hard)
        );
        assert_eq!(
            compaction_attempt.and_then(|attempt| attempt.memory_flush_attempt_id.as_deref()),
            None
        );
    }

    #[tokio::test]
    async fn test_manual_compaction_bypasses_automatic_thresholds_without_memory_flush() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message("Investigate the compaction contract.");
        session.add_assistant_message("Need to preserve the current next step.", None);

        let runtime_config = super::RuntimeConfig {
            compaction_trigger_messages: 100,
            compaction_keep_last: 1,
            context_window_tokens: 128_000,
            compaction_trigger_ratio: 0.95,
            compaction_soft_trigger_ratio: 0.90,
            compaction_hard_trigger_ratio: 0.95,
            ..super::RuntimeConfig::default()
        };

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "Manual compaction below threshold",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::manual(None),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, CompactionOutcome::Applied(_)));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::MemoryFlushObserved { .. }))
        );
        assert_eq!(
            state.session.tape.summary(),
            Some("Manual compaction below threshold")
        );
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_maybe_compact_context_allows_mid_turn_emergency_near_hard_limit() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message(&"x".repeat(1200));
        session.add_assistant_message(&"y".repeat(1200), None);
        let estimated_prompt_tokens = session.tape.estimated_prompt_tokens();

        let mut runtime_config = super::RuntimeConfig::default();
        runtime_config.compaction_trigger_messages = 100;
        runtime_config.compaction_keep_last = 1;
        runtime_config.context_window_tokens = (estimated_prompt_tokens + 10) as u32;
        runtime_config.compaction_trigger_ratio = 1.0;

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "Summary from emergency mid-turn compaction",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut emit = |_event: Event| async {};
        let result = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::automatic_mid_turn(),
        )
        .await;

        assert!(matches!(result, Ok(CompactionOutcome::Applied(_))));
        assert_eq!(
            state.session.tape.summary(),
            Some("Summary from emergency mid-turn compaction")
        );
    }

    #[tokio::test]
    async fn test_manual_compaction_records_audit_fields() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let config = Config::default();
        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        for i in 0..65 {
            session.add_user_message(&format!("Message {}", i));
        }

        let rollout_path = session.rollout_path().unwrap().clone();
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: Some("sub-compact".to_string()),
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "Manual compaction summary",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };
        maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::manual(Some("preserve todos and constraints".to_string())),
        )
        .await
        .unwrap();
        state.session.flush().await;

        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let attempt = items.iter().find_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt),
            _ => None,
        });
        let compacted = items.iter().find_map(|item| match item {
            RolloutItem::Compacted(compacted) => Some(compacted),
            _ => None,
        });

        let attempt = attempt.expect("expected compaction attempt rollout item");
        let compacted = compacted.expect("expected compacted rollout item");
        assert_eq!(attempt.result, CompactionResult::Success);
        assert_eq!(attempt.submission_id.as_deref(), Some("sub-compact"));
        assert_eq!(attempt.request.trigger, CompactionTrigger::Manual);
        assert_eq!(attempt.request.reason, CompactionReason::ExplicitRequest);
        assert_eq!(
            attempt.request.focus.as_deref(),
            Some("preserve todos and constraints")
        );
        assert!(attempt.tape_mutated);
        assert_eq!(
            compacted.attempt_id.as_deref(),
            Some(attempt.attempt_id.as_str())
        );
        assert_eq!(compacted.message, "Manual compaction summary");
        assert_eq!(compacted.trigger, Some(CompactionTrigger::Manual));
        assert_eq!(compacted.reason, Some(CompactionReason::ExplicitRequest));
        assert_eq!(
            compacted.focus.as_deref(),
            Some("preserve todos and constraints")
        );
        assert_eq!(compacted.result, Some(CompactionResult::Success));
        assert!(compacted.input_messages.is_some());
        assert!(compacted.output_messages.is_some());
        assert!(compacted.input_tokens.is_some());
        assert!(compacted.output_tokens.is_some());
        assert!(compacted.duration_ms.is_some());
        assert_eq!(compacted.reference_context_revision, Some(0));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::CompactionObserved { attempt }
                if attempt.submission_id.as_deref() == Some("sub-compact")
                    && attempt.result == CompactionResult::Success
        )));
    }

    #[tokio::test]
    async fn test_compaction_retry_result_is_audited_when_trimming_succeeds() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let config = Config::default();
        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        for i in 0..65 {
            session.add_user_message(&format!("Message {}", i));
        }

        let rollout_path = session.rollout_path().unwrap().clone();
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(FailThenSucceedMockProvider::new(
                1,
                "Compaction summary after retry",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut emit = |_event: Event| async {};
        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::manual(None),
        )
        .await
        .unwrap();
        state.session.flush().await;

        match outcome {
            CompactionOutcome::Applied(outcome) => {
                assert_eq!(outcome.result, CompactionResult::Retry);
            }
            other => panic!("expected compaction to apply after retry, got {other:?}"),
        }

        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let attempt = items.iter().find_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt),
            _ => None,
        });
        let compacted = items.iter().find_map(|item| match item {
            RolloutItem::Compacted(compacted) => Some(compacted),
            _ => None,
        });

        let attempt = attempt.expect("expected compaction attempt rollout item");
        let compacted = compacted.expect("expected compacted rollout item");
        assert_eq!(attempt.result, CompactionResult::Retry);
        assert_eq!(attempt.retry_count, 1);
        assert!(attempt.tape_mutated);
        assert_eq!(
            compacted.attempt_id.as_deref(),
            Some(attempt.attempt_id.as_str())
        );
        assert_eq!(compacted.message, "Compaction summary after retry");
        assert_eq!(compacted.retry_count, Some(1));
        assert_eq!(compacted.result, Some(CompactionResult::Retry));
    }

    #[tokio::test]
    async fn test_compaction_generation_failure_uses_degraded_fallback_and_audits_it() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let config = Config::default();
        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        for i in 0..65 {
            session.add_user_message(&format!("Message {}", i));
        }

        let rollout_path = session.rollout_path().unwrap().clone();
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(ErrorMockProvider::new(
                "synthetic compaction failure",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::manual(Some("preserve open todos".to_string())),
        )
        .await
        .unwrap();

        match outcome {
            CompactionOutcome::Applied(outcome) => {
                assert_eq!(outcome.result, CompactionResult::Degraded);
            }
            _ => panic!("expected degraded compaction to apply"),
        }
        assert!(
            state
                .session
                .tape
                .summary()
                .is_some_and(|summary| summary.contains("Deterministic fallback summary"))
        );
        assert!(events.iter().any(|event| {
            matches!(event, Event::Warning { message } if message.contains("deterministic fallback summary"))
        }));

        state.session.flush().await;
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let compacted = items.iter().find_map(|item| match item {
            RolloutItem::Compacted(compacted) => Some(compacted),
            _ => None,
        });
        let compacted = compacted.expect("expected compacted rollout item");
        assert_eq!(compacted.result, Some(CompactionResult::Degraded));

        let attempt = items.iter().find_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt),
            _ => None,
        });
        let attempt = attempt.expect("expected compaction attempt item");
        assert_eq!(attempt.result, CompactionResult::Degraded);
        assert!(attempt.tape_mutated);
        assert_eq!(
            attempt.request.focus.as_deref(),
            Some("preserve open todos")
        );
        assert_eq!(
            compacted.attempt_id.as_deref(),
            Some(attempt.attempt_id.as_str())
        );
    }

    #[tokio::test]
    async fn test_degraded_compaction_rebases_active_turn_start() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message("older turn 1");
        session.add_user_message("older turn 2");
        session.add_user_message("current turn");

        let runtime_config = super::RuntimeConfig {
            compaction_keep_last: 1,
            ..super::RuntimeConfig::default()
        };

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(ErrorMockProvider::new(
                "synthetic compaction failure",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let retention_start = state
            .session
            .tape
            .compaction_retention_start(state.runtime_config.compaction_keep_last);
        assert!(retention_start > 0);
        state.turn_state.begin_turn(retention_start);

        let mut emit = |_event: Event| async {};
        let outcome = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::manual(None),
        )
        .await
        .unwrap();

        match outcome {
            CompactionOutcome::Applied(outcome) => {
                assert_eq!(outcome.result, CompactionResult::Degraded);
            }
            _ => panic!("expected degraded compaction to apply"),
        }

        assert_eq!(state.turn_state.active_turn_message_start(), Some(0));
    }

    #[test]
    fn test_build_degraded_compaction_summary_bounds_prior_summary_growth() {
        let huge_summary = "legacy summary ".repeat(1_000);
        let messages = vec![
            crate::tape::Message::user("user context ".repeat(40)),
            crate::tape::Message::assistant("assistant context ".repeat(40)),
        ];

        let summary_one =
            build_degraded_compaction_summary(&messages, Some(&huge_summary)).unwrap();
        let summary_two = build_degraded_compaction_summary(&messages, Some(&summary_one)).unwrap();

        assert!(summary_one.contains("Prior summary excerpt:"));
        assert!(summary_one.chars().count() <= DEGRADED_COMPACTION_SUMMARY_MAX_CHARS);
        assert!(summary_two.contains("Prior summary excerpt:"));
        assert!(summary_two.chars().count() <= DEGRADED_COMPACTION_SUMMARY_MAX_CHARS);
    }

    #[test]
    fn test_build_degraded_compaction_summary_bounds_existing_summary_without_snippets() {
        let huge_summary = "legacy summary ".repeat(1_000);
        let summary = build_degraded_compaction_summary(
            &[crate::tape::Message::context("reference-only")],
            Some(&huge_summary),
        )
        .unwrap();

        assert!(summary.chars().count() <= DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS);
    }

    #[tokio::test]
    async fn test_compaction_failure_without_fallback_escalates_warning_and_preserves_tape() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let config = Config::default();
        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        for _ in 0..65 {
            session.tape.push(crate::tape::Message::assistant(""));
        }

        let original_messages = stateful_messages_snapshot(&session);
        let rollout_path = session.rollout_path().unwrap().clone();
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(ErrorMockProvider::new(
                "synthetic compaction failure",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let first = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::manual(None),
        )
        .await
        .unwrap();
        let second = maybe_compact_context_for_request(
            &mut state,
            &mut emit,
            CompactionRequest::manual(None),
        )
        .await
        .unwrap();

        assert!(matches!(first, CompactionOutcome::Failed(_)));
        assert!(matches!(second, CompactionOutcome::Failed(_)));
        assert_eq!(
            stateful_messages_snapshot(&state.session),
            original_messages
        );
        assert!(state.session.tape.summary().is_none());

        let warning_messages: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                Event::Warning { message } => Some(message.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(warning_messages.len(), 2);
        assert!(warning_messages[1].contains("consider starting a new session"));

        state.session.flush().await;
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let failure_attempts: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                RolloutItem::CompactionAttempt(attempt) => Some(attempt),
                _ => None,
            })
            .collect();
        assert_eq!(failure_attempts.len(), 2);
        assert!(
            failure_attempts
                .iter()
                .all(|attempt| attempt.result == CompactionResult::Failure && !attempt.tape_mutated)
        );
    }

    fn stateful_messages_snapshot(session: &Session) -> Vec<String> {
        session
            .tape
            .messages()
            .iter()
            .map(crate::tape::Message::text_content)
            .collect()
    }

    // Tests for handle_submission
    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_handle_submission_cancel() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message("existing history");
        session.has_active_task = true;
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_live_process(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "",
            ))
            .await,
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let submission = Submission::new(alan_agent_protocol::Op::Interrupt);

        let result = handle_submission(&mut state, submission, &mut emit).await;

        assert!(result.is_ok(), "interrupt should succeed: {result:?}");
        assert_eq!(events.len(), 1);
        assert_eq!(state.session.tape.messages().len(), 1);
        assert_eq!(
            state.session.tape.messages()[0].text_content(),
            "existing history"
        );
        assert!(!state.session.has_active_task);
        match &events[0] {
            Event::TurnCompleted { summary } => {
                assert_eq!(summary.as_deref(), Some("Task cancelled by user"));
            }
            _ => panic!("Expected TurnCompleted event"),
        }
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_handle_submission_rollback() {
        let config = Config::default();
        let mut session = Session::new();
        session.add_user_message("u1");
        session.add_assistant_message("a1", None);
        session.add_user_message("u2");
        session.add_assistant_message("a2", None);
        session.has_active_task = true;
        let runtime_config = super::RuntimeConfig::default();

        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_with_provider(DelayedMockProvider::new(
                tokio::time::Duration::from_millis(0),
                "",
            )),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let submission = Submission::new(alan_agent_protocol::Op::Rollback { turns: 1 });

        let result = handle_submission(&mut state, submission, &mut emit).await;

        assert!(result.is_ok());
        assert_eq!(state.session.tape.messages().len(), 2);
        assert_eq!(events.len(), 3);
        assert!(events.iter().any(|event| matches!(
            event,
            Event::SessionRolledBack {
                turns: 1,
                removed_messages: 2,
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::TextDelta { chunk, is_final }
                if *is_final && chunk.contains("Rolled back 1 turn(s), removed 2 message(s).")
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Warning { message }
                if message == crate::ROLLBACK_NON_DURABLE_WARNING
        )));
    }
}
