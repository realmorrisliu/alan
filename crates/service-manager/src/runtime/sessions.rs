use alan_ap::InProcessTransport;
use alan_kernel::Pid;
use anyhow::{Context, Result};

use super::ServiceManager;
use super::supervisor::SupervisorCommand;
pub use crate::agent_runtime::sessions::SessionLaunchOptions;

/// Handle returned for an opened or child interactive session running inside one [`ServiceManager`].
#[derive(Clone)]
pub struct SessionHandle {
    pub pid: Pid,
    pub agent_path: String,
    pub namespace: InProcessTransport,
    closer: tokio::sync::mpsc::UnboundedSender<SupervisorCommand>,
}

impl SessionHandle {
    /// Close and release this interactive session from its owning [`ServiceManager`].
    pub async fn close(&self) -> Result<()> {
        close_via_channel(&self.closer, self.pid).await
    }
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("pid", &self.pid)
            .field("agent_path", &self.agent_path)
            .finish_non_exhaustive()
    }
}

async fn close_via_channel(
    sender: &tokio::sync::mpsc::UnboundedSender<SupervisorCommand>,
    pid: Pid,
) -> Result<()> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    sender
        .send(SupervisorCommand::Close {
            pid,
            reply: reply_tx,
        })
        .map_err(|_| anyhow::anyhow!("Service Manager supervisor has stopped"))?;
    reply_rx
        .await
        .context("Service Manager dropped close_session reply")?
}

impl ServiceManager {
    /// Open an independent interactive Agent session on this running [`ServiceManager`].
    pub async fn open_session(&self, options: SessionLaunchOptions) -> Result<SessionHandle> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.supervisor_cmd
            .send(SupervisorCommand::Open {
                options,
                reply: reply_tx,
            })
            .map_err(|_| anyhow::anyhow!("Service Manager supervisor has stopped"))?;
        let (pid, namespace) = reply_rx
            .await
            .context("Service Manager dropped open_session reply")??;
        Ok(SessionHandle {
            pid,
            agent_path: format!("/agent/{}", pid.0),
            namespace,
            closer: self.supervisor_cmd.clone(),
        })
    }

    /// Spawn an interactive child session whose Host Mount lineage belongs to `parent_pid`.
    pub async fn spawn_child_session(
        &self,
        parent_pid: Pid,
        options: SessionLaunchOptions,
    ) -> Result<SessionHandle> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.supervisor_cmd
            .send(SupervisorCommand::SpawnChild {
                parent_pid,
                options,
                reply: reply_tx,
            })
            .map_err(|_| anyhow::anyhow!("Service Manager supervisor has stopped"))?;
        let (pid, namespace) = reply_rx
            .await
            .context("Service Manager dropped spawn_child_session reply")??;
        Ok(SessionHandle {
            pid,
            agent_path: format!("/agent/{}", pid.0),
            namespace,
            closer: self.supervisor_cmd.clone(),
        })
    }

    /// Stop and release an interactive session opened with [`Self::open_session`] or [`Self::spawn_child_session`].
    pub async fn close_session(&self, pid: Pid) -> Result<()> {
        close_via_channel(&self.supervisor_cmd, pid).await
    }

    /// Replace the primary Root Agent Process cleanly without rebooting Service Manager.
    pub async fn reset_session(&self, options: SessionLaunchOptions) -> Result<Pid> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.supervisor_cmd
            .send(SupervisorCommand::ResetRoot {
                options,
                reply: reply_tx,
            })
            .map_err(|_| anyhow::anyhow!("Service Manager supervisor has stopped"))?;
        reply_rx
            .await
            .context("Service Manager dropped reset_session reply")?
    }
}
