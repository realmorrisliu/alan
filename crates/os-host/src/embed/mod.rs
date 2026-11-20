//! Self-contained, embeddable Alan runtime instances with isolated storage and namespaces.

pub mod client;
pub mod config;
pub mod policy_hook;
#[cfg(test)]
mod tests;

#[path = "../embed_events.rs"]
mod embed_events;

use std::path::Path;

pub use alan_agent_engine::{
    AgentProcessConfig, HookDecision, LlmClient, SharedLlmProvider, SystemPromptMode,
    ToolPolicyHook,
};
pub use alan_agent_protocol::{UiInputStatus, UiUsageSnapshot};
pub use alan_ap::{FileServer, InProcessTransport};
pub use alan_kernel::{Access, Pid};
pub use alan_service_manager::{
    HostMountAccess, HostMountExportAdapter, HostMountGrantRecord, LlmClientFactory,
    ProcessLaunchContext, ServiceManager, SessionLaunchOptions, UnavailableHostMountExportAdapter,
};
use alan_shell::Shell;
use anyhow::{Result, ensure};
use uuid::Uuid;

pub use client::{AgentSessionHandle, RootAgentClient};
pub use config::{EmbeddedConfig, LlmSource, StorageMode};
pub use embed_events::{EmbedEvent, EmbedEventStream};

/// Convenience alias for session creation/reset options in embedded Alan apps.
pub type SessionOptions = SessionLaunchOptions;

/// One running, self-contained embedded Alan runtime instance.
pub struct EmbeddedAlan {
    manager: Option<ServiceManager>,
    root_transport: InProcessTransport,
    host_mounts_enabled: bool,
}

impl EmbeddedAlan {
    /// Boot an isolated embedded Alan instance in-process.
    pub async fn boot(config: EmbeddedConfig) -> Result<Self> {
        let (sm_config, host_mounts_enabled) = config.into_service_manager_config()?;
        let manager = ServiceManager::boot(sm_config).await?;
        let root_transport =
            InProcessTransport::new(manager.local_entry().namespace_for_local_client());
        Ok(Self {
            manager: Some(manager),
            root_transport,
            host_mounts_enabled,
        })
    }

    fn manager(&self) -> &ServiceManager {
        self.manager
            .as_ref()
            .expect("EmbeddedAlan manager remains valid until shutdown")
    }

    /// Unique boot identifier for this embedded instance.
    pub fn boot_id(&self) -> Uuid {
        self.manager().boot_id()
    }

    /// Process ID of the instance's active Root Agent Process.
    pub fn root_pid(&self) -> Pid {
        self.manager().root_pid()
    }

    /// Raw `aP` transport bound to the root namespace of this instance.
    pub fn root_transport(&self) -> InProcessTransport {
        self.root_transport.clone()
    }

    /// Typed `aP`-only [`Shell`] bound to the root namespace of this instance.
    pub fn shell(&self) -> Shell {
        Shell::new(self.root_transport.clone())
    }

    /// Convenience handle targeting the instance's primary Root Agent Process (`/agent/root`).
    pub fn root_agent(&self) -> RootAgentClient {
        RootAgentClient::new(self.shell(), "/agent/root")
    }

    /// Open a new independent interactive Agent session on this running instance.
    ///
    /// Reuses the booted `ServiceManager`, kernel, and shared LLM provider factory without
    /// altering `/agent/root`.
    pub async fn open_session(&self, options: SessionOptions) -> Result<AgentSessionHandle> {
        let session = self.manager().open_session(options).await?;
        Ok(AgentSessionHandle::new(session))
    }

    /// Spawn an interactive child Agent session parented to `parent_pid`.
    pub async fn spawn_child_session(
        &self,
        parent_pid: Pid,
        options: SessionOptions,
    ) -> Result<AgentSessionHandle> {
        let session = self
            .manager()
            .spawn_child_session(parent_pid, options)
            .await?;
        Ok(AgentSessionHandle::new(session))
    }

    /// Close and release a session opened via [`Self::open_session`] or [`Self::spawn_child_session`].
    pub async fn close_session(&self, pid: Pid) -> Result<()> {
        self.manager().close_session(pid).await
    }

    /// Shut down and relaunch the primary Root Agent Process cleanly without rebooting Service Manager.
    pub async fn reset_session(&self, options: SessionOptions) -> Result<Pid> {
        self.manager().reset_session(options).await
    }

    /// Authorize and project a Host directory grant onto the active Root Agent Process.
    ///
    /// Requires [`StorageMode::Isolated`] or explicit native Host Mount adapter activation.
    pub fn host_mount(
        &self,
        host_path: impl AsRef<Path>,
        access: HostMountAccess,
    ) -> Result<HostMountGrantRecord> {
        self.host_mount_for(self.root_pid(), host_path, access)
    }

    /// Authorize and project a Host directory grant onto a specific running session Process.
    pub fn host_mount_for(
        &self,
        pid: Pid,
        host_path: impl AsRef<Path>,
        access: HostMountAccess,
    ) -> Result<HostMountGrantRecord> {
        ensure!(
            self.host_mounts_enabled,
            "Host Mounts are disabled on this EmbeddedAlan instance"
        );
        let host_mount = self.manager().host_mount();
        let request_id = host_mount.request_project_mount(pid, access)?;
        crate::host_mounts::approve_host_mount(
            &host_mount,
            &request_id,
            host_path.as_ref(),
            "embedded-host-mount",
            "embedded-host",
        )
    }

    /// Cleanly stop the embedded Alan instance and its Service Manager.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(manager) = self.manager.take() {
            manager.shutdown().await?;
        }
        Ok(())
    }
}

impl Drop for EmbeddedAlan {
    fn drop(&mut self) {
        if let Some(manager) = self.manager.as_mut() {
            manager.signal_shutdown();
        }
    }
}
