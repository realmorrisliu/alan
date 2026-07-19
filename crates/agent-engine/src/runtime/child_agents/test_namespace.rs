use super::ChildNamespaceAssemblyPlan;
use crate::runtime::{
    AgentProcessLifecycle, ApprovedMountGrant, ApprovedMountGrantAccess,
    MountGrantApplicatorFactory, NamespaceRuntimeEnvironment,
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{ExecNamespaceAccess, ExecNamespaceManifest, ExecNamespaceMount, ExecSpec};
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CHILD_NAMESPACE_FID: AtomicU64 = AtomicU64::new(80_000);

#[cfg(test)]
impl ChildNamespaceAssemblyPlan {
    pub(super) fn runtime_execution_binding(
        &self,
        scratch: Option<PathBuf>,
    ) -> Result<Option<crate::tools::ToolExecutionBinding>> {
        if self.launch_context.host_mounts.is_empty() {
            return Ok(None);
        }
        let scratch = scratch.context(
            "child Agent Process with Host Mounts requires Agent Runtime Service store bindings",
        )?;
        self.execution_binding(scratch)
    }

    pub(super) fn execution_binding(
        &self,
        scratch: PathBuf,
    ) -> Result<Option<crate::tools::ToolExecutionBinding>> {
        if self.launch_context.host_mounts.is_empty() {
            return Ok(None);
        }
        let launch_context = self.launch_context.clone();
        Ok(Some(
            crate::tools::ToolExecutionBinding::from_launch_context(&launch_context, scratch)?,
        ))
    }
    pub(super) fn clone_exec_spec_for_pid<I, S>(
        &self,
        child_pid: &str,
        executable: impl Into<String>,
        args: I,
    ) -> ExecSpec
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        ExecSpec {
            executable: executable.into(),
            args: args.into_iter().map(Into::into).collect(),
            namespace: Some(self.namespace_manifest_for_pid(child_pid)),
            descriptors: self
                .launch_context
                .descriptors
                .iter()
                .zip(3_u32..)
                .map(|((_, descriptor), number)| (number, descriptor.path.clone()))
                .collect(),
        }
    }

    pub(super) fn namespace_manifest_for_pid(&self, _child_pid: &str) -> ExecNamespaceManifest {
        let mut mounts = vec![
            ExecNamespaceMount::new(self.agent_mount.clone(), ExecNamespaceAccess::ReadWrite),
            ExecNamespaceMount::new(self.llm_mount.clone(), ExecNamespaceAccess::ReadWrite),
            ExecNamespaceMount::new(self.route_mount.clone(), ExecNamespaceAccess::ReadWrite),
            ExecNamespaceMount::new(self.srv_mount.clone(), ExecNamespaceAccess::ReadOnly),
        ];
        mounts.extend(
            self.bin_tool_mounts
                .iter()
                .cloned()
                .map(|path| ExecNamespaceMount::new(path, ExecNamespaceAccess::ReadOnly)),
        );
        mounts.extend(self.bin_tool_names().map(|name| {
            ExecNamespaceMount::new(format!("/lib/exec/{name}"), ExecNamespaceAccess::ReadOnly)
        }));
        mounts.extend(self.launch_context.host_mounts.iter().map(|grant| {
            ExecNamespaceMount::new(
                grant.namespace_path.clone(),
                match grant.access {
                    alan_kernel::Access::ReadOnly => ExecNamespaceAccess::ReadOnly,
                    alan_kernel::Access::ReadWrite => ExecNamespaceAccess::ReadWrite,
                },
            )
        }));
        mounts.extend(
            self.launch_context
                .package_references
                .iter()
                .filter_map(|reference| {
                    self.launch_context
                        .namespace
                        .resolve(&reference.namespace_path)
                        .ok()
                        .map(|resolved| {
                            let access = match resolved.access {
                                alan_kernel::Access::ReadOnly => ExecNamespaceAccess::ReadOnly,
                                alan_kernel::Access::ReadWrite => ExecNamespaceAccess::ReadWrite,
                            };
                            ExecNamespaceMount::new(reference.namespace_path.clone(), access)
                        })
                }),
        );
        mounts.extend(
            self.launch_context
                .descriptors
                .values()
                .filter(|descriptor| {
                    !self
                        .launch_context
                        .package_references
                        .iter()
                        .any(|reference| {
                            Path::new(&descriptor.path).starts_with(&reference.namespace_path)
                        })
                })
                .filter_map(|descriptor| {
                    self.launch_context
                        .namespace
                        .resolve(&descriptor.path)
                        .ok()
                        .map(|resolved| {
                            let access = match resolved.access {
                                alan_kernel::Access::ReadOnly => ExecNamespaceAccess::ReadOnly,
                                alan_kernel::Access::ReadWrite => ExecNamespaceAccess::ReadWrite,
                            };
                            ExecNamespaceMount::new(descriptor.path.clone(), access)
                        })
                }),
        );
        mounts.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.access.cmp(&right.access))
        });
        mounts.dedup();
        ExecNamespaceManifest { mounts }
    }
}

#[derive(Clone)]
#[cfg(test)]
pub(super) struct ChildNamespaceLaunchHandles {
    agent_tree: Arc<alan_agentfs::AgentFs>,
    llm_connection: InProcessTransport,
    srv: InProcessTransport,
    route: InProcessTransport,
    bin_tools: Vec<(String, InProcessTransport)>,
    tool_manifests: Vec<(String, InProcessTransport)>,
}

#[cfg(test)]
impl ChildNamespaceLaunchHandles {
    pub(super) fn new(
        agent_tree: Arc<alan_agentfs::AgentFs>,
        llm_connection: InProcessTransport,
        srv: InProcessTransport,
        route: InProcessTransport,
    ) -> Self {
        Self {
            agent_tree,
            llm_connection,
            srv,
            route,
            bin_tools: Vec::new(),
            tool_manifests: Vec::new(),
        }
    }

    pub(super) fn with_tool_package(
        mut self,
        bin_path: impl Into<String>,
        bin_tree: InProcessTransport,
        manifest_path: impl Into<String>,
        manifest_tree: InProcessTransport,
    ) -> Self {
        self.bin_tools.push((bin_path.into(), bin_tree));
        self.tool_manifests
            .push((manifest_path.into(), manifest_tree));
        self
    }
}

#[cfg(test)]
pub(super) struct ChildNamespaceRuntimeLaunch {
    pub(super) pid: String,
    pub(super) exec: ExecSpec,
    pub(super) environment: NamespaceRuntimeEnvironment,
    pub(super) agent_root: Arc<alan_agentfs::AgentRootFs>,
    pub(super) lifecycle: Arc<dyn AgentProcessLifecycle>,
}

#[cfg(test)]
#[derive(Clone)]
pub(super) struct TestParentProcessContext {
    pub(super) agent_root: Arc<alan_agentfs::AgentRootFs>,
    pub(super) pid: alan_kernel::Pid,
}

#[allow(
    clippy::too_many_arguments,
    reason = "arguments expose each namespace resource explicitly at the transitional assembly seam"
)]
#[cfg(test)]
pub(super) async fn spawn_child_namespace_runtime_environment(
    launch_procfs: &alan_kernel::ProcFs,
    runtime_procfs: &alan_kernel::ProcFs,
    plan: &ChildNamespaceAssemblyPlan,
    handles: ChildNamespaceLaunchHandles,
    parent_process_context: Option<TestParentProcessContext>,
    tool_runner: crate::tools::ToolProcessRunner,
    tool_binding: Option<crate::tools::ToolExecutionBinding>,
    mount_grant_applicator_factory: Option<Arc<dyn MountGrantApplicatorFactory>>,
    executable: &str,
) -> Result<ChildNamespaceRuntimeLaunch> {
    validate_child_namespace_launch_handles(plan, &handles)?;

    let (agent_root, parent_pid) = match parent_process_context {
        Some(context) => (context.agent_root, Some(context.pid)),
        None => (
            Arc::new(alan_agentfs::AgentRootFs::new(Arc::new(
                launch_procfs.clone(),
            ))),
            None,
        ),
    };
    let agent_root_tree = InProcessTransport::new(agent_root.clone());
    let spawner_namespace =
        child_spawner_namespace_from_launch_handles(plan, agent_root_tree.clone(), &handles);
    let spawner_procfs = launch_procfs.for_spawner(
        parent_pid,
        spawner_namespace,
        alan_kernel::Credentials::user("root-agent"),
    );
    let clone_fid = next_child_namespace_fid();
    spawner_procfs
        .walk(Fid::ROOT, clone_fid, &["clone".to_string()])
        .await
        .context("walk child /proc/clone")?;
    spawner_procfs
        .open(clone_fid, OpenMode::ReadWrite)
        .await
        .context("open child /proc/clone")?;
    let pid = String::from_utf8(
        spawner_procfs
            .read(clone_fid, 0, 64)
            .await
            .context("read child /proc/clone pid")?,
    )
    .context("child /proc/clone pid is utf8")?;
    let exec = plan.clone_exec_spec_for_pid(&pid, executable, std::iter::empty::<String>());
    let exec_bytes = serde_json::to_vec(&exec).context("serialize child exec spec")?;
    spawner_procfs
        .write(clone_fid, 0, &exec_bytes)
        .await
        .context("write child exec spec to /proc/clone")?;
    spawner_procfs
        .clunk(clone_fid)
        .await
        .context("commit child /proc/clone")?;
    agent_root
        .bind_process(pid.clone(), handles.agent_tree.clone())
        .await;

    let child_pid = alan_kernel::Pid(
        pid.parse::<u64>()
            .with_context(|| format!("parse child pid '{pid}'"))?,
    );
    if let Some(binding) = tool_binding {
        tool_runner.register_process_binding(child_pid, binding);
    }
    let child_namespace =
        child_runtime_namespace_from_launch_handles(plan, agent_root_tree, &handles);
    let live_namespace = alan_kernel::LiveNamespace::new(child_namespace);
    runtime_procfs
        .bind_live_namespace(child_pid, live_namespace.clone())
        .await;
    let child_procfs = runtime_procfs.for_live_spawner(
        Some(child_pid),
        live_namespace.clone(),
        alan_kernel::Credentials::user("child-agent"),
    );
    live_namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(child_procfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::from_live_namespace(
        live_namespace.clone(),
    )));
    let mut environment = NamespaceRuntimeEnvironment::new(
        root,
        format!("/agent/{pid}"),
        plan.llm_connection_name()?,
    )
    .with_launch_context(plan.launch_context.clone())
    .with_tool_process_context(child_pid, tool_runner.clone());
    let has_mount_grant_applicator = mount_grant_applicator_factory.is_some();
    if let Some(factory) = mount_grant_applicator_factory {
        let applicator = factory.create(child_pid, live_namespace, &[]);
        if let Some(authority) = factory.tool_execution_authority() {
            tool_runner.register_process_authority(child_pid, authority);
        }
        environment = environment.with_mount_grant_applicator(applicator);
    }
    if let Some(grant) = plan
        .launch_context
        .host_mounts
        .iter()
        .find(|grant| grant.namespace_path == "/agent-definition")
    {
        let applied = environment
            .mount_control()
            .apply_approved_grant(&ApprovedMountGrant::new(
                grant.namespace_path.clone(),
                grant.host_path.clone(),
                match grant.access {
                    alan_kernel::Access::ReadOnly => ApprovedMountGrantAccess::ReadOnly,
                    alan_kernel::Access::ReadWrite => ApprovedMountGrantAccess::ReadWrite,
                },
                "Agent Definition launch reference",
            ));
        if has_mount_grant_applicator {
            anyhow::ensure!(
                applied.namespace_applied,
                "failed to project child Agent Process definition: {}",
                applied
                    .namespace_error
                    .unwrap_or_else(|| "unknown projection error".to_string())
            );
        }
    }

    let lifecycle: Arc<dyn AgentProcessLifecycle> = Arc::new(TestAgentProcessLifecycle {
        procfs: launch_procfs.clone(),
        agent_root: agent_root.clone(),
        pid: child_pid,
    });
    Ok(ChildNamespaceRuntimeLaunch {
        pid,
        exec,
        environment,
        agent_root,
        lifecycle,
    })
}

#[cfg(test)]
struct TestAgentProcessLifecycle {
    procfs: alan_kernel::ProcFs,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    pid: alan_kernel::Pid,
}

#[cfg(test)]
impl std::fmt::Debug for TestAgentProcessLifecycle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestAgentProcessLifecycle")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl AgentProcessLifecycle for TestAgentProcessLifecycle {
    async fn finish(&self, exit_code: i32) {
        self.procfs.record_exit(self.pid, exit_code).await;
        self.agent_root
            .unbind_process(&self.pid.0.to_string())
            .await;
    }
}

#[cfg(test)]
fn validate_child_namespace_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    handles: &ChildNamespaceLaunchHandles,
) -> Result<()> {
    let expected: BTreeSet<&str> = plan.bin_tool_mounts.iter().map(String::as_str).collect();
    let actual: BTreeSet<&str> = handles
        .bin_tools
        .iter()
        .map(|(mount, _)| mount.as_str())
        .collect();
    let expected_manifests = plan
        .bin_tool_names()
        .map(|name| format!("/lib/exec/{name}"))
        .collect::<BTreeSet<_>>();
    let actual_manifests = handles
        .tool_manifests
        .iter()
        .map(|(mount, _)| mount.clone())
        .collect::<BTreeSet<_>>();
    if expected == actual && expected_manifests == actual_manifests {
        return Ok(());
    }

    let missing = expected
        .difference(&actual)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let unexpected = actual
        .difference(&expected)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "child namespace launch handles do not match plan: missing [{}], unexpected [{}]",
        missing,
        unexpected
    );
}

#[cfg(test)]
fn child_spawner_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    child_namespace_from_launch_handles(plan, agent_root_tree, handles)
}

#[cfg(test)]
fn child_runtime_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    child_namespace_from_launch_handles(plan, agent_root_tree, handles)
}

#[cfg(test)]
fn child_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    let mut namespace = plan.launch_context.namespace.child();
    namespace.mount(
        &plan.agent_mount,
        agent_root_tree,
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        &plan.llm_mount,
        handles.llm_connection.clone(),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        &plan.srv_mount,
        handles.srv.clone(),
        alan_kernel::Access::ReadOnly,
    );
    namespace.mount(
        &plan.route_mount,
        handles.route.clone(),
        alan_kernel::Access::ReadWrite,
    );
    for (mount, tree) in &handles.bin_tools {
        namespace.mount(mount, tree.clone(), alan_kernel::Access::ReadOnly);
    }
    for (mount, tree) in &handles.tool_manifests {
        namespace.mount(mount, tree.clone(), alan_kernel::Access::ReadOnly);
    }
    namespace
}

#[cfg(test)]
pub(super) async fn child_observation_environment(
    procfs: &alan_kernel::ProcFs,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    pid: &str,
    plan: &ChildNamespaceAssemblyPlan,
) -> Result<NamespaceRuntimeEnvironment> {
    let agent_path = format!("/agent/{pid}");
    let agent_tree = agent_root
        .process_tree(pid)
        .await
        .with_context(|| format!("attach observer to child AgentFS {agent_path}"))?;
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        &agent_path,
        InProcessTransport::new(agent_tree),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        alan_kernel::Access::ReadWrite,
    );
    Ok(NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace))),
        agent_path,
        plan.llm_connection_name.clone(),
    )
    .with_launch_context(plan.launch_context.clone()))
}

#[cfg(test)]
fn next_child_namespace_fid() -> Fid {
    Fid(NEXT_CHILD_NAMESPACE_FID.fetch_add(1, Ordering::Relaxed))
}
