use super::*;

#[tokio::test]
async fn structured_input_cancellation_settles_only_owned_service_id() {
    let mut state = create_test_state_with_provider(alan_llm::MockLlmProvider::new());
    let files = state.agent_files();
    let pending = crate::approval::PendingStructuredInputRequest {
        request_id: "logical-question-not-directory".into(),
        title: "Question".into(),
        prompt: "Choose".into(),
        questions: Vec::new(),
    };
    let owned = files
        .write_structured_input_request(&pending)
        .await
        .unwrap();
    let unrelated = files
        .write_structured_input_request(&pending)
        .await
        .unwrap();
    state
        .machine
        .set_structured_input_for_request(&owned, pending);
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    crate::runtime::turn_support::cancel_current_task(
        &mut state.machine,
        &files,
        &state.environment.host_mount_requests(),
        &mut |_event| async {},
    )
    .await
    .unwrap();
    assert!(!state.machine.has_pending_interaction());
    assert!(state.machine.submission_was_cancelled());
    assert_eq!(
        shell
            .cat(&format!(
                "{}/requests/{owned}/status",
                state.environment.agent_path()
            ))
            .await
            .unwrap(),
        b"cancelled"
    );
    assert_eq!(
        shell
            .cat(&format!(
                "{}/requests/{owned}/response",
                state.environment.agent_path()
            ))
            .await
            .unwrap(),
        b""
    );
    assert_eq!(
        shell
            .cat(&format!(
                "{}/requests/{unrelated}/status",
                state.environment.agent_path()
            ))
            .await
            .unwrap(),
        b"pending"
    );
}

// Descriptor-level fault: the owner write succeeds, but terminal evidence is
// unavailable (or remains pending). No Engine-only cancellation API is mocked.
struct CancellationEvidenceFs {
    inner: alan_agentfs::AgentFs,
    status_fids: tokio::sync::Mutex<std::collections::HashSet<alan_ap::Fid>>,
    cancelled: std::sync::atomic::AtomicBool,
    fail_read: bool,
}

#[async_trait]
impl alan_ap::FileServer for CancellationEvidenceFs {
    async fn walk(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        names: &[String],
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        if names.last().is_some_and(|name| name == "status") {
            self.status_fids.lock().await.insert(newfid);
        }
        Ok(qid)
    }
    async fn open(
        &self,
        fid: alan_ap::Fid,
        mode: alan_ap::OpenMode,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        count: u32,
    ) -> Result<Vec<u8>, alan_ap::ErrorCode> {
        if self.cancelled.load(Ordering::SeqCst) && self.status_fids.lock().await.contains(&fid) {
            if self.fail_read {
                return Err(alan_ap::ErrorCode::NoAccess);
            }
            return Ok(b"pending".to_vec());
        }
        self.inner.read(fid, offset, count).await
    }
    async fn write(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        data: &[u8],
    ) -> Result<u32, alan_ap::ErrorCode> {
        let count = self.inner.write(fid, offset, data).await?;
        if data == b"cancel" {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        Ok(count)
    }
    async fn stat(&self, fid: alan_ap::Fid) -> Result<alan_ap::Stat, alan_ap::ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        name: &str,
        kind: alan_ap::FileKind,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.inner.remove(fid).await
    }
    async fn clunk(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.status_fids.lock().await.remove(&fid);
        self.inner.clunk(fid).await
    }
}

#[tokio::test]
async fn cancellation_terminal_evidence_failure_preserves_logical_wait() {
    use alan_ap::FileServer;
    for fail_read in [true, false] {
        for confirmation in [true, false] {
            let owner = Arc::new(CancellationEvidenceFs {
                inner: alan_agentfs::AgentFs::new(),
                status_fids: Default::default(),
                cancelled: std::sync::atomic::AtomicBool::new(false),
                fail_read,
            });
            let mut namespace = alan_kernel::Namespace::new();
            namespace.mount(
                "/agent/37",
                alan_ap::InProcessTransport::new(owner.clone()),
                alan_kernel::Access::ReadWrite,
            );
            let root =
                alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
            let mut state = create_test_state_with_provider(alan_llm::MockLlmProvider::new());
            state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/37", "default");
            let files = state.agent_files();
            let pending = crate::approval::PendingStructuredInputRequest {
                request_id: "not-r0".into(),
                title: "Question".into(),
                prompt: "Choose".into(),
                questions: Vec::new(),
            };
            let id = files
                .write_structured_input_request(&pending)
                .await
                .unwrap();
            if confirmation {
                state.machine.set_confirmation_for_request(
                    &id,
                    crate::approval::PendingConfirmation {
                        checkpoint_id: "not-r0".into(),
                        checkpoint_type: "test".into(),
                        summary: "Confirm".into(),
                        details: json!({}),
                        options: vec!["approve".into(), "reject".into()],
                    },
                );
            } else {
                state.machine.set_structured_input_for_request(&id, pending);
            }
            state.machine.set_turn_activity(TurnActivityState::Paused);
            let mut events = Vec::new();
            let result = crate::runtime::turn_support::cancel_current_task(
                &mut state.machine,
                &files,
                &state.environment.host_mount_requests(),
                &mut |event| {
                    events.push(event);
                    async {}
                },
            )
            .await;
            assert!(result.is_err());
            assert!(
                owner.cancelled.load(Ordering::SeqCst),
                "owner operation succeeded before evidence failure"
            );
            assert!(state.machine.pending_yield(&id).is_some());
            assert!(!state.machine.submission_was_cancelled());
            assert!(events.is_empty());
            assert!(files.action_ids().await.unwrap().is_empty());
            // Independent direct owner read proves cancellation happened, while
            // the Engine must still retain its association until evidence works.
            owner
                .inner
                .walk(
                    alan_ap::Fid::ROOT,
                    alan_ap::Fid(900),
                    &["requests".into(), id, "status".into()],
                )
                .await
                .unwrap();
            owner
                .inner
                .open(alan_ap::Fid(900), alan_ap::OpenMode::Read)
                .await
                .unwrap();
            assert_eq!(
                owner.inner.read(alan_ap::Fid(900), 0, 100).await.unwrap(),
                b"cancelled"
            );
        }
    }
}

#[tokio::test]
async fn test_run_turn_confirmation_includes_active_skill_permission_hints() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let skill_dir = definition_root.join("skills/release-check");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("skill.yaml"),
        r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
    )
    .unwrap();

    let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "checkpoint_type": "test",
                "summary": "Confirm risky action"
            }),
        }],
        "",
    ));
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
    let cancel = CancellationToken::new();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text(
            "please use $release-check for this task",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let confirmation = events.into_iter().find_map(|event| match event {
        Event::Yield {
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload,
            ..
        } => Some(payload),
        _ => None,
    });
    let confirmation = confirmation.expect("expected confirmation yield");
    let hints = confirmation["details"]["skill_permission_hints"]
        .as_array()
        .cloned()
        .unwrap();

    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0]["skill_id"], "release-check");
    assert_eq!(
        hints[0]["permission_hints"][0],
        "May require write approval."
    );
}
