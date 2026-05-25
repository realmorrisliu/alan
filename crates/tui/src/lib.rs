//! Rust terminal UI for alan.

pub mod app;
pub mod composer;
pub mod daemon_client;
pub mod history;
pub mod terminal;
pub mod ui;

use crate::app::{AppEvent, TuiApp};
use crate::daemon_client::{CreateSessionRequest, DaemonClient, EndpointContract};
use crate::terminal::{TerminalSession, terminal_capability_error};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Runtime configuration supplied by the `alan` binary.
#[derive(Clone)]
pub struct RunConfig {
    /// Base daemon URL selected by host configuration or `ALAN_AGENTD_URL`.
    pub base_url: String,
    /// Optional named agent root selected by `--agent`.
    pub agent_name: Option<String>,
    /// Optional workspace directory. Defaults to the current directory.
    pub workspace_dir: Option<PathBuf>,
    /// Shared endpoint contract owned by the daemon crate.
    pub endpoints: Arc<dyn EndpointContract>,
    /// Whether stdin/stdout must be interactive before entering the UI.
    pub require_interactive_terminal: bool,
}

impl RunConfig {
    pub fn new(base_url: impl Into<String>, endpoints: Arc<dyn EndpointContract>) -> Self {
        Self {
            base_url: base_url.into(),
            agent_name: None,
            workspace_dir: None,
            endpoints,
            require_interactive_terminal: true,
        }
    }

    fn workspace_dir_for_session(&self) -> Result<PathBuf> {
        self.workspace_dir.clone().map(Ok).unwrap_or_else(|| {
            std::env::current_dir().context("failed to determine current directory")
        })
    }
}

/// Launch the daemon-backed terminal UI.
pub async fn run(config: RunConfig) -> Result<()> {
    if config.require_interactive_terminal && !terminal::is_interactive_terminal() {
        anyhow::bail!("{}", terminal_capability_error());
    }

    let client = DaemonClient::new(config.base_url.clone(), config.endpoints.clone());
    let session = client
        .create_session(CreateSessionRequest {
            workspace_dir: Some(config.workspace_dir_for_session()?),
            agent_name: config.agent_name.clone(),
        })
        .await
        .with_context(|| {
            format!(
                "failed to create daemon-backed session at {}",
                config.base_url
            )
        })?;

    let mut terminal = TerminalSession::enter()?;
    let mut app = TuiApp::new(session.clone());
    app.dispatch(AppEvent::Status(format!(
        "Connected to {} ({})",
        session
            .resolved_model
            .as_deref()
            .unwrap_or("unresolved model"),
        session.profile_id.as_deref().unwrap_or("default profile")
    )));

    terminal.draw(&app)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AppEvent>(128);
    terminal::spawn_terminal_events(tx.clone());
    daemon_client::spawn_event_stream(client.clone(), session.session_id.clone(), tx);

    match client.hydrate_session(&session.session_id).await {
        Ok(hydration) => {
            app.dispatch(AppEvent::Hydrated(hydration));
        }
        Err(err) => {
            app.dispatch(AppEvent::Error(format!(
                "session hydration failed: {err:#}"
            )));
        }
    }

    let mut frame_tick = tokio::time::interval(Duration::from_millis(33));
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = true;

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                let action = app.dispatch(event);
                if let Some(action) = action {
                    match action {
                        app::AppAction::SubmitTurn(text) => {
                            if let Err(err) = client.submit_turn(&session.session_id, text).await {
                                app.dispatch(AppEvent::Error(format!("submit failed: {err:#}")));
                            }
                        }
                        app::AppAction::Resume {
                            request_id,
                            content,
                        } => {
                            if let Err(err) = client
                                .resume(&session.session_id, &request_id, content)
                                .await
                            {
                                app.dispatch(AppEvent::Error(format!("resume failed: {err:#}")));
                            } else {
                                app.clear_pending_yield(&request_id);
                            }
                        }
                        app::AppAction::Interrupt => {
                            if let Err(err) = client.interrupt(&session.session_id).await {
                                app.dispatch(AppEvent::Error(format!("interrupt failed: {err:#}")));
                            }
                        }
                        app::AppAction::Compact => {
                            if let Err(err) = client.compact(&session.session_id, None).await {
                                app.dispatch(AppEvent::Error(format!("compact failed: {err:#}")));
                            }
                        }
                        app::AppAction::Rollback(turns) => {
                            if let Err(err) = client.rollback(&session.session_id, turns).await {
                                app.dispatch(AppEvent::Error(format!("rollback failed: {err:#}")));
                            }
                        }
                        app::AppAction::Quit => break,
                    }
                }
                dirty = true;
            }
            _ = frame_tick.tick(), if dirty => {
                let (viewport_width, viewport_height) = terminal.viewport_size();
                let committed = app.drain_committed_scrollback(viewport_width, viewport_height);
                terminal.write_scrollback(&committed)?;
                terminal.draw(&app)?;
                dirty = false;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeEndpoints;

    impl EndpointContract for FakeEndpoints {
        fn health(&self) -> &'static str {
            "/health"
        }

        fn sessions(&self) -> &'static str {
            "/sessions"
        }

        fn session_read(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/read")
        }

        fn session_reconnect_snapshot(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/reconnect_snapshot")
        }

        fn session_history(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/history")
        }

        fn session_events_read(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/events/read")
        }

        fn session_events(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/events")
        }

        fn session_submit(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/submit")
        }

        fn session_resume(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/resume")
        }

        fn session_rollback(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/rollback")
        }

        fn session_compact(&self, session_id: &str) -> String {
            format!("/sessions/{session_id}/compact")
        }

        fn connections_current(&self) -> &'static str {
            "/connections/current"
        }

        fn skills_catalog(&self) -> &'static str {
            "/skills/catalog"
        }
    }

    #[test]
    fn default_workspace_dir_uses_current_directory() {
        let config = RunConfig::new("http://daemon", Arc::new(FakeEndpoints));
        assert_eq!(
            config.workspace_dir_for_session().unwrap(),
            std::env::current_dir().unwrap()
        );
    }

    #[test]
    fn explicit_workspace_dir_overrides_current_directory() {
        let mut config = RunConfig::new("http://daemon", Arc::new(FakeEndpoints));
        config.workspace_dir = Some(PathBuf::from("/tmp/alan-explicit-workspace"));
        assert_eq!(
            config.workspace_dir_for_session().unwrap(),
            PathBuf::from("/tmp/alan-explicit-workspace")
        );
    }
}
