mod delegated_launch;
mod runtime_inputs;
mod task_context;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};

use alan_agent_protocol::{
    AGENT_DEFINITION_DESCRIPTOR as AGENT_DEFINITION_FD, AgentExecutableRequest,
    ProcessNamespaceAccess, ProcessNamespaceMount, SpawnHandle, SpawnMountAccess, SpawnSpec,
    SpawnTarget,
};
use anyhow::{Context, Result, bail, ensure};
use tokio_util::sync::CancellationToken;

use super::{
    child_runs::ChildRunRecord,
    delegated_child_run::{
        ChildProcessStartup, DelegatedChildRunSupervision, DelegatedChildRunSupervisor,
    },
};
use delegated_launch::evaluate_delegated_launch_capabilities;

pub(crate) use runtime_inputs::{ChildLaunchRuntime, ChildTaskContext};
pub(crate) use task_context::project_child_task_context;

const CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE: &str = "Child Agent Process launch cancelled";
const AGENT_EXECUTABLE: &str = "/bin/alan-agent";

#[derive(Debug, Clone)]
struct ChildNamespacePlan {
    process_mounts: Vec<ProcessNamespaceMount>,
    effective_mounts: Vec<ProcessNamespaceMount>,
    bin_tool_mounts: Vec<String>,
    llm_connection_name: String,
    cwd: PathBuf,
}

impl ChildNamespacePlan {
    fn namespace_summary(&self) -> alan_agent_protocol::DelegatedNamespaceSummary {
        alan_agent_protocol::DelegatedNamespaceSummary {
            mounts: self
                .effective_mounts
                .iter()
                .map(|mount| mount.path.clone())
                .collect(),
            writable_mounts: self
                .effective_mounts
                .iter()
                .filter(|mount| mount.access == ProcessNamespaceAccess::ReadWrite)
                .map(|mount| mount.path.clone())
                .collect(),
            bin_bindings: self.bin_tool_mounts.clone(),
            cwd: Some(self.cwd.clone()),
            llm_connection: Some(self.llm_connection_name.clone()),
        }
    }
}

pub(crate) async fn spawn_child_runtime_cancellable(
    parent: ChildLaunchRuntime,
    spec: SpawnSpec,
    cancel: &CancellationToken,
) -> Result<DelegatedChildRunSupervisor> {
    spawn_child_runtime_inner(parent, spec, Some(cancel)).await
}

async fn spawn_child_runtime_inner(
    parent: ChildLaunchRuntime,
    mut spec: SpawnSpec,
    cancel: Option<&CancellationToken>,
) -> Result<DelegatedChildRunSupervisor> {
    ensure_not_cancelled(cancel)?;
    validate_child_launch_contract(&spec)?;

    let process_files = parent.child_launch.process_files();
    let parent_pid = process_files.current_pid()?.to_string();
    let parent_mounts = process_files.read_process_namespace(&parent_pid).await?;
    let parent_descriptors = process_files.read_process_descriptors(&parent_pid).await?;
    let target_path = resolve_target_path(&parent, &spec.target, &parent_descriptors)?;
    let tool_names = select_tool_names(&parent, &spec).await?;
    let plan = build_child_namespace_plan(
        &spec,
        &parent_mounts,
        &target_path,
        &parent_descriptors,
        &tool_names,
        parent.child_launch.connection_name(),
    )?;
    let delegation_capability_decision = evaluate_delegated_launch_capabilities(
        &mut spec,
        &plan,
        &parent_mounts,
        parent.child_launch.namespace_cwd(),
    )?;

    let mut descriptors = BTreeMap::from([(AGENT_DEFINITION_FD, target_path)]);
    if spec.has_handle(SpawnHandle::Memory)
        && let Some(path) = parent_descriptors.get(&alan_agent_protocol::MEMORY_STORE_DESCRIPTOR)
    {
        descriptors.insert(alan_agent_protocol::MEMORY_STORE_DESCRIPTOR, path.clone());
    }
    let request = AgentExecutableRequest {
        initial_task: task_context::build_child_task_text(&parent.task_context, &spec),
        spawn: spec.clone(),
    };
    ensure_not_cancelled(cancel)?;
    let child_pid = process_files
        .spawn_agent_process(&request, plan.process_mounts.clone(), descriptors)
        .await
        .context("Failed to spawn child Agent Process through /proc/clone")?;
    let (agent_files, child_process_files) = parent.child_launch.observation_handles(&child_pid);
    wait_for_child_process_startup(&agent_files, &child_process_files, &child_pid, cancel).await?;

    let child_run_id = uuid::Uuid::new_v4().to_string();
    let process_path = format!("/proc/{child_pid}");
    let agent_path = format!("/agent/{child_pid}");
    let mut child_run_record = ChildRunRecord::new(
        child_run_id.clone(),
        parent.parent_process_path,
        process_path.clone(),
        Some(agent_path),
        Some(format!("{:?}", spec.target)),
    );
    if let Some(decision) = delegation_capability_decision {
        child_run_record = child_run_record.with_delegation_capability_decision(decision);
    }
    parent.child_run_registry.register(child_run_record);
    parent.child_run_registry.mark_running(&child_run_id);

    Ok(DelegatedChildRunSupervisor::new(
        DelegatedChildRunSupervision {
            startup: ChildProcessStartup {
                process_path,
                rollout_path: None,
                warnings: Vec::new(),
            },
            child_run_id,
            child_run_registry: parent.child_run_registry,
            timeout: spec.launch.timeout_secs.map(Duration::from_secs),
            agent_files,
            process_files: child_process_files,
            process_pid: child_pid,
        },
    ))
}

fn ensure_not_cancelled(cancel: Option<&CancellationToken>) -> Result<()> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
    }
    Ok(())
}

async fn wait_for_child_process_startup(
    agent_files: &super::transition::NamespaceAgentFiles,
    process_files: &super::transition::NamespaceProcessFiles,
    pid: &str,
    cancel: Option<&CancellationToken>,
) -> Result<()> {
    let wait = async {
        loop {
            if let Some(exit_code) = process_files.read_process_exit_code(pid).await? {
                let result = process_files.read_agent_process_result(pid).await.ok();
                if result
                    .as_ref()
                    .is_some_and(|result| child_reached_observable_terminal(exit_code, result))
                {
                    return Ok(());
                }
                let detail = result
                    .and_then(|result| result.error_message)
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default();
                bail!("Child Agent Process exited during startup with code {exit_code}{detail}");
            }
            if agent_files.read_ui_activity_snapshot().await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    if let Some(cancel) = cancel {
        tokio::select! {
            result = wait => result,
            _ = cancel.cancelled() => {
                let _ = process_files.write_process_control_for_pid(pid, "cancel").await;
                bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE)
            }
        }
    } else {
        wait.await
    }
}

fn child_reached_observable_terminal(
    exit_code: i32,
    result: &alan_agent_protocol::AgentExecutableResult,
) -> bool {
    matches!(
        (exit_code, result.status),
        (0, alan_agent_protocol::AgentExecutableStatus::Completed)
            | (
                1,
                alan_agent_protocol::AgentExecutableStatus::Paused
                    | alan_agent_protocol::AgentExecutableStatus::Failed
            )
    )
}

async fn select_tool_names(parent: &ChildLaunchRuntime, spec: &SpawnSpec) -> Result<Vec<String>> {
    let packages = parent.tool_execution.discover_packages().await?;
    let available = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    let names = if let Some(profile) = spec.runtime_overrides.tool_profile.as_ref() {
        let missing = profile
            .allowed_tools
            .iter()
            .filter(|name| !available.contains(name.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            missing.is_empty(),
            "Child Agent Process launch requested unavailable tools: {}",
            missing.join(", ")
        );
        profile
            .allowed_tools
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        available.into_iter().map(str::to_string).collect()
    };
    Ok(names)
}

fn resolve_target_path(
    parent: &ChildLaunchRuntime,
    target: &SpawnTarget,
    descriptors: &BTreeMap<u32, String>,
) -> Result<String> {
    match target {
        SpawnTarget::DefinitionDescriptor { descriptor } => {
            ensure!(
                descriptor == crate::AGENT_DEFINITION_DESCRIPTOR,
                "parent Process has no `{descriptor}` descriptor"
            );
            descriptors
                .get(&AGENT_DEFINITION_FD)
                .cloned()
                .context("parent Process has no Agent Definition descriptor")
        }
        SpawnTarget::PackageChildAgent { .. } => parent
            .capability_view
            .as_ref()
            .map(crate::skills::ResolvedCapabilityView::refresh)
            .and_then(|view| view.resolve_child_agent_export(target).cloned())
            .with_context(|| format!("Unknown package child Agent Executable target: {target:?}"))?
            .root_dir
            .to_str()
            .map(str::to_string)
            .context("package child Agent Executable path is not utf8"),
    }
}

fn build_child_namespace_plan(
    spec: &SpawnSpec,
    parent_mounts: &[ProcessNamespaceMount],
    target_path: &str,
    parent_descriptors: &BTreeMap<u32, String>,
    tool_names: &[String],
    llm_connection_name: &str,
) -> Result<ChildNamespacePlan> {
    let mut selected = BTreeMap::<String, ProcessNamespaceAccess>::new();
    for path in [
        "/proc",
        "/agent",
        "/srv",
        AGENT_EXECUTABLE,
        "/mnt/llm",
        "/mnt/route",
        "/mnt/host-mount",
        "/man",
    ] {
        retain_exact_mount(parent_mounts, &mut selected, path)?;
    }
    retain_resolving_mount(parent_mounts, &mut selected, target_path)?;
    for mount in parent_mounts
        .iter()
        .filter(|mount| mount.path.starts_with("/lib/pkg/"))
    {
        selected.insert(mount.path.clone(), mount.access);
    }
    let bin_tool_mounts = tool_names
        .iter()
        .map(|name| format!("/bin/{name}"))
        .collect::<Vec<_>>();
    for name in tool_names {
        retain_exact_mount(parent_mounts, &mut selected, &format!("/bin/{name}"))?;
        retain_exact_mount(parent_mounts, &mut selected, &format!("/lib/exec/{name}"))?;
    }
    if spec.has_handle(SpawnHandle::Memory) {
        let memory = parent_descriptors
            .get(&alan_agent_protocol::MEMORY_STORE_DESCRIPTOR)
            .context("parent Process has no Memory Store descriptor")?;
        retain_resolving_mount(parent_mounts, &mut selected, memory)?;
    }
    let mut effective = selected.clone();
    for host_mount in &spec.host_mounts {
        let target = host_mount
            .target
            .to_str()
            .context("Host Mount target is not utf8")?;
        effective.insert(
            target.to_string(),
            match host_mount.access {
                SpawnMountAccess::ReadOnly => ProcessNamespaceAccess::ReadOnly,
                SpawnMountAccess::ReadWrite => ProcessNamespaceAccess::ReadWrite,
            },
        );
    }

    let cwd = spec
        .launch
        .cwd
        .clone()
        .unwrap_or_else(|| PathBuf::from("/"));
    ensure!(
        cwd.is_absolute(),
        "Child Agent Process cwd must be absolute"
    );
    if cwd != Path::new("/") {
        ensure!(
            effective
                .keys()
                .any(|path| cwd.starts_with(Path::new(path))),
            "Child Agent Process cwd '{}' is outside its delegated namespace",
            cwd.display()
        );
    }

    Ok(ChildNamespacePlan {
        process_mounts: selected
            .into_iter()
            .map(|(path, access)| ProcessNamespaceMount::new(path, access))
            .collect(),
        effective_mounts: effective
            .into_iter()
            .map(|(path, access)| ProcessNamespaceMount::new(path, access))
            .collect(),
        bin_tool_mounts,
        llm_connection_name: llm_connection_name.to_string(),
        cwd,
    })
}

fn retain_exact_mount(
    parent_mounts: &[ProcessNamespaceMount],
    selected: &mut BTreeMap<String, ProcessNamespaceAccess>,
    path: &str,
) -> Result<()> {
    let mount = parent_mounts
        .iter()
        .find(|mount| mount.path == path)
        .with_context(|| format!("parent Process namespace has no `{path}` mount"))?;
    selected.insert(mount.path.clone(), mount.access);
    Ok(())
}

fn retain_resolving_mount(
    parent_mounts: &[ProcessNamespaceMount],
    selected: &mut BTreeMap<String, ProcessNamespaceAccess>,
    path: &str,
) -> Result<()> {
    let path = Path::new(path);
    let mount = parent_mounts
        .iter()
        .filter(|mount| path.starts_with(Path::new(&mount.path)))
        .max_by_key(|mount| mount.path.len())
        .with_context(|| {
            format!(
                "parent Process namespace cannot resolve `{}`",
                path.display()
            )
        })?;
    selected.insert(mount.path.clone(), mount.access);
    Ok(())
}

fn validate_child_launch_contract(spec: &SpawnSpec) -> Result<()> {
    spec.validate_agent_process_launch()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_protocol::{SpawnHostMount, SpawnLaunchInputs, SpawnRuntimeOverrides};

    fn mount(path: &str, access: ProcessNamespaceAccess) -> ProcessNamespaceMount {
        ProcessNamespaceMount::new(path, access)
    }

    fn parent_mounts() -> Vec<ProcessNamespaceMount> {
        use ProcessNamespaceAccess::{ReadOnly, ReadWrite};
        vec![
            mount("/proc", ReadWrite),
            mount("/agent", ReadWrite),
            mount("/srv", ReadOnly),
            mount("/bin/alan-agent", ReadOnly),
            mount("/mnt/llm", ReadWrite),
            mount("/mnt/route", ReadWrite),
            mount("/mnt/host-mount", ReadWrite),
            mount("/man", ReadOnly),
            mount("/lib/agents/root", ReadOnly),
            mount("/lib/pkg/example", ReadOnly),
            mount("/memory", ReadWrite),
            mount("/mnt/source", ReadWrite),
        ]
    }

    fn descriptors() -> BTreeMap<u32, String> {
        BTreeMap::from([
            (
                alan_agent_protocol::AGENT_DEFINITION_DESCRIPTOR,
                "/lib/agents/root".to_string(),
            ),
            (
                alan_agent_protocol::MEMORY_STORE_DESCRIPTOR,
                "/memory".to_string(),
            ),
        ])
    }

    fn spec() -> SpawnSpec {
        SpawnSpec {
            target: SpawnTarget::DefinitionDescriptor {
                descriptor: crate::AGENT_DEFINITION_DESCRIPTOR.to_string(),
            },
            launch: SpawnLaunchInputs {
                task: "inspect".to_string(),
                ..SpawnLaunchInputs::default()
            },
            handles: Vec::new(),
            host_mounts: Vec::new(),
            runtime_overrides: SpawnRuntimeOverrides::default(),
            delegated: None,
        }
    }

    fn plan(spec: &SpawnSpec) -> Result<ChildNamespacePlan> {
        build_child_namespace_plan(
            spec,
            &parent_mounts(),
            "/lib/agents/root",
            &descriptors(),
            &[],
            "default",
        )
    }

    #[test]
    fn child_namespace_defaults_to_root_without_ambient_host_mounts() {
        let plan = plan(&spec()).unwrap();

        assert_eq!(plan.cwd, Path::new("/"));
        assert!(
            plan.process_mounts
                .iter()
                .any(|mount| mount.path == "/bin/alan-agent")
        );
        assert!(
            plan.process_mounts
                .iter()
                .any(|mount| mount.path == "/lib/pkg/example")
        );
        for ambient in ["/bin", "/lib", "/memory", "/mnt/source"] {
            assert!(
                !plan
                    .process_mounts
                    .iter()
                    .any(|mount| mount.path == ambient),
                "child unexpectedly inherited {ambient}"
            );
        }
    }

    #[test]
    fn child_namespace_projects_only_explicit_host_mount_with_narrower_access() {
        let mut spec = spec();
        spec.launch.cwd = Some(PathBuf::from("/mnt/review"));
        spec.host_mounts.push(SpawnHostMount {
            grant: "grant-source".to_string(),
            target: PathBuf::from("/mnt/review"),
            access: SpawnMountAccess::ReadOnly,
        });

        let plan = plan(&spec).unwrap();

        assert_eq!(plan.cwd, Path::new("/mnt/review"));
        assert!(plan.effective_mounts.iter().any(|mount| {
            mount.path == "/mnt/review" && mount.access == ProcessNamespaceAccess::ReadOnly
        }));
        assert!(
            !plan
                .process_mounts
                .iter()
                .any(|mount| mount.path == "/mnt/source" || mount.path == "/mnt/review")
        );
    }

    #[test]
    fn child_namespace_rejects_cwd_outside_explicit_capabilities() {
        let mut spec = spec();
        spec.launch.cwd = Some(PathBuf::from("/mnt/source"));

        let error = plan(&spec).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("outside its delegated namespace")
        );
    }

    #[test]
    fn child_namespace_passes_memory_only_by_descriptor_handle() {
        let mut spec = spec();
        assert!(
            !plan(&spec)
                .unwrap()
                .effective_mounts
                .iter()
                .any(|mount| mount.path == "/memory")
        );

        spec.handles.push(SpawnHandle::Memory);
        assert!(
            plan(&spec)
                .unwrap()
                .effective_mounts
                .iter()
                .any(|mount| mount.path == "/memory")
        );
    }

    #[test]
    fn child_launch_requires_absolute_normalized_cwd() {
        for cwd in ["docs", "/mnt/source/../secret", "/mnt//source"] {
            let mut spec = spec();
            spec.launch.cwd = Some(PathBuf::from(cwd));

            assert!(
                validate_child_launch_contract(&spec).is_err(),
                "accepted invalid cwd {cwd}"
            );
        }
    }

    #[test]
    fn terminal_child_can_finish_before_live_agentfs_is_observed() {
        let completed = alan_agent_protocol::AgentExecutableResult::completed("done", Vec::new());
        assert!(child_reached_observable_terminal(0, &completed));
        assert!(!child_reached_observable_terminal(1, &completed));

        let paused = alan_agent_protocol::AgentExecutableResult::paused(
            "partial",
            Vec::new(),
            alan_agent_protocol::AgentExecutablePause {
                request_id: "request-1".to_string(),
                kind: alan_agent_protocol::YieldKind::Confirmation,
            },
        );
        assert!(child_reached_observable_terminal(1, &paused));

        let failed = alan_agent_protocol::AgentExecutableResult::failed("failed");
        assert!(child_reached_observable_terminal(1, &failed));
        assert!(!child_reached_observable_terminal(0, &failed));
    }
}
