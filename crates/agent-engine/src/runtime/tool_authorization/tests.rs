use super::*;
use crate::agent_machine::AgentMachine;
use crate::config::GuardianConfig;
use crate::policy::PolicyEngine;
use crate::runtime::model_binding::{CallableIdentity, CapturedCallable, ConnectionAuthority};
use crate::runtime::transition::NamespaceRuntimeEnvironment;
use crate::tools::{ToolExecutionAuthority, ToolExecutionBinding, ToolProcessRunner, ToolRegistry};
use alan_agent_protocol::{GovernanceConfig, GovernanceProfile, ToolCapability};
use alan_ap::InProcessTransport;
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Clone)]
struct MockReviewProvider {
    calls: Arc<AtomicUsize>,
    response: std::result::Result<String, String>,
    delay_ms: u64,
    bump_revision: Option<Arc<AtomicU64>>,
    require_no_tools: bool,
}

#[async_trait::async_trait]
impl LlmProvider for MockReviewProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.require_no_tools {
            assert!(
                request.tools.is_empty(),
                "Guardian review request must never carry tool definitions"
            );
        }
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }
        if let Some(rev) = &self.bump_revision {
            rev.fetch_add(1, Ordering::SeqCst);
        }
        match &self.response {
            Ok(content) => Ok(GenerationResponse {
                content: content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            }),
            Err(err) => anyhow::bail!("{err}"),
        }
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        anyhow::bail!("not used")
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let resp = self.generate(request).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let _ = tx
            .send(StreamChunk {
                text: Some(resp.content),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: None,
                provider_response_id: None,
                provider_response_status: None,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some("stop".to_string()),
            })
            .await;
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "openai_responses"
    }
}

struct MockConnectionAuthority {
    guardian: std::result::Result<CapturedCallable, String>,
}

#[async_trait::async_trait]
impl ConnectionAuthority for MockConnectionAuthority {
    async fn capture(&self, _model: Option<&str>) -> anyhow::Result<CapturedCallable> {
        anyhow::bail!("unused")
    }

    async fn restore(&self, _identity: &CallableIdentity) -> anyhow::Result<CapturedCallable> {
        anyhow::bail!("unused")
    }

    async fn catalog(&self) -> anyhow::Result<serde_json::Value> {
        Ok(json!({}))
    }

    async fn capture_guardian(&self, profile: &str) -> anyhow::Result<CapturedCallable> {
        match &self.guardian {
            Ok(captured) if captured.identity.profile == profile => Ok(captured.clone()),
            Ok(_) => anyhow::bail!("guardian profile mismatch"),
            Err(err) => anyhow::bail!("{err}"),
        }
    }
}

#[derive(Debug)]
struct RevisionTrackingAuthority {
    revision: Arc<AtomicU64>,
}

impl ToolExecutionAuthority for RevisionTrackingAuthority {
    fn reconcile(
        &self,
        _pid: u64,
        binding: ToolExecutionBinding,
    ) -> anyhow::Result<ToolExecutionBinding> {
        Ok(binding)
    }

    fn authority_revision(&self, _pid: u64) -> Option<u64> {
        Some(self.revision.load(Ordering::SeqCst))
    }
}

fn build_captured_guardian(profile: &str, provider: MockReviewProvider) -> CapturedCallable {
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection_profile(
        profile,
        alan_llmfs::ConnectionProfile::new("openai_responses", "guardian-model", "cred-g"),
        Box::new(provider),
    );
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    CapturedCallable {
        identity: CallableIdentity {
            profile: profile.into(),
            provider: "mock".into(),
            model: "guardian-model".into(),
            credential_ref: Some("cred-g".into()),
            revision: "rev-1".into(),
        },
        root,
        connection: profile.into(),
        config: crate::Config::default(),
    }
}

async fn setup_env(
    main_provider: MockReviewProvider,
    authority: Option<Arc<dyn ConnectionAuthority>>,
    revision: Arc<AtomicU64>,
) -> (NamespaceRuntimeEnvironment, AgentMachine) {
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection_profile(
        "default",
        alan_llmfs::ConnectionProfile::new("openai_responses", "main-model", "cred-m"),
        Box::new(main_provider),
    );
    let procfs = Arc::new(alan_kernel::ProcFs::new());
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(procfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/bin/agent",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        alan_kernel::Access::ReadOnly,
    );
    let mount_fs = Arc::new(alan_kernel::MountFs::new(ns));
    let root = InProcessTransport::new(mount_fs);
    let shell = alan_shell::Shell::new(root.clone());
    let pid_str = shell
        .spawn(r#"{"executable":"/bin/agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#)
        .await
        .unwrap();
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    agent_root.bind_process(pid_str.clone(), agentfs).await;
    let pid: u64 = pid_str.parse().unwrap();

    let registry = ToolRegistry::with_config(Arc::new(crate::Config::default()));
    let runner = ToolProcessRunner::from_registry(&registry);
    runner.register_process_binding(
        pid,
        ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), std::env::temp_dir()),
    );
    runner.register_process_authority(pid, Arc::new(RevisionTrackingAuthority { revision }));

    let mut env = NamespaceRuntimeEnvironment::new(root, format!("/agent/{pid}"), "default")
        .with_tool_process_context(pid, runner);
    if let Some(auth) = authority {
        env = env.with_connection_authority(auth);
    }
    let machine = AgentMachine::new();
    (env, machine)
}

#[tokio::test]
async fn dedicated_guardian_profile_runs_without_touching_main_model() {
    let main_calls = Arc::new(AtomicUsize::new(0));
    let guardian_calls = Arc::new(AtomicUsize::new(0));
    let rev = Arc::new(AtomicU64::new(1));

    let guardian = build_captured_guardian(
        "guardian-sec",
        MockReviewProvider {
            calls: guardian_calls.clone(),
            response: Ok("{\"decision\":\"allow\",\"rationale\":\"bounded test command\"}".into()),
            delay_ms: 0,
            bump_revision: None,
            require_no_tools: true,
        },
    );
    let authority = Arc::new(MockConnectionAuthority {
        guardian: Ok(guardian),
    });
    let (env, mut machine) = setup_env(
        MockReviewProvider {
            calls: main_calls.clone(),
            response: Ok("{\"decision\":\"deny\",\"rationale\":\"unused\"}".into()),
            delay_ms: 0,
            bump_revision: None,
            require_no_tools: true,
        },
        Some(authority),
        rev,
    )
    .await;

    let policy = PolicyEngine::autonomous();
    let governance = GovernanceConfig {
        profile: GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let cancel = CancellationToken::new();
    let args = json!({"command": "cargo test"});
    let call = NormalizedToolCall {
        id: "call_g_1".into(),
        name: "bash".into(),
        arguments: args.clone(),
    };
    let mut emit = |_event: Event| async {};

    let runtime = ToolAuthorizationRuntime::new(
        &mut machine,
        &policy,
        &governance,
        env.generation(),
        env.agent_files(),
        30,
    )
    .with_guardian_profile(Some("guardian-sec"))
    .with_authority(env.tool_authorization());

    let outcome = authorize_tool_call(
        runtime,
        ToolAuthorizationRequest {
            explicit_command: false,
            tool_call: &call,
            tool_arguments: &args,
            tool_capability: ToolCapability::Unknown,
            current_tool_cwd: None,
            allow_approved_tool_escalation_execution: false,
            cancel: &cancel,
        },
        &mut emit,
    )
    .await
    .unwrap();

    if cfg!(target_os = "macos") {
        assert!(matches!(
            outcome,
            ToolAuthorizationOutcome::Authorized { .. }
        ));
        assert_eq!(guardian_calls.load(Ordering::SeqCst), 1);
        assert_eq!(main_calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn dedicated_guardian_outage_uses_bounded_fallback_then_escalates_to_human() {
    let main_calls = Arc::new(AtomicUsize::new(0));
    let rev = Arc::new(AtomicU64::new(1));
    let authority = Arc::new(MockConnectionAuthority {
        guardian: Err("dedicated guardian connection down".into()),
    });
    let (env, mut machine) = setup_env(
        MockReviewProvider {
            calls: main_calls.clone(),
            response: Ok("{\"decision\":\"allow\",\"rationale\":\"ok via fallback\"}".into()),
            delay_ms: 0,
            bump_revision: None,
            require_no_tools: true,
        },
        Some(authority),
        rev,
    )
    .await;

    let policy = PolicyEngine::autonomous();
    let governance = GovernanceConfig {
        profile: GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let cancel = CancellationToken::new();
    let guardian_cfg = GuardianConfig {
        policy_path: None,
        timeout_secs: 10,
        max_fallbacks_per_turn: 1,
    };

    if cfg!(target_os = "macos") {
        let mut emit = |_event: Event| async {};
        let args = json!({"command": "cargo check"});
        let call1 = NormalizedToolCall {
            id: "call_fb_1".into(),
            name: "bash".into(),
            arguments: args.clone(),
        };
        // First escalation uses the 1 fallback slot -> Authorized.
        let out1 = authorize_tool_call(
            ToolAuthorizationRuntime::new(
                &mut machine,
                &policy,
                &governance,
                env.generation(),
                env.agent_files(),
                30,
            )
            .with_guardian_profile(Some("guardian-sec"))
            .with_guardian_config(guardian_cfg.clone())
            .with_authority(env.tool_authorization()),
            ToolAuthorizationRequest {
                explicit_command: false,
                tool_call: &call1,
                tool_arguments: &args,
                tool_capability: ToolCapability::Unknown,
                current_tool_cwd: None,
                allow_approved_tool_escalation_execution: false,
                cancel: &cancel,
            },
            &mut emit,
        )
        .await
        .unwrap();
        assert!(matches!(out1, ToolAuthorizationOutcome::Authorized { .. }));
        assert_eq!(main_calls.load(Ordering::SeqCst), 1);

        // Second escalation in the same turn finds fallback budget exhausted -> PauseTurn (human).
        let call2 = NormalizedToolCall {
            id: "call_fb_2".into(),
            name: "bash".into(),
            arguments: args.clone(),
        };
        let out2 = authorize_tool_call(
            ToolAuthorizationRuntime::new(
                &mut machine,
                &policy,
                &governance,
                env.generation(),
                env.agent_files(),
                30,
            )
            .with_guardian_profile(Some("guardian-sec"))
            .with_guardian_config(guardian_cfg)
            .with_authority(env.tool_authorization()),
            ToolAuthorizationRequest {
                explicit_command: false,
                tool_call: &call2,
                tool_arguments: &args,
                tool_capability: ToolCapability::Unknown,
                current_tool_cwd: None,
                allow_approved_tool_escalation_execution: false,
                cancel: &cancel,
            },
            &mut emit,
        )
        .await
        .unwrap();
        assert!(matches!(out2, ToolAuthorizationOutcome::PauseTurn));
        assert_eq!(main_calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn guardian_approval_is_revoked_when_authority_changes_in_flight() {
    let rev = Arc::new(AtomicU64::new(10));
    let guardian_calls = Arc::new(AtomicUsize::new(0));
    let guardian = build_captured_guardian(
        "guardian-sec",
        MockReviewProvider {
            calls: guardian_calls.clone(),
            response: Ok("{\"decision\":\"allow\",\"rationale\":\"ok\"}".into()),
            delay_ms: 0,
            bump_revision: Some(rev.clone()),
            require_no_tools: true,
        },
    );
    let authority = Arc::new(MockConnectionAuthority {
        guardian: Ok(guardian),
    });
    let (env, mut machine) = setup_env(
        MockReviewProvider {
            calls: Arc::new(AtomicUsize::new(0)),
            response: Ok("{\"decision\":\"allow\",\"rationale\":\"unused\"}".into()),
            delay_ms: 0,
            bump_revision: None,
            require_no_tools: true,
        },
        Some(authority),
        rev,
    )
    .await;

    if cfg!(target_os = "macos") {
        let policy = PolicyEngine::autonomous();
        let governance = GovernanceConfig {
            profile: GovernanceProfile::Autonomous,
            policy_path: None,
        };
        let cancel = CancellationToken::new();
        let args = json!({"command": "cargo test"});
        let call = NormalizedToolCall {
            id: "call_stale_1".into(),
            name: "bash".into(),
            arguments: args.clone(),
        };
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_ref = events.clone();
        let mut emit = move |event: Event| {
            events_ref.lock().unwrap().push(event);
            async {}
        };

        let outcome = authorize_tool_call(
            ToolAuthorizationRuntime::new(
                &mut machine,
                &policy,
                &governance,
                env.generation(),
                env.agent_files(),
                30,
            )
            .with_guardian_profile(Some("guardian-sec"))
            .with_authority(env.tool_authorization()),
            ToolAuthorizationRequest {
                explicit_command: false,
                tool_call: &call,
                tool_arguments: &args,
                tool_capability: ToolCapability::Unknown,
                current_tool_cwd: None,
                allow_approved_tool_escalation_execution: false,
                cancel: &cancel,
            },
            &mut emit,
        )
        .await
        .unwrap();

        assert!(
            matches!(outcome, ToolAuthorizationOutcome::PauseTurn),
            "stale Guardian allow after grant/authority change must pause for human"
        );
        assert!(events.lock().unwrap().iter().any(|ev| {
            matches!(ev, Event::Warning { message } if message.contains("invalidated"))
        }));
    }
}

#[test]
fn custom_guardian_policy_loads_bounded_regular_file_and_rejects_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let policy_file = dir.path().join("guardian.md");
    std::fs::write(&policy_file, "Custom Guardian Rule: deny exfil").unwrap();
    assert_eq!(
        resolve_guardian_policy(Some(&policy_file)),
        "Custom Guardian Rule: deny exfil"
    );

    #[cfg(unix)]
    {
        std::fs::write(outside.path().join("ext.md"), "EXTERNAL").unwrap();
        let symlink_path = dir.path().join("link.md");
        std::os::unix::fs::symlink(outside.path().join("ext.md"), &symlink_path).unwrap();
        assert_eq!(
            resolve_guardian_policy(Some(&symlink_path)),
            super::super::guardian::DEFAULT_REVIEWER_POLICY
        );
    }

    let oversized = dir.path().join("big.md");
    std::fs::write(&oversized, "x".repeat(65 * 1024)).unwrap();
    assert_eq!(
        resolve_guardian_policy(Some(&oversized)),
        super::super::guardian::DEFAULT_REVIEWER_POLICY
    );
}

#[tokio::test]
async fn stale_pending_confirmation_cannot_stack_duplicate_escalations() {
    let rev = Arc::new(AtomicU64::new(1));
    let (env, mut machine) = setup_env(
        MockReviewProvider {
            calls: Arc::new(AtomicUsize::new(0)),
            response: Ok("{\"decision\":\"deny\",\"rationale\":\"unused\"}".into()),
            delay_ms: 0,
            bump_revision: None,
            require_no_tools: true,
        },
        None,
        rev,
    )
    .await;

    machine.set_confirmation_for_request(
        "existing-req".to_string(),
        crate::approval::PendingConfirmation {
            checkpoint_id: "tool_escalation_existing".to_string(),
            checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
            summary: "already waiting".to_string(),
            details: json!({}),
            options: vec!["approve".into(), "reject".into()],
        },
    );

    let policy = PolicyEngine::autonomous();
    let governance = GovernanceConfig {
        profile: GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let cancel = CancellationToken::new();
    let args = json!({"command": "sudo ls"});
    let call = NormalizedToolCall {
        id: "call_dup_1".into(),
        name: "bash".into(),
        arguments: args.clone(),
    };
    let mut emit = |_event: Event| async {};

    let outcome = authorize_tool_call(
        ToolAuthorizationRuntime::new(
            &mut machine,
            &policy,
            &governance,
            env.generation(),
            env.agent_files(),
            30,
        )
        .with_authority(env.tool_authorization()),
        ToolAuthorizationRequest {
            explicit_command: true,
            tool_call: &call,
            tool_arguments: &args,
            tool_capability: ToolCapability::Unknown,
            current_tool_cwd: None,
            allow_approved_tool_escalation_execution: false,
            cancel: &cancel,
        },
        &mut emit,
    )
    .await
    .unwrap();

    assert!(matches!(outcome, ToolAuthorizationOutcome::PauseTurn));
    assert_eq!(
        machine.pending_confirmation().unwrap().checkpoint_id,
        "tool_escalation_existing"
    );
}

#[tokio::test]
async fn guardian_aggregate_timeout_expires_cleanly_without_auto_allow() {
    let rev = Arc::new(AtomicU64::new(1));
    let guardian_calls = Arc::new(AtomicUsize::new(0));
    let main_calls = Arc::new(AtomicUsize::new(0));
    let guardian = build_captured_guardian(
        "guardian-sec",
        MockReviewProvider {
            calls: guardian_calls.clone(),
            response: Ok("{\"decision\":\"allow\",\"rationale\":\"too late\"}".into()),
            delay_ms: 2500,
            bump_revision: None,
            require_no_tools: true,
        },
    );
    let authority = Arc::new(MockConnectionAuthority {
        guardian: Ok(guardian),
    });
    let (env, mut machine) = setup_env(
        MockReviewProvider {
            calls: main_calls.clone(),
            response: Ok("{\"decision\":\"allow\",\"rationale\":\"too late\"}".into()),
            delay_ms: 2500,
            bump_revision: None,
            require_no_tools: true,
        },
        Some(authority),
        rev,
    )
    .await;

    if cfg!(target_os = "macos") {
        let policy = PolicyEngine::autonomous();
        let governance = GovernanceConfig {
            profile: GovernanceProfile::Autonomous,
            policy_path: None,
        };
        let cancel = CancellationToken::new();
        let args = json!({"command": "cargo test"});
        let call = NormalizedToolCall {
            id: "call_timeout_1".into(),
            name: "bash".into(),
            arguments: args.clone(),
        };
        let mut emit = |_event: Event| async {};

        let outcome = authorize_tool_call(
            ToolAuthorizationRuntime::new(
                &mut machine,
                &policy,
                &governance,
                env.generation(),
                env.agent_files(),
                30,
            )
            .with_guardian_profile(Some("guardian-sec"))
            .with_guardian_config(GuardianConfig {
                policy_path: None,
                timeout_secs: 1,
                max_fallbacks_per_turn: 2,
            })
            .with_authority(env.tool_authorization()),
            ToolAuthorizationRequest {
                explicit_command: false,
                tool_call: &call,
                tool_arguments: &args,
                tool_capability: ToolCapability::Unknown,
                current_tool_cwd: None,
                allow_approved_tool_escalation_execution: false,
                cancel: &cancel,
            },
            &mut emit,
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ToolAuthorizationOutcome::PauseTurn));
        assert_eq!(guardian_calls.load(Ordering::SeqCst), 1);
        assert_eq!(main_calls.load(Ordering::SeqCst), 0);
    }
}
