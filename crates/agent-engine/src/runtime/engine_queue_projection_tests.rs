//! Authoritative queue receipts, distinct from interactive waiting/activity.
use super::*;

#[tokio::test]
async fn explicit_recovery_publishes_paused_exact_queue_receipt_before_ready() {
    let temp = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let source = AgentMachine::new_with_recorder_in_dir("/agent/old", "test", &stores.rollouts)
        .await
        .unwrap();
    let path = source.rollout_path().unwrap().clone();
    let input = Submission::new(Op::Input {
        parts: vec![ContentPart::text("private queued content")],
        mode: InputMode::FollowUp,
    });
    source.admit_input(&input).await.unwrap();
    source.input_recorder().unwrap().close().await.unwrap();
    let mock = MockLlmProvider::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/2",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let env = NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/2",
        "default",
    );
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(stores),
            recovery_rollout_path: Some(path),
            ..Default::default()
        },
        env,
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    runtime.wait_until_ready().await.unwrap();
    let observed = shell.cat("/agent/2/machine/ui/queue").await;
    runtime.shutdown().await.unwrap();
    let doc: serde_json::Value =
        serde_json::from_slice(&observed.expect("queue receipt exists before Ready")).unwrap();
    assert_eq!(doc["pending_submission_ids"], serde_json::json!([input.id]));
    assert_eq!(doc["active_submission_ids"], serde_json::json!([]));
    assert_eq!(doc["paused"], true);
    assert_eq!(doc["known"], true);
    assert!(doc["revision"].as_u64().unwrap() > 0);
    assert!(!doc.to_string().contains("private queued content"));
    let activity: alan_agent_protocol::UiActivitySnapshot =
        serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await.unwrap()).unwrap();
    assert!(activity.waiting_submission_ids.is_empty());
    assert!(mock.recorded_requests().is_empty());
}

#[tokio::test]
async fn admission_failure_never_publishes_accepted_receipt() {
    let temp = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", temp.path())
        .await
        .unwrap();
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    let env = NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/1",
        "default",
    );
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    crate::runtime::queue_publication::initialize(&machine.input_queue(), env.agent_files())
        .await
        .unwrap();
    queues.environment = Some(env);
    let (probe, mut observed) = machine.input_recorder().unwrap().batch_failure_probe(false);
    queues.recorder = Some(probe);
    let input = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("not accepted")],
        context: None,
    });
    queues
        .initialize_bindings(
            &crate::Config::default(),
            crate::RequestControlIntent::default(),
        )
        .await
        .unwrap();
    let admission = queues.admit_input(&input).await;
    let batch = tokio::time::timeout(Duration::from_secs(2), observed.recv()).await;
    let projection = shell.cat("/agent/1/machine/ui/queue").await;
    queues.recorder.as_ref().unwrap().close().await.unwrap();
    machine.input_recorder().unwrap().close().await.unwrap();
    let error = admission.expect_err("writer must reject admission");
    assert_eq!(error.to_string(), "injected batch writer failure");
    let batch = batch
        .expect("bounded writer observation")
        .expect("writer batch");
    assert!(
        matches!(&batch[0], crate::rollout::RolloutItem::Event(event) if event.event_type == "machine_input_admitted_v1" && event.payload["id"] == input.id)
    );
    let doc: serde_json::Value = serde_json::from_slice(
        &projection.expect("unknown queue surface exists without an admission receipt"),
    )
    .unwrap();
    assert!(
        !doc["pending_submission_ids"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(input.id))
    );
    assert!(
        machine
            .input_queue()
            .lock()
            .unwrap()
            .admitted_ids
            .is_empty()
    );
}

#[tokio::test]
async fn accepted_queue_lifecycle_is_serialized_and_not_activity() {
    let temp = TempDir::new().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", temp.path())
        .await
        .unwrap();
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    let env = NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/1",
        "default",
    );
    let shell = alan_shell::Shell::new(env.root_transport());
    let files = env.agent_files();
    let queue = machine.input_queue();
    crate::runtime::queue_publication::initialize(&queue, files.clone())
        .await
        .unwrap();
    async fn read(shell: &alan_shell::Shell) -> alan_agent_protocol::UiQueueSnapshot {
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/queue").await.unwrap()).unwrap()
    }
    assert!(!read(&shell).await.known);
    let first = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("secret first")],
        context: None,
    });
    let second = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("secret second")],
        context: None,
    });
    machine.admit_input(&first).await.unwrap();
    machine.admit_input(&second).await.unwrap();
    let admitted = read(&shell).await;
    assert!(admitted.known && admitted.is_valid());
    assert!(admitted.pending_submission_ids.contains(&first.id));
    assert!(admitted.pending_submission_ids.contains(&second.id));
    let mut queues = RuntimeSubmissionQueues::new(queue.clone());
    queues.recorder = machine.input_recorder();
    queues.push_outer_submission(first.clone());
    queues.push_outer_submission(second.clone());
    queues.pause();
    crate::runtime::queue_publication::publish(&queue)
        .await
        .unwrap();
    let paused = read(&shell).await;
    assert!(paused.paused && paused.revision > admitted.revision);
    queues
        .handle_control(&Submission::new(Op::ContinueQueue), &files, None)
        .await;
    crate::runtime::queue_publication::publish(&queue)
        .await
        .unwrap();
    let resumed = read(&shell).await;
    assert!(!resumed.paused && resumed.revision > paused.revision);
    assert!(
        matches!(queues.pop_outer(), Some(QueuedRuntimeItem::Submission(input)) if input.id == first.id)
    );
    machine.accept_submission(first.id.clone());
    machine.dispatch_input(&first).await.unwrap();
    let dispatched = read(&shell).await;
    assert_eq!(dispatched.active_submission_ids, vec![first.id.clone()]);
    assert_eq!(dispatched.pending_submission_ids, vec![second.id.clone()]);
    machine.finish_submission();
    queues.pause();
    let (probe, mut observed) = machine.input_recorder().unwrap().batch_failure_probe(false);
    queues.recorder = Some(probe);
    queues
        .handle_control(&Submission::new(Op::DiscardQueue), &files, None)
        .await;
    observed.recv().await.unwrap();
    crate::runtime::queue_publication::publish(&queue)
        .await
        .unwrap();
    let uncertain = read(&shell).await;
    assert_eq!(uncertain.pending_submission_ids, vec![second.id.clone()]);
    assert_eq!(uncertain.uncertain_submission_ids, vec![second.id.clone()]);
    assert!(uncertain.paused);
    queues.recorder.take().unwrap().close().await.unwrap();
    queues.recorder = machine.input_recorder();
    queues
        .handle_control(&Submission::new(Op::DiscardQueue), &files, None)
        .await;
    crate::runtime::queue_publication::publish(&queue)
        .await
        .unwrap();
    let removed = read(&shell).await;
    assert!(removed.pending_submission_ids.is_empty());
    assert!(removed.active_submission_ids.is_empty());
    assert!(removed.uncertain_submission_ids.is_empty());
    assert!(!removed.paused && removed.revision > uncertain.revision);
    crate::runtime::ui_surfaces::heartbeat(&files)
        .await
        .unwrap();
    assert_eq!(read(&shell).await, removed);
    assert!(
        !String::from_utf8(shell.cat("/agent/1/machine/ui/queue").await.unwrap())
            .unwrap()
            .contains("secret")
    );
    machine.add_user_message("deferred secret");
    let job = crate::runtime::memory_promotion::build_turn_memory_promotion_job(
        &machine,
        Some(temp.path().to_path_buf()),
        "/agent/1".to_owned(),
        10,
        "queue test",
    )
    .unwrap();
    queues
        .push_outer_deferred(crate::agent_machine::DeferredRuntimeAction::TurnMemoryPromotion(job));
    crate::runtime::queue_publication::publish(&queue)
        .await
        .unwrap();
    let deferred = read(&shell).await;
    assert!(deferred.deferred && deferred.revision > removed.revision);
    assert!(deferred.pending_submission_ids.is_empty());
    assert!(queues.pop_outer_deferred().is_some());
    crate::runtime::queue_publication::publish(&queue)
        .await
        .unwrap();
    let drained = read(&shell).await;
    assert!(!drained.deferred && drained.revision > deferred.revision);
    machine.input_recorder().unwrap().close().await.unwrap();
}
