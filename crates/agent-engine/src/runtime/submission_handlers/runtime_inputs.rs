use crate::{
    agent_machine::AgentMachine,
    runtime::transition::{NamespaceAgentFiles, NamespaceHostMountRequests, NamespaceMountControl},
};

/// Explicit inputs for classifying and applying one non-compaction runtime operation.
pub(crate) struct SubmissionRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) host_mount_requests: NamespaceHostMountRequests,
    pub(super) mount_control: NamespaceMountControl<'a>,
}

impl<'a> SubmissionRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        agent_files: NamespaceAgentFiles,
        host_mount_requests: NamespaceHostMountRequests,
        mount_control: NamespaceMountControl<'a>,
    ) -> Self {
        Self {
            machine,
            agent_files,
            host_mount_requests,
            mount_control,
        }
    }

    pub(super) fn record_projected_host_mount(
        &mut self,
        grant_reference: String,
        namespace_path: String,
        access: alan_kernel::Access,
    ) {
        self.mount_control
            .record_projected_host_mount(grant_reference, namespace_path, access);
    }
}
