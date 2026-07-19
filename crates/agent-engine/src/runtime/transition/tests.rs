use super::super::compaction::{
    COMPACTION_TOOL_OUTPUT_CHAR_LIMIT, CompactionRequest, DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS,
    DEGRADED_COMPACTION_SUMMARY_MAX_CHARS, build_degraded_compaction_summary,
    maybe_compact_context_for_request as run_compaction, sanitize_tool_text_for_compaction,
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

mod tool_batch;

async fn maybe_compact_context_for_request<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    request: CompactionRequest,
) -> anyhow::Result<CompactionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    run_compaction(super::compaction_runtime(state), emit, request).await
}

#[test]
fn accepted_transition_classifies_only_turn_and_brokered_input_as_inband() {
    assert!(accepts_inband_submissions(&Op::Turn {
        parts: vec![alan_agent_protocol::ContentPart::text("turn")],
        context: None,
    }));
    assert!(accepts_inband_submissions(&Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("steer")],
        mode: InputMode::Steer,
    }));
    assert!(accepts_inband_submissions(&Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("follow-up")],
        mode: InputMode::FollowUp,
    }));

    assert!(!accepts_inband_submissions(&Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("next turn")],
        mode: InputMode::NextTurn,
    }));
    assert!(!accepts_inband_submissions(&Op::CompactWithOptions {
        focus: None,
    }));
    assert!(!accepts_inband_submissions(&Op::Rollback { turns: 1 }));
    assert!(!accepts_inband_submissions(&Op::Interrupt));
    assert!(!accepts_inband_submissions(&Op::Resume {
        request_id: "request-1".to_string(),
        content: Vec::new(),
    }));
}

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
) -> NamespaceRuntimeEnvironment {
    let (root, _procfs) = namespace_root_with_provider(provider);
    NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
}

fn runtime_state_with_provider(provider: impl LlmProvider + 'static) -> RuntimeLoopState {
    runtime_state_with_environment(namespace_environment_with_provider(provider))
}

fn runtime_state_with_environment(environment: NamespaceRuntimeEnvironment) -> RuntimeLoopState {
    RuntimeLoopState {
        machine: AgentMachine::new(),
        environment,
        core_config: Config::default(),
        runtime_config: super::RuntimeConfig::default(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    }
}

async fn namespace_environment_with_live_process(
    provider: impl LlmProvider + 'static,
) -> NamespaceRuntimeEnvironment {
    let (root, procfs) = namespace_root_with_provider(provider);
    let pid = spawn_test_process(&procfs).await;
    assert_eq!(pid, "1");
    NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
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
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
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
    machine: AgentMachine,
) -> RuntimeLoopState {
    RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        )),
        core_config: {
            let mut config = Config::default();
            config.memory.store_dir = Some(memory_dir);
            config.memory.enabled = true;
            config
        },
        runtime_config: super::RuntimeConfig::default(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    }
}

async fn run_deferred_runtime_actions(state: &mut RuntimeLoopState) -> usize {
    let cancel = CancellationToken::new();
    let actions = state.machine.drain_deferred_runtime_actions();
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
    let state = runtime_state_with_provider(SequencedMockProvider::new(vec![
        SequencedStep::Error("503 unavailable".to_string()),
        SequencedStep::Success("recovered".to_string()),
    ]));
    let cancel = CancellationToken::new();

    let response = state
        .namespace_generation()
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
    let (root, _procfs) =
        namespace_root_with_provider(TimeoutThenSucceedStreamProvider::new(Arc::clone(&attempts)));
    let shell = Shell::new(root.clone());
    let state = runtime_state_with_environment(NamespaceRuntimeEnvironment::new(
        root, "/agent/1", "default",
    ));
    let cancel = CancellationToken::new();

    let response = state
        .namespace_generation()
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

fn stateful_messages_snapshot(machine: &AgentMachine) -> Vec<String> {
    machine
        .messages()
        .iter()
        .map(crate::tape::Message::text_content)
        .collect()
}

include!("tests/compaction_recovery.rs");
include!("tests/compaction_thresholds.rs");
include!("tests/core_behaviors.rs");
include!("tests/submissions.rs");
