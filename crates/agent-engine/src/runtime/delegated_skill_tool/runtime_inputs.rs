use std::path::PathBuf;

use crate::{
    TaskBudgetBinding,
    agent_machine::AgentMachine,
    runtime::{
        async_workstreams::AsyncWorkstreamRegistry,
        child_agents::{ChildLaunchRuntime, project_child_task_context},
        child_runs::ChildRunRegistry,
        project_startup::ProjectWorkstreamStoreRef,
        prompt_cache::PromptAssemblyCache,
        transition::{
            NamespaceAgentFiles, NamespaceChildLaunch, NamespaceHostMountRequests,
            NamespaceToolExecution,
        },
    },
};
use alan_agent_protocol::SpawnSpec;

/// Explicit inputs for one delegated-skill Tool transition.
pub(crate) struct DelegatedSkillRuntime<'a> {
    pub(in crate::runtime) machine: &'a mut AgentMachine,
    pub(in crate::runtime) prompt_cache: &'a mut PromptAssemblyCache,
    pub(in crate::runtime) agent_files: NamespaceAgentFiles,
    pub(in crate::runtime) host_mount_requests: NamespaceHostMountRequests,
    child_runtime_inputs: DelegatedChildRuntimeInputs,
}

/// Owned inputs that can produce one real child-launch handle after the spawn spec is resolved.
#[derive(Clone)]
pub(crate) struct DelegatedChildRuntimeInputs {
    child_launch: NamespaceChildLaunch,
    tool_execution: NamespaceToolExecution,
    child_run_registry: ChildRunRegistry,
    async_workstreams: AsyncWorkstreamRegistry,
    task_budget_binding: TaskBudgetBinding,
    parent_process_path: String,
    workstream_store: Option<ProjectWorkstreamStoreRef>,
    rollouts_dir: Option<PathBuf>,
}

impl<'a> DelegatedSkillRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        prompt_cache: &'a mut PromptAssemblyCache,
        agent_files: NamespaceAgentFiles,
        host_mount_requests: NamespaceHostMountRequests,
        child_runtime_inputs: DelegatedChildRuntimeInputs,
    ) -> Self {
        Self {
            machine,
            prompt_cache,
            agent_files,
            host_mount_requests,
            child_runtime_inputs,
        }
    }

    pub(in crate::runtime) async fn publish_ensured_skills(&mut self) {
        let mut last = self.prompt_cache.skill_publication.clone();
        match crate::runtime::ui_surfaces::publish_skills(
            &self.agent_files,
            self.prompt_cache,
            self.child_runtime_inputs.parent_process_path.clone(),
            &mut last,
        )
        .await
        {
            Ok(()) => self.prompt_cache.skill_publication = last,
            Err(error) => tracing::warn!(%error, "Delegated skill presentation publication failed"),
        }
    }

    pub(in crate::runtime) async fn publish_prelaunch_rejection(&self, message: &str) {
        if let Err(error) =
            crate::runtime::ui_surfaces::error_notice(&self.agent_files, message).await
        {
            tracing::warn!(%error, "Delegated skill rejection publication failed");
        }
    }

    pub(in crate::runtime) fn child_namespace_cwd(&self) -> &std::path::Path {
        self.child_runtime_inputs.child_launch.namespace_cwd()
    }

    pub(in crate::runtime) fn child_run_registry(&self) -> &ChildRunRegistry {
        &self.child_runtime_inputs.child_run_registry
    }

    pub(in crate::runtime) fn async_workstreams(&self) -> &AsyncWorkstreamRegistry {
        &self.child_runtime_inputs.async_workstreams
    }

    pub(in crate::runtime) async fn authorize_async_workstream(
        &self,
    ) -> anyhow::Result<Option<String>> {
        self.child_runtime_inputs
            .task_budget_binding
            .authorize_async_workstream()
            .await
    }

    pub(in crate::runtime) fn tool_execution(&self) -> &NamespaceToolExecution {
        &self.child_runtime_inputs.tool_execution
    }

    pub(in crate::runtime) fn workstream_store(&self) -> Option<&ProjectWorkstreamStoreRef> {
        self.child_runtime_inputs.workstream_store.as_ref()
    }

    pub(in crate::runtime) fn rollouts_dir(&self) -> Option<&std::path::Path> {
        self.child_runtime_inputs.rollouts_dir.as_deref()
    }

    pub(in crate::runtime) fn child_launch_runtime(&self, spec: &SpawnSpec) -> ChildLaunchRuntime {
        self.child_launch_runtime_with_lease(spec, None)
    }

    pub(in crate::runtime) fn child_launch_runtime_with_lease(
        &self,
        spec: &SpawnSpec,
        async_workstream_lease: Option<String>,
    ) -> ChildLaunchRuntime {
        let (plan_explanation, plan_items) = match self.machine.plan_snapshot() {
            Some(snapshot) => (snapshot.explanation.as_deref(), snapshot.items.as_slice()),
            None => (None, &[][..]),
        };
        let task_context = project_child_task_context(
            self.machine.tape_summary(),
            self.machine.messages(),
            plan_explanation,
            plan_items,
            spec,
        );
        ChildLaunchRuntime::new(
            self.child_runtime_inputs.child_launch.clone(),
            self.child_runtime_inputs.tool_execution.clone(),
            self.child_runtime_inputs.child_run_registry.clone(),
            self.child_runtime_inputs.parent_process_path.clone(),
            self.prompt_cache.capability_view().cloned(),
            task_context,
        )
        .with_async_workstream_lease(async_workstream_lease)
    }
}

impl DelegatedChildRuntimeInputs {
    pub(crate) fn new(
        child_launch: NamespaceChildLaunch,
        tool_execution: NamespaceToolExecution,
        child_run_registry: ChildRunRegistry,
        parent_process_path: String,
    ) -> Self {
        Self {
            child_launch,
            tool_execution,
            child_run_registry,
            async_workstreams: AsyncWorkstreamRegistry::new(),
            task_budget_binding: TaskBudgetBinding::default(),
            parent_process_path,
            workstream_store: None,
            rollouts_dir: None,
        }
    }

    pub(crate) fn with_async_workstreams(
        mut self,
        async_workstreams: AsyncWorkstreamRegistry,
    ) -> Self {
        self.async_workstreams = async_workstreams;
        self
    }

    pub(crate) fn with_task_budget_binding(
        mut self,
        task_budget_binding: TaskBudgetBinding,
    ) -> Self {
        self.task_budget_binding = task_budget_binding;
        self
    }

    pub(crate) fn with_workstream_store(
        mut self,
        workstream_store: Option<ProjectWorkstreamStoreRef>,
    ) -> Self {
        self.workstream_store = workstream_store;
        self
    }

    pub(crate) fn with_rollouts_dir(mut self, rollouts_dir: Option<PathBuf>) -> Self {
        self.rollouts_dir = rollouts_dir;
        self
    }
}
