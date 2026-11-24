use std::sync::{Arc, Mutex};

use alan_agent_protocol::{Event, Op, YieldKind};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_llm::{GenerationResponse, MockLlmProvider};
use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{
    agent_machine::{AgentMachine, NormalizedToolCall, PendingYield},
    approval::SERVICE_ACTIVATION_CHECKPOINT_TYPE,
    config::Config,
    policy::TrustPosture,
    runtime::{
        RuntimeConfig,
        loop_guard::ToolLoopGuard,
        service_activation::{
            ActivatedServiceMount, AvailableFileserverPackage, ServiceActivationAuthority,
            ServiceActivationRequest, ServiceMountAccess, render_service_activation_context,
        },
        submission_handlers::{
            RuntimeOpAction, SubmissionRuntime, handle_non_compaction_runtime_op,
        },
        tool_batch::{ToolBatchOrchestratorOutcome, ToolOrchestratorInputs},
        transition::{
            NamespaceRuntimeEnvironment, RuntimeLoopState, TurnRunKind, dispatch_virtual_tool_call,
            orchestrate_tool_batch, run_turn_with_cancel,
        },
        virtual_tool::VirtualToolOutcome,
        virtual_tools::virtual_tool_definitions,
    },
    tape::ContentPart,
};

#[derive(Default)]
struct MockServiceAuthority {
    packages: Mutex<Vec<AvailableFileserverPackage>>,
    active_grants: Mutex<std::collections::HashMap<String, (ServiceMountAccess, bool)>>,
    activations: Mutex<Vec<(ServiceActivationRequest, bool)>>,
    fail_activation: Mutex<Option<String>>,
}

impl MockServiceAuthority {
    fn with_github_inactive() -> Arc<Self> {
        Arc::new(Self {
            packages: Mutex::new(vec![AvailableFileserverPackage {
                package_id: "github-fileserver".to_string(),
                service_id: "github".to_string(),
                mount_path: "/mnt/github".to_string(),
                description: Some("GitHub aP fileserver".to_string()),
                allow_network: true,
                network_domains: vec!["api.github.com".to_string(), "github.com".to_string()],
                active: false,
                active_access: None,
                active_allow_network: false,
            }]),
            ..Default::default()
        })
    }

    fn set_active(&self, service_id: &str, access: ServiceMountAccess, allow_network: bool) {
        let mut pkgs = self.packages.lock().unwrap();
        for pkg in pkgs.iter_mut() {
            if pkg.service_id == service_id {
                pkg.active = true;
                pkg.active_access = Some(access);
                pkg.active_allow_network = allow_network;
            }
        }
        self.active_grants
            .lock()
            .unwrap()
            .insert(service_id.to_string(), (access, allow_network));
    }
}

#[async_trait]
impl ServiceActivationAuthority for MockServiceAuthority {
    fn list_available_fileservers(&self) -> Vec<AvailableFileserverPackage> {
        self.packages.lock().unwrap().clone()
    }

    fn inspect_request(
        &self,
        request: &ServiceActivationRequest,
    ) -> Result<AvailableFileserverPackage, String> {
        let pkg = self
            .packages
            .lock()
            .unwrap()
            .iter()
            .find(|pkg| pkg.package_id == request.package_id)
            .cloned()
            .ok_or_else(|| format!("package `{}` is not installed", request.package_id))?;
        if request.allow_network && !pkg.allow_network {
            return Err(format!(
                "fileserver package `{}` does not allow network access",
                pkg.package_id
            ));
        }
        Ok(pkg)
    }

    fn active_service_grant(&self, service_id: &str) -> Option<(ServiceMountAccess, bool)> {
        self.active_grants.lock().unwrap().get(service_id).cloned()
    }

    async fn activate_fileserver(
        &self,
        request: &ServiceActivationRequest,
        persist_for_root: bool,
    ) -> Result<ActivatedServiceMount, String> {
        if let Some(err) = self.fail_activation.lock().unwrap().clone() {
            return Err(err);
        }
        let pkg = self.inspect_request(request)?;
        self.activations
            .lock()
            .unwrap()
            .push((request.clone(), persist_for_root));
        self.set_active(&pkg.service_id, request.access, request.allow_network);
        Ok(ActivatedServiceMount {
            package_id: pkg.package_id,
            service_id: pkg.service_id,
            mount_path: pkg.mount_path,
            description: pkg.description.unwrap_or_default(),
            access: request.access,
            allow_network: request.allow_network,
            persisted: persist_for_root,
        })
    }
}

fn create_state(authority: Option<Arc<dyn ServiceActivationAuthority>>) -> RuntimeLoopState {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let mut env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    if let Some(auth) = authority {
        env = env.with_service_activation_authority(auth);
    }
    RuntimeLoopState {
        machine: AgentMachine::new(),
        environment: env,
        core_config: Config::default(),
        runtime_config: RuntimeConfig::default(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    }
}

#[tokio::test]
async fn request_service_invalid_or_missing_package_returns_error_without_prompt() {
    let authority = MockServiceAuthority::with_github_inactive();
    let mut state = create_state(Some(authority));
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    for (id, args) in [
        (
            "call_missing",
            json!({"package_id": "nope", "reason": "need it"}),
        ),
        (
            "call_empty_reason",
            json!({"package_id": "github-fileserver", "reason": "   "}),
        ),
        (
            "call_extra",
            json!({"package_id": "github-fileserver", "reason": "ok", "extra": 1}),
        ),
        (
            "call_bad_access",
            json!({"package_id": "github-fileserver", "access": "admin", "reason": "ok"}),
        ),
    ] {
        let tc = NormalizedToolCall {
            id: id.to_string(),
            name: "request_service".to_string(),
            arguments: args.clone(),
        };
        let outcome =
            dispatch_virtual_tool_call(&mut state, &tc, &args, &cancel, &cancel, false, &mut emit)
                .await
                .unwrap();
        assert!(matches!(outcome, VirtualToolOutcome::Continue { .. }));
        let payload = state.machine.tool_payload_by_call_id(id).unwrap();
        assert_eq!(payload["status"], "invalid_request");
    }
    assert!(!events.iter().any(|e| matches!(e, Event::Yield { .. })));
    assert!(state.machine.pending_confirmation().is_none());
}

#[tokio::test]
async fn request_service_already_active_with_sufficient_grant_skips_prompt() {
    let authority = MockServiceAuthority::with_github_inactive();
    authority.set_active("github", ServiceMountAccess::ReadWrite, true);
    let mut state = create_state(Some(authority));
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let args = json!({
        "package_id": "github-fileserver",
        "access": "read_only",
        "allow_network": true,
        "reason": "Inspect PR"
    });
    let tc = NormalizedToolCall {
        id: "call_active".to_string(),
        name: "request_service".to_string(),
        arguments: args.clone(),
    };
    let outcome =
        dispatch_virtual_tool_call(&mut state, &tc, &args, &cancel, &cancel, false, &mut emit)
            .await
            .unwrap();
    assert!(matches!(outcome, VirtualToolOutcome::Continue { .. }));
    let payload = state
        .machine
        .tool_payload_by_call_id("call_active")
        .unwrap();
    assert_eq!(payload["status"], "already_active");
    assert_eq!(payload["description"], "GitHub aP fileserver");
    assert!(!events.iter().any(|e| matches!(e, Event::Yield { .. })));
}

#[tokio::test]
async fn request_service_is_always_human_even_under_confined_trusted_and_session_allowlist() {
    let authority = MockServiceAuthority::with_github_inactive();
    let mut state = create_state(Some(authority));
    state.runtime_config.trust_posture = TrustPosture::ConfinedTrusted;
    state.runtime_config.guardian_profile = Some("guardian-main".to_string());
    state.machine.allow_tool_for_session(
        "request_service",
        SERVICE_ACTIVATION_CHECKPOINT_TYPE,
        &serde_json::Value::Null,
    );

    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let args = json!({
        "package_id": "github-fileserver",
        "access": "read_write",
        "allow_network": true,
        "reason": "Create issue via /mnt/github"
    });
    let tc = NormalizedToolCall {
        id: "call_svc_1".to_string(),
        name: "request_service".to_string(),
        arguments: args.clone(),
    };
    let outcome =
        dispatch_virtual_tool_call(&mut state, &tc, &args, &cancel, &cancel, false, &mut emit)
            .await
            .unwrap();
    assert!(matches!(outcome, VirtualToolOutcome::PauseTurn));

    let pending = state.machine.pending_confirmation().unwrap();
    assert_eq!(pending.checkpoint_id, "call_svc_1");
    assert_eq!(pending.checkpoint_type, SERVICE_ACTIVATION_CHECKPOINT_TYPE);
    assert_eq!(
        pending.options,
        vec!["approve", "allow_persistent", "reject"]
    );
    assert_eq!(
        pending.summary,
        "Activate 'github-fileserver' at /mnt/github (read-write, network)?"
    );
    let prompt_text = pending.details["prompt"].as_str().unwrap();
    assert!(prompt_text.contains("github-fileserver"));
    assert!(prompt_text.contains("bash"));

    assert!(events.iter().any(|e| matches!(
        e,
        Event::Yield {
            kind: YieldKind::Confirmation,
            ..
        }
    )));
}

#[tokio::test]
async fn request_service_resume_handles_approve_allow_persistent_and_reject() {
    for choice in ["approve", "allow_persistent", "reject", "modify"] {
        let authority = MockServiceAuthority::with_github_inactive();
        let mut state = create_state(Some(authority.clone()));
        let cancel = CancellationToken::new();
        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let args = json!({
            "package_id": "github-fileserver",
            "access": "read_write",
            "allow_network": true,
            "reason": "Inspect repository issues"
        });
        let tc = NormalizedToolCall {
            id: "call_resume_1".to_string(),
            name: "request_service".to_string(),
            arguments: args.clone(),
        };
        let outcome =
            dispatch_virtual_tool_call(&mut state, &tc, &args, &cancel, &cancel, false, &mut emit)
                .await
                .unwrap();
        assert!(matches!(outcome, VirtualToolOutcome::PauseTurn));

        let request_id = state.machine.pending_request_ids().pop().unwrap();
        assert!(matches!(
            state.machine.pending_yield(&request_id),
            Some(PendingYield::Confirmation(_))
        ));

        let agent_files = state.agent_files();
        let host_mounts = state.environment.host_mount_requests();
        let mut writer = None;
        let runtime = SubmissionRuntime::new(&mut state.machine, agent_files, host_mounts)
            .with_service_activation(Some(authority.clone()));
        let content = if choice == "modify" {
            vec![ContentPart::structured(json!({
                "choice": "approve",
                "modifications": "read_only only"
            }))]
        } else {
            vec![ContentPart::structured(json!({"choice": choice}))]
        };
        let action = handle_non_compaction_runtime_op(
            runtime,
            Op::Resume {
                request_id,
                content,
            },
            &mut writer,
            &mut emit,
        )
        .await
        .unwrap();
        writer.unwrap().finish().await.unwrap();

        assert!(matches!(
            action,
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                user_input: None,
                activate_task: false
            }
        ));
        let tool_payload = state
            .machine
            .tool_payload_by_call_id("call_resume_1")
            .unwrap();
        let recorded = authority.activations.lock().unwrap().clone();
        match choice {
            "approve" => {
                assert_eq!(recorded.len(), 1);
                assert!(!recorded[0].1);
                assert_eq!(tool_payload["status"], "activated");
                assert_eq!(tool_payload["description"], "GitHub aP fileserver");
                assert_eq!(tool_payload["persisted"], false);
            }
            "allow_persistent" => {
                assert_eq!(recorded.len(), 1);
                assert!(recorded[0].1);
                assert_eq!(tool_payload["status"], "activated");
                assert_eq!(tool_payload["description"], "GitHub aP fileserver");
                assert_eq!(tool_payload["persisted"], true);
            }
            "reject" | "modify" => {
                assert!(recorded.is_empty());
                assert_eq!(tool_payload["status"], "rejected");
            }
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn multi_tool_batch_pausing_on_request_service_leaves_no_dangling_tool_calls() {
    let authority = MockServiceAuthority::with_github_inactive();
    let mut state = create_state(Some(authority.clone()));
    let mut loop_guard = ToolLoopGuard::new(None, 4);
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let tool_calls = vec![
        NormalizedToolCall {
            id: "call_svc".to_string(),
            name: "request_service".to_string(),
            arguments: json!({
                "package_id": "github-fileserver",
                "reason": "Need /mnt/github"
            }),
        },
        NormalizedToolCall {
            id: "call_trailing_read".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "/mnt/github/status"}),
        },
    ];
    state
        .machine
        .add_assistant_message_with_tool_calls_and_reasoning(
            "",
            tool_calls
                .iter()
                .map(|tc| crate::tape::ToolRequest {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                })
                .collect(),
            None,
            None,
            &[],
        );

    let writer = state.agent_files().begin_tape_generation().await.unwrap();
    let outcome = orchestrate_tool_batch(
        &mut loop_guard,
        &mut state,
        &tool_calls,
        ToolOrchestratorInputs {
            explicit_command: false,
            cancel: &cancel,
            steering_broker: None,
        },
        &writer,
        &mut emit,
    )
    .await
    .unwrap();
    writer.finish().await.unwrap();

    assert_eq!(outcome, ToolBatchOrchestratorOutcome::PauseTurn);
    // Trailing call must already have a correlated Tool response on tape.
    let trailing = state
        .machine
        .tool_payload_by_call_id("call_trailing_read")
        .unwrap();
    assert_eq!(trailing["status"], "skipped_due_to_service_activation");

    // Resume with approval so call_svc also gets its Tool response.
    let request_id = state.machine.pending_request_ids().pop().unwrap();
    let agent_files = state.agent_files();
    let host_mounts = state.environment.host_mount_requests();
    let mut writer = None;
    let runtime = SubmissionRuntime::new(&mut state.machine, agent_files, host_mounts)
        .with_service_activation(Some(authority));
    handle_non_compaction_runtime_op(
        runtime,
        Op::Resume {
            request_id,
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        },
        &mut writer,
        &mut emit,
    )
    .await
    .unwrap();
    writer.unwrap().finish().await.unwrap();

    // Every assistant tool request ID must have a matching Tool response on tape.
    let messages = state.machine.messages();
    let requested_ids: std::collections::BTreeSet<_> = messages
        .iter()
        .flat_map(|msg| msg.tool_requests().iter().map(|req| req.id.clone()))
        .collect();
    let responded_ids: std::collections::BTreeSet<_> = messages
        .iter()
        .flat_map(|msg| msg.tool_responses().iter().map(|resp| resp.id.clone()))
        .collect();
    assert_eq!(requested_ids, responded_ids);
}

#[tokio::test]
async fn child_agent_without_authority_excludes_request_service_and_dynamic_context() {
    let child_defs = virtual_tool_definitions(true, false, false);
    assert!(!child_defs.iter().any(|d| d.name == "request_service"));

    let mut child_state = create_state(None);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let args = json!({"package_id": "github-fileserver", "reason": "nope"});
    let tc = NormalizedToolCall {
        id: "child_call".to_string(),
        name: "request_service".to_string(),
        arguments: args.clone(),
    };
    let outcome = dispatch_virtual_tool_call(
        &mut child_state,
        &tc,
        &args,
        &cancel,
        &cancel,
        false,
        &mut emit,
    )
    .await
    .unwrap();
    assert!(matches!(outcome, VirtualToolOutcome::NotVirtual));

    // Also verify dynamic turn context guidance preserves static system prompt prefix.
    let authority = MockServiceAuthority::with_github_inactive();
    let rendered =
        render_service_activation_context(&authority.list_available_fileservers()).unwrap();
    assert!(rendered.contains("request_service"));
    assert!(rendered.contains("github-fileserver"));
    assert!(rendered.contains("/mnt/github"));

    let mock = MockLlmProvider::new().with_responses(vec![GenerationResponse {
        content: "done".to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }]);
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let mut root_state = RuntimeLoopState {
        machine: AgentMachine::new(),
        environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
            .with_service_activation_authority(authority),
        core_config: Config::default(),
        runtime_config: RuntimeConfig::default(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };
    run_turn_with_cancel(
        &mut root_state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Check github")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();
    let reqs = mock.recorded_requests();
    assert_eq!(reqs.len(), 1);
    assert!(
        !reqs[0]
            .system_prompt
            .as_deref()
            .unwrap_or_default()
            .contains("Fileserver services (Root Agent):")
    );
    let trailing_context = reqs[0]
        .messages
        .iter()
        .rev()
        .find(|m| m.role == alan_llm::MessageRole::Context)
        .expect("expected dynamic context message");
    assert!(trailing_context.content.contains("request_service"));
    assert!(trailing_context.content.contains("github-fileserver"));
}

#[tokio::test]
async fn request_service_upgrade_summary_reflects_active_mount_and_scope() {
    let authority = MockServiceAuthority::with_github_inactive();
    authority.set_active("github", ServiceMountAccess::ReadOnly, false);
    let mut state = create_state(Some(authority));
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let args = json!({
        "package_id": "github-fileserver",
        "access": "read_write",
        "allow_network": true,
        "reason": "Need write and network access"
    });
    let tc = NormalizedToolCall {
        id: "call_upgrade".to_string(),
        name: "request_service".to_string(),
        arguments: args.clone(),
    };
    let outcome =
        dispatch_virtual_tool_call(&mut state, &tc, &args, &cancel, &cancel, false, &mut emit)
            .await
            .unwrap();
    assert!(matches!(outcome, VirtualToolOutcome::PauseTurn));
    let pending = state.machine.pending_confirmation().unwrap();
    assert_eq!(
        pending.summary,
        "Upgrade 'github-fileserver' at /mnt/github (read-write, network)?"
    );
}

#[test]
fn render_service_activation_context_formats_active_and_inactive_packages() {
    let packages = vec![
        AvailableFileserverPackage {
            package_id: "github-fileserver".to_string(),
            service_id: "github".to_string(),
            mount_path: "/mnt/github".to_string(),
            description: Some("GitHub aP fileserver".to_string()),
            allow_network: true,
            network_domains: vec!["api.github.com".to_string()],
            active: true,
            active_access: Some(ServiceMountAccess::ReadOnly),
            active_allow_network: false,
        },
        AvailableFileserverPackage {
            package_id: "notes-fileserver".to_string(),
            service_id: "notes".to_string(),
            mount_path: "/mnt/notes".to_string(),
            description: Some("Local notes mount".to_string()),
            allow_network: false,
            network_domains: Vec::new(),
            active: false,
            active_access: None,
            active_allow_network: false,
        },
        AvailableFileserverPackage {
            package_id: "remote-fileserver".to_string(),
            service_id: "remote".to_string(),
            mount_path: "/mnt/remote".to_string(),
            description: Some("Remote API mount".to_string()),
            allow_network: true,
            network_domains: vec!["example.com".to_string()],
            active: false,
            active_access: None,
            active_allow_network: false,
        },
    ];
    let rendered = render_service_activation_context(&packages).unwrap();
    assert!(rendered.contains(
        "- `github-fileserver` -> `/mnt/github` (active: read-only, no-network) — GitHub aP fileserver"
    ));
    assert!(
        rendered.contains("- `notes-fileserver` -> `/mnt/notes` (local only) — Local notes mount")
    );
    assert!(
        rendered.contains(
            "- `remote-fileserver` -> `/mnt/remote` (network capable) — Remote API mount"
        )
    );

    let active_rw_net = vec![AvailableFileserverPackage {
        package_id: "github-fileserver".to_string(),
        service_id: "github".to_string(),
        mount_path: "/mnt/github".to_string(),
        description: Some("GitHub aP fileserver".to_string()),
        allow_network: true,
        network_domains: vec![],
        active: true,
        active_access: Some(ServiceMountAccess::ReadWrite),
        active_allow_network: true,
    }];
    let rendered_rw = render_service_activation_context(&active_rw_net).unwrap();
    assert!(rendered_rw.contains(
        "- `github-fileserver` -> `/mnt/github` (active: read-write, network) — GitHub aP fileserver"
    ));
}
