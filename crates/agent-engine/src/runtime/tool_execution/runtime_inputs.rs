use crate::{
    agent_machine::AgentMachine,
    runtime::transition::{NamespaceAgentFiles, NamespaceToolExecution},
};

/// Explicit inputs for one policy-approved Tool Process execution transition.
pub(crate) struct ToolExecutionRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) tool_execution: NamespaceToolExecution,
    pub(super) process_path: String,
}

impl<'a> ToolExecutionRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        agent_files: NamespaceAgentFiles,
        tool_execution: NamespaceToolExecution,
        process_path: String,
    ) -> Self {
        Self {
            machine,
            agent_files,
            tool_execution,
            process_path,
        }
    }
}
