//! Host-local `/settings` and store-reset control bridge for the file-backed TUI.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use alan_ap::InProcessTransport;
use anyhow::Result;
use tokio::sync::mpsc;

use super::app::{FileBackedAction, FileBackedEvent};
use super::{FileBackedApp, FileBackedRunConfig, PendingRootAgentTurn};

/// Scope of an in-TUI Alan store reset initiated from `/settings`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsResetScope {
    /// Clear connection profiles, secrets/OAuth, project mounts, UI history,
    /// updater cache, and the active rollout pointer. Keeps rollouts & Memory Stores.
    Settings,
    /// Factory reset: clear all Alan services, Memory Stores, and rollouts
    /// plus Host credentials/OAuth.
    All,
}

/// Control request sent from `/settings` in the TUI to the Host adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsControl {
    /// Clear Alan stores according to `scope` and reload runtime connections.
    Reset { scope: SettingsResetScope },
    /// Persist the preferred release channel (`"stable"` or `"beta"`) in System Store.
    SetReleaseChannel { channel: String },
}

/// Result returned by [`SettingsControlHandler`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsResult {
    /// Reset finished and live runtime connection registry was reloaded.
    ResetCompleted {
        /// Scope that was cleared.
        scope: SettingsResetScope,
        /// Number of top-level Alan store paths removed.
        removed_count: usize,
    },
    /// Preferred release channel was saved in System Store.
    ReleaseChannelUpdated {
        /// Normalized channel name (`"stable"` or `"beta"`).
        channel: String,
        /// Whether the selected channel matches the running binary's compiled channel.
        matches_binary: bool,
    },
}

/// Future returned by [`SettingsControlHandler`].
pub type SettingsControlFuture =
    Pin<Box<dyn Future<Output = Result<SettingsResult>> + Send + 'static>>;

/// Host-local `/settings` reset handler supplied by the CLI to the TUI.
pub type SettingsControlHandler =
    Arc<dyn Fn(SettingsControl) -> SettingsControlFuture + Send + Sync + 'static>;

pub(in crate::file_backed) fn spawn_settings_control(
    handler: SettingsControlHandler,
    tx: mpsc::Sender<FileBackedEvent>,
    generation: u64,
    control: SettingsControl,
) -> tokio::task::AbortHandle {
    let handle = tokio::spawn(async move {
        let result = handler(control).await.map_err(|err| format!("{err:#}"));
        let _ = tx
            .send(FileBackedEvent::SettingsCompleted { generation, result })
            .await;
    });
    handle.abort_handle()
}

pub(in crate::file_backed) fn spawn_post_reset_session_sync(
    transport: InProcessTransport,
    owner: String,
    model_reads: super::observation_io::Request,
) {
    tokio::spawn(async move {
        let shell = alan_shell::Shell::new(transport);
        let target = if owner.trim().is_empty() {
            "/agent/root".to_string()
        } else {
            owner.clone()
        };
        let _ = super::file_surface::write_machine_ctl(&shell, &target, "reset-session").await;
        super::observation_io::request(&model_reads, target);
    });
}

pub(in crate::file_backed) fn maybe_start_first_run_setup(
    config: &FileBackedRunConfig,
    app: &mut FileBackedApp,
    tx: &mpsc::Sender<FileBackedEvent>,
    active_login_task: &mut Option<(u64, tokio::task::AbortHandle)>,
) {
    if config.auto_setup_on_unconfigured
        && app.is_connection_unconfigured()
        && let Some(FileBackedAction::LoginListProviders { generation }) =
            app.start_first_run_setup()
        && let Some(handler) = config.login_control.clone()
    {
        let abort = super::login::spawn_list_providers(handler, tx.clone(), generation);
        *active_login_task = Some((generation, abort));
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "forwards file-backed event loop refs out of file_backed.rs"
)]
pub(in crate::file_backed) fn handle_settings_completed(
    config: &FileBackedRunConfig,
    app: &mut FileBackedApp,
    tx: &mpsc::Sender<FileBackedEvent>,
    owner: String,
    generation: u64,
    result: Result<SettingsResult, String>,
    active_settings_task: &mut Option<(u64, tokio::task::AbortHandle)>,
    active_login_task: &mut Option<(u64, tokio::task::AbortHandle)>,
    pending_root_agent_turns: &mut std::collections::VecDeque<PendingRootAgentTurn>,
    model_reads: &super::observation_io::Request,
) {
    if active_settings_task
        .as_ref()
        .is_some_and(|(task_gen, _)| *task_gen == generation)
    {
        *active_settings_task = None;
    }
    let (was_reset, next_action) = app.apply_settings_result(generation, result);
    if was_reset {
        pending_root_agent_turns.clear();
        spawn_post_reset_session_sync(config.root_transport.clone(), owner, model_reads.clone());
    }
    if let Some(FileBackedAction::LoginListProviders { generation }) = next_action
        && let Some(handler) = config.login_control.clone()
    {
        if let Some((_, prev)) = active_login_task.take() {
            prev.abort();
        }
        *active_login_task = Some((
            generation,
            super::login::spawn_list_providers(handler, tx.clone(), generation),
        ));
    }
}

pub(in crate::file_backed) fn dispatch_settings_action(
    config: &FileBackedRunConfig,
    app: &mut FileBackedApp,
    tx: &mpsc::Sender<FileBackedEvent>,
    generation: u64,
    control: SettingsControl,
    active_settings_task: &mut Option<(u64, tokio::task::AbortHandle)>,
) {
    if let Some((_, prev)) = active_settings_task.take() {
        prev.abort();
    }
    if let Some(handler) = config.settings_control.clone() {
        *active_settings_task = Some((
            generation,
            spawn_settings_control(handler, tx.clone(), generation, control),
        ));
    } else {
        app.apply_settings_result(
            generation,
            Err("Settings reset is unavailable in this Host.".into()),
        );
    }
}
