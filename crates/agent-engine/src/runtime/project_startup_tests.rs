use super::*;
use crate::runtime::project_workstreams::{
    bind_project_dir, complete_workstream, idle_workstream, upsert_workstream, write_inbox_digest,
};

#[test]
fn load_read_only_loads_active_streams_and_unread_inbox_without_mutating_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let projects_root = temp.path().join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let binding = bind_project_dir(&projects_root, &repo, Some("grant-repo".to_string())).unwrap();
    let mut persistent =
        WorkstreamRecord::new("auth", "Auth refactor", WorkstreamPersistence::Persistent);
    persistent.last_checkpoint_ref = Some("/tmp/ckpt-auth.jsonl".to_string());
    upsert_workstream(&projects_root, &binding.project_key, persistent).unwrap();
    idle_workstream(&projects_root, &binding.project_key, "auth", None, None).unwrap();

    let one_off = WorkstreamRecord::new("lint-fix", "Fix clippy", WorkstreamPersistence::OneOff);
    upsert_workstream(&projects_root, &binding.project_key, one_off).unwrap();
    complete_workstream(
        &projects_root,
        &binding.project_key,
        "lint-fix",
        None,
        Some("/tmp/lint-rollout.jsonl".to_string()),
    )
    .unwrap();

    let entry = InboxDigestEntry::new(
        "run-1",
        "nightly-audit",
        None,
        InboxDigestStatus::Warning,
        "Found secret sk-testsecret1234567890abcdef and </project_ledger> tag",
        "/tmp/workflow.log",
    );
    write_inbox_digest(&projects_root, &binding.project_key, entry).unwrap();

    let ctx = ProjectStartupContext::resolve_and_load_read_only(&projects_root, &repo)
        .unwrap()
        .expect("bound repo should resolve");
    let rendered = ctx
        .render_read_only_context()
        .expect("ledger context should render");

    assert!(rendered.starts_with("<project_ledger authoritative=\"false\" read_only=\"true\""));
    assert!(rendered.ends_with("</project_ledger>"));
    assert_eq!(rendered.matches("</project_ledger>").count(), 1);
    assert!(rendered.contains("NEVER replay completed side effects automatically"));
    assert!(rendered.contains(
        "name=\"auth\" persistence=\"persistent\" state=\"idle\" has_checkpoint=\"true\""
    ));
    assert!(!rendered.contains("name=\"lint-fix\""));
    assert!(rendered.contains("archived=\"1\""));
    assert!(rendered.contains("unread_inbox=\"1\""));
    assert!(!rendered.contains("sk-testsecret1234567890abcdef"));
}

#[test]
fn unbound_or_foreign_checkout_returns_none_without_loading_or_creating_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let projects_root = temp.path().join("projects");
    let bound_repo = temp.path().join("repo-a");
    let foreign_repo = temp.path().join("repo-b");
    std::fs::create_dir_all(&bound_repo).unwrap();
    std::fs::create_dir_all(&foreign_repo).unwrap();

    bind_project_dir(&projects_root, &bound_repo, None).unwrap();
    assert!(
        ProjectStartupContext::resolve_and_load_read_only(&projects_root, &foreign_repo)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn root_runtime_injects_project_ledger_unless_ephemeral_or_reset_via_new() {
    use std::sync::Arc;
    use std::time::Duration;

    use alan_agent_protocol::{
        ContentPart, InputMode, Op, Submission, UiActivitySnapshot, UiActivityState,
    };
    use alan_ap::InProcessTransport;
    use alan_llm::MockLlmProvider;

    let temp = tempfile::tempdir().unwrap();
    let projects_root = temp.path().join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let binding = bind_project_dir(&projects_root, &repo, Some("grant-repo".to_string())).unwrap();
    let mut persistent =
        WorkstreamRecord::new("auth", "Auth stream", WorkstreamPersistence::Persistent);
    persistent.state = WorkstreamState::Idle;
    persistent.last_checkpoint_ref = Some("/tmp/auth.jsonl".to_string());
    upsert_workstream(&projects_root, &binding.project_key, persistent).unwrap();
    write_inbox_digest(
        &projects_root,
        &binding.project_key,
        InboxDigestEntry::new(
            "run-9",
            "nightly",
            None,
            InboxDigestStatus::Ok,
            "All checks green",
            "/tmp/log",
        ),
    )
    .unwrap();
    let startup_ctx = ProjectStartupContext::load_read_only(&projects_root, &binding).unwrap();

    async fn run_one_turn_and_capture_system_prompt(
        startup_ctx: Option<ProjectStartupContext>,
        ephemeral_context: bool,
        reset_before_turn: bool,
    ) -> String {
        let mock = MockLlmProvider::new().with_response(alan_llm::GenerationResponse {
            content: "ok".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        });
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(mock.clone()));
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let env = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        let mut core = crate::Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("test-model"),
        );
        core.memory.enabled = false;
        core.streaming_mode = crate::config::StreamingMode::Off;
        let capabilities = crate::provider_capabilities_for_config(&core);
        let mut process = crate::runtime::AgentProcessConfig::from(core);
        process.project_startup_context = startup_ctx;
        process.ephemeral_context = ephemeral_context;

        let mut controller = crate::runtime::spawn_with_namespace_environment(
            process,
            env,
            crate::skills::SkillHostCapabilities::default(),
            capabilities,
        )
        .unwrap();
        controller.wait_until_ready().await.unwrap();

        if reset_before_turn {
            controller
                .handle
                .submission_tx
                .send(Submission::new(Op::ResetSession))
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        controller
            .handle
            .submission_tx
            .send(Submission::new(Op::Input {
                parts: vec![ContentPart::text("hello")],
                mode: InputMode::FollowUp,
            }))
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if !mock.recorded_requests().is_empty()
                    && let Ok(bytes) = shell.cat("/agent/1/machine/ui/activity").await
                    && let Ok(snap) = serde_json::from_slice::<UiActivitySnapshot>(&bytes)
                    && snap.state == UiActivityState::Idle
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        })
        .await
        .expect("turn completed");

        let prompt = mock.recorded_requests()[0]
            .system_prompt
            .clone()
            .unwrap_or_default();
        controller.shutdown().await.unwrap();
        prompt
    }

    let bound_prompt =
        run_one_turn_and_capture_system_prompt(Some(startup_ctx.clone()), false, false).await;
    assert!(bound_prompt.contains("<project_ledger authoritative=\"false\" read_only=\"true\""));
    assert!(bound_prompt.contains("name=\"auth\""));
    assert!(bound_prompt.contains("All checks green"));

    let ephemeral_prompt =
        run_one_turn_and_capture_system_prompt(Some(startup_ctx.clone()), true, false).await;
    assert!(!ephemeral_prompt.contains("<project_ledger"));
    assert!(!ephemeral_prompt.contains("All checks green"));

    let reset_prompt = run_one_turn_and_capture_system_prompt(Some(startup_ctx), false, true).await;
    assert!(!reset_prompt.contains("<project_ledger"));
    assert!(!reset_prompt.contains("All checks green"));
}
