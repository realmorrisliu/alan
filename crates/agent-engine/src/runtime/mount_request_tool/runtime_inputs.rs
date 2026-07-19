use crate::{
    agent_machine::AgentMachine,
    policy::PolicyEngine,
    runtime::transition::{NamespaceAgentFiles, NamespaceToolExecution},
};

/// Explicit inputs for one approval-gated mount-request Tool transition.
pub(crate) struct MountRequestRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) policy_engine: &'a PolicyEngine,
    pub(super) governance: &'a alan_agent_protocol::GovernanceConfig,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) tool_execution: NamespaceToolExecution,
}

impl<'a> MountRequestRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        policy_engine: &'a PolicyEngine,
        governance: &'a alan_agent_protocol::GovernanceConfig,
        agent_files: NamespaceAgentFiles,
        tool_execution: NamespaceToolExecution,
    ) -> Self {
        Self {
            machine,
            policy_engine,
            governance,
            agent_files,
            tool_execution,
        }
    }
}
