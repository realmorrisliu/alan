use super::*;
use alan_ap::{ErrorCode, FileKind, InProcessTransport, Qid, Stat};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountTapeOpens {
    inner: alan_agentfs::AgentFs,
    tape_path: u64,
    writes: AtomicUsize,
}

#[async_trait::async_trait]
impl FileServer for CountTapeOpens {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.inner.walk(fid, newfid, names).await
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let qid = self.inner.open(fid, mode).await?;
        if qid.path == self.tape_path && mode == OpenMode::Write {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
        Ok(qid)
    }
    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(&self, fid: Fid, offset: u64, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid).await
    }
}

#[tokio::test]
async fn approved_replay_and_resumed_generation_share_one_tape_lease() {
    for batch in [false, true] {
        let inner = alan_agentfs::AgentFs::new();
        let tape_path = inner
            .walk(Fid::ROOT, Fid(9), &["machine".into(), "tape".into()])
            .await
            .unwrap()
            .path;
        inner.clunk(Fid(9)).await.unwrap();
        let agentfs = Arc::new(CountTapeOpens {
            inner,
            tape_path,
            writes: AtomicUsize::new(0),
        });
        let procfs = Arc::new(alan_kernel::ProcFs::new());
        spawn_test_process(&procfs).await;
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(DelayedMockProvider::new(
                tokio::time::Duration::ZERO,
                "resumed answer",
            )),
        );
        let mut ns = alan_kernel::Namespace::new();
        for (path, server) in [
            ("/agent/1", InProcessTransport::new(agentfs.clone())),
            ("/proc", InProcessTransport::new(procfs)),
            ("/mnt/llm", InProcessTransport::new(llmfs)),
        ] {
            ns.mount(path, server, alan_kernel::Access::ReadWrite);
        }
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let mut state = runtime_state_with_environment(NamespaceRuntimeEnvironment::new(
            root.clone(),
            "/agent/1",
            "default",
        ));
        state.core_config.memory.enabled = false;
        state.machine.add_user_message("task");
        state.machine.begin_turn(0);
        let call = NormalizedToolCall {
            id: "call-1".into(),
            name: "update_plan".into(),
            arguments: json!({"items":[{"id":"1","content":"Done","status":"completed"}]}),
        };
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "approval".into(),
            checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.into(),
            summary: "approve".into(),
            details: json!({"replay_tool_call": {
                "call_id":call.id,"tool_name":call.name,"arguments":call.arguments,
            }}),
            options: vec!["approve".into()],
        });
        if batch {
            state
                .machine
                .set_tool_replay_batch("approval", vec![call], true);
        }
        state.machine.accept_submission("originating-input");
        let broker = crate::runtime::turn_input::TurnInputBroker::default();
        let compact = advance_accepted_submission(
            &mut state,
            Submission::new(Op::CompactWithOptions { focus: None }),
            &broker,
            &CancellationToken::new(),
        )
        .await
        .result
        .unwrap_err();
        assert!(compact.to_string().contains("pending interaction"));
        assert_eq!(
            state.machine.current_submission_id(),
            Some("originating-input")
        );
        let external = state.agent_files().begin_tape_generation().await.unwrap();
        let messages_before = state.machine.messages().len();
        let blocked = advance_accepted_submission(
            &mut state,
            Submission::new(Op::Resume {
                request_id: "approval".into(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    json!({"choice":"approve"}),
                )],
            }),
            &broker,
            &CancellationToken::new(),
        )
        .await
        .result;
        assert!(blocked.is_err());
        assert_eq!(
            state.machine.current_submission_id(),
            Some("originating-input")
        );
        assert!(
            state.machine.pending_confirmation().is_some(),
            "busy Tape must preserve approval"
        );
        assert_eq!(state.machine.messages().len(), messages_before);
        let shell = Shell::new(root.clone());
        let events = shell.cat("/agent/1/machine/ui/events").await.unwrap();
        assert!(
            !String::from_utf8(events).unwrap().lines().any(|line| {
                matches!(
                    serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
                    alan_agent_protocol::UiEvent::InputCompleted { .. }
                )
            }),
            "retryable pending input must not be settled as failed"
        );
        external.finish().await.unwrap();
        agentfs.writes.store(0, Ordering::SeqCst);
        advance_accepted_submission(
            &mut state,
            Submission::new(Op::Resume {
                request_id: "approval".into(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    json!({"choice":"approve"}),
                )],
            }),
            &broker,
            &CancellationToken::new(),
        )
        .await
        .result
        .unwrap();
        let tape = Shell::new(root).cat("/agent/1/machine/tape").await.unwrap();
        let tape = String::from_utf8(tape).unwrap();
        let answer: serde_json::Value = tape
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|record| record["content"] == "resumed answer")
            .unwrap();
        assert_eq!(answer["submission_id"], "originating-input");
        let events = shell.cat("/agent/1/machine/ui/events").await.unwrap();
        let completed: Vec<_> = String::from_utf8(events)
            .unwrap()
            .lines()
            .filter_map(|line| {
                match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
                    alan_agent_protocol::UiEvent::InputCompleted {
                        submission_ids,
                        status,
                        ..
                    } => Some((submission_ids, status)),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(
            completed,
            vec![(
                vec!["originating-input".to_owned()],
                alan_agent_protocol::UiInputStatus::Completed
            )]
        );
        assert_eq!(
            agentfs.writes.load(Ordering::SeqCst),
            1,
            "replay and its generation must not release/reacquire Tape"
        );
        state
            .agent_files()
            .begin_tape_generation()
            .await
            .unwrap()
            .finish()
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn busy_tape_does_not_drain_queued_next_turn_inputs() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "answer",
        ))
        .await,
    );
    state.core_config.memory.enabled = false;
    state
        .machine
        .queue_next_turn_input(vec![alan_agent_protocol::ContentPart::text("queued")]);
    let external = state.agent_files().begin_tape_generation().await.unwrap();
    let submission = Submission::new(Op::Turn {
        parts: vec![alan_agent_protocol::ContentPart::text("now")],
        context: None,
    });
    let mut emit = |_| async {};
    let cancel = CancellationToken::new();
    assert!(
        handle_submission_with_cancel(&mut state, submission.clone(), &mut emit, &cancel)
            .await
            .is_err()
    );
    assert_eq!(state.machine.queued_next_turn_input_count(), 1);
    external.finish().await.unwrap();
    handle_submission_with_cancel(&mut state, submission, &mut emit, &cancel)
        .await
        .unwrap();
    assert_eq!(state.machine.queued_next_turn_input_count(), 0);
    assert!(
        state
            .machine
            .messages()
            .iter()
            .any(|message| message.text_content().contains("queued")
                && message.text_content().contains("now"))
    );
}

#[tokio::test]
async fn non_generating_resume_does_not_require_the_tape_lease() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "must not generate",
        ))
        .await,
    );
    let call = NormalizedToolCall {
        id: "command-input".into(),
        name: "bash".into(),
        arguments: json!({"command":"echo test"}),
    };
    state.machine.set_confirmation(PendingConfirmation {
        checkpoint_id: "approval".into(),
        checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.into(),
        summary: "approve command".into(),
        details: json!({}),
        options: vec!["approve".into(), "reject".into()],
    });
    state
        .machine
        .set_tool_replay_batch("approval", vec![call], false);
    let writer = state.agent_files().begin_tape_generation().await.unwrap();
    let mut events = Vec::new();
    let mut emit = |event| {
        events.push(event);
        async {}
    };
    for request_id in ["unknown", "approval"] {
        handle_submission_with_cancel(
            &mut state,
            Submission::new(Op::Resume {
                request_id: request_id.into(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    json!({"choice":"reject"}),
                )],
            }),
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    }
    writer.finish().await.unwrap();
    assert!(!state.machine.has_pending_interaction());
    assert!(events.iter().any(
        |event| matches!(event, Event::Error { message, .. } if message.contains("does not match"))
    ));
    assert!(events.iter().any(|event| matches!(event, Event::ToolCallCompleted { id, success: Some(false), .. } if id == "command-input")));
}

#[tokio::test(start_paused = true)]
async fn manual_compaction_retains_the_tape_lease_until_generation_finishes() {
    let mut state = runtime_state_with_environment(namespace_environment_with_provider(
        DelayedMockProvider::new(tokio::time::Duration::from_secs(1), "summary"),
    ));
    state.core_config.memory.enabled = false;
    for index in 0..65 {
        state.machine.add_user_message(&format!("Message {index}"));
    }
    let files = state.agent_files();
    let cancel = CancellationToken::new();
    let mut emit = |_| async {};
    let compact = handle_submission_with_cancel(
        &mut state,
        Submission::new(Op::CompactWithOptions { focus: None }),
        &mut emit,
        &cancel,
    );
    tokio::pin!(compact);
    assert!(
        tokio::time::timeout(tokio::time::Duration::from_millis(1), &mut compact)
            .await
            .is_err()
    );
    assert!(
        files.begin_tape_generation().await.is_err(),
        "compaction excludes external amendments"
    );
    compact.await.unwrap();
    files
        .begin_tape_generation()
        .await
        .unwrap()
        .finish()
        .await
        .unwrap();
}
