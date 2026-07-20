use crate::{
    agent_machine::{AgentMachine, PendingHostMountRequest},
    runtime::{
        transition::{HostMountTerminalResult, NamespaceAgentFiles, NamespaceHostMountRequests},
        turn_support::{
            cancel_current_task, preserve_approved_host_mount,
            reset_turn_after_cancelling_host_mounts,
        },
    },
};
use alan_agent_protocol::Event;
use anyhow::Result;

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

    pub(super) async fn cancel_current_task<E, F>(&mut self, emit: &mut E) -> Result<()>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        cancel_current_task(
            self.machine,
            &self.agent_files,
            &self.host_mount_requests,
            emit,
        )
        .await
    }

    pub(super) async fn reset_turn_after_cancelling_host_mounts(&mut self) -> Result<()> {
        reset_turn_after_cancelling_host_mounts(self.machine, &self.host_mount_requests).await
    }

    pub(super) fn preserve_approved_host_mount(
        &mut self,
        pending: &PendingHostMountRequest,
        terminal: &HostMountTerminalResult,
    ) -> Result<()> {
        preserve_approved_host_mount(pending, terminal)
    }
}
