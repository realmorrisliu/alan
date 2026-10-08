use super::*;
use crate::agent_machine::TurnActivityState;
use crate::agent_machine::owner_work::Outcome;
use crate::runtime::transition::owner_work::handle;
use std::time::Duration;

#[tokio::test]
async fn cancelled_evaluation_settlement_does_not_settle_owner_work() {
    evaluation_settlement_race(true).await;
}

#[tokio::test]
async fn expired_evaluation_settlement_does_not_settle_owner_work() {
    evaluation_settlement_race(false).await;
}

async fn evaluation_settlement_race(cancel_wait: bool) {
    for persist_before_release in [false, true] {
        let (dir, mut state, reads, calls, generation) =
            fixture(Some("hostfs"), Some(1000), usize::MAX).await;
        let backing = state.machine.input_recorder().unwrap();
        let (probe, mut observed, release) =
            backing.terminal_evaluation_gate(persist_before_release);
        state.machine.set_input_recorder_for_test(probe.clone());
        let input = control("Who owns old fids?");
        let cancel = CancellationToken::new();
        let mut emit = |_| async {};
        let result = {
            let transition = handle(&mut state, &input, &mut emit, &cancel);
            tokio::pin!(transition);
            let batch = tokio::select! {
                batch = observed.recv() => batch.unwrap(),
                result = &mut transition => panic!("settlement gate not reached: {result:?}"),
            };
            assert!(matches!(&batch[0], RolloutItem::Event(event)
                if event.payload["outcome"]["state"] == "selected"));
            if cancel_wait {
                cancel.cancel();
            }
            let early = tokio::time::timeout(
                if cancel_wait {
                    Duration::from_millis(50)
                } else {
                    Duration::from_millis(1200)
                },
                &mut transition,
            )
            .await;
            release.send(()).unwrap();
            match early {
                Ok(result) => result,
                Err(_) => tokio::time::timeout(Duration::from_secs(2), &mut transition)
                    .await
                    .unwrap(),
            }
        };
        let error = result.unwrap_err();
        assert!(
            error
                .downcast_ref::<NamespaceEvaluationUncertainty>()
                .is_some(),
            "{error:#}"
        );
        assert_eq!(
            error.downcast_ref::<NamespaceEvaluationFailure>(),
            Some(&if cancel_wait {
                NamespaceEvaluationFailure::Cancelled
            } else {
                NamespaceEvaluationFailure::TimedOut
            })
        );
        let work = state.machine.owner_work.as_ref().unwrap();
        assert_eq!(work.outcome, Outcome::Started);
        assert_eq!(work.work_id, input.id);
        assert_eq!(work.evaluator_calls, 1);
        assert_eq!(work.generation_calls, 0);
        assert_eq!(
            state.machine.evaluation_observation.as_ref().unwrap()["outcome"]["state"],
            "started"
        );
        probe.flush().await.unwrap();
        let recovered = AgentMachine::load_from_rollout_in_dir(
            &backing.path().to_path_buf(),
            "/proc/9",
            "mock-model",
            dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            recovered.owner_work.as_ref().unwrap().outcome,
            Outcome::Interrupted
        );
        assert_eq!(recovered.owner_work.as_ref().unwrap().work_id, input.id);
        assert_eq!(recovered.owner_work.as_ref().unwrap().evaluator_calls, 1);
        let reads_before = reads.load(Ordering::SeqCst);
        state.machine = recovered;
        assert!(
            handle(&mut state, &input, &mut emit, &CancellationToken::new())
                .await
                .is_err()
        );
        assert_eq!(reads.load(Ordering::SeqCst), reads_before);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(generation.recorded_requests().is_empty());
        probe.close().await.unwrap();
        state
            .machine
            .input_recorder()
            .unwrap()
            .close()
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn acknowledged_wait_remains_paused_when_publication_and_error_notice_fail() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    state.environment = state.environment.with_work_publisher(|value| async move {
        if value.is_some_and(|value| value["state"] == "waiting") {
            anyhow::bail!("waiting publication unavailable");
        }
        Ok(())
    });
    let mut emit = |_| async {};
    let error = handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("waiting publication unavailable")
    );
    let work = state.machine.owner_work.clone().unwrap();
    let request_id = work.owned_request.clone().unwrap();
    assert!(state.machine.pending_yield(&request_id).is_some());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Paused);
    let files = state.agent_files();
    crate::runtime::ui_surfaces::turn_failed(&files, "publication failed", Some(&state.machine))
        .await
        .unwrap();
    assert_eq!(
        files.read_ui_activity_snapshot().await.unwrap().state,
        alan_agent_protocol::UiActivityState::Paused
    );
    crate::runtime::ui_surfaces::turn_started(&files)
        .await
        .unwrap();
    let blocked = with_read_only_node(&state.environment, "/agent/1/machine/ui/notice");
    assert!(
        crate::runtime::ui_surfaces::turn_failed(
            &blocked.agent_files(),
            "publication failed",
            Some(&state.machine)
        )
        .await
        .is_err()
    );
    assert_eq!(
        files.read_ui_activity_snapshot().await.unwrap().state,
        alan_agent_protocol::UiActivityState::Paused
    );
    state.environment.work_publisher = None;
    handle(
        &mut state,
        &Submission::new(Op::Resume {
            request_id,
            content: vec![alan_agent_protocol::ContentPart::structured(
                json!({"owner":"hostfs"}),
            )],
        }),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(matches!(
        state.machine.owner_work.as_ref().unwrap().outcome,
        Outcome::Completed { .. }
    ));
    assert_eq!(
        state.machine.owner_work.as_ref().unwrap().work_id,
        work.work_id
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn runtime_error_preserves_a_recovered_owned_wait() {
    let (dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let request_id = state
        .machine
        .owner_work
        .as_ref()
        .unwrap()
        .owned_request
        .clone()
        .unwrap();
    let path = state.machine.rollout_path().unwrap().clone();
    state
        .machine
        .input_recorder()
        .unwrap()
        .close()
        .await
        .unwrap();
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    let files = state.agent_files();
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    state.environment.model_bindings.lock().await.authority = None;
    let fail_publication = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = fail_publication.clone();
    state.environment = state.environment.with_work_publisher(move |_| {
        let fail = probe.load(Ordering::SeqCst);
        async move {
            if fail {
                anyhow::bail!("waiting publication unavailable");
            }
            Ok(())
        }
    });
    let mut runtime = crate::runtime::spawn_with_namespace_environment(
        crate::runtime::AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: dir.path().to_path_buf(),
                checkpoints: dir.path().join("checkpoints"),
                cache: dir.path().join("cache"),
                tmp: dir.path().join("tmp"),
                metadata: dir.path().join("metadata"),
            }),
            recovery_rollout_path: Some(path),
            ..Default::default()
        },
        state.environment,
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    runtime.wait_until_ready().await.unwrap();
    fail_publication.store(true, Ordering::SeqCst);
    runtime
        .handle
        .submission_tx
        .send(Submission::new(Op::Resume {
            request_id: "stale-request".into(),
            content: vec![],
        }))
        .await
        .unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if files
                .read_ui_notice_snapshot()
                .await
                .unwrap()
                .message
                .contains("waiting publication unavailable")
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let request_status = shell
        .cat(&format!("/agent/1/requests/{request_id}/status"))
        .await
        .unwrap();
    runtime.shutdown().await.unwrap();
    observed.unwrap();
    assert_eq!(
        files.read_ui_activity_snapshot().await.unwrap().state,
        alan_agent_protocol::UiActivityState::Paused
    );
    assert_eq!(request_status, b"pending");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn cancellation_during_wait_yield_retires_the_owned_request() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let cancel = CancellationToken::new();
    let probe = cancel.clone();
    let mut emit = move |event| {
        if matches!(event, Event::Yield { .. }) {
            probe.cancel();
        }
        async {}
    };
    handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &cancel,
    )
    .await
    .unwrap();
    let work = state.machine.owner_work.as_ref().unwrap();
    assert_eq!(work.outcome, Outcome::Cancelled);
    assert!(!state.machine.has_pending_interaction());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Idle);
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    assert_eq!(
        shell
            .cat(&format!(
                "/agent/1/requests/{}/status",
                work.owned_request.as_ref().unwrap()
            ))
            .await
            .unwrap(),
        b"cancelled"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn acknowledged_terminal_work_clears_its_wait_before_publication_can_fail() {
    for (owner, cancelled) in [("hostfs", false), ("unknown", false), ("hostfs", true)] {
        let (dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
        let mut emit = |_| async {};
        handle(
            &mut state,
            &control("Who owns old fids?"),
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let original = state.machine.owner_work.clone().unwrap();
        let request_id = original.owned_request.clone().unwrap();
        state.environment = state
            .environment
            .with_work_publisher(|_| async { anyhow::bail!("projection publication unavailable") });
        let response = Submission::new(Op::Resume {
            request_id: request_id.clone(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                json!({"owner":owner}),
            )],
        });
        let cancel = CancellationToken::new();
        if cancelled {
            cancel.cancel();
        }
        let error = handle(&mut state, &response, &mut emit, &cancel)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("projection publication unavailable")
        );
        let work = state.machine.owner_work.clone().unwrap();
        assert!(
            matches!(&work.outcome, Outcome::Completed { .. } if owner == "hostfs" && !cancelled)
                || matches!(&work.outcome, Outcome::Failed { .. } if owner == "unknown")
                || (work.outcome == Outcome::Cancelled && cancelled)
        );
        assert!(!state.machine.has_pending_interaction());
        assert_eq!(state.machine.turn_activity(), TurnActivityState::Idle);
        assert_eq!(work.work_id, original.work_id);
        let recorder = state.machine.input_recorder().unwrap();
        let recovered = AgentMachine::load_from_rollout_in_dir(
            &recorder.path().to_path_buf(),
            "/proc/9",
            "mock-model",
            dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(recovered.owner_work, Some(work));
        assert!(!recovered.has_pending_interaction());
        assert!(
            handle(&mut state, &response, &mut emit, &CancellationToken::new())
                .await
                .unwrap_err()
                .to_string()
                .contains("already terminal")
        );
        state.environment.work_publisher = None;
        let next = control("Which crate defines HostDirFs?");
        state.machine.accept_submission(next.id.clone());
        handle(&mut state, &next, &mut emit, &CancellationToken::new())
            .await
            .unwrap();
        assert!(matches!(
            state.machine.owner_work.as_ref().unwrap().outcome,
            Outcome::Completed { .. }
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(generation.recorded_requests().is_empty());
    }
}

#[tokio::test]
async fn acknowledged_completion_survives_a_read_only_completion_ui() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let request_id = state
        .machine
        .owner_work
        .as_ref()
        .unwrap()
        .owned_request
        .clone()
        .unwrap();
    let original = state.environment.clone();
    state.environment = with_read_only_node(&original, "/agent/1/machine/ui/activity");
    let response = Submission::new(Op::Resume {
        request_id,
        content: vec![alan_agent_protocol::ContentPart::structured(
            json!({"owner":"hostfs"}),
        )],
    });
    let error = handle(&mut state, &response, &mut emit, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("activity"));
    assert!(matches!(
        state.machine.owner_work.as_ref().unwrap().outcome,
        Outcome::Completed { .. }
    ));
    assert!(!state.machine.has_pending_interaction());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Idle);
    state.environment = original;
    let next = control("Which crate defines HostDirFs?");
    state.machine.accept_submission(next.id.clone());
    handle(&mut state, &next, &mut emit, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn unacknowledged_terminal_work_keeps_its_owned_wait_and_activity() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let original = state.machine.owner_work.clone().unwrap();
    let request_id = original.owned_request.clone().unwrap();
    state
        .machine
        .input_recorder()
        .unwrap()
        .close()
        .await
        .unwrap();
    let published = Arc::new(AtomicUsize::new(0));
    let probe = published.clone();
    state.environment = state.environment.with_work_publisher(move |_| {
        probe.fetch_add(1, Ordering::SeqCst);
        async { Ok(()) }
    });
    let response = Submission::new(Op::Resume {
        request_id: request_id.clone(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            json!({"owner":"hostfs"}),
        )],
    });
    assert!(
        handle(&mut state, &response, &mut emit, &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(state.machine.owner_work, Some(original));
    assert!(state.machine.pending_yield(&request_id).is_some());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Paused);
    assert_eq!(published.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn acknowledged_cancellation_clears_the_machine_wait_and_retries_prompt_cleanup() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let request_id = state
        .machine
        .owner_work
        .as_ref()
        .unwrap()
        .owned_request
        .clone()
        .unwrap();
    let blocked = with_read_only_node(
        &state.environment,
        &format!("/agent/1/requests/{request_id}/ctl"),
    )
    .agent_files();
    let files = state.agent_files();
    let mounts = state.environment.host_mount_requests();
    assert!(
        crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
            &mut state.machine,
            &blocked,
            &mounts
        )
        .await
        .is_err()
    );
    assert_eq!(
        state.machine.owner_work.as_ref().unwrap().outcome,
        Outcome::Cancelled
    );
    assert!(!state.machine.has_pending_interaction());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Idle);
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    assert_eq!(
        shell
            .cat(&format!("/agent/1/requests/{request_id}/status"))
            .await
            .unwrap(),
        b"pending"
    );
    crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
        &mut state.machine,
        &files,
        &mounts,
    )
    .await
    .unwrap();
    assert_eq!(
        shell
            .cat(&format!("/agent/1/requests/{request_id}/status"))
            .await
            .unwrap(),
        b"cancelled"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

fn with_read_only_node(
    original: &NamespaceRuntimeEnvironment,
    path: &str,
) -> NamespaceRuntimeEnvironment {
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/",
        original.root_transport(),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        path,
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        alan_kernel::Access::ReadOnly,
    );
    let mut blocked = NamespaceRuntimeEnvironment::new(
        alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/1",
        "default",
    )
    .with_namespace_cwd("/mnt/source");
    blocked.model_bindings = original.model_bindings.clone();
    blocked.active_binding = original.active_binding.clone();
    blocked
}

#[tokio::test]
async fn accepted_cancellation_publishes_terminal_work_even_if_prompt_cleanup_fails() {
    let (_dir, mut state, _, calls, generation) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let work = state.machine.owner_work.clone().unwrap();
    let request_id = work.owned_request.as_ref().unwrap();
    let published = Arc::new(std::sync::Mutex::new(None));
    let probe = published.clone();
    state.environment = with_read_only_node(
        &state.environment,
        &format!("/agent/1/requests/{request_id}/ctl"),
    )
    .with_work_publisher(move |value| {
        *probe.lock().unwrap() = value;
        async { Ok(()) }
    });
    let broker = TurnInputBroker::from_queue(state.machine.input_queue());
    let result = advance_accepted_submission(
        &mut state,
        Submission::new(Op::Interrupt),
        &broker,
        &CancellationToken::new(),
    )
    .await;
    assert!(result.result.is_err());
    let published = published.lock().unwrap();
    assert_eq!(published.as_ref().unwrap()["state"], "cancelled");
    assert_eq!(published.as_ref().unwrap()["work_id"], work.work_id);
    assert!(!state.machine.has_pending_interaction());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Idle);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn recovered_terminal_request_id_does_not_cancel_or_steal_a_new_interaction() {
    let (dir, mut state, _, _, _) = fixture(None, None, usize::MAX).await;
    let mut emit = |_| async {};
    handle(
        &mut state,
        &control("Who owns old fids?"),
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let files = state.agent_files();
    let mounts = state.environment.host_mount_requests();
    crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
        &mut state.machine,
        &files,
        &mounts,
    )
    .await
    .unwrap();
    let recorder = state.machine.input_recorder().unwrap();
    let recovered = AgentMachine::load_from_rollout_in_dir(
        &recorder.path().to_path_buf(),
        "/proc/9",
        "mock-model",
        dir.path(),
    )
    .await
    .unwrap();
    let work = recovered.owner_work.clone().unwrap();
    let (_fresh_dir, mut fresh, _, calls, generation) = fixture(None, None, usize::MAX).await;
    fresh.machine = recovered;
    let files = fresh.agent_files();
    let mounts = fresh.environment.host_mount_requests();
    crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
        &mut fresh.machine,
        &files,
        &mounts,
    )
    .await
    .unwrap();
    let mut ordinary = work.waiting_request();
    ordinary.request_id = "ordinary-input".into();
    let id = files
        .write_structured_input_request(&ordinary)
        .await
        .unwrap();
    assert_eq!(
        Some(&id),
        work.owned_request.as_ref(),
        "fresh AgentFS may reuse r0"
    );
    let shell = alan_shell::Shell::new(fresh.environment.root_transport());
    let options_path = format!("/agent/1/requests/{id}/options");
    let options = shell.cat(&options_path).await.unwrap();
    shell.write(&options_path, b"").await.unwrap();
    crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
        &mut fresh.machine,
        &files,
        &mounts,
    )
    .await
    .unwrap();
    assert_eq!(
        shell
            .cat(&format!("/agent/1/requests/{id}/status"))
            .await
            .unwrap(),
        b"pending"
    );
    shell.write(&options_path, &options).await.unwrap();
    crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
        &mut fresh.machine,
        &files,
        &mounts,
    )
    .await
    .unwrap();
    assert_eq!(
        shell
            .cat(&format!("/agent/1/requests/{id}/status"))
            .await
            .unwrap(),
        b"pending"
    );
    fresh
        .machine
        .set_structured_input_for_request(&id, ordinary);
    let response = Submission::new(Op::Resume {
        request_id: id.clone(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            json!({"owner":"hostfs"}),
        )],
    });
    assert!(
        !handle(&mut fresh, &response, &mut emit, &CancellationToken::new())
            .await
            .unwrap()
    );
    shell
        .write(
            &format!("/agent/1/requests/{id}/response"),
            br#"{"owner":"hostfs"}"#,
        )
        .await
        .unwrap();
    let broker = TurnInputBroker::from_queue(fresh.machine.input_queue());
    let result =
        advance_accepted_submission(&mut fresh, response, &broker, &CancellationToken::new()).await;
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert!(!fresh.machine.has_pending_interaction());
    assert_eq!(fresh.machine.owner_work, Some(work));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        generation.recorded_requests().len(),
        1,
        "only the later ordinary interaction generates"
    );
}
