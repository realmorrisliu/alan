use super::*;
use crate::{
    agent_machine::{AgentMachine, NormalizedToolCall, TurnActivityState},
    config::Config,
    rollout::{RolloutItem, RolloutRecorder},
    runtime::{
        ChildRunRecord, ChildRunStatus, NamespaceRuntimeEnvironment, RuntimeConfig,
        delegated_child_run::{
            ChildRuntimeResult, ChildRuntimeStatus, MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS,
            MAX_DELEGATED_RESULT_SUMMARY_CHARS,
        },
        delegated_skill_evidence::persist_delegated_child_evidence,
        delegated_skill_tool::{
            DEFAULT_DELEGATED_TIMEOUT_SECS, DelegatedSkillInvocationRequest,
            MAX_DELEGATED_SKILL_ID_CHARS, MAX_DELEGATED_TARGET_CHARS, MAX_DELEGATED_TASK_CHARS,
            handle_invoke_delegated_skill_with_spawn as handle_invoke_delegated_skill_with_runtime,
        },
        delegation_capabilities::DelegatedSpawnRejected,
        interaction_tools::{
            parse_confirmation_request, parse_plan_status, parse_plan_update,
            parse_structured_user_input_request,
        },
        mount_request_tool::{MountRequestAccess, parse_mount_request},
        transition::NamespaceActionRecord,
        virtual_tool::VirtualToolOutcome,
    },
    skills::{
        ActiveSkillEnvelope, DelegatedSkillInvocationRecord, ResolvedCapabilityView,
        ResolvedSkillExecution, ScopedPackageDir, SkillActivationReason,
        SkillExecutionResolutionSource, SkillHostCapabilities, SkillMetadata, SkillScope,
    },
    tools::ToolRegistry,
};
use alan_agent_protocol::{Event, SpawnHandle, SpawnSpec};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_shell::Shell;
use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn namespace_environment_for_virtual_tool_test(
    tools: &ToolRegistry,
) -> NamespaceRuntimeEnvironment {
    let agentfs = Arc::new(AgentFs::new());
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let runner = crate::tools::ToolProcessRunner::from_registry(tools);
    NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_tool_process_context(alan_kernel::Pid(1), runner)
}

fn create_test_transition_state() -> super::super::transition::RuntimeLoopState {
    let config = Config::default();
    let machine = AgentMachine::new();
    let host_source = PathBuf::from("/tmp/alan-delegated-parent");
    let mut launch_namespace = Namespace::new();
    launch_namespace.mount(
        "/mnt/source",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let launch_context = crate::ProcessLaunchContext::new(
        launch_namespace,
        alan_kernel::Credentials::user("parent-agent"),
        "/mnt/source",
    )
    .unwrap()
    .with_host_mount(
        crate::HostMountGrant::new("/mnt/source", &host_source, Access::ReadWrite).unwrap(),
    );
    let mut tools = ToolRegistry::new();
    tools.set_default_execution_binding(
        crate::tools::ToolExecutionBinding::from_launch_context(
            &launch_context,
            PathBuf::from("/tmp/alan-system-store/tmp"),
        )
        .unwrap(),
    );
    let runtime_config = RuntimeConfig::default();
    let mut prompt_cache = crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new());
    prompt_cache.set_host_capabilities(
        SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation(),
    );

    super::super::transition::RuntimeLoopState {
        machine,
        environment: namespace_environment_for_virtual_tool_test(&tools)
            .with_launch_context(launch_context),
        core_config: config,
        runtime_config,
        prompt_cache,
    }
}

fn create_namespace_transition_state_and_shell()
-> (super::super::transition::RuntimeLoopState, Shell) {
    let agentfs = Arc::new(AgentFs::new());
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let shell = Shell::new(root.clone());
    let mut state = create_test_transition_state();
    state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    (state, shell)
}

async fn read_shell_utf8(shell: &Shell, path: &str) -> String {
    String::from_utf8(shell.cat(path).await.expect("read agent file")).expect("agent file utf8")
}

fn delegated_test_skill_metadata(skill_id: &str, target: &str) -> SkillMetadata {
    SkillMetadata {
        id: skill_id.to_string(),
        package_id: Some(format!("skill:{skill_id}")),
        name: skill_id.to_string(),
        description: format!("Delegated test skill {skill_id}"),
        short_description: None,
        path: PathBuf::from(format!("/tmp/{skill_id}/SKILL.md")),
        package_root: Some(PathBuf::from(format!("/tmp/{skill_id}"))),
        resource_root: Some(PathBuf::from(format!("/tmp/{skill_id}"))),
        scope: SkillScope::Descriptor,
        tags: Vec::new(),
        capabilities: None,
        compatibility: Default::default(),
        source: Default::default(),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: ResolvedSkillExecution::Delegate {
            target: target.to_string(),
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        },
    }
}

fn activate_test_delegated_skill(
    state: &mut super::super::transition::RuntimeLoopState,
    skill_id: &str,
    target: &str,
) {
    state
        .machine
        .set_active_skills(vec![ActiveSkillEnvelope::available(
            delegated_test_skill_metadata(skill_id, target),
            SkillActivationReason::ExplicitMention {
                mention: skill_id.to_string(),
            },
        )]);
}

fn capability_view_for_package_store(package_store: &std::path::Path) -> ResolvedCapabilityView {
    ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
        path: package_store.to_path_buf(),
        scope: SkillScope::Descriptor,
    }])
}

fn test_child_run_record(child_run_id: &str, parent_process_path: &str) -> ChildRunRecord {
    ChildRunRecord::new(
        child_run_id.to_string(),
        parent_process_path.to_string(),
        "/proc/42".to_string(),
        Some("/agent/42".to_string()),
        Some("definition:reviewer".to_string()),
    )
}

fn tool_result_text_for_call(
    state: &super::super::transition::RuntimeLoopState,
    call_id: &str,
) -> String {
    state
        .machine
        .prompt_view()
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == call_id)
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected tool result")
}

async fn dispatch_virtual_tool_call_for_test<E, F>(
    state: &mut super::super::transition::RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let cancel = CancellationToken::new();
    super::super::transition::dispatch_virtual_tool_call(
        state,
        tool_call,
        &tool_call.arguments,
        &cancel,
        false,
        emit,
    )
    .await
}

async fn handle_invoke_delegated_skill_with_spawn<E, F, S>(
    state: &mut super::super::transition::RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    emit: &mut E,
    spawn_child: S,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
    S: for<'a> FnOnce(
        super::super::child_agents::ChildLaunchRuntime,
        SpawnSpec,
        &'a CancellationToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ChildRuntimeResult>> + Send + 'a>,
    >,
{
    let runtime = super::super::transition::delegated_skill_runtime(state);
    handle_invoke_delegated_skill_with_runtime(
        runtime,
        tool_call,
        tool_arguments,
        cancel,
        emit,
        spawn_child,
    )
    .await
}

#[path = "child_run_termination_tool_tests.rs"]
mod child_run_termination_tool;
#[path = "delegated_skill_evidence_tests.rs"]
mod delegated_skill_evidence;
#[path = "delegated_skill_tool_tests.rs"]
mod delegated_skill_tool;
#[path = "interaction_tool_tests.rs"]
mod interaction_tool;
#[path = "mount_request_tool_tests.rs"]
mod mount_request_tool;
#[path = "plan_tool_tests.rs"]
mod plan_tool;

#[tokio::test]
async fn test_try_handle_non_virtual_tool() {
    let mut state = create_test_transition_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "test.txt"}),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = dispatch_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::NotVirtual));
}

#[tokio::test]
async fn test_try_handle_unknown_tool() {
    let mut state = create_test_transition_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "unknown_tool".to_string(),
        arguments: json!({}),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = dispatch_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::NotVirtual));
}
