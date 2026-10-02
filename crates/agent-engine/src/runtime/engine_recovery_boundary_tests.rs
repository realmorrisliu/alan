//! Owning intake, disposition, and explicit recovery boundary regressions.
use super::*;

#[tokio::test]
async fn command_active_steering_recovers_as_accepted_follow_up() {
    let dir = tempfile::tempdir().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.recorder = machine.input_recorder();
    let input = Submission::new(Op::Input {
        parts: vec![ContentPart::text("after command")],
        mode: InputMode::Steer,
    });
    queues
        .admit_during_submission(
            input.clone(),
            alan_agent_protocol::InputIntent::Command,
            true,
        )
        .await;
    assert!(queues.active_turn_broker.try_recv().await.is_none());
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    let accepted = history
        .iter()
        .find_map(|item| match item {
            crate::rollout::RolloutItem::Event(event)
                if event.event_type == "machine_input_admitted_v1" =>
            {
                Some(serde_json::from_value::<Submission>(event.payload.clone()).unwrap())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(accepted.id, input.id);
    assert!(matches!(
        accepted.op,
        Op::Input {
            mode: InputMode::FollowUp,
            ..
        }
    ));
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
        .await
        .unwrap();
    let queue = recovered.input_queue();
    assert!(
        matches!(queue.lock().unwrap().pending.front(), Some(QueuedRuntimeItem::Submission(s)) if s.id == input.id && matches!(s.op, Op::Input { mode: InputMode::FollowUp, .. }))
    );
}

#[tokio::test]
async fn discard_batches_all_removals_before_mutation_and_rejection_preserves_recovery() {
    for (fail, written) in [(false, false), (true, false), (false, true)] {
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let files = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default").agent_files();
        let dir = tempfile::tempdir().unwrap();
        let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
            .await
            .unwrap();
        let path = machine.rollout_path().unwrap().clone();
        let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
        queues.recorder = machine.input_recorder();
        let inputs = vec![
            Submission::new(Op::Turn {
                parts: vec![ContentPart::text("first")],
                context: None,
            }),
            Submission::new(Op::Input {
                parts: vec![ContentPart::text("second")],
                mode: InputMode::FollowUp,
            }),
            Submission::new(Op::Turn {
                parts: vec![ContentPart::text("third")],
                context: None,
            }),
        ];
        let ids: Vec<_> = inputs.iter().map(|input| input.id.clone()).collect();
        for input in &inputs {
            queues.admit_input(input).await.unwrap();
            queues.push_outer_submission(input.clone());
        }
        let control = Submission::new(Op::CompactWithOptions { focus: None });
        queues.push_outer_submission(control.clone());
        queues.pause();
        let mut observed = if fail || written {
            let (probe, observed) = queues
                .recorder
                .as_ref()
                .unwrap()
                .batch_failure_probe(written);
            queues.recorder = Some(probe);
            Some(observed)
        } else {
            None
        };
        assert!(
            queues
                .handle_control(&Submission::new(Op::DiscardQueue), &files, None)
                .await
        );
        if let Some(observed) = &mut observed {
            let batch = observed.recv().await.unwrap();
            assert_eq!(batch.len(), 1, "the ID set is one recovery record");
            let crate::rollout::RolloutItem::Event(event) = &batch[0] else {
                panic!("expected removal evidence")
            };
            assert_eq!(event.event_type, "machine_inputs_removed_v1");
            let removed: Vec<String> =
                serde_json::from_value(event.payload["submission_ids"].clone()).unwrap();
            assert_eq!(removed, ids, "the entire discard must be sent in one batch");
            assert!(
                observed.try_recv().is_err(),
                "no per-input persistence requests"
            );
        }
        assert_eq!(queues.is_paused(), fail);
        {
            let queue = queues.outer_queue.lock().unwrap();
            let live: Vec<_> = queue
                .pending
                .iter()
                .filter_map(|item| match item {
                    QueuedRuntimeItem::Submission(input) if input.id != control.id => {
                        Some(input.id.clone())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(live, if fail { ids.clone() } else { vec![] });
            assert!(queue.pending.iter().any(|item| matches!(item, QueuedRuntimeItem::Submission(input) if input.id == control.id)));
        }
        let history = crate::rollout::RolloutRecorder::load_history(&path)
            .await
            .unwrap();
        let tombstones = history.iter().filter(|item| matches!(item,
            crate::rollout::RolloutItem::Event(event) if event.event_type == "machine_inputs_removed_v1")).count();
        assert_eq!(tombstones, if fail { 0 } else { 1 });
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
                .await
                .unwrap();
        let recovered_ids: Vec<_> = recovered
            .input_queue()
            .lock()
            .unwrap()
            .pending
            .iter()
            .filter_map(|item| match item {
                QueuedRuntimeItem::Submission(input) => Some(input.id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(recovered_ids, if fail { ids.clone() } else { vec![] });
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        let cancelled: Vec<_> = events
            .lines()
            .filter_map(|line| {
                match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
                    alan_agent_protocol::UiEvent::InputCompleted {
                        submission_ids,
                        status: alan_agent_protocol::UiInputStatus::Cancelled,
                        ..
                    } => Some(submission_ids),
                    _ => None,
                }
            })
            .flatten()
            .collect();
        assert_eq!(cancelled, if fail { vec![] } else { ids });
        if fail || written {
            queues.recorder.as_ref().unwrap().close().await.unwrap();
        }
    }
}

#[tokio::test]
async fn queue_persistence_failure_rejects_admission_and_preserves_pending_removals() {
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let files = environment.agent_files();
    let dir = tempfile::tempdir().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.recorder = machine.recorder();
    queues.environment = Some(environment);
    queues
        .initialize_bindings(&crate::Config::default(), Default::default())
        .await
        .unwrap();
    let pending = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("pending")],
        context: None,
    });
    queues.admit_input(&pending).await.unwrap();
    queues.push_outer_submission(pending.clone());
    queues.pause();
    queues.recorder.as_ref().unwrap().close().await.unwrap();

    let rejected = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("rejected")],
        context: None,
    });
    let (sender, mut receiver) = mpsc::channel(2);
    sender.send(rejected.clone()).await.unwrap();
    assert!(
        queues
            .admit_api_before_dispatch(&mut receiver)
            .await
            .is_none()
    );
    assert!(
        !queues
            .outer_queue
            .lock()
            .unwrap()
            .admitted_ids
            .contains(&rejected.id)
    );
    let during = Submission::new(Op::Input {
        parts: vec![ContentPart::text("during")],
        mode: InputMode::Steer,
    });
    queues
        .admit_during_submission(
            during.clone(),
            alan_agent_protocol::InputIntent::Command,
            true,
        )
        .await;
    assert!(
        !queues
            .outer_queue
            .lock()
            .unwrap()
            .admitted_ids
            .contains(&during.id)
    );
    assert!(queues.active_turn_broker.try_recv().await.is_none());
    for control in [
        Op::InterruptSubmission {
            submission_id: pending.id.clone(),
        },
        Op::DiscardQueue,
    ] {
        assert!(
            queues
                .handle_control(&Submission::new(control), &files, None)
                .await
        );
        assert!(queues.is_paused());
        let queue = queues.outer_queue.lock().unwrap();
        assert_eq!(queue.pending.len(), 1);
        assert!(
            matches!(queue.pending.front(), Some(QueuedRuntimeItem::Submission(s)) if s.id == pending.id)
        );
    }
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    for rejected_id in [&rejected.id, &during.id] {
        assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
            alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, error: Some(error) }
            if submission_ids == [rejected_id.clone()] && error.contains("admission persistence failed"))));
    }
    assert!(!events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
        alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Cancelled, .. }
        if submission_ids.contains(&pending.id))));
}

#[tokio::test]
async fn targeted_queue_cancellation_preserves_other_inputs_and_active_work() {
    for storage in ["ordinary", "inband", "buffered", "next_turn"] {
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let files = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default").agent_files();
        let mut machine = AgentMachine::new();
        let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
        let input = Submission::new(Op::Input {
            parts: vec![ContentPart::text("cancel this")],
            mode: InputMode::FollowUp,
        });
        match storage {
            "ordinary" => queues.push_outer_submission(input.clone()),
            "inband" => assert!(queues.active_turn_broker.push(input.clone()).await),
            "buffered" => machine.push_buffered_inband_submission(input.clone()),
            "next_turn" => {
                machine.accept_submission(&input.id);
                machine.queue_next_turn_input(vec![ContentPart::text("cancel this")]);
                machine.finish_submission();
            }
            _ => unreachable!(),
        }
        let survivor = Submission::new(Op::Turn {
            parts: vec![ContentPart::text("later")],
            context: None,
        });
        queues.push_outer_submission(survivor.clone());
        machine.accept_submission("active");
        let cancel = CancellationToken::new();
        assert!(
            queues
                .handle_control(
                    &Submission::new(Op::InterruptSubmission {
                        submission_id: "unknown".into(),
                    }),
                    &files,
                    Some(&cancel)
                )
                .await
        );
        assert!(!cancel.is_cancelled());
        assert!(!queues.is_paused());
        assert!(
            queues
                .handle_control(
                    &Submission::new(Op::InterruptSubmission {
                        submission_id: input.id.clone(),
                    }),
                    &files,
                    Some(&cancel)
                )
                .await
        );
        assert!(
            !cancel.is_cancelled(),
            "a queued cancellation cannot cancel other active work"
        );
        assert!(queues.is_paused());
        assert!(queues.active_turn_broker.try_recv().await.is_none());
        assert!(machine.drain_buffered_inband_submissions().is_empty());
        assert_eq!(machine.queued_next_turn_input_count(), 0);
        {
            let pending = queues.outer_queue.lock().unwrap();
            assert_eq!(pending.pending.len(), 1);
            assert!(
                matches!(pending.pending.front(), Some(QueuedRuntimeItem::Submission(s)) if s.id == survivor.id)
            );
        }
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert_eq!(events.lines().filter(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
            alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Cancelled, .. } if submission_ids == [input.id.clone()])).count(), 1);
        machine.set_turn_activity(crate::agent_machine::TurnActivityState::Idle);
        assert!(
            !machine.submission_was_cancelled(),
            "queued cancellation leaves active work intact"
        );
        machine.accept_submission("finishing-command");
        machine.set_turn_activity(crate::agent_machine::TurnActivityState::Running);
        queues
            .handle_control(
                &Submission::new(Op::InterruptSubmission {
                    submission_id: "finishing-command".into(),
                }),
                &files,
                Some(&cancel),
            )
            .await;
        machine.set_turn_activity(crate::agent_machine::TurnActivityState::Idle);
        assert!(
            machine.submission_was_cancelled(),
            "accepted cancellation survives asynchronous command finalization"
        );
        assert_eq!(machine.current_submission_id(), Some("finishing-command"));
        machine.accept_submission("later-input");
        machine.set_turn_activity(crate::agent_machine::TurnActivityState::Idle);
        assert!(
            !machine.submission_was_cancelled(),
            "settlement consumes the cancellation request"
        );
        machine.accept_submission("admitting-next-turn");
        machine.queue_next_turn_input(vec![ContentPart::text("queued payload")]);
        let overlap_cancel = CancellationToken::new();
        queues
            .handle_control(
                &Submission::new(Op::InterruptSubmission {
                    submission_id: "admitting-next-turn".into(),
                }),
                &files,
                Some(&overlap_cancel),
            )
            .await;
        assert!(overlap_cancel.is_cancelled());
        assert_eq!(machine.queued_next_turn_input_count(), 0);
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert!(
            !events.contains("admitting-next-turn"),
            "active admission owns its eventual settlement"
        );
        machine.set_turn_activity(crate::agent_machine::TurnActivityState::Idle);
        assert!(machine.submission_was_cancelled());
    }
}

#[tokio::test]
async fn runtime_admission_shutdown_and_explicit_recovery_preserve_paused_inputs() {
    let temp = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let mock = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        "default",
        Box::new(GatedFirstGeneration {
            mock: mock.clone(),
            started: started.clone(),
            release: release.clone(),
        }),
    );
    let mut namespace = alan_kernel::Namespace::new();
    for path in ["/agent/1", "/agent/2"] {
        namespace.mount(
            path,
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
    }
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let agent_config = crate::AgentConfig::from(core);
    let capabilities = crate::provider_capabilities_for_config(&agent_config.core_config);
    let mut controller = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: agent_config.clone(),
            store_bindings: Some(stores.clone()),
            ..AgentProcessConfig::default()
        },
        NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    let metadata = controller.wait_until_ready().await.unwrap();
    assert!(metadata.durability.durable);
    let path = metadata.rollout_path.unwrap();
    let first = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("active")],
        context: None,
    });
    controller
        .handle
        .submission_tx
        .send(first.clone())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), started.notified())
        .await
        .unwrap();
    let second = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("pending api")],
        context: None,
    });
    controller
        .handle
        .submission_tx
        .send(second.clone())
        .await
        .unwrap();
    // Observe actual durable intake before committing the next namespace frame.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let history = crate::rollout::RolloutRecorder::load_history(&path).await.unwrap();
            if history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(event)
                if event.event_type == "machine_input_admitted_v1" && event.payload["id"] == second.id)) { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    let third_id = uuid::Uuid::new_v4().to_string();
    let frame = serde_json::json!({"version":1,"submission_id":third_id,"intent":"agent","mode":"follow_up","body":"pending namespace"});
    shell
        .write(
            "/agent/1/io/input",
            format!("alan-input-v1\n{frame}").as_bytes(),
        )
        .await
        .unwrap();
    shell
        .write("/agent/1/machine/ctl", b"interrupt")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let activity: serde_json::Value =
                serde_json::from_slice(&shell.cat("/agent/1/machine/ui/activity").await.unwrap())
                    .unwrap();
            if activity["state"] == "paused" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    release.notify_one();
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
        alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Cancelled, .. }
            if submission_ids.contains(&first.id))), "interruption must settle the active input before shutdown");
    controller.shutdown().await.unwrap();
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    let admitted: Vec<_> = history
        .iter()
        .filter_map(|item| match item {
            crate::rollout::RolloutItem::Event(event)
                if event.event_type == "machine_input_admitted_v1" =>
            {
                Some(event.payload["id"].as_str().unwrap().to_owned())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        admitted,
        [first.id.clone(), second.id.clone(), third_id.clone()]
    );
    assert_eq!(
        mock.recorded_requests().len(),
        1,
        "shutdown must not dispatch paused work"
    );
    let recovered =
        AgentMachine::load_from_rollout_in_dir(&path, "/proc/2", "test-model", &stores.rollouts)
            .await
            .unwrap();
    {
        let queue = recovered.input_queue();
        let queue = queue.lock().unwrap();
        let ids: Vec<_> = queue
            .pending
            .iter()
            .filter_map(|item| match item {
                QueuedRuntimeItem::Submission(input) => Some(input.id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, [second.id.clone(), third_id.clone()]);
        assert!(queue.paused);
        assert!(queue.active_submission_ids.is_empty());
    }
    // Recovered Process has a real binding and a live authority owner, but the
    // current Host Mount projection supplies neither an active grant nor adapter.
    // This reaches directory validation rather than the missing-environment guard.
    #[derive(Debug)]
    struct NoActiveGrant;
    impl crate::tools::ToolExecutionAuthority for NoActiveGrant {
        fn reconcile(
            &self,
            _: u64,
            binding: crate::tools::ToolExecutionBinding,
        ) -> anyhow::Result<crate::tools::ToolExecutionBinding> {
            assert!(binding.cwd_grant_id.is_none());
            assert!(!binding.has_adapter());
            Ok(binding)
        }
    }
    let runner = crate::tools::ToolProcessRunner::from_registry(&crate::tools::ToolRegistry::new());
    runner.register_process_binding(
        2,
        crate::tools::ToolExecutionBinding::awaiting_host_projection(
            "/mnt/project".into(),
            temp.path().into(),
        ),
    );
    runner.register_process_authority(2, Arc::new(NoActiveGrant));
    let recovered_environment = NamespaceRuntimeEnvironment::new(root, "/agent/2", "default")
        .with_namespace_cwd("/mnt/project")
        .with_tool_process_context(2, runner.clone());
    assert_eq!(
        recovered_environment.tool_execution().default_cwd(),
        Some("/mnt/project".into())
    );
    let preflight_error = recovered_environment
        .tool_execution()
        .change_process_directory(std::path::Path::new("/mnt/project"))
        .unwrap_err();
    assert!(
        preflight_error
            .to_string()
            .contains("Process has no active Host Mount execution adapter")
    );
    // Explicitly restart the runtime too, rather than testing only the loader.
    let mut restarted = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config,
            store_bindings: Some(stores.clone()),
            recovery_rollout_path: Some(path),
            ..AgentProcessConfig::default()
        },
        recovered_environment,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    let restarted_metadata = restarted.wait_until_ready().await.unwrap();
    let startup_activity: alan_agent_protocol::UiActivitySnapshot =
        serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await.unwrap()).unwrap();
    assert_eq!(
        startup_activity.state,
        alan_agent_protocol::UiActivityState::Paused
    );
    let startup_events =
        String::from_utf8(shell.cat("/agent/2/machine/ui/events").await.unwrap()).unwrap();
    assert!(startup_events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
        alan_agent_protocol::UiEvent::Activity { snapshot } if snapshot.state == alan_agent_protocol::UiActivityState::Paused)));
    assert!(restarted_metadata.durability.durable);
    let restarted_path = restarted_metadata.rollout_path.unwrap();
    assert_eq!(
        mock.recorded_requests().len(),
        1,
        "explicit recovery must not automatically execute"
    );
    restarted
        .handle
        .submission_tx
        .send(Submission::new(Op::ContinueQueue))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let notice =
                String::from_utf8(shell.cat("/agent/2/machine/ui/notice").await.unwrap()).unwrap();
            if notice.contains("Queue control rejected:") {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        mock.recorded_requests().len(),
        1,
        "recovery and unauthorized continue must not execute"
    );
    restarted.shutdown().await.unwrap();
    let after_rejection = AgentMachine::load_from_rollout_in_dir(
        &restarted_path,
        "/proc/3",
        "test-model",
        &stores.rollouts,
    )
    .await
    .unwrap();
    {
        let queue = after_rejection.input_queue();
        let queue = queue.lock().unwrap();
        let ids: Vec<_> = queue
            .pending
            .iter()
            .filter_map(|item| match item {
                QueuedRuntimeItem::Submission(input) => Some(input.id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            [second.id, third_id],
            "rejected continue must retain queued identities and order"
        );
        assert!(queue.paused);
        assert!(queue.active_submission_ids.is_empty());
    }
    assert!(runner.process_binding(2).unwrap().cwd_grant_id.is_none());
    assert!(!runner.process_binding(2).unwrap().has_adapter());
    let notice = String::from_utf8(shell.cat("/agent/2/machine/ui/notice").await.unwrap()).unwrap();
    assert!(
        notice.contains("Recovered Process requires current explicit project authority before continuing queued work"),
        "unauthorized recovered ContinueQueue must give actionable authority guidance: {notice}"
    );
}
