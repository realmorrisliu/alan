use crate::{
    agent_machine::AgentMachine,
    policy::PolicyEngine,
    runtime::{
        child_runs::ChildRunRegistry,
        transition::{NamespaceAgentFiles, NamespaceToolExecution},
    },
};

/// Explicit inputs for one child-run termination Tool transition.
pub(crate) struct ChildRunTerminationRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) policy_engine: &'a PolicyEngine,
    pub(super) governance: &'a alan_agent_protocol::GovernanceConfig,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) tool_execution: NamespaceToolExecution,
    pub(super) child_run_registry: ChildRunRegistry,
    pub(super) parent_process_path: String,
}

impl<'a> ChildRunTerminationRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        policy_engine: &'a PolicyEngine,
        governance: &'a alan_agent_protocol::GovernanceConfig,
        agent_files: NamespaceAgentFiles,
        tool_execution: NamespaceToolExecution,
        child_run_registry: ChildRunRegistry,
        parent_process_path: String,
    ) -> Self {
        Self {
            machine,
            policy_engine,
            governance,
            agent_files,
            tool_execution,
            child_run_registry,
            parent_process_path,
        }
    }
}
