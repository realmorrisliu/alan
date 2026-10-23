use super::*;
use crate::runtime::{
    model_binding::{CallableIdentity, CapturedCallable},
    tool_authorization::{ToolAuthorizationOutcome, ToolAuthorizationRequest, authorize_tool_call},
};
use std::num::NonZeroU64;

fn candidate(provider: alan_llm::MockLlmProvider) -> CapturedCallable {
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("reviewer", Box::new(provider));
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    CapturedCallable {
        identity: CallableIdentity {
            profile: "reviewer".into(),
            provider: "mock".into(),
            model: "candidate".into(),
            credential_ref: None,
            revision: "frozen-candidate".into(),
        },
        root: alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace))),
        connection: "reviewer".into(),
        config: Config::default(),
    }
}

fn response(decision: &str) -> GenerationResponse {
    let mut response =
        generation_response(json!({"decision":decision,"rationale":"fixture"}).to_string());
    response.usage = Some(alan_llm::TokenUsage {
        prompt_tokens: 10,
        cached_prompt_tokens: None,
        completion_tokens: 5,
        total_tokens: 15,
        reasoning_tokens: None,
    });
    response
}

async fn authorize(state: &mut RuntimeLoopState) -> Result<ToolAuthorizationOutcome> {
    let call = NormalizedToolCall {
        id: "shadowed-call".into(),
        name: "do_thing".into(),
        arguments: json!({}),
    };
    authorize_tool_call(
        tool_authorization_runtime(state),
        ToolAuthorizationRequest {
            explicit_command: false,
            tool_call: &call,
            tool_arguments: &call.arguments,
            tool_capability: alan_agent_protocol::ToolCapability::Unknown,
            current_tool_cwd: None,
            allow_approved_tool_escalation_execution: false,
            cancel: &CancellationToken::new(),
        },
        &mut |_| async {},
    )
    .await
}

#[tokio::test]
async fn independent_shadow_records_disagreement_without_replacing_the_main_decision() {
    for (main, shadow, expected) in [
        ("allow", "deny", "authorized"),
        ("deny", "allow", "completed"),
        ("allow", "malformed", "authorized"),
        ("malformed", "allow", "paused"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let main_provider = alan_llm::MockLlmProvider::new().with_response(response(main));
        let shadow_provider = alan_llm::MockLlmProvider::new().with_response(response(shadow));
        let mut state = runtime_state_with_provider(main_provider.clone());
        state.environment = state
            .environment
            .with_guardian_shadow(
                candidate(shadow_provider.clone()),
                Some("CANDIDATE-ONLY policy".into()),
            )
            .unwrap();
        state.machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "main", dir.path())
            .await
            .unwrap();
        state.machine.accept_submission("shadow-task");
        state.core_config.memory.enabled = false;
        crate::runtime::task_budget::begin(&mut state)
            .await
            .unwrap();
        let outcome = authorize(&mut state).await.unwrap();
        let actual = match outcome {
            ToolAuthorizationOutcome::Authorized { .. } => "authorized",
            ToolAuthorizationOutcome::Completed => "completed",
            ToolAuthorizationOutcome::PauseTurn => "paused",
        };
        assert_eq!(actual, expected);
        let main_requests = main_provider.recorded_requests();
        let shadow_requests = shadow_provider.recorded_requests();
        assert_eq!((main_requests.len(), shadow_requests.len()), (1, 1));
        assert!(main_requests[0].tools.is_empty() && shadow_requests[0].tools.is_empty());
        assert!(
            !main_requests[0]
                .system_prompt
                .as_ref()
                .unwrap()
                .contains("CANDIDATE-ONLY")
        );
        assert!(
            shadow_requests[0]
                .system_prompt
                .as_ref()
                .unwrap()
                .contains("CANDIDATE-ONLY")
        );
        assert_eq!(
            serde_json::to_value(&main_requests[0].messages).unwrap(),
            serde_json::to_value(&shadow_requests[0].messages).unwrap()
        );
        assert_eq!(
            state
                .machine
                .task_budget
                .as_ref()
                .unwrap()
                .snapshot()
                .await
                .model_calls,
            2
        );
        state.machine.flush_recorder().await.unwrap();
        let history = RolloutRecorder::load_history(state.machine.rollout_path().unwrap())
            .await
            .unwrap();
        let observations: Vec<_> = history
            .iter()
            .filter_map(|item| match item {
                RolloutItem::Event(event) if event.event_type == "guardian_shadow_v1" => {
                    Some(&event.payload)
                }
                _ => None,
            })
            .collect();
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0]["stage"]["state"], "started");
        assert_eq!(observations[1]["stage"]["state"], "finished");
        assert_eq!(
            observations[1]["identity"]["reviewer"]["profile"],
            "reviewer"
        );
        assert_eq!(
            observations[1]["stage"]["final_response_usage"]["total_tokens"],
            15
        );
        assert!(observations[1]["stage"]["cost_microusd"].is_null());
        assert_eq!(observations[0]["identity"], observations[1]["identity"]);
        let fingerprint = NamespaceGeneration::request_fingerprint(&shadow_requests[0]).unwrap();
        assert_eq!(observations[1]["identity"]["request_digest"], fingerprint);
        assert_eq!(
            observations[1]["identity"]["baseline_request_digest"],
            NamespaceGeneration::request_fingerprint(&main_requests[0]).unwrap()
        );
        let mut changed = shadow_requests[0].clone();
        changed.temperature = Some(0.5);
        assert_ne!(
            NamespaceGeneration::request_fingerprint(&changed).unwrap(),
            fingerprint
        );
        if expected == "authorized" {
            assert!(matches!(
                authorize(&mut state).await.unwrap(),
                ToolAuthorizationOutcome::Authorized { .. }
            ));
            assert_eq!(main_provider.recorded_requests().len(), 2);
            assert_eq!(
                shadow_provider.recorded_requests().len(),
                1,
                "settled candidate calls must not repeat"
            );
            assert_eq!(
                state
                    .machine
                    .task_budget
                    .as_ref()
                    .unwrap()
                    .snapshot()
                    .await
                    .model_calls,
                3
            );
        }
    }
}

#[tokio::test]
async fn shadow_cannot_obtain_a_fresh_allowance_after_main_consumes_the_last_call() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = runtime_state_with_provider(
        alan_llm::MockLlmProvider::new().with_response(response("allow")),
    );
    let shadow = alan_llm::MockLlmProvider::new().with_response(response("allow"));
    state.environment = state
        .environment
        .with_guardian_shadow(candidate(shadow.clone()), None)
        .unwrap();
    state.machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "main", dir.path())
        .await
        .unwrap();
    state.machine.accept_submission("last-model-call");
    state.core_config.task_budget.model_calls = NonZeroU64::new(1).unwrap();
    state.core_config.memory.enabled = false;
    crate::runtime::task_budget::begin(&mut state)
        .await
        .unwrap();
    assert!(
        authorize(&mut state)
            .await
            .unwrap_err()
            .is::<crate::agent_machine::task_budget::Exhausted>()
    );
    assert!(shadow.recorded_requests().is_empty());
    assert_eq!(
        state
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await
            .model_calls,
        1
    );
    assert!(state.machine.pending_confirmation().is_none());
}
