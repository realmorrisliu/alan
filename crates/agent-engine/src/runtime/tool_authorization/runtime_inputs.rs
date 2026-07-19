use crate::{
    agent_machine::AgentMachine,
    policy::PolicyEngine,
    runtime::transition::{NamespaceAgentFiles, NamespaceGeneration},
};

/// Explicit inputs for one Tool policy and approval transition.
pub(crate) struct ToolAuthorizationRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) policy_engine: &'a PolicyEngine,
    pub(super) governance: &'a alan_agent_protocol::GovernanceConfig,
    pub(super) generation: NamespaceGeneration,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) llm_request_timeout_secs: u64,
}

impl<'a> ToolAuthorizationRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        policy_engine: &'a PolicyEngine,
        governance: &'a alan_agent_protocol::GovernanceConfig,
        generation: NamespaceGeneration,
        agent_files: NamespaceAgentFiles,
        llm_request_timeout_secs: u64,
    ) -> Self {
        Self {
            machine,
            policy_engine,
            governance,
            generation,
            agent_files,
            llm_request_timeout_secs,
        }
    }
}
