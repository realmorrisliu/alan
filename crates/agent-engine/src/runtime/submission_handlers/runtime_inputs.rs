use std::sync::Arc;

use crate::{
    agent_machine::{AgentMachine, PendingHostMountRequest},
    runtime::{
        service_activation::ServiceActivationAuthority,
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
    pub(super) service_activation: Option<Arc<dyn ServiceActivationAuthority>>,
    pub(super) smart_auto_extensions: bool,
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
            service_activation: None,
            smart_auto_extensions: false,
        }
    }

    pub(crate) fn with_service_activation(
        mut self,
        service_activation: Option<Arc<dyn ServiceActivationAuthority>>,
    ) -> Self {
        self.service_activation = service_activation;
        self
    }

    pub(crate) fn with_smart_auto_extensions(mut self, enabled: bool) -> Self {
        self.smart_auto_extensions = enabled;
        self
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
        reset_turn_after_cancelling_host_mounts(
            self.machine,
            &self.agent_files,
            &self.host_mount_requests,
        )
        .await
    }

    pub(super) fn preserve_approved_host_mount(
        &mut self,
        pending: &PendingHostMountRequest,
        terminal: &HostMountTerminalResult,
    ) -> Result<()> {
        preserve_approved_host_mount(pending, terminal)
    }
}
