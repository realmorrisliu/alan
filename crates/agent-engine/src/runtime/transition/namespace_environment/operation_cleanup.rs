//! Bounded retirement of adapter operations whose owning runtime was dropped.

use super::client::NamespaceClient;
use anyhow::{Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio_util::task::TaskTracker;

#[derive(Clone, Default)]
pub(crate) struct NamespaceOperationCleanup {
    tasks: TaskTracker,
    uncertain: Arc<AtomicBool>,
}

impl NamespaceOperationCleanup {
    pub(crate) async fn finish(&self) -> Result<()> {
        self.tasks.close();
        self.tasks.wait().await;
        ensure!(
            !self.uncertain.load(Ordering::Acquire),
            "namespace operation cleanup is unconfirmed"
        );
        Ok(())
    }

    pub(super) fn abort_on_drop(
        &self,
        client: NamespaceClient,
        path: String,
        command: &'static [u8],
    ) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.uncertain.store(true, Ordering::Release);
            return;
        };
        let mut attempt = CleanupAttempt {
            uncertain: self.uncertain.clone(),
            confirmed: false,
        };
        self.tasks.spawn_on(
            async move {
                let result = tokio::time::timeout(Duration::from_secs(1), async {
                    match client.write_document(&path, command).await {
                        Ok(()) => Ok(()),
                        Err(error)
                            if command == b"abort"
                                && error.downcast_ref::<alan_ap::ErrorCode>()
                                    == Some(&alan_ap::ErrorCode::BadRequest) =>
                        {
                            // An already-terminal model operation refuses another abort.
                            let status = client
                                .read_file(&format!("{}/status", path.trim_end_matches("/ctl")))
                                .await?;
                            let status: serde_json::Value = serde_json::from_slice(&status)?;
                            ensure!(
                                matches!(
                                    status["status"].as_str(),
                                    Some("done" | "error" | "rejected" | "aborted")
                                ),
                                "model operation is not terminal"
                            );
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                })
                .await;
                if matches!(result, Ok(Ok(()))) {
                    attempt.confirm();
                } else {
                    tracing::warn!("Dropped runtime operation cleanup is unconfirmed");
                }
            },
            &runtime,
        );
    }

    pub(super) fn check_admission(&self) -> Result<()> {
        ensure!(
            !self.tasks.is_closed(),
            "Process operation scope has closed"
        );
        ensure!(
            !self.uncertain.load(Ordering::Acquire),
            "previous operation cleanup is unconfirmed"
        );
        Ok(())
    }

    pub(super) fn lease(
        &self,
        client: NamespaceClient,
        command: &'static [u8],
    ) -> Result<OperationLease> {
        self.check_admission()?;
        Ok(OperationLease {
            cleanup: self.clone(),
            client,
            path: None,
            command,
            armed: true,
        })
    }
}

// A panicked or dropped cleanup job must not look like an acknowledged cancellation.
struct CleanupAttempt {
    uncertain: Arc<AtomicBool>,
    confirmed: bool,
}
impl CleanupAttempt {
    fn confirm(&mut self) {
        self.confirmed = true;
    }
}

impl Drop for CleanupAttempt {
    fn drop(&mut self) {
        if !self.confirmed {
            self.uncertain.store(true, Ordering::Release);
        }
    }
}

pub(super) struct OperationLease {
    cleanup: NamespaceOperationCleanup,
    client: NamespaceClient,
    path: Option<String>,
    command: &'static [u8],
    armed: bool,
}

impl std::fmt::Debug for OperationLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperationLease")
            .field("armed", &self.armed)
            .finish_non_exhaustive()
    }
}

impl OperationLease {
    pub(super) fn capture(&mut self, path: String) -> Result<()> {
        // Retain the known identity even if scope admission changed during allocation.
        self.path = Some(path);
        self.cleanup.check_admission()
    }

    pub(super) fn settled(&mut self) {
        self.armed = false;
    }
}

impl Drop for OperationLease {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(path) = &self.path {
            self.cleanup
                .abort_on_drop(self.client.clone(), path.clone(), self.command);
        }
    }
}

#[cfg(test)]
mod tests;
