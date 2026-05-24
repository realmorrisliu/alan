use alan::daemon::{
    routes::{
        ReconnectSnapshotResponse, SessionReadResponse, SubmitRequest, compact_session,
        read_session, reconnect_snapshot, submit_operation,
    },
    state::{AppState, SessionEntry, SessionEventLog},
};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use alan_protocol::{
    CompactionAttemptSnapshot, CompactionPressureLevel, CompactionResult, ContentPart, Event,
    EventEnvelope, GovernanceConfig, GovernanceProfile, MemoryFlushAttemptSnapshot,
    MemoryFlushResult, MemoryFlushSkipReason, Op,
};
use alan_runtime::{
    Config, LlmClient, RolloutItem, RolloutRecorder, RuntimeEventEnvelope, Session, StreamingMode,
    WorkspaceRuntimeConfig, runtime::spawn_with_llm_client,
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
};
use std::{
    collections::VecDeque,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::{RwLock, broadcast, mpsc};

const MODEL: &str = "gpt-5.4";

#[derive(Clone)]
enum ScriptedStep {
    Success {
        expected_request: ExpectedRequestKind,
        response: GenerationResponse,
    },
    Error {
        expected_request: ExpectedRequestKind,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedRequestKind {
    Any,
    Turn,
    Compaction,
    MemoryFlush,
}

struct ScriptedProvider {
    steps: Arc<Mutex<VecDeque<ScriptedStep>>>,
}

impl ScriptedProvider {
    fn new(steps: Vec<ScriptedStep>) -> Self {
        Self {
            steps: Arc::new(Mutex::new(steps.into())),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ScriptedProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        let actual_request = classify_request(&request);
        match self.steps.lock().unwrap().pop_front() {
            Some(ScriptedStep::Success {
                expected_request,
                response,
            }) => {
                assert_request_kind(expected_request, actual_request, &request);
                Ok(response)
            }
            Some(ScriptedStep::Error {
                expected_request,
                message,
            }) => {
                assert_request_kind(expected_request, actual_request, &request);
                Err(anyhow::anyhow!(message))
            }
            None => Err(anyhow::anyhow!("scripted provider exhausted")),
        }
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Err(anyhow::anyhow!("scripted provider does not implement chat"))
    }

    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        Err(anyhow::anyhow!(
            "scripted provider does not implement generate_stream"
        ))
    }

    fn provider_name(&self) -> &'static str {
        "scripted_provider"
    }
}

fn success_step(text: impl Into<String>) -> ScriptedStep {
    success_step_for(ExpectedRequestKind::Any, text)
}

fn error_step(message: impl Into<String>) -> ScriptedStep {
    error_step_for(ExpectedRequestKind::Any, message)
}

fn success_step_for(
    expected_request: ExpectedRequestKind,
    text: impl Into<String>,
) -> ScriptedStep {
    ScriptedStep::Success {
        expected_request,
        response: GenerationResponse {
            content: text.into(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    }
}

fn error_step_for(
    expected_request: ExpectedRequestKind,
    message: impl Into<String>,
) -> ScriptedStep {
    ScriptedStep::Error {
        expected_request,
        message: message.into(),
    }
}

fn classify_request(request: &GenerationRequest) -> ExpectedRequestKind {
    match request.system_prompt.as_deref() {
        Some(prompt) if prompt.contains("SILENT PRE-COMPACTION MEMORY FLUSH") => {
            ExpectedRequestKind::MemoryFlush
        }
        Some(prompt) if prompt.contains("CONTEXT CHECKPOINT COMPACTION") => {
            ExpectedRequestKind::Compaction
        }
        _ => ExpectedRequestKind::Turn,
    }
}

fn assert_request_kind(
    expected: ExpectedRequestKind,
    actual: ExpectedRequestKind,
    request: &GenerationRequest,
) {
    if expected != ExpectedRequestKind::Any {
        assert_eq!(
            actual,
            expected,
            "unexpected scripted provider request kind: expected {:?}, got {:?}, system prompt: {:?}",
            expected,
            actual,
            request.system_prompt.as_deref()
        );
    }
}

fn memory_flush_json_response() -> String {
    serde_json::json!({
        "why": "retain durable blockers for follow-up work",
        "key_decisions": ["Compaction must preserve retry/degraded visibility"],
        "constraints": ["Do not lose identifiers or file paths"],
        "next_steps": ["Land the follow-up PR after verifying the harness"],
        "important_refs": ["crates/runtime/src/runtime/compaction.rs"]
    })
    .to_string()
}

fn empty_memory_flush_json_response() -> String {
    serde_json::json!({
        "why": "",
        "key_decisions": [],
        "constraints": [],
        "next_steps": [],
        "important_refs": []
    })
    .to_string()
}

fn absolute_memory_note_path(memory_dir: &FsPath, attempt: &MemoryFlushAttemptSnapshot) -> PathBuf {
    let output_path = attempt
        .output_path
        .as_deref()
        .expect("expected output_path for successful memory flush");
    let relative = std::path::Path::new(output_path)
        .strip_prefix(".alan/runtime/stable/memory/")
        .or_else(|_| std::path::Path::new(output_path).strip_prefix(".alan/memory/"))
        .unwrap_or_else(|_| std::path::Path::new(output_path));
    memory_dir.join(relative)
}

fn base_config() -> Config {
    Config::for_openai_responses("sk-test", None, Some(MODEL))
}

fn prepare_workspace(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let workspace_root = temp.path().join("workspace");
    let alan_dir = workspace_root.join(".alan");
    let sessions_dir = alan_runtime::workspace_runtime_sessions_dir_from_alan_dir(
        &alan_dir,
        alan_runtime::InstallChannel::Stable,
    );
    let memory_dir = alan_runtime::workspace_runtime_memory_dir_from_alan_dir(
        &alan_dir,
        alan_runtime::InstallChannel::Stable,
    );
    std::fs::create_dir_all(alan_dir.join("skills")).unwrap();
    std::fs::create_dir_all(&sessions_dir).unwrap();
    alan_runtime::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
    std::fs::create_dir_all(alan_dir.join("persona")).unwrap();
    (workspace_root, alan_dir, sessions_dir)
}

fn session_rollout_path_from_read(read: &SessionReadResponse) -> PathBuf {
    PathBuf::from(
        read.rollout_path
            .clone()
            .expect("read_session should resolve rollout path"),
    )
}

fn latest_attempt_from_rollout(items: &[RolloutItem]) -> CompactionAttemptSnapshot {
    items
        .iter()
        .rev()
        .find_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt.clone()),
            _ => None,
        })
        .expect("expected compaction attempt item in rollout")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompactedSnapshot {
    attempt_id: Option<String>,
    focus: Option<String>,
    retry_count: Option<u32>,
    result: Option<CompactionResult>,
}

fn latest_compacted_snapshot(items: &[RolloutItem]) -> Option<CompactedSnapshot> {
    items.iter().rev().find_map(|item| match item {
        RolloutItem::Compacted(compacted) => Some(CompactedSnapshot {
            attempt_id: compacted.attempt_id.clone(),
            focus: compacted.focus.clone(),
            retry_count: compacted.retry_count,
            result: compacted.result,
        }),
        _ => None,
    })
}

fn latest_memory_flush_from_rollout(items: &[RolloutItem]) -> Option<MemoryFlushAttemptSnapshot> {
    items.iter().rev().find_map(|item| match item {
        RolloutItem::MemoryFlushAttempt(attempt) => Some(attempt.clone()),
        _ => None,
    })
}

#[derive(Debug)]
struct AutoTurnCompactionOutcome {
    memory_flush_attempts: Vec<MemoryFlushAttemptSnapshot>,
    compaction_attempt: CompactionAttemptSnapshot,
    warnings: Vec<String>,
}

struct AutoCompactionSurfaces {
    compaction: CompactionSurfaces,
}

impl AutoCompactionSurfaces {
    fn read_memory_flush(&self) -> Option<&MemoryFlushAttemptSnapshot> {
        self.compaction.read.latest_memory_flush_attempt.as_ref()
    }

    fn reconnect_memory_flush(&self) -> Option<&MemoryFlushAttemptSnapshot> {
        self.compaction
            .reconnect
            .execution
            .latest_memory_flush_attempt
            .as_ref()
    }

    fn rollout_memory_flush(&self) -> Option<MemoryFlushAttemptSnapshot> {
        latest_memory_flush_from_rollout(&self.compaction.rollout_items)
    }

    fn recovered_memory_flush(&self) -> Option<&MemoryFlushAttemptSnapshot> {
        self.compaction.recovered.latest_memory_flush_attempt()
    }
}

async fn seed_rollout<F>(sessions_dir: &FsPath, session_id: &str, seed: F) -> PathBuf
where
    F: FnOnce(&mut Session),
{
    let mut session = Session::new_with_id_and_recorder_in_dir(session_id, MODEL, sessions_dir)
        .await
        .unwrap();
    seed(&mut session);
    session.flush().await;
    session.rollout_path().expect("rollout path").clone()
}

struct CompactionHarness {
    _temp: TempDir,
    state: AppState,
    controller: alan_runtime::RuntimeController,
    bridge_task: tokio::task::JoinHandle<()>,
    runtime_events_rx: broadcast::Receiver<RuntimeEventEnvelope>,
    session_id: String,
    memory_dir: PathBuf,
}

struct CompactionSurfaces {
    read: SessionReadResponse,
    reconnect: ReconnectSnapshotResponse,
    rollout_items: Vec<RolloutItem>,
    recovered: Session,
}

impl CompactionHarness {
    async fn new(
        session_id: &str,
        replay_capacity: usize,
        resume_rollout_path: Option<PathBuf>,
        steps: Vec<ScriptedStep>,
        configure_runtime: impl FnOnce(&mut WorkspaceRuntimeConfig),
    ) -> Self {
        let temp = TempDir::new().unwrap();
        let (workspace_root, alan_dir, sessions_dir) = prepare_workspace(&temp);
        let memory_dir = alan_runtime::workspace_runtime_memory_dir_from_alan_dir(
            &alan_dir,
            alan_runtime::InstallChannel::Stable,
        );
        let app_state =
            AppState::with_alan_home(base_config(), temp.path().join("daemon-home").join(".alan"))
                .unwrap();
        app_state.ensure_sessions_recovered().await.unwrap();

        let mut runtime_config = WorkspaceRuntimeConfig {
            session_id: Some(session_id.to_string()),
            workspace_id: alan::generate_workspace_id(&workspace_root),
            workspace_root_dir: Some(workspace_root.clone()),
            workspace_alan_dir: Some(alan_dir.clone()),
            agent_home_paths: Some(alan_runtime::AlanHomePaths::from_alan_home_dir(
                &temp.path().join("daemon-home").join(".alan"),
            )),
            resume_rollout_path,
            ..WorkspaceRuntimeConfig::default()
        };
        runtime_config.agent_config.core_config = base_config();
        runtime_config.agent_config.runtime_config.streaming_mode = StreamingMode::Off;
        runtime_config.agent_config.runtime_config.governance = GovernanceConfig {
            profile: GovernanceProfile::Autonomous,
            policy_path: None,
        };
        runtime_config
            .agent_config
            .runtime_config
            .compaction_keep_last = 1;
        runtime_config
            .agent_config
            .runtime_config
            .durability_required = false;
        runtime_config.agent_config.core_config.durability.required = false;
        configure_runtime(&mut runtime_config);

        let llm_client = LlmClient::new(ScriptedProvider::new(steps));
        let mut controller = spawn_with_llm_client(runtime_config, llm_client).unwrap();
        let startup = controller.wait_until_ready().await.unwrap();

        let runtime_events_rx = controller.handle.event_sender.subscribe();
        let event_log = Arc::new(RwLock::new(SessionEventLog::new(replay_capacity)));
        let (events_tx, _) = broadcast::channel::<EventEnvelope>(256);
        let mut bridge_rx = controller.handle.event_sender.subscribe();
        let event_log_clone = Arc::clone(&event_log);
        let events_tx_clone = events_tx.clone();
        let bridge_session_id = session_id.to_string();
        let bridge_task = tokio::spawn(async move {
            loop {
                match bridge_rx.recv().await {
                    Ok(runtime_event) => {
                        let envelope = {
                            let mut guard = event_log_clone.write().await;
                            guard.append_runtime_event(&bridge_session_id, runtime_event)
                        };
                        let _ = events_tx_clone.send(envelope);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });

        let entry = SessionEntry::new(
            workspace_root,
            alan_dir,
            None,
            None,
            None,
            MODEL.to_string(),
            Some(alan_protocol::ReasoningEffort::Medium),
            GovernanceConfig {
                profile: GovernanceProfile::Autonomous,
                policy_path: None,
            },
            StreamingMode::Off,
            alan_runtime::PartialStreamRecoveryMode::ContinueOnce,
            startup.durability,
            controller.handle.submission_tx.clone(),
            events_tx,
            event_log,
            None,
            latest_rollout_path(&sessions_dir),
        );

        app_state
            .sessions
            .write()
            .await
            .insert(session_id.to_string(), entry);

        Self {
            _temp: temp,
            state: app_state,
            controller,
            bridge_task,
            runtime_events_rx,
            session_id: session_id.to_string(),
            memory_dir,
        }
    }

    async fn request_manual_compaction(&self, focus: Option<&str>) -> String {
        let mut headers = HeaderMap::new();
        let body = if let Some(focus) = focus {
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            Bytes::from(
                serde_json::to_vec(&serde_json::json!({
                    "focus": focus,
                }))
                .unwrap(),
            )
        } else {
            Bytes::new()
        };

        let Json(response) = compact_session(
            State(self.state.clone()),
            Path(self.session_id.clone()),
            headers,
            body,
        )
        .await
        .unwrap();
        assert!(response.accepted);
        response.submission_id
    }

    async fn request_turn(&self, text: &str) -> String {
        let Json(response) = submit_operation(
            State(self.state.clone()),
            Path(self.session_id.clone()),
            None,
            Json(SubmitRequest {
                op: Op::Turn {
                    parts: vec![ContentPart::text(text)],
                    context: None,
                },
            }),
        )
        .await
        .unwrap();
        assert!(response.accepted);
        response.submission_id
    }

    async fn wait_for_compaction_attempt(
        &mut self,
        submission_id: &str,
    ) -> (CompactionAttemptSnapshot, Vec<String>) {
        let mut warnings = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

        loop {
            tokio::select! {
                result = self.runtime_events_rx.recv() => {
                    match result {
                        Ok(envelope) => match (envelope.submission_id.as_deref(), envelope.event) {
                            (_, Event::Warning { message }) => warnings.push(message),
                            (Some(event_submission_id), Event::CompactionObserved { attempt })
                                if event_submission_id == submission_id =>
                            {
                                assert_eq!(attempt.submission_id.as_deref(), Some(submission_id));
                                return (attempt, warnings);
                            }
                            _ => {}
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            panic!("runtime event stream closed before compaction result")
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    panic!("timed out waiting for compaction attempt")
                }
            }
        }
    }

    async fn wait_for_turn_completion_with_auto_compaction(
        &mut self,
        submission_id: &str,
    ) -> AutoTurnCompactionOutcome {
        let mut warnings = Vec::new();
        let mut memory_flush_attempts = Vec::new();
        let mut compaction_attempt = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

        loop {
            tokio::select! {
                result = self.runtime_events_rx.recv() => {
                    match result {
                        Ok(envelope) => {
                            if envelope.submission_id.as_deref() != Some(submission_id) {
                                continue;
                            }

                            match envelope.event {
                                Event::Warning { message } => warnings.push(message),
                                Event::MemoryFlushObserved { attempt } => {
                                    memory_flush_attempts.push(attempt);
                                }
                                Event::CompactionObserved { attempt } => {
                                    compaction_attempt = Some(attempt);
                                }
                                Event::TurnCompleted { .. } => {
                                    return AutoTurnCompactionOutcome {
                                        memory_flush_attempts,
                                        compaction_attempt: compaction_attempt
                                            .expect("expected compaction attempt before turn completion"),
                                        warnings,
                                    };
                                }
                                Event::Error { message, .. } => {
                                    panic!("turn failed before completion: {message}");
                                }
                                _ => {}
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            panic!("runtime event stream closed before turn completion")
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    panic!("timed out waiting for turn completion with auto compaction")
                }
            }
        }
    }

    async fn collect_surfaces(
        &self,
        expected_attempt: &CompactionAttemptSnapshot,
    ) -> CompactionSurfaces {
        let Json(read) = read_session(State(self.state.clone()), Path(self.session_id.clone()))
            .await
            .unwrap();
        let Json(reconnect) =
            reconnect_snapshot(State(self.state.clone()), Path(self.session_id.clone()))
                .await
                .unwrap();
        let rollout_path = session_rollout_path_from_read(&read);
        let rollout_items = wait_for_rollout_attempt(&rollout_path, expected_attempt).await;
        let recovered_dir = TempDir::new().unwrap();
        let recovered =
            Session::load_from_rollout_in_dir(&rollout_path, MODEL, recovered_dir.path())
                .await
                .unwrap();

        assert_eq!(
            read.latest_compaction_attempt.as_ref(),
            Some(expected_attempt)
        );
        assert_eq!(
            reconnect.execution.latest_compaction_attempt.as_ref(),
            Some(expected_attempt)
        );
        assert_eq!(
            latest_attempt_from_rollout(&rollout_items),
            *expected_attempt
        );
        assert_eq!(
            recovered.latest_compaction_attempt(),
            Some(expected_attempt)
        );

        CompactionSurfaces {
            read,
            reconnect,
            rollout_items,
            recovered,
        }
    }

    async fn collect_auto_compaction_surfaces(
        &self,
        expected_compaction: &CompactionAttemptSnapshot,
        expected_memory_flush: Option<&MemoryFlushAttemptSnapshot>,
    ) -> AutoCompactionSurfaces {
        let compaction = self.collect_surfaces(expected_compaction).await;
        let surfaces = AutoCompactionSurfaces { compaction };

        assert_eq!(surfaces.read_memory_flush(), expected_memory_flush);
        assert_eq!(surfaces.reconnect_memory_flush(), expected_memory_flush);
        assert_eq!(
            surfaces.rollout_memory_flush().as_ref(),
            expected_memory_flush
        );
        assert_eq!(surfaces.recovered_memory_flush(), expected_memory_flush);

        surfaces
    }
    async fn shutdown(self) {
        let _ = self.controller.shutdown().await;
        self.bridge_task.abort();
    }
}

async fn wait_for_rollout_attempt(
    rollout_path: &PathBuf,
    expected_attempt: &CompactionAttemptSnapshot,
) -> Vec<RolloutItem> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let items = RolloutRecorder::load_history(rollout_path).await.unwrap();
        if items.iter().rev().any(
            |item| matches!(item, RolloutItem::CompactionAttempt(attempt) if attempt == expected_attempt),
        ) {
            return items;
        }

        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for rollout compaction attempt persistence");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn latest_rollout_path(sessions_dir: &FsPath) -> Option<PathBuf> {
    let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(sessions_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let modified = entry
            .metadata()
            .unwrap()
            .modified()
            .unwrap_or(std::time::UNIX_EPOCH);
        match &latest {
            Some((best_modified, best_path))
                if modified < *best_modified
                    || (modified == *best_modified && path <= *best_path) => {}
            _ => latest = Some((modified, path)),
        }
    }
    latest.map(|(_, path)| path)
}

fn nonempty_history_seed(session: &mut Session) {
    session.add_user_message("Summarize the work completed so far.");
    session.add_assistant_message(
        "Reviewed the task list and updated the implementation plan.",
        None,
    );
    session.add_user_message("Keep the remaining blockers and file paths.");
    session.add_assistant_message(
        "Remaining blockers are in crates/runtime/src/session.rs.",
        None,
    );
}

fn seed_soft_threshold_history(session: &mut Session) {
    for i in 0..6 {
        session.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
        session.add_assistant_message(
            &format!("Need to preserve file paths and next steps for blocker {i}."),
            None,
        );
    }
}

fn failure_only_history_seed(session: &mut Session) {
    for _ in 0..64 {
        session.add_assistant_message("", None);
    }
    session.add_user_message("tail marker survives failure");
}

fn soft_threshold_context_window_tokens(pending_turn_text: &str) -> u32 {
    let mut session = Session::new();
    seed_soft_threshold_history(&mut session);
    session.add_user_message(pending_turn_text);
    ((session.tape.estimated_prompt_tokens() as f64) / 0.75).ceil() as u32
}

fn hard_threshold_context_window_tokens() -> u32 {
    let mut session = Session::new();
    seed_soft_threshold_history(&mut session);
    ((session.tape.estimated_prompt_tokens() as f64) / 0.95).ceil() as u32
}

fn configure_auto_pre_turn_soft_threshold(
    config: &mut WorkspaceRuntimeConfig,
    pending_turn_text: &str,
) {
    config
        .agent_config
        .runtime_config
        .compaction_trigger_messages = 100;
    config.agent_config.runtime_config.compaction_keep_last = 1;
    config.agent_config.runtime_config.context_window_tokens =
        soft_threshold_context_window_tokens(pending_turn_text);
    config.agent_config.runtime_config.compaction_trigger_ratio = 0.85;
    config
        .agent_config
        .runtime_config
        .compaction_soft_trigger_ratio = 0.70;
    config
        .agent_config
        .runtime_config
        .compaction_hard_trigger_ratio = 0.85;
}

fn configure_auto_pre_turn_hard_threshold(config: &mut WorkspaceRuntimeConfig) {
    config
        .agent_config
        .runtime_config
        .compaction_trigger_messages = 100;
    config.agent_config.runtime_config.compaction_keep_last = 1;
    config.agent_config.runtime_config.context_window_tokens =
        hard_threshold_context_window_tokens();
    config.agent_config.runtime_config.compaction_trigger_ratio = 0.80;
    config
        .agent_config
        .runtime_config
        .compaction_soft_trigger_ratio = 0.70;
    config
        .agent_config
        .runtime_config
        .compaction_hard_trigger_ratio = 0.80;
}

#[tokio::test]
async fn compaction_manual_success_surfaces_match() {
    let session_id = format!("sess-success-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, nonempty_history_seed).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![success_step(
            "Summary: blockers tracked in crates/runtime/src/session.rs.",
        )],
        |config| {
            config
                .agent_config
                .runtime_config
                .compaction_trigger_messages = 1;
        },
    )
    .await;

    let submission_id = harness.request_manual_compaction(None).await;
    let (attempt, warnings) = harness.wait_for_compaction_attempt(&submission_id).await;
    assert!(warnings.is_empty());
    assert_eq!(attempt.result, CompactionResult::Success);
    assert_eq!(attempt.retry_count, 0);

    let surfaces = harness.collect_surfaces(&attempt).await;
    let compacted =
        latest_compacted_snapshot(&surfaces.rollout_items).expect("expected compacted item");
    assert_eq!(compacted.result, Some(CompactionResult::Success));
    assert_eq!(
        compacted.attempt_id.as_deref(),
        Some(attempt.attempt_id.as_str())
    );
    assert!(
        surfaces
            .recovered
            .tape
            .summary()
            .is_some_and(|summary| summary.contains("blockers tracked"))
    );
    assert_eq!(
        surfaces
            .reconnect
            .execution
            .latest_compaction_attempt
            .as_ref()
            .and_then(|snapshot| snapshot.submission_id.as_deref()),
        Some(submission_id.as_str())
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_auto_pre_turn_soft_flush_success_surfaces_match() {
    let pending_turn_text = "Proceed with the compaction follow-up work.";
    let session_id = format!("sess-soft-flush-success-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, seed_soft_threshold_history).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![
            success_step_for(
                ExpectedRequestKind::MemoryFlush,
                memory_flush_json_response(),
            ),
            success_step_for(
                ExpectedRequestKind::Compaction,
                "Summary after soft-threshold memory flush.",
            ),
            success_step_for(
                ExpectedRequestKind::Turn,
                "Final response after automatic compaction.",
            ),
        ],
        |config| configure_auto_pre_turn_soft_threshold(config, pending_turn_text),
    )
    .await;

    let submission_id = harness.request_turn(pending_turn_text).await;
    let outcome = harness
        .wait_for_turn_completion_with_auto_compaction(&submission_id)
        .await;
    assert_eq!(outcome.memory_flush_attempts.len(), 1);
    assert!(outcome.warnings.is_empty());

    let flush_attempt = &outcome.memory_flush_attempts[0];
    let compaction_attempt = &outcome.compaction_attempt;
    assert_eq!(flush_attempt.result, MemoryFlushResult::Success);
    assert_eq!(flush_attempt.pressure_level, CompactionPressureLevel::Soft);
    assert_eq!(
        compaction_attempt.pressure_level,
        Some(CompactionPressureLevel::Soft)
    );
    assert_eq!(compaction_attempt.result, CompactionResult::Success);
    assert_eq!(
        compaction_attempt.memory_flush_attempt_id.as_deref(),
        Some(flush_attempt.attempt_id.as_str())
    );
    assert!(flush_attempt.output_path.as_deref().is_some_and(|path| {
        path.starts_with(".alan/runtime/stable/memory/daily/") && path.ends_with(".md")
    }));

    let surfaces = harness
        .collect_auto_compaction_surfaces(compaction_attempt, Some(flush_attempt))
        .await;
    let note_path = absolute_memory_note_path(&harness.memory_dir, flush_attempt);
    let note = tokio::fs::read_to_string(&note_path).await.unwrap();
    assert!(note.contains(flush_attempt.attempt_id.as_str()));
    assert!(note.contains("crates/runtime/src/runtime/compaction.rs"));
    assert_eq!(
        surfaces
            .compaction
            .read
            .latest_compaction_attempt
            .as_ref()
            .and_then(|attempt| attempt.memory_flush_attempt_id.as_deref()),
        Some(flush_attempt.attempt_id.as_str())
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_auto_pre_turn_soft_flush_skip_surfaces_match() {
    let pending_turn_text = "Proceed after noop memory flush.";
    let session_id = format!("sess-soft-flush-skip-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, seed_soft_threshold_history).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![
            success_step_for(
                ExpectedRequestKind::MemoryFlush,
                empty_memory_flush_json_response(),
            ),
            success_step_for(
                ExpectedRequestKind::Compaction,
                "Summary after noop memory flush.",
            ),
            success_step_for(
                ExpectedRequestKind::Turn,
                "Final response after noop memory flush.",
            ),
        ],
        |config| configure_auto_pre_turn_soft_threshold(config, pending_turn_text),
    )
    .await;

    let submission_id = harness.request_turn(pending_turn_text).await;
    let outcome = harness
        .wait_for_turn_completion_with_auto_compaction(&submission_id)
        .await;
    assert_eq!(outcome.memory_flush_attempts.len(), 1);
    assert!(outcome.warnings.is_empty());

    let flush_attempt = &outcome.memory_flush_attempts[0];
    let compaction_attempt = &outcome.compaction_attempt;
    assert_eq!(flush_attempt.result, MemoryFlushResult::Skipped);
    assert_eq!(
        flush_attempt.skip_reason,
        Some(MemoryFlushSkipReason::NoDurableContent)
    );
    assert!(flush_attempt.warning_message.is_none());
    assert!(flush_attempt.error_message.is_none());
    assert!(flush_attempt.output_path.is_none());
    assert_eq!(
        compaction_attempt.memory_flush_attempt_id.as_deref(),
        Some(flush_attempt.attempt_id.as_str())
    );

    harness
        .collect_auto_compaction_surfaces(compaction_attempt, Some(flush_attempt))
        .await;

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_auto_pre_turn_soft_flush_failure_surfaces_match() {
    let pending_turn_text = "Proceed after memory flush failure.";
    let session_id = format!("sess-soft-flush-failure-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, seed_soft_threshold_history).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![
            error_step_for(
                ExpectedRequestKind::MemoryFlush,
                "synthetic memory flush failure",
            ),
            success_step_for(
                ExpectedRequestKind::Compaction,
                "Summary after failed memory flush.",
            ),
            success_step_for(
                ExpectedRequestKind::Turn,
                "Final response after failed memory flush.",
            ),
        ],
        |config| configure_auto_pre_turn_soft_threshold(config, pending_turn_text),
    )
    .await;

    let submission_id = harness.request_turn(pending_turn_text).await;
    let outcome = harness
        .wait_for_turn_completion_with_auto_compaction(&submission_id)
        .await;
    assert_eq!(outcome.memory_flush_attempts.len(), 1);

    let flush_attempt = &outcome.memory_flush_attempts[0];
    let compaction_attempt = &outcome.compaction_attempt;
    assert_eq!(flush_attempt.result, MemoryFlushResult::Failure);
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("Silent memory flush failed before compaction"))
    );
    assert!(flush_attempt.output_path.is_none());
    assert_eq!(
        compaction_attempt.memory_flush_attempt_id.as_deref(),
        Some(flush_attempt.attempt_id.as_str())
    );

    harness
        .collect_auto_compaction_surfaces(compaction_attempt, Some(flush_attempt))
        .await;

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_auto_pre_turn_hard_skips_memory_flush_surfaces_match() {
    let pending_turn_text = "Proceed after hard-threshold compaction.";
    let session_id = format!("sess-hard-no-flush-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, seed_soft_threshold_history).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![
            success_step_for(
                ExpectedRequestKind::Compaction,
                "Summary at hard threshold without memory flush.",
            ),
            success_step_for(
                ExpectedRequestKind::Turn,
                "Final response after hard-threshold compaction.",
            ),
        ],
        configure_auto_pre_turn_hard_threshold,
    )
    .await;

    let submission_id = harness.request_turn(pending_turn_text).await;
    let outcome = harness
        .wait_for_turn_completion_with_auto_compaction(&submission_id)
        .await;
    assert!(outcome.memory_flush_attempts.is_empty());
    assert!(outcome.warnings.is_empty());
    assert_eq!(
        outcome.compaction_attempt.pressure_level,
        Some(CompactionPressureLevel::Hard)
    );
    assert!(outcome.compaction_attempt.memory_flush_attempt_id.is_none());

    harness
        .collect_auto_compaction_surfaces(&outcome.compaction_attempt, None)
        .await;

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_retry_surfaces_match_after_trimmed_retry() {
    let session_id = format!("sess-retry-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, nonempty_history_seed).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![
            error_step("synthetic retryable compaction failure"),
            success_step("Summary after retry."),
        ],
        |config| {
            config
                .agent_config
                .runtime_config
                .compaction_trigger_messages = 1;
        },
    )
    .await;

    let submission_id = harness.request_manual_compaction(None).await;
    let (attempt, warnings) = harness.wait_for_compaction_attempt(&submission_id).await;
    assert!(warnings.is_empty());
    assert_eq!(attempt.result, CompactionResult::Retry);
    assert_eq!(attempt.retry_count, 1);

    let surfaces = harness.collect_surfaces(&attempt).await;
    let compacted =
        latest_compacted_snapshot(&surfaces.rollout_items).expect("expected compacted item");
    assert_eq!(compacted.result, Some(CompactionResult::Retry));
    assert_eq!(compacted.retry_count, Some(1));
    assert_eq!(
        surfaces
            .read
            .latest_compaction_attempt
            .as_ref()
            .map(|snapshot| snapshot.retry_count),
        Some(1)
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_degraded_surfaces_match() {
    let session_id = format!("sess-degraded-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, nonempty_history_seed).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![error_step("synthetic compaction failure")],
        |config| {
            config
                .agent_config
                .runtime_config
                .compaction_trigger_messages = 1;
        },
    )
    .await;

    let submission_id = harness
        .request_manual_compaction(Some("preserve blockers and file paths"))
        .await;
    let (attempt, warnings) = harness.wait_for_compaction_attempt(&submission_id).await;
    assert_eq!(attempt.result, CompactionResult::Degraded);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("deterministic fallback summary"))
    );

    let surfaces = harness.collect_surfaces(&attempt).await;
    let compacted =
        latest_compacted_snapshot(&surfaces.rollout_items).expect("expected compacted item");
    assert_eq!(compacted.result, Some(CompactionResult::Degraded));
    assert_eq!(
        compacted.focus.as_deref(),
        Some("preserve blockers and file paths")
    );
    assert!(
        surfaces
            .recovered
            .tape
            .summary()
            .is_some_and(|summary| summary.contains("Deterministic fallback summary"))
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_failure_preserves_tape_and_recovery_state() {
    let session_id = format!("sess-failure-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, failure_only_history_seed).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![error_step("synthetic compaction failure")],
        |_config| {},
    )
    .await;

    let submission_id = harness.request_manual_compaction(None).await;
    let (attempt, warnings) = harness.wait_for_compaction_attempt(&submission_id).await;
    assert_eq!(attempt.result, CompactionResult::Failure);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Preserving existing context"))
    );

    let surfaces = harness.collect_surfaces(&attempt).await;
    assert!(latest_compacted_snapshot(&surfaces.rollout_items).is_none());
    assert!(surfaces.recovered.tape.summary().is_none());
    assert_eq!(surfaces.recovered.tape.len(), 65);
    assert_eq!(
        surfaces
            .recovered
            .tape
            .messages()
            .last()
            .expect("tail message")
            .text_content(),
        "tail marker survives failure"
    );
    assert!(
        surfaces
            .read
            .messages
            .iter()
            .any(|message| message.content == "tail marker survives failure")
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn compaction_repeated_failure_escalation_surfaces_match() {
    let session_id = format!("sess-failure-streak-{}", uuid::Uuid::new_v4());
    let temp = TempDir::new().unwrap();
    let (_, _, sessions_dir) = prepare_workspace(&temp);
    let seed_rollout = seed_rollout(&sessions_dir, &session_id, failure_only_history_seed).await;

    let mut harness = CompactionHarness::new(
        &session_id,
        32,
        Some(seed_rollout),
        vec![
            error_step("synthetic compaction failure"),
            error_step("synthetic compaction failure"),
        ],
        |_config| {},
    )
    .await;

    let first_submission = harness.request_manual_compaction(None).await;
    let (first_attempt, first_warnings) =
        harness.wait_for_compaction_attempt(&first_submission).await;
    assert_eq!(first_attempt.result, CompactionResult::Failure);
    assert_eq!(first_attempt.failure_streak, Some(1));
    assert!(
        !first_warnings
            .iter()
            .any(|warning| warning.contains("consider starting a new session"))
    );

    let second_submission = harness.request_manual_compaction(None).await;
    let (second_attempt, second_warnings) = harness
        .wait_for_compaction_attempt(&second_submission)
        .await;
    assert_eq!(second_attempt.result, CompactionResult::Failure);
    assert_eq!(second_attempt.failure_streak, Some(2));
    assert!(
        second_warnings
            .iter()
            .any(|warning| warning.contains("consider starting a new session"))
    );

    let surfaces = harness.collect_surfaces(&second_attempt).await;
    let failure_attempts: Vec<CompactionAttemptSnapshot> = surfaces
        .rollout_items
        .iter()
        .filter_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(failure_attempts.len(), 2);
    assert_eq!(failure_attempts[0].failure_streak, Some(1));
    assert_eq!(failure_attempts[1].failure_streak, Some(2));
    assert_eq!(
        surfaces
            .reconnect
            .execution
            .latest_compaction_attempt
            .as_ref()
            .and_then(|attempt| attempt.failure_streak),
        Some(2)
    );

    harness.shutdown().await;
}
