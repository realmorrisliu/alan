use super::*;
use crate::runtime::model_binding::{
    CallableIdentity, CapturedCallable, ConnectionAuthority, GenerationCostBound,
};
use crate::tools::{Tool, ToolContext, ToolRegistry, ToolResult};
use alan_agent_protocol::{OwnerCandidate, OwnerSourceRange, OwnerWorkControl, OwnerWorkRequest};
use alan_llm::{ChoiceEvaluationRequest, ChoiceEvaluationResponse, EvaluationSelection};
use std::sync::atomic::{AtomicUsize, Ordering};

struct SourceTool {
    reads: Arc<AtomicUsize>,
    revoke_after: usize,
}
impl Tool for SourceTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read the test's one source range"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type":"object"})
    }
    fn capability(&self, _: &serde_json::Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
    fn execute(&self, arguments: serde_json::Value, _: &ToolContext) -> ToolResult {
        let read = self.reads.fetch_add(1, Ordering::SeqCst);
        let revoked = read >= self.revoke_after;
        Box::pin(async move {
            if revoked {
                anyhow::bail!("namespace authority revoked");
            }
            let content = if arguments["path"] == "/mnt/source/hostfs.rs" {
                "pub struct HostDirFs {}"
            } else {
                "pub struct Kernel {}"
            };
            Ok(json!({"content":content,"start_line":1,"end_line":1,"type":"text"}))
        })
    }
}

struct ChoiceProvider {
    calls: Arc<AtomicUsize>,
    selection: Option<String>,
    recorder: RolloutRecorder,
    cancel: Option<CancellationToken>,
    hold_after_cancel: bool,
}
#[async_trait::async_trait]
impl LlmProvider for ChoiceProvider {
    fn provider_name(&self) -> &'static str {
        "test"
    }
    fn supports_generation(&self) -> bool {
        false
    }
    fn supports_choice_evaluation(&self) -> bool {
        true
    }
    async fn evaluate_choice(
        &mut self,
        request: ChoiceEvaluationRequest,
    ) -> anyhow::Result<ChoiceEvaluationResponse> {
        let history = RolloutRecorder::load_history(self.recorder.path()).await?;
        assert!(history.iter().any(|i| matches!(i, RolloutItem::Event(e) if e.event_type == "machine_owner_work_v1" && e.payload["evaluator_calls"] == 1)));
        assert!(history.iter().any(|i| matches!(i, RolloutItem::Event(e) if e.event_type == "machine_evaluation_v1" && e.payload["outcome"]["state"] == "started" && e.payload["identity"]["operation_id"].is_string())));
        assert_eq!(request.candidates.len(), 2);
        assert!(
            request
                .candidates
                .iter()
                .all(|c| c.description.len() <= 4096)
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
            if self.hold_after_cancel {
                std::future::pending::<()>().await;
            }
        }
        Ok(ChoiceEvaluationResponse {
            selection: self
                .selection
                .clone()
                .map(EvaluationSelection::Selected)
                .unwrap_or(EvaluationSelection::NoMatch),
            usage: None,
        })
    }
}

struct Authority {
    captured: CapturedCallable,
    quote: Option<u64>,
    close_before_wait: Option<RolloutRecorder>,
}
#[async_trait::async_trait]
impl ConnectionAuthority for Authority {
    async fn capture(&self, _: Option<&str>) -> anyhow::Result<CapturedCallable> {
        anyhow::bail!("test has no new generation selection")
    }
    async fn restore(&self, _: &CallableIdentity) -> anyhow::Result<CapturedCallable> {
        anyhow::bail!("test has no binding restoration")
    }
    async fn catalog(&self) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("test has no catalog")
    }
    async fn capture_evaluation(&self, profile: &str) -> anyhow::Result<CapturedCallable> {
        anyhow::ensure!(
            profile == self.captured.identity.profile,
            "profile outside authority"
        );
        Ok(self.captured.clone())
    }
    async fn quote_generation(
        &self,
        _: &CallableIdentity,
        request: &GenerationRequest,
    ) -> anyhow::Result<Option<GenerationCostBound>> {
        assert_eq!(request.max_tokens, Some(64));
        assert!(request.tools.is_empty());
        if let Some(recorder) = &self.close_before_wait {
            recorder.close().await?;
        }
        Ok(self.quote.map(|cost_microusd| GenerationCostBound {
            cost_microusd,
            provenance: "test fixed total bill for the bounded request".into(),
        }))
    }
}

fn control(question: &str) -> Submission {
    OwnerWorkControl {
        id: uuid::Uuid::new_v4(),
        request: OwnerWorkRequest {
            version: 1,
            question: question.into(),
            evaluator_profile: "evaluator".into(),
            candidates: [
                ("hostfs", "/mnt/source/hostfs.rs"),
                ("kernel", "/mnt/source/kernel.rs"),
            ]
            .into_iter()
            .map(|(id, path)| OwnerCandidate {
                id: id.into(),
                sources: vec![OwnerSourceRange {
                    path: path.into(),
                    start_line: 1,
                    end_line: 1,
                }],
            })
            .collect(),
        },
    }
    .into_submission()
    .unwrap()
}

async fn fixture(
    selection: Option<&str>,
    quote: Option<u64>,
    revoke_after: usize,
) -> (
    TempDir,
    RuntimeLoopState,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    alan_llm::MockLlmProvider,
) {
    let dir = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock-model", dir.path())
        .await
        .unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut tools = ToolRegistry::new();
    tools.register(SourceTool {
        reads: reads.clone(),
        revoke_after,
    });
    let mut response = GenerationResponse {
        content: "{\"owner\":\"hostfs\"}".into(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: vec![],
        tool_calls: vec![],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: vec![],
    };
    response.finish_reason = Some("stop".into());
    let provider = alan_llm::MockLlmProvider::new().with_responses(vec![response]);
    let probe = provider.clone();
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection_profile(
        "evaluator",
        alan_llmfs::ConnectionProfile::new("test", "pinned", "ref"),
        Box::new(ChoiceProvider {
            calls: calls.clone(),
            selection: selection.map(str::to_owned),
            recorder: machine.input_recorder().unwrap(),
            cancel: None,
            hold_after_cancel: false,
        }),
    );
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let captured = CapturedCallable {
        identity: CallableIdentity {
            profile: "evaluator".into(),
            provider: "test".into(),
            model: "pinned".into(),
            credential_ref: Some("ref".into()),
            revision: "fixture-v1".into(),
        },
        root,
        connection: "evaluator".into(),
        config: Config::default(),
    };
    let mut state =
        tool_batch::create_test_state_with_machine_tools_and_provider(machine, tools, provider)
            .await;
    let generation_callable = CapturedCallable {
        identity: CallableIdentity {
            profile: "default".into(),
            provider: "mock".into(),
            model: "mock-model".into(),
            credential_ref: None,
            revision: "fixture-v1".into(),
        },
        root: state.environment.root_transport(),
        connection: "default".into(),
        config: Config::default(),
    };
    *state.environment.active_binding.write().unwrap() = Some((
        crate::runtime::model_binding::InputBinding {
            callable_binding: generation_callable.identity.clone(),
            request_controls: crate::ResolvedRequestControls::default(),
        },
        generation_callable,
    ));
    state.environment = state
        .environment
        .with_connection_authority(Arc::new(Authority {
            captured,
            quote,
            close_before_wait: None,
        }));
    (dir, state, reads, calls, probe)
}

#[tokio::test]
async fn literal_and_typed_completion_do_not_generate_an_answer_or_exit_the_process() {
    for (question, expected_calls) in [
        ("Which crate defines HostDirFs?", 0),
        ("Which crate revokes old file handles?", 1),
    ] {
        let (_dir, mut state, reads, calls, generation) =
            fixture(Some("hostfs"), None, usize::MAX).await;
        let submission = control(question);
        let mut emit = |_| async {};
        assert!(
            super::super::owner_work::handle(
                &mut state,
                &submission,
                &mut emit,
                &CancellationToken::new()
            )
            .await
            .unwrap()
        );
        let work = state.machine.owner_work.as_ref().unwrap();
        assert!(
            matches!(&work.outcome, crate::agent_machine::owner_work::Outcome::Completed { owner } if owner == "hostfs")
        );
        assert_eq!(work.work_id, submission.id);
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        assert_eq!(generation.recorded_requests().len(), 0);
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        assert!(!state.machine.has_pending_interaction());
        assert!(!state.machine.messages().iter().any(|m| matches!(m, crate::tape::Message::Assistant { parts, .. } if parts.iter().any(|p| p.as_text().is_some_and(|s| !s.is_empty())))));
        assert_eq!(
            state
                .machine
                .owner_work
                .as_ref()
                .unwrap()
                .projection()
                .unwrap()["citations"][0]["sha256"],
            crate::agent_machine::owner_work::digest(b"pub struct HostDirFs {}")
        );
        assert!(
            !super::super::owner_work::handle(
                &mut state,
                &Submission::new(Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text(
                        "{\"owner_work_v1\":{}}"
                    )],
                    mode: InputMode::FollowUp
                }),
                &mut emit,
                &CancellationToken::new()
            )
            .await
            .unwrap()
        );
    }
}

#[tokio::test]
async fn no_match_unknown_or_over_budget_cost_waits_and_form_response_never_repeats_models() {
    for quote in [None, Some(1001)] {
        let (_dir, mut state, _reads, calls, generation) = fixture(None, quote, usize::MAX).await;
        let mut emit = |_| async {};
        super::super::owner_work::handle(
            &mut state,
            &control("Who owns old fids?"),
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let original = state.machine.owner_work.clone().unwrap();
        let crate::agent_machine::owner_work::Outcome::Waiting { request_id, .. } =
            &original.outcome
        else {
            panic!("expected owned wait")
        };
        assert!(state.machine.has_pending_interaction());
        let response = Submission::new(Op::Resume {
            request_id: request_id.clone(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                json!({"answers":[{"question_id":"owner","value":"hostfs"}]}),
            )],
        });
        super::super::owner_work::handle(
            &mut state,
            &response,
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            state.machine.owner_work.as_ref().unwrap().work_id,
            original.work_id
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(generation.recorded_requests().len(), 0);
        assert!(!state.machine.has_pending_interaction());
    }
}

#[tokio::test]
async fn priced_fallback_is_single_and_revoked_evidence_cannot_complete() {
    let (_dir, mut state, _, calls, generation) = fixture(None, Some(1000), usize::MAX).await;
    let mut emit = |_| async {};
    super::super::owner_work::handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(generation.recorded_requests().len(), 1);
    let snapshot = state.machine.owner_work.as_ref().unwrap();
    assert_eq!(snapshot.generation_calls, 1);
    assert_eq!(
        snapshot.fallback.as_ref().unwrap().quote.cost_microusd,
        1000
    );
    let (_dir, mut state, _, calls, generation) = fixture(Some("hostfs"), None, 2).await;
    assert!(
        super::super::owner_work::handle(
            &mut state,
            &control("Who owns old fids?"),
            &mut emit,
            &CancellationToken::new()
        )
        .await
        .is_err()
    );
    assert!(matches!(
        state.machine.owner_work.as_ref().unwrap().outcome,
        crate::agent_machine::owner_work::Outcome::Failed { .. }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(generation.recorded_requests().len(), 0);
}

#[tokio::test]
async fn explicit_recovery_retains_owned_wait_and_spent_attempt_without_re_evaluation() {
    let (dir, mut state, reads, calls, generation) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    super::super::owner_work::handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let original = state.machine.owner_work.clone().unwrap();
    let crate::agent_machine::owner_work::Outcome::Waiting { request_id, .. } = &original.outcome
    else {
        panic!("wait")
    };
    let recorder = state.machine.input_recorder().unwrap();
    recorder.flush().await.unwrap();
    let recovered = AgentMachine::load_from_rollout_in_dir(
        &recorder.path().to_path_buf(),
        "/proc/9",
        "mock-model",
        dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(recovered.owner_work.as_ref(), Some(&original));
    assert!(recovered.pending_yield(request_id).is_some());
    assert_eq!(recovered.turn_activity(), TurnActivityState::Paused);
    let mut tools = ToolRegistry::new();
    tools.register(SourceTool {
        reads: reads.clone(),
        revoke_after: usize::MAX,
    });
    let mut recovered_state = tool_batch::create_test_state_with_machine_tools_and_provider(
        recovered,
        tools,
        generation.clone(),
    )
    .await;
    let resume = Submission::new(Op::Resume {
        request_id: request_id.clone(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            json!({"answers":[{"question_id":"owner","value":"hostfs"}]}),
        )],
    });
    super::super::owner_work::handle(
        &mut recovered_state,
        &resume,
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let result = recovered_state.machine.owner_work.as_ref().unwrap();
    assert_eq!(result.work_id, original.work_id);
    assert_eq!(result.source_rollout_id, original.source_rollout_id);
    assert_eq!(result.evaluator_calls, 1);
    assert_eq!(result.generation_calls, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(generation.recorded_requests().len(), 0);
    let reads_before = reads.load(Ordering::SeqCst);
    let repeated = alan_agent_protocol::OwnerWorkControl {
        id: uuid::Uuid::parse_str(&original.work_id).unwrap(),
        request: original.request.clone(),
    }
    .into_submission()
    .unwrap();
    assert!(
        super::super::owner_work::handle(
            &mut recovered_state,
            &repeated,
            &mut emit,
            &CancellationToken::new()
        )
        .await
        .is_err()
    );
    assert_eq!(reads.load(Ordering::SeqCst), reads_before);
}

#[tokio::test]
async fn cancelled_and_unacknowledged_work_do_not_dispatch_sources_or_models() {
    let (_dir, mut state, reads, calls, generation) =
        fixture(Some("hostfs"), Some(1000), usize::MAX).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut emit = |_| async {};
    super::super::owner_work::handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &cancel,
    )
    .await
    .unwrap();
    assert!(matches!(
        state.machine.owner_work.as_ref().unwrap().outcome,
        crate::agent_machine::owner_work::Outcome::Cancelled
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(generation.recorded_requests().len(), 0);
    state
        .machine
        .input_recorder()
        .unwrap()
        .close()
        .await
        .unwrap();
    assert!(
        super::super::owner_work::handle(
            &mut state,
            &control("Another question"),
            &mut emit,
            &CancellationToken::new()
        )
        .await
        .is_err()
    );
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelling_a_wait_is_durable_and_cannot_reopen_a_generation_response_path() {
    let (dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    super::super::owner_work::handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let work = state.machine.owner_work.clone().unwrap();
    let request_id = work.owned_request.clone().unwrap();
    let agent_files = state.agent_files();
    let host_mounts = state.environment.host_mount_requests();
    crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
        &mut state.machine,
        &agent_files,
        &host_mounts,
    )
    .await
    .unwrap();
    assert!(matches!(
        state.machine.owner_work.as_ref().unwrap().outcome,
        crate::agent_machine::owner_work::Outcome::Cancelled
    ));
    assert!(!state.machine.has_pending_interaction());
    let response = Submission::new(Op::Resume {
        request_id,
        content: vec![alan_agent_protocol::ContentPart::structured(
            json!({"owner":"hostfs"}),
        )],
    });
    assert!(
        super::super::owner_work::handle(
            &mut state,
            &response,
            &mut emit,
            &CancellationToken::new()
        )
        .await
        .is_err()
    );
    let recorder = state.machine.input_recorder().unwrap();
    recorder.flush().await.unwrap();
    let recovered = AgentMachine::load_from_rollout_in_dir(
        &recorder.path().to_path_buf(),
        "/proc/9",
        "mock-model",
        dir.path(),
    )
    .await
    .unwrap();
    assert!(!recovered.has_pending_interaction());
    assert_eq!(recovered.owner_work, state.machine.owner_work);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(generation.recorded_requests().len(), 0);
}

#[tokio::test]
async fn invalid_typed_modes_and_ungranted_evaluator_never_dispatch_models() {
    let (_dir, mut state, reads, calls, generation) =
        fixture(Some("hostfs"), None, usize::MAX).await;
    let mut emit = |_| async {};
    for mode in [InputMode::Steer, InputMode::NextTurn] {
        let mut input = control("Who owns fids?");
        if let Op::Input {
            mode: input_mode, ..
        } = &mut input.op
        {
            *input_mode = mode;
        }
        assert!(
            super::super::owner_work::handle(
                &mut state,
                &input,
                &mut emit,
                &CancellationToken::new()
            )
            .await
            .is_err()
        );
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    let mut input = control("Who owns fids?");
    if let Op::Input { parts, .. } = &mut input.op
        && let alan_agent_protocol::ContentPart::Structured { data } = &mut parts[0]
    {
        data["owner_work_v1"]["evaluator_profile"] = json!("ungranted");
    }
    assert!(
        super::super::owner_work::handle(&mut state, &input, &mut emit, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(matches!(
        state.machine.owner_work.as_ref().unwrap().outcome,
        crate::agent_machine::owner_work::Outcome::Failed { .. }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(generation.recorded_requests().len(), 0);
}

struct BlockingGeneration;
#[async_trait::async_trait]
impl LlmProvider for BlockingGeneration {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        std::future::pending().await
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("unused")
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        std::future::pending().await
    }
    fn provider_name(&self) -> &'static str {
        "blocked"
    }
}

#[tokio::test]
async fn fallback_deadline_aborts_the_existing_generation_without_allocating_a_retry() {
    let (_dir, mut state, _, _, _) = fixture(None, Some(1000), usize::MAX).await;
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(BlockingGeneration));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    state.environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
    let error = super::super::owner_work::generate_fallback(
        &state,
        &GenerationRequest::new().with_user_message("owner"),
        tokio::time::Instant::now() + std::time::Duration::from_millis(20),
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("never retry"));
    let shell = alan_shell::Shell::new(root);
    let status: serde_json::Value = serde_json::from_slice(
        &shell
            .cat("/mnt/llm/connections/default/g0/status")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(status["status"], "aborted");
    assert!(
        shell
            .cat("/mnt/llm/connections/default/g1/status")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn work_evaluation_and_owned_response_do_not_alias_input_shadow_identity() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let captured = state
        .environment
        .model_bindings
        .lock()
        .await
        .authority
        .as_ref()
        .unwrap()
        .capture_evaluation("evaluator")
        .await
        .unwrap();
    state.environment = state
        .environment
        .with_shadow_evaluation(
            captured.root,
            captured.identity,
            crate::runtime::EvaluationSurface::Interactive,
            30_000,
        )
        .unwrap();
    let input = control("Who owns old fids?");
    let cancel = CancellationToken::new();
    state.observe_input_shadow(&input, &cancel).await.unwrap();
    assert!(state.machine.evaluation_observation.is_none());
    let mut emit = |_| async {};
    super::super::owner_work::handle(&mut state, &input, &mut emit, &cancel)
        .await
        .unwrap();
    let crate::agent_machine::owner_work::Outcome::Waiting { request_id, .. } =
        &state.machine.owner_work.as_ref().unwrap().outcome
    else {
        panic!("wait")
    };
    let response = Submission::new(Op::Resume {
        request_id: request_id.clone(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            json!({"owner":"hostfs"}),
        )],
    });
    state
        .observe_input_shadow(&response, &cancel)
        .await
        .unwrap();
    assert_eq!(
        state.machine.evaluation_observation.as_ref().unwrap()["outcome"]["reason"],
        "request_response"
    );
    super::super::owner_work::handle(&mut state, &response, &mut emit, &cancel)
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn generic_cancellation_correlates_input_and_preserves_uncertain_model_settlement() {
    for hold_after_cancel in [true, false] {
        let (dir, mut state, _, calls, generation) =
            fixture(Some("hostfs"), None, usize::MAX).await;
        let cancel = CancellationToken::new();
        let input = control("Who owns old fids?");
        let mut captured = state
            .environment
            .model_bindings
            .lock()
            .await
            .authority
            .as_ref()
            .unwrap()
            .capture_evaluation("evaluator")
            .await
            .unwrap();
        let llmfs = alan_llmfs::LlmFs::new();
        llmfs.register_connection_profile(
            "evaluator",
            alan_llmfs::ConnectionProfile::new("test", "pinned", "ref"),
            Box::new(ChoiceProvider {
                calls: calls.clone(),
                selection: Some("hostfs".into()),
                recorder: state.machine.input_recorder().unwrap(),
                cancel: Some(cancel.clone()),
                hold_after_cancel,
            }),
        );
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/mnt/llm",
            alan_ap::InProcessTransport::new(Arc::new(llmfs)),
            alan_kernel::Access::ReadWrite,
        );
        captured.root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        state.environment = state
            .environment
            .with_connection_authority(Arc::new(Authority {
                captured,
                quote: None,
                close_before_wait: None,
            }));
        let (binding, callable) = state
            .environment
            .active_binding
            .read()
            .unwrap()
            .clone()
            .unwrap();
        state
            .environment
            .model_bindings
            .lock()
            .await
            .captured
            .insert(input.id.clone(), callable);
        state
            .machine
            .input_queue()
            .lock()
            .unwrap()
            .bindings
            .insert(input.id.clone(), binding);
        state.machine.admit_input(&input).await.unwrap();
        let broker = TurnInputBroker::from_queue(state.machine.input_queue());
        let id = input.id.clone();
        let outcome = advance_accepted_submission(&mut state, input, &broker, &cancel).await;
        assert_eq!(outcome.result.is_ok(), hold_after_cancel);
        let work = state.machine.owner_work.as_ref().unwrap();
        assert_eq!(work.evaluator_calls, 1);
        if hold_after_cancel {
            assert!(matches!(
                work.outcome,
                crate::agent_machine::owner_work::Outcome::Cancelled
            ));
        } else {
            assert!(matches!(
                work.outcome,
                crate::agent_machine::owner_work::Outcome::Started
            ));
            assert!(
                outcome
                    .result
                    .unwrap_err()
                    .downcast_ref::<NamespaceEvaluationUncertainty>()
                    .is_some()
            );
            let recorder = state.machine.input_recorder().unwrap();
            recorder.flush().await.unwrap();
            let recovered = AgentMachine::load_from_rollout_in_dir(
                &recorder.path().to_path_buf(),
                "/proc/9",
                "mock-model",
                dir.path(),
            )
            .await
            .unwrap();
            assert!(matches!(
                recovered.owner_work.as_ref().unwrap().outcome,
                crate::agent_machine::owner_work::Outcome::Interrupted
            ));
        }
        let shell = alan_shell::Shell::new(state.environment.root_transport());
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
            alan_agent_protocol::UiEvent::InputCompleted {submission_ids,status:alan_agent_protocol::UiInputStatus::Cancelled,..} if submission_ids.contains(&id))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(generation.recorded_requests().is_empty());
    }
}

#[tokio::test]
async fn failed_wait_persistence_cancels_the_exposed_request_without_yield_or_retry() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let captured = state
        .environment
        .model_bindings
        .lock()
        .await
        .authority
        .as_ref()
        .unwrap()
        .capture_evaluation("evaluator")
        .await
        .unwrap();
    state.environment = state
        .environment
        .with_connection_authority(Arc::new(Authority {
            captured,
            quote: None,
            close_before_wait: state.machine.input_recorder(),
        }));
    let mut events = vec![];
    let mut emit = |event| {
        events.push(event);
        async {}
    };
    assert!(
        super::super::owner_work::handle(
            &mut state,
            &control("Who owns old fids?"),
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .is_err()
    );
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    assert_eq!(
        shell.cat("/agent/1/requests/r0/status").await.unwrap(),
        b"cancelled"
    );
    assert!(!state.machine.has_pending_interaction());
    assert!(!events.iter().any(|e| matches!(e, Event::Yield { .. })));
    let work = state.machine.owner_work.as_ref().unwrap();
    assert!(matches!(
        work.outcome,
        crate::agent_machine::owner_work::Outcome::Interrupted
    ));
    assert!(work.owned_request.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(generation.recorded_requests().len(), 0);
}
