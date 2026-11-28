use std::ops::Deref;

use alan_agent_engine::{InputIntent, InputMode, UserInputRecord, validate_response_schema};
use alan_kernel::Pid;
use alan_service_manager::SessionHandle;
use alan_shell::{Shell, Tail};
use anyhow::{Context, Result};

use super::embed_events::EmbedEventStream;

/// Ergonomic handle for interacting with an embedded Agent Process (`/agent/root` or `/agent/<pid>`).
#[derive(Clone)]
pub struct RootAgentClient {
    pub(crate) shell: Shell,
    pub(crate) agent_path: String,
}

impl RootAgentClient {
    /// Bind a client to a specific `/agent/<pid>` or `/agent/root` path.
    pub fn new(shell: Shell, agent_path: impl Into<String>) -> Self {
        Self {
            shell,
            agent_path: agent_path.into(),
        }
    }

    /// Underlying `aP`-only [`Shell`] client.
    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    /// Target agent path (e.g. `/agent/root` or `/agent/8`).
    pub fn agent_path(&self) -> &str {
        &self.agent_path
    }

    /// Submit a plain text turn to `io/input` and return its `submission_id`.
    pub async fn submit_text(&self, text: impl Into<String>) -> Result<String> {
        let record = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, text);
        let submission_id = record.submission_id.clone();
        self.submit_record(&record).await?;
        Ok(submission_id)
    }

    /// Submit a text turn constrained by a per-turn JSON Schema (`response_schema`).
    pub async fn submit_text_with_schema(
        &self,
        text: impl Into<String>,
        response_schema: serde_json::Value,
    ) -> Result<String> {
        validate_response_schema(&response_schema).context("invalid per-turn response_schema")?;
        let record = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, text)
            .with_response_schema(response_schema);
        let submission_id = record.submission_id.clone();
        self.submit_record(&record).await?;
        Ok(submission_id)
    }

    /// Submit an encoded [`UserInputRecord`] to `io/input`.
    pub async fn submit_record(&self, record: &UserInputRecord) -> Result<()> {
        let path = format!("{}/io/input", self.agent_path);
        self.shell
            .write(&path, &record.encode_payload()?)
            .await
            .map_err(|err| anyhow::anyhow!("write embedded agent input failed: {err:?}"))
    }

    /// Open a streaming tail on `io/output` for assistant text.
    pub async fn tail_output(&self) -> Result<Tail> {
        let path = format!("{}/io/output", self.agent_path);
        self.shell
            .tail(&path)
            .await
            .map_err(|err| anyhow::anyhow!("tail embedded agent output failed: {err:?}"))
    }

    /// Open a streaming tail on `machine/ui/events` for structured UI events.
    pub async fn tail_ui_events(&self) -> Result<Tail> {
        let path = format!("{}/machine/ui/events", self.agent_path);
        self.shell
            .tail(&path)
            .await
            .map_err(|err| anyhow::anyhow!("tail embedded ui events failed: {err:?}"))
    }

    /// Open a streaming tail on `requests/events` for pending yield notifications.
    pub async fn tail_requests(&self) -> Result<Tail> {
        let path = format!("{}/requests/events", self.agent_path);
        self.shell
            .tail(&path)
            .await
            .map_err(|err| anyhow::anyhow!("tail embedded requests failed: {err:?}"))
    }

    /// Write a response to a pending yield request under `requests/<request_id>/response`.
    pub async fn respond_request(&self, request_id: &str, response: &str) -> Result<()> {
        let path = format!("{}/requests/{request_id}/response", self.agent_path);
        self.shell
            .write(&path, response.as_bytes())
            .await
            .map_err(|err| anyhow::anyhow!("write embedded request response failed: {err:?}"))
    }

    /// Open live output, action, request, UI, and tape tails decoded into V2 [`super::EmbedEvent`]s.
    ///
    /// Open this before submitting turns. A single stream remains valid across consecutive turns.
    pub async fn event_stream(&self) -> Result<EmbedEventStream> {
        EmbedEventStream::open(self.shell.clone(), self.agent_path.clone()).await
    }

    /// Issue an interrupt command to `machine/ctl`.
    pub async fn interrupt(&self, submission_id: Option<&str>) -> Result<()> {
        let path = format!("{}/machine/ctl", self.agent_path);
        let cmd = match submission_id {
            Some(id) => format!("queue-v1 interrupt {id}"),
            None => "interrupt".to_string(),
        };
        self.shell
            .write(&path, cmd.as_bytes())
            .await
            .map_err(|err| anyhow::anyhow!("write embedded interrupt failed: {err:?}"))
    }
}

/// Scoped handle for a dynamically opened or child interactive session on one [`super::EmbeddedAlan`].
///
/// Implements [`Deref<Target = RootAgentClient>`] so every submission, streaming, and request API
/// works directly on the session handle alongside [`Self::pid`] and [`Self::close`].
#[derive(Clone)]
pub struct AgentSessionHandle {
    client: RootAgentClient,
    session: SessionHandle,
}

impl AgentSessionHandle {
    pub(super) fn new(session: SessionHandle) -> Self {
        let shell = Shell::new(session.namespace.clone());
        let client = RootAgentClient::new(shell, session.agent_path.clone());
        Self { client, session }
    }

    /// Process ID of this session's Agent Process.
    pub fn pid(&self) -> Pid {
        self.session.pid
    }

    /// Borrow the underlying [`RootAgentClient`].
    pub fn client(&self) -> &RootAgentClient {
        &self.client
    }

    #[cfg(test)]
    pub(super) fn transport(&self) -> alan_ap::InProcessTransport {
        self.session.namespace.clone()
    }

    /// Consume this handle and return its scoped [`RootAgentClient`].
    pub fn into_client(self) -> RootAgentClient {
        self.client
    }

    /// Stop and release this session from the running [`super::EmbeddedAlan`] instance.
    pub async fn close(self) -> Result<()> {
        self.session.close().await
    }
}

impl Deref for AgentSessionHandle {
    type Target = RootAgentClient;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
