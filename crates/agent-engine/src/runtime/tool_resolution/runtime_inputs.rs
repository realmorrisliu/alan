use crate::{agent_machine::AgentMachine, runtime::transition::NamespaceToolExecution};

/// Explicit inputs for resolving one mounted Tool package for execution.
pub(crate) struct ToolResolutionRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) tool_execution: NamespaceToolExecution,
}

impl<'a> ToolResolutionRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        tool_execution: NamespaceToolExecution,
    ) -> Self {
        Self {
            machine,
            tool_execution,
        }
    }
}
