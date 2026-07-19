use crate::{agent_machine::AgentMachine, runtime::transition::NamespaceAgentFiles};

/// Explicit inputs for Agent-facing confirmation, structured-input, and plan Tool transitions.
pub(crate) struct AgentInteractionRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) agent_files: NamespaceAgentFiles,
}

impl<'a> AgentInteractionRuntime<'a> {
    pub(crate) fn new(machine: &'a mut AgentMachine, agent_files: NamespaceAgentFiles) -> Self {
        Self {
            machine,
            agent_files,
        }
    }
}
