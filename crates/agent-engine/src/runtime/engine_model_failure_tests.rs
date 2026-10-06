//! Model-binding failure barriers at the owning runtime queue boundary.
use super::*;
use crate::runtime::model_binding::{
    CallableIdentity, CapturedCallable, ConnectionAuthority, InputBinding,
};

#[path = "engine_model_command_tests.rs"]
mod command;
#[path = "engine_model_qualification_tests.rs"]
mod qualification;
#[path = "engine_model_retry_tests.rs"]
mod retry;
#[path = "engine_model_settlement_tests.rs"]
mod settlement;

fn environment() -> NamespaceRuntimeEnvironment {
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/1",
        "default",
    )
}

fn callable(env: &NamespaceRuntimeEnvironment, model: &str) -> CapturedCallable {
    CapturedCallable {
        identity: CallableIdentity {
            profile: "default".into(),
            provider: "openai_responses".into(),
            model: model.into(),
            credential_ref: None,
            revision: "test".into(),
        },
        root: env.root_transport(),
        connection: "default".into(),
        config: crate::Config::default(),
    }
}

#[tokio::test]
async fn incompatible_steer_removal_failure_retains_exact_admitted_input() {
    let dir = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    let env = environment();
    let a = callable(&env, "a");
    let b = callable(&env, "b");
    {
        let mut bindings = env.model_bindings.lock().await;
        bindings.confirmed = Some(b);
        *env.active_binding.write().unwrap() = Some((
            InputBinding {
                callable_binding: a.identity.clone(),
                request_controls: Default::default(),
            },
            a,
        ));
    }
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.environment = Some(env.clone());
    queues.recorder = machine.input_recorder();
    let input = Submission {
        id: "exact-steer".into(),
        intent: Default::default(),
        op: Op::Input {
            parts: vec![ContentPart::text("do not execute")],
            mode: InputMode::Steer,
        },
    };
    queues.admit_input(&input).await.unwrap();
    let admitted_binding = queues.outer_queue.lock().unwrap().bindings[&input.id].clone();
    let captured_identity = env.model_bindings.lock().await.captured[&input.id]
        .identity
        .clone();
    let (probe, mut observed) = queues.recorder.as_ref().unwrap().batch_failure_probe(false);
    queues.recorder = Some(probe.clone());
    // This is retained admitted work, not a second external intake. Exercise
    // its existing compatibility/removal owner without redelivering the ID.
    let removal_batch = tokio::time::timeout(Duration::from_secs(5), async {
        assert!(queues.reject_incompatible_steer(&input).await.unwrap());
        observed
            .recv()
            .await
            .expect("failed removal batch must be observed")
    })
    .await
    .expect("incompatible rejection/removal observation must complete within 5 seconds");
    assert_eq!(removal_batch.len(), 1);
    assert!(
        matches!(&removal_batch[0], crate::rollout::RolloutItem::Event(e)
        if e.event_type == "machine_inputs_removed_v1"
            && e.payload["submission_ids"] == serde_json::json!([input.id]))
    );
    {
        let queue = queues.outer_queue.lock().unwrap();
        assert!(queue.admitted_ids.contains(&input.id));
        assert!(!queue.settled_ids.contains(&input.id));
        assert!(queue.queue_uncertain_ids.contains(&input.id));
        assert!(queue.pending_binding_rejections.contains(&input.id));
        assert_eq!(queue.bindings[&input.id], admitted_binding);
    }
    assert_eq!(
        env.model_bindings.lock().await.captured[&input.id].identity,
        captured_identity
    );
    assert!(queues.is_paused());
    assert!(queues.pop_outer().is_none());
    let pending = queues
        .outer_queue
        .lock()
        .unwrap()
        .pending
        .iter()
        .filter_map(|item| match item {
            QueuedRuntimeItem::Submission(s) => Some(s.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pending.len(),
        1,
        "accepted input must remain locally retained"
    );
    assert_eq!(pending[0].id, input.id);
    assert_eq!(
        serde_json::to_value(&pending[0].op).unwrap(),
        serde_json::to_value(&input.op).unwrap()
    );
    assert!(queues.active_turn_broker.drain().await.is_empty());
    assert!(
        queues
            .handle_control(
                &Submission::new(Op::ContinueQueue),
                &env.agent_files(),
                None
            )
            .await
    );
    assert!(
        queues.is_paused(),
        "continue must not execute an unresolved rejection"
    );
    assert!(queues.pop_outer().is_none());
    assert!(machine.messages().is_empty());
    let shell = alan_shell::Shell::new(env.root_transport());
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    assert!(!events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(), alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. } if submission_ids.contains(&input.id))));
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1")));
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
        .await
        .unwrap();
    assert!(
        recovered
            .input_queue()
            .lock()
            .unwrap()
            .pending
            .iter()
            .any(|item| matches!(item, QueuedRuntimeItem::Submission(s) if s.id == input.id))
    );
    queues.recorder = machine.input_recorder();
    assert!(
        queues
            .handle_control(&Submission::new(Op::DiscardQueue), &env.agent_files(), None)
            .await
    );
    assert!(!queues.is_paused());
    assert!(
        queues
            .outer_queue
            .lock()
            .unwrap()
            .pending_binding_rejections
            .is_empty()
    );
    assert!(queues.pop_outer().is_none());
    probe.close().await.unwrap();
}

struct SecretFailure;
#[async_trait]
impl ConnectionAuthority for SecretFailure {
    async fn capture(&self, _: Option<&str>) -> Result<CapturedCallable> {
        Err(anyhow!("SECRET_SENTINEL /Users/private/HOST_SENTINEL").context("authority failure"))
    }
    async fn restore(&self, _: &CallableIdentity) -> Result<CapturedCallable> {
        self.capture(None).await
    }
    async fn catalog(&self) -> Result<serde_json::Value> {
        Err(anyhow!("unavailable"))
    }
}

#[tokio::test]
async fn runtime_capture_failure_is_not_ready_and_reason_is_safe() {
    let env = environment().with_connection_authority(Arc::new(SecretFailure));
    let config = crate::Config::default();
    let capabilities = crate::provider_capabilities_for_config(&config);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(config),
            ..Default::default()
        },
        env,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    let ready = runtime.wait_until_ready().await;
    runtime.shutdown().await.unwrap();
    let error = ready
        .expect_err("Connection capture must succeed before Ready")
        .to_string();
    assert!(
        error.contains("Connection binding initialization failed"),
        "{error}"
    );
    assert!(!error.contains("SECRET_SENTINEL"));
    assert!(!error.contains("HOST_SENTINEL"));
}

#[tokio::test]
async fn selection_authority_error_never_enters_user_projection() {
    let dir = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    let env = environment().with_connection_authority(Arc::new(SecretFailure));
    let old = callable(&env, "old");
    env.model_bindings.lock().await.confirmed = Some(old.clone());
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.environment = Some(env.clone());
    queues.recorder = machine.input_recorder();
    let input = Submission {
        id: "selection".into(),
        intent: Default::default(),
        op: Op::SelectModel {
            model: "new".into(),
        },
    };
    assert!(queues.model_control(&input).await);
    assert_eq!(
        env.model_bindings
            .lock()
            .await
            .confirmed
            .as_ref()
            .unwrap()
            .identity,
        old.identity
    );
    let shell = alan_shell::Shell::new(env.root_transport());
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    assert!(events.contains("selection"));
    assert!(
        !events.contains("SECRET_SENTINEL"),
        "authority secret leaked: {events}"
    );
    assert!(!events.contains("HOST_SENTINEL"));
    let durable = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(!durable.contains("SECRET_SENTINEL"));
    assert!(!durable.contains("HOST_SENTINEL"));
}
