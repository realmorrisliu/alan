use std::path::PathBuf;

use crate::{
    agent_machine::AgentMachine,
    runtime::transition::{NamespaceAgentFiles, NamespaceMountControl, NamespaceToolExecution},
};

/// Explicit inputs for classifying and applying one non-compaction runtime operation.
pub(crate) struct SubmissionRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) mount_control: NamespaceMountControl<'a>,
    pub(super) tool_execution: NamespaceToolExecution,
    pub(super) runtime_scratch_dir: Option<PathBuf>,
}

impl<'a> SubmissionRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        agent_files: NamespaceAgentFiles,
        mount_control: NamespaceMountControl<'a>,
        tool_execution: NamespaceToolExecution,
        runtime_scratch_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            machine,
            agent_files,
            mount_control,
            tool_execution,
            runtime_scratch_dir,
        }
    }
}
