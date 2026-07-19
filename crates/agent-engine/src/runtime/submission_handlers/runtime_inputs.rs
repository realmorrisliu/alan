use crate::{
    agent_machine::AgentMachine,
    runtime::transition::{NamespaceAgentFiles, NamespaceHostMountRequests},
};

/// Explicit inputs for classifying and applying one non-compaction runtime operation.
pub(crate) struct SubmissionRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) host_mount_requests: NamespaceHostMountRequests,
}

impl<'a> SubmissionRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        agent_files: NamespaceAgentFiles,
        host_mount_requests: NamespaceHostMountRequests,
    ) -> Self {
        Self {
            machine,
            agent_files,
            host_mount_requests,
        }
    }
}
