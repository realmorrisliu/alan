//! Rust terminal UI for alan.

pub mod app;
pub mod completion;
pub mod composer;
pub mod daemon_client;
pub mod form;
pub mod history;
pub mod terminal;
pub mod ui;
pub mod workspace_input;
pub mod workspace_render;

use crate::app::{AppEvent, TuiApp};
use crate::completion::CompletionCandidate;
use crate::composer::{Composer, load_history};
use crate::daemon_client::{CreateSessionRequest, DaemonClient, EndpointContract};
use crate::terminal::{TerminalSession, terminal_capability_error};

/// Maximum number of composer history entries kept in memory.
const HISTORY_LIMIT: usize = 1000;
/// Maximum number of workspace files indexed for `@` completion.
const FILE_INDEX_LIMIT: usize = 5000;

/// Directory names skipped when indexing workspace files for `@` completion.
const SKIP_DIRS: [&str; 5] = [".git", "target", "node_modules", ".alan", "dist"];

/// Parse the daemon skills catalog into `$` completion candidates, defensively
/// handling either a top-level array or a `{ "skills": [...] }` object.
fn parse_skill_candidates(catalog: &serde_json::Value) -> Vec<CompletionCandidate> {
    let array = catalog
        .get("skills")
        .and_then(serde_json::Value::as_array)
        .or_else(|| catalog.as_array());
    let Some(array) = array else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|item| {
            let name = item
                .get("name")
                .or_else(|| item.get("id"))
                .and_then(serde_json::Value::as_str)?;
            let detail = item
                .get("description")
                .or_else(|| item.get("summary"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            Some(CompletionCandidate::new(name, detail))
        })
        .collect()
}

/// Build a bounded list of workspace-relative file paths for `@` completion.
fn build_file_index(root: &std::path::Path, limit: usize) -> Vec<CompletionCandidate> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if files.len() >= limit {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file()
                && let Ok(relative) = path.strip_prefix(root)
            {
                files.push(CompletionCandidate::new(
                    relative.to_string_lossy().to_string(),
                    None,
                ));
                if files.len() >= limit {
                    break;
                }
            }
        }
    }
    files.sort_by(|a, b| a.value.cmp(&b.value));
    files
}
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
    /// Optional file used to persist composer input history across sessions.
    pub history_path: Option<PathBuf>,
}

impl RunConfig {
    pub fn new(base_url: impl Into<String>, endpoints: Arc<dyn EndpointContract>) -> Self {
        Self {
            base_url: base_url.into(),
            agent_name: None,
            workspace_dir: None,
            endpoints,
            require_interactive_terminal: true,
            history_path: None,
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
    if let Some(history_path) = &config.history_path {
        let history = load_history(history_path, HISTORY_LIMIT);
        app.composer = Composer::with_history(history, Some(history_path.clone()));
    }
    tracing::info!(
        model = session.resolved_model.as_deref().unwrap_or("unresolved"),
        profile = session.profile_id.as_deref().unwrap_or("default"),
        "connected to daemon session"
    );

    match client.read_skills_catalog().await {
        Ok(catalog) => app.set_skill_candidates(parse_skill_candidates(&catalog)),
        Err(err) => tracing::debug!(%err, "skill catalog unavailable; $ completion disabled"),
    }
    if let Ok(workspace) = config.workspace_dir_for_session() {
        app.set_file_candidates(build_file_index(&workspace, FILE_INDEX_LIMIT));
    }

    terminal.draw(&app)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AppEvent>(128);
    terminal::spawn_terminal_events(tx.clone());

    // Hydrate first to capture the replay cursor, then start the live stream from
    // it so events emitted between hydration and the first subscribe are drained,
    // not missed (the `/events` stream is future-only).
    let mut replay_cursor = None;
    match client.hydrate_session(&session.session_id).await {
        Ok(hydration) => {
            replay_cursor = hydration.latest_event_id.clone();
            app.dispatch(AppEvent::Hydrated(hydration));
        }
        Err(err) => {
            app.dispatch(AppEvent::Error(format!(
                "session hydration failed: {err:#}"
            )));
        }
    }
    daemon_client::spawn_event_stream(
        client.clone(),
        session.session_id.clone(),
        tx,
        replay_cursor,
    );

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

    #[test]
    fn parse_skill_candidates_reads_named_skills() {
        let catalog = serde_json::json!({
            "skills": [
                { "name": "code-review", "description": "review a diff" },
                { "id": "commit" }
            ]
        });
        let candidates = parse_skill_candidates(&catalog);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].value, "code-review");
        assert_eq!(candidates[0].detail.as_deref(), Some("review a diff"));
        assert_eq!(candidates[1].value, "commit");
    }

    #[test]
    fn build_file_index_lists_files_and_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "x").unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git").join("config"), "x").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("lib.rs"), "x").unwrap();

        let index = build_file_index(dir.path(), 100);
        let values: Vec<_> = index.iter().map(|c| c.value.as_str()).collect();
        assert!(values.contains(&"main.rs"));
        assert!(values.contains(&"src/lib.rs"));
        assert!(!values.iter().any(|value| value.contains(".git")));
    }
}
