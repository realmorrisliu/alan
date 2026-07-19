use crate::{
    runtime::{
        child_runs::ChildRunRegistry,
        launch_config::AgentConfig,
        transition::{NamespaceChildLaunch, NamespaceToolExecution},
    },
    skills::ResolvedCapabilityView,
};

pub(crate) struct ChildTaskContext {
    pub(super) conversation_snapshot: Option<String>,
    pub(super) plan_snapshot: Option<String>,
    pub(super) tool_results_snapshot: Option<String>,
}

impl ChildTaskContext {
    pub(crate) fn new(
        conversation_snapshot: Option<String>,
        plan_snapshot: Option<String>,
        tool_results_snapshot: Option<String>,
    ) -> Self {
        Self {
            conversation_snapshot,
            plan_snapshot,
            tool_results_snapshot,
        }
    }
}

pub(crate) struct ChildLaunchRuntime {
    pub(super) base_agent_config: AgentConfig,
    pub(super) child_launch: NamespaceChildLaunch,
    pub(super) tool_execution: NamespaceToolExecution,
    pub(super) child_run_registry: ChildRunRegistry,
    pub(super) parent_process_path: String,
    pub(super) capability_view: Option<ResolvedCapabilityView>,
    pub(super) task_context: ChildTaskContext,
}

impl ChildLaunchRuntime {
    pub(crate) fn new(
        base_agent_config: AgentConfig,
        child_launch: NamespaceChildLaunch,
        tool_execution: NamespaceToolExecution,
        child_run_registry: ChildRunRegistry,
        parent_process_path: String,
        capability_view: Option<ResolvedCapabilityView>,
        task_context: ChildTaskContext,
    ) -> Self {
        Self {
            base_agent_config,
            child_launch,
            tool_execution,
            child_run_registry,
            parent_process_path,
            capability_view,
            task_context,
        }
    }
}
