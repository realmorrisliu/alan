use std::{fs::File, path::Path};

use alan_agent_protocol::UiActivityState;
use anyhow::{Result, bail};

use super::{acquire_task_submission_lock, file_surface};

pub(super) async fn prepare_root_agent_submission(
    shell: &alan_shell::Shell,
    agent_path: &str,
    lock_path: Option<&Path>,
) -> Result<Option<File>> {
    let lock = lock_path.map(acquire_task_submission_lock).transpose()?;
    let activity = file_surface::read_activity_snapshot(shell, agent_path).await?;
    require_root_agent_idle(activity.state)?;
    Ok(lock)
}

pub(super) fn require_root_agent_idle(activity: UiActivityState) -> Result<()> {
    match activity {
        UiActivityState::Idle => Ok(()),
        UiActivityState::Running => bail!("Root Agent is already working; retry after it finishes"),
        UiActivityState::Paused => {
            bail!("Root Agent is waiting for interactive input; resume its pending request first")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};

    #[test]
    fn root_agent_submission_requires_idle_activity() {
        assert!(require_root_agent_idle(UiActivityState::Idle).is_ok());
        assert!(
            require_root_agent_idle(UiActivityState::Running)
                .unwrap_err()
                .to_string()
                .contains("already working")
        );
        assert!(
            require_root_agent_idle(UiActivityState::Paused)
                .unwrap_err()
                .to_string()
                .contains("resume its pending request")
        );
    }

    #[tokio::test]
    async fn active_root_agent_blocks_tty_submission_after_the_old_renderer_exits() {
        let activity =
            serde_json::to_vec(&alan_agent_protocol::UiActivitySnapshot::running(1)).unwrap();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/root/machine/ui",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "activity", activity,
            ))),
            Access::ReadOnly,
        );
        let shell =
            alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))));
        let runtime = tempfile::tempdir().unwrap();
        let lock_path = runtime.path().join("task.lock");

        let err = prepare_root_agent_submission(&shell, "/agent/root", Some(&lock_path))
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Root Agent is already working"));
        drop(acquire_task_submission_lock(&lock_path).unwrap());
    }
}
