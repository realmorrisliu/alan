//! Real observer selection contention must not redirect a fresh guardian generation.
use super::*;
#[path = "engine_model_deferred_tests.rs"]
mod deferred;
#[path = "engine_next_turn_tests.rs"]
mod next_turn;
#[path = "engine_next_turn_acceptance_tests.rs"]
mod next_turn_acceptance;
#[path = "engine_model_observation_failure_tests.rs"]
mod observation_failure;
#[path = "engine_model_projection_tests.rs"]
mod projection;
use crate::runtime::model_binding::{CallableIdentity, CapturedCallable, ConnectionAuthority};
use crate::runtime::transition::tests::tool_batch::{
    CountingEffectTool, create_test_state_with_machine_tools_and_provider, reviewer_response,
};
use std::sync::atomic::{AtomicUsize, Ordering};

struct GatedSelection {
    a: CapturedCallable,
    b: CapturedCallable,
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
#[async_trait]
impl ConnectionAuthority for GatedSelection {
    async fn capture(&self, model: Option<&str>) -> anyhow::Result<CapturedCallable> {
        if model == Some("B") {
            self.started.notify_one();
            self.release.notified().await;
            Ok(self.b.clone())
        } else {
            Ok(self.a.clone())
        }
    }
    async fn restore(&self, identity: &CallableIdentity) -> anyhow::Result<CapturedCallable> {
        let callable = if identity == &self.a.identity {
            &self.a
        } else {
            &self.b
        };
        anyhow::ensure!(identity == &callable.identity, "exact restore unavailable");
        Ok(callable.clone())
    }
    async fn catalog(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
}

async fn completed(shell: &alan_shell::Shell, id: &str) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await?)?;
            for line in events.lines() {
                if let alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids,
                    status,
                    ..
                } = serde_json::from_str(line)?
                    && submission_ids.iter().any(|entry| entry == id)
                {
                    anyhow::ensure!(
                        status == alan_agent_protocol::UiInputStatus::Completed,
                        "input {id} failed"
                    );
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(anyhow::Error::from)?
}

#[tokio::test]
async fn observer_selection_preserves_active_guardian_and_admitted_callable() {
    let temp = TempDir::new().unwrap();
    let l = MockLlmProvider::new().with_response(reviewer_response("allow"));
    let counter = Arc::new(AtomicUsize::new(0));
    let mut tools = crate::tools::ToolRegistry::new();
    tools.register(CountingEffectTool {
        name: "do_thing",
        capability: alan_agent_protocol::ToolCapability::Unknown,
        counter: counter.clone(),
    });
    let state =
        create_test_state_with_machine_tools_and_provider(AgentMachine::new(), tools, l.clone())
            .await;
    let mut first = reviewer_response("allow");
    first.content.clear();
    first.tool_calls.push(alan_llm::ToolCall {
        id: Some("unknown-call".into()),
        name: "do_thing".into(),
        arguments: serde_json::json!({"payload":"effect"}),
    });
    let mut answer = reviewer_response("allow");
    answer.content = "A answer".into();
    let a = MockLlmProvider::new().with_responses(vec![
        first,
        reviewer_response("allow"),
        answer.clone(),
        answer.clone(),
    ]);
    let b = MockLlmProvider::new().with_response(answer);
    let started_a = Arc::new(tokio::sync::Notify::new());
    let release_a = Arc::new(tokio::sync::Notify::new());
    let started_b = Arc::new(tokio::sync::Notify::new());
    let release_b = Arc::new(tokio::sync::Notify::new());
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection(
        "A",
        Box::new(GatedFirstGeneration {
            mock: a.clone(),
            started: started_a.clone(),
            release: release_a.clone(),
        }),
    );
    registry.register_connection("B", Box::new(b.clone()));
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    core.model_reasoning_effort = None;
    let make_callable = |name: &str, config: crate::Config| {
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(Arc::new(registry.connection_snapshot(name))),
            alan_kernel::Access::ReadWrite,
        );
        CapturedCallable {
            identity: CallableIdentity {
                profile: "managed".into(),
                provider: "openai_responses".into(),
                model: name.into(),
                credential_ref: None,
                revision: name.into(),
            },
            root: InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
            connection: name.into(),
            config,
        }
    };
    let mut b_config = core.clone();
    b_config.model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::High);
    let mut a_config = core.clone();
    a_config.model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::Low);
    let authority = Arc::new(GatedSelection {
        a: make_callable("A", a_config),
        b: make_callable("B", b_config),
        started: started_b.clone(),
        release: release_b.clone(),
    });
    let env = state.environment.with_connection_authority(authority);
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: temp.path().join("rollouts"),
                checkpoints: temp.path().join("checkpoints"),
                cache: temp.path().join("cache"),
                tmp: temp.path().join("tmp"),
                metadata: temp.path().join("metadata"),
            }),
            ..Default::default()
        },
        env,
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let active = Submission::new(Op::Input {
        parts: vec![ContentPart::text("active A")],
        mode: InputMode::FollowUp,
    });
    let queued = Submission::new(Op::Input {
        parts: vec![ContentPart::text("admitted A")],
        mode: InputMode::FollowUp,
    });
    let selection = Submission::new(Op::SelectModel { model: "B".into() });
    let next = Submission::new(Op::Input {
        parts: vec![ContentPart::text("new B")],
        mode: InputMode::FollowUp,
    });
    // Collect errors, then release both gates and shut down before any assertion can panic.
    let result: anyhow::Result<bool> = async {
        runtime.handle.submission_tx.send(active.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started_a.notified()).await?;
        runtime.handle.submission_tx.send(queued.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let history = crate::rollout::RolloutRecorder::load_history(&path).await?;
                if history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1" && e.payload["id"] == queued.id)) { return Ok::<_, anyhow::Error>(()); }
                tokio::task::yield_now().await;
            }
        }).await??;
        runtime.handle.submission_tx.send(selection.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started_b.notified()).await?;
        release_a.notify_one();
        let observed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if a.recorded_requests().iter().chain(l.recorded_requests().iter()).any(is_review) { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.is_ok();
        let correct = observed && a.recorded_requests().iter().any(is_review) && l.recorded_requests().is_empty();
        release_b.notify_one();
        completed(&shell, &selection.id).await?;
        runtime.handle.submission_tx.send(next.clone()).await?;
        completed(&shell, &active.id).await?;
        completed(&shell, &queued.id).await?;
        completed(&shell, &next.id).await?;
        Ok(correct)
    }.await;
    release_a.notify_one();
    release_b.notify_one();
    runtime.shutdown().await.unwrap();
    assert!(
        result.unwrap(),
        "guardian generation must use active A while B holds selection lock; A={}, L={}",
        a.recorded_requests().len(),
        l.recorded_requests().len()
    );
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    let requests = a.recorded_requests();
    let ordinary: Vec<_> = requests.iter().filter(|r| !is_review(r)).collect();
    assert_eq!(
        ordinary.len(),
        3,
        "active first, tool follow-up, and admitted A"
    );
    assert!(
        ordinary
            .iter()
            .all(|r| r.reasoning.effort == Some(alan_agent_protocol::ReasoningEffort::Low))
    );
    let requests = b.recorded_requests();
    assert_eq!(requests.len(), 1, "new input actually uses B");
    assert_eq!(
        requests[0].reasoning.effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
    assert!(l.recorded_requests().is_empty());
}

fn is_review(request: &GenerationRequest) -> bool {
    request
        .system_prompt
        .as_deref()
        .is_some_and(|system| system.contains("You are a security reviewer"))
}
