use crate::{
    agent_machine::AgentMachine,
    runtime::{
        child_agents::{ChildLaunchRuntime, project_child_task_context},
        child_runs::ChildRunRegistry,
        launch_config::AgentConfig,
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
    pub(super) machine: &'a mut AgentMachine,
    pub(super) prompt_cache: &'a mut PromptAssemblyCache,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) host_mount_requests: NamespaceHostMountRequests,
    child_runtime_inputs: DelegatedChildRuntimeInputs,
}

/// Owned inputs that can produce one real child-launch handle after the spawn spec is resolved.
pub(crate) struct DelegatedChildRuntimeInputs {
    base_agent_config: AgentConfig,
    child_launch: NamespaceChildLaunch,
    tool_execution: NamespaceToolExecution,
    child_run_registry: ChildRunRegistry,
    parent_process_path: String,
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

    pub(super) fn child_launch_context(&self) -> Option<&crate::ProcessLaunchContext> {
        self.child_runtime_inputs.child_launch.launch_context()
    }

    pub(super) fn child_run_registry(&self) -> &ChildRunRegistry {
        &self.child_runtime_inputs.child_run_registry
    }

    pub(super) fn child_launch_runtime(&self, spec: &SpawnSpec) -> ChildLaunchRuntime {
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
            self.child_runtime_inputs.base_agent_config.clone(),
            self.child_runtime_inputs.child_launch.clone(),
            self.child_runtime_inputs.tool_execution.clone(),
            self.child_runtime_inputs.child_run_registry.clone(),
            self.child_runtime_inputs.parent_process_path.clone(),
            self.prompt_cache.capability_view().cloned(),
            task_context,
        )
    }
}

impl DelegatedChildRuntimeInputs {
    pub(crate) fn new(
        base_agent_config: AgentConfig,
        child_launch: NamespaceChildLaunch,
        tool_execution: NamespaceToolExecution,
        child_run_registry: ChildRunRegistry,
        parent_process_path: String,
    ) -> Self {
        Self {
            base_agent_config,
            child_launch,
            tool_execution,
            child_run_registry,
            parent_process_path,
        }
    }
}
