//! Runtime readiness, control, and shutdown ownership.

use alan_agent_protocol::Submission;
use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Effective durability state for a runtime machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentMachineDurabilityState {
    /// Whether the active machine has a persistent recorder attached.
    pub durable: bool,
    /// Whether startup required durability instead of allowing in-memory fallback.
    pub required: bool,
}

/// Metadata produced once runtime startup completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStartupMetadata {
    /// Authoritative lifecycle path of the launched Agent Process.
    pub process_path: String,
    /// AgentFS projection path of the launched Agent Process.
    pub agent_path: String,
    /// Identity of the fresh rollout produced by this process, when durable.
    pub rollout_id: Option<String>,
    /// Host-owned rollout file used for durable execution evidence, when available.
    pub rollout_path: Option<PathBuf>,
    /// Effective durability state of the Agent Machine.
    pub durability: AgentMachineDurabilityState,
    /// Active Tool execution backend name.
    pub execution_backend: String,
    /// Request controls resolved for this Agent Machine.
    pub request_controls: crate::ResolvedRequestControls,
    /// Non-fatal startup diagnostics.
    pub warnings: Vec<String>,
}

impl RuntimeStartupMetadata {
    pub(super) fn ready(
        process_path: String,
        agent_path: String,
        rollout_id: Option<String>,
        rollout_path: Option<PathBuf>,
        durability: AgentMachineDurabilityState,
        request_controls: crate::ResolvedRequestControls,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            process_path,
            agent_path,
            rollout_id,
            rollout_path,
            durability,
            execution_backend: crate::tools::active_backend_name().to_string(),
            request_controls,
            warnings,
        }
    }

    fn already_ready_fallback() -> Self {
        Self::ready(
            String::new(),
            String::new(),
            None,
            None,
            AgentMachineDurabilityState {
                durable: true,
                required: false,
            },
            crate::ResolvedRequestControls::default(),
            Vec::new(),
        )
    }
}

/// Handle for communicating with an agent runtime.
#[derive(Clone)]
pub struct RuntimeHandle {
    /// Submission channel for runtime input and control operations.
    pub submission_tx: mpsc::Sender<Submission>,
    /// Shutdown signal sender for graceful shutdown.
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl RuntimeHandle {
    fn new(submission_tx: mpsc::Sender<Submission>, shutdown_tx: Option<mpsc::Sender<()>>) -> Self {
        Self {
            submission_tx,
            shutdown_tx,
        }
    }

    /// Request graceful shutdown of the runtime.
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(ref tx) = self.shutdown_tx {
            tx.send(()).await.map_err(|_| {
                anyhow::anyhow!("Failed to send shutdown signal - runtime may already be stopped")
            })?;
            info!("Shutdown signal sent to runtime");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Shutdown channel not available"))
        }
    }
}

/// Runtime controller for managing a spawned agent runtime.
pub struct RuntimeController {
    /// Handle for communicating with the runtime.
    pub handle: RuntimeHandle,
    /// Join handle for the main runtime task (Option to allow take on abort).
    task_handle: Option<JoinHandle<()>>,
    /// Runtime readiness channel.
    ready_rx: Option<oneshot::Receiver<std::result::Result<RuntimeStartupMetadata, String>>>,
    /// Cached startup metadata for repeated readiness checks and child-launch introspection.
    startup_metadata: Option<RuntimeStartupMetadata>,
}

impl RuntimeController {
    pub(super) fn spawned(
        submission_tx: mpsc::Sender<Submission>,
        shutdown_tx: mpsc::Sender<()>,
        task_handle: JoinHandle<()>,
        ready_rx: oneshot::Receiver<std::result::Result<RuntimeStartupMetadata, String>>,
    ) -> Self {
        Self {
            handle: RuntimeHandle::new(submission_tx, Some(shutdown_tx)),
            task_handle: Some(task_handle),
            ready_rx: Some(ready_rx),
            startup_metadata: None,
        }
    }

    /// Returns true if the runtime task has already exited.
    pub fn is_finished(&self) -> bool {
        self.task_handle
            .as_ref()
            .map(tokio::task::JoinHandle::is_finished)
            .unwrap_or(true)
    }

    /// Wait until the runtime has completed startup.
    pub async fn wait_until_ready(&mut self) -> Result<RuntimeStartupMetadata> {
        if let Some(metadata) = self.startup_metadata.clone() {
            return Ok(metadata);
        }

        let Some(ready_rx) = self.ready_rx.take() else {
            return Ok(RuntimeStartupMetadata::already_ready_fallback());
        };

        match ready_rx.await {
            Ok(Ok(metadata)) => {
                self.startup_metadata = Some(metadata.clone());
                Ok(metadata)
            }
            Ok(Err(message)) => Err(anyhow::anyhow!(message)),
            Err(_) => Err(anyhow::anyhow!(
                "Runtime stopped before signaling startup readiness"
            )),
        }
    }

    /// Shutdown the runtime gracefully and wait for it to complete.
    ///
    /// First sends shutdown signal, then waits up to 10s for graceful shutdown.
    /// If timeout occurs, the task is explicitly aborted and awaited to ensure
    /// the runtime is truly stopped.
    pub async fn shutdown(mut self) -> Result<()> {
        self.ready_rx.take();

        if let Some(ref tx) = self.handle.shutdown_tx
            && tx.send(()).await.is_err()
        {
            warn!("Shutdown channel closed - runtime may already be stopped");
        }

        let timeout = tokio::time::Duration::from_secs(10);
        if let Some(ref mut handle) = self.task_handle {
            match tokio::time::timeout(timeout, &mut *handle).await {
                Ok(Ok(())) => {
                    info!("Runtime task completed gracefully");
                    Ok(())
                }
                Ok(Err(err)) => Err(anyhow::anyhow!("Runtime task panicked: {}", err)),
                Err(_) => {
                    warn!("Runtime shutdown timeout, aborting task");
                    handle.abort();
                    match tokio::time::timeout(Duration::from_secs(5), handle).await {
                        Ok(_) => {
                            info!("Runtime task aborted successfully");
                            Ok(())
                        }
                        Err(_) => Err(anyhow::anyhow!("Runtime shutdown timeout and abort failed")),
                    }
                }
            }
        } else {
            Err(anyhow::anyhow!("Task handle not available"))
        }
    }

    /// Abort the runtime immediately without waiting for graceful shutdown.
    ///
    /// This takes ownership of the task handles and aborts them immediately.
    /// Use this when you need to guarantee the runtime stops.
    pub async fn abort(mut self) {
        self.ready_rx.take();

        if let Some(ref tx) = self.handle.shutdown_tx {
            let _ = tx.try_send(());
        }

        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }
    }
}

impl Drop for RuntimeController {
    fn drop(&mut self) {
        self.ready_rx.take();
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;
