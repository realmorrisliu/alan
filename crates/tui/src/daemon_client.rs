use alan_protocol::{ContentPart, EventEnvelope, Op, Submission};
use anyhow::{Context, Result};
use futures::{Stream, StreamExt};
use reqwest::StatusCode;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use crate::app::{AppEvent, SessionHydration};

/// Path construction contract supplied by the daemon crate.
pub trait EndpointContract: Send + Sync {
    fn health(&self) -> &'static str;
    fn sessions(&self) -> &'static str;
    fn session_read(&self, session_id: &str) -> String;
    fn session_reconnect_snapshot(&self, session_id: &str) -> String;
    fn session_history(&self, session_id: &str) -> String;
    fn session_events_read(&self, session_id: &str) -> String;
    fn session_events(&self, session_id: &str) -> String;
    fn session_submit(&self, session_id: &str) -> String;
    fn session_resume(&self, session_id: &str) -> String;
    fn session_rollback(&self, session_id: &str) -> String;
    fn session_compact(&self, session_id: &str) -> String;
    fn connections_current(&self) -> &'static str;
    fn skills_catalog(&self) -> &'static str;
}

#[derive(Clone)]
pub struct DaemonClient {
    base_url: String,
    http: reqwest::Client,
    endpoints: Arc<dyn EndpointContract>,
}

impl DaemonClient {
    pub fn new(base_url: impl Into<String>, endpoints: Arc<dyn EndpointContract>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            endpoints,
        }
    }

    pub fn endpoint_paths(&self, session_id: &str) -> TuiEndpointPaths {
        TuiEndpointPaths {
            health: self.endpoints.health().to_string(),
            sessions: self.endpoints.sessions().to_string(),
            session_read: self.endpoints.session_read(session_id),
            session_reconnect_snapshot: self.endpoints.session_reconnect_snapshot(session_id),
            session_history: self.endpoints.session_history(session_id),
            session_events_read: self.endpoints.session_events_read(session_id),
            session_events: self.endpoints.session_events(session_id),
            session_submit: self.endpoints.session_submit(session_id),
            session_resume: self.endpoints.session_resume(session_id),
            session_rollback: self.endpoints.session_rollback(session_id),
            session_compact: self.endpoints.session_compact(session_id),
            connections_current: self.endpoints.connections_current().to_string(),
            skills_catalog: self.endpoints.skills_catalog().to_string(),
        }
    }

    pub async fn health(&self) -> Result<()> {
        let response = self
            .http
            .get(self.url(self.endpoints.health()))
            .send()
            .await
            .context("failed to connect to daemon health endpoint")?;
        if !response.status().is_success() {
            anyhow::bail!("daemon health check failed: {}", response.status());
        }
        Ok(())
    }

    pub async fn create_session(&self, request: CreateSessionRequest) -> Result<CreateSession> {
        let mut body = serde_json::Map::new();
        if let Some(path) = request.workspace_dir {
            let canonical = std::fs::canonicalize(&path)
                .with_context(|| format!("cannot resolve workspace path: {}", path.display()))?;
            body.insert(
                "workspace_dir".to_string(),
                serde_json::Value::String(canonical.to_string_lossy().to_string()),
            );
        }
        if let Some(agent_name) = request.agent_name {
            body.insert(
                "agent_name".to_string(),
                serde_json::Value::String(agent_name),
            );
        }

        let response = self
            .http
            .post(self.url(self.endpoints.sessions()))
            .json(&body)
            .send()
            .await
            .context("failed to create session")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status == StatusCode::CONFLICT
                && let Some(session_id) = active_workspace_session_id_from_body(&body)
            {
                return self
                    .read_session_summary(&session_id)
                    .await
                    .with_context(|| {
                        format!("failed to attach existing workspace session {session_id}")
                    });
            }
            anyhow::bail!("failed to create session ({status}): {body}");
        }
        response
            .json::<CreateSession>()
            .await
            .context("failed to parse create-session response")
    }

    async fn read_session_summary(&self, session_id: &str) -> Result<CreateSession> {
        let response = self
            .http
            .get(self.url(&self.endpoints.session_read(session_id)))
            .send()
            .await
            .with_context(|| format!("failed to read session {session_id}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("failed to read session {session_id} ({status}): {body}");
        }
        response
            .json::<CreateSession>()
            .await
            .with_context(|| format!("failed to parse session {session_id} summary"))
    }

    pub async fn submit_turn(&self, session_id: &str, text: String) -> Result<()> {
        self.submit(
            session_id,
            Submission::new(Op::Turn {
                parts: vec![ContentPart::text(text)],
                context: None,
            }),
        )
        .await
    }

    pub async fn resume(
        &self,
        session_id: &str,
        request_id: &str,
        content: Vec<ContentPart>,
    ) -> Result<()> {
        self.submit(
            session_id,
            Submission::new(Op::Resume {
                request_id: request_id.to_string(),
                content,
            }),
        )
        .await
    }

    pub async fn interrupt(&self, session_id: &str) -> Result<()> {
        self.submit(session_id, Submission::new(Op::Interrupt))
            .await
    }

    pub async fn rollback(&self, session_id: &str, turns: u32) -> Result<()> {
        self.submit(session_id, Submission::new(Op::Rollback { turns }))
            .await
    }

    pub async fn compact(&self, session_id: &str, focus: Option<String>) -> Result<()> {
        self.submit(
            session_id,
            Submission::new(Op::CompactWithOptions { focus }),
        )
        .await
    }

    pub async fn read_reconnect_snapshot(&self, session_id: &str) -> Result<serde_json::Value> {
        self.get_json(self.endpoints.session_reconnect_snapshot(session_id))
            .await
    }

    pub async fn read_history(&self, session_id: &str) -> Result<serde_json::Value> {
        self.get_json(self.endpoints.session_history(session_id))
            .await
    }

    pub async fn read_skills_catalog(&self) -> Result<serde_json::Value> {
        self.get_json(self.endpoints.skills_catalog().to_string())
            .await
    }

    /// Buffered transport events for cursor-based replay (`/events/read`); used to
    /// recover the full payload of a yield the NDJSON stream won't replay.
    pub async fn read_buffered_events(&self, session_id: &str) -> Result<Vec<EventEnvelope>> {
        let value = self
            .get_json(self.endpoints.session_events_read(session_id))
            .await?;
        let events = value
            .get("events")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        Ok(serde_json::from_value(events).unwrap_or_default())
    }

    pub async fn hydrate_session(&self, session_id: &str) -> Result<SessionHydration> {
        let history = self.read_history(session_id).await?;
        let reconnect = self.read_reconnect_snapshot(session_id).await?;
        let mut hydration = SessionHydration::from_values(&history, &reconnect);
        // The snapshot signal carries only the yield's id+kind. Recover the full
        // payload (form questions, approval command/diff) from the buffered event
        // log so the restored prompt is fully resumable; the NDJSON stream is
        // future-only and won't replay the original Yield.
        if let Some(pending) = &hydration.pending
            && let Ok(events) = self.read_buffered_events(session_id).await
        {
            hydration.pending_event = events
                .into_iter()
                .rev()
                .find(|env| {
                    matches!(
                        &env.event,
                        alan_protocol::Event::Yield { request_id, .. }
                            if *request_id == pending.request_id
                    )
                })
                .map(Box::new);
        }
        Ok(hydration)
    }

    pub async fn events(&self, session_id: &str) -> Result<NdjsonEventStream> {
        let response = self
            .http
            .get(self.url(&self.endpoints.session_events(session_id)))
            .send()
            .await
            .context("failed to connect to daemon event stream")?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("failed to stream events: {status}");
        }
        Ok(NdjsonEventStream::new(Box::pin(response.bytes_stream())))
    }

    async fn submit(&self, session_id: &str, submission: Submission) -> Result<()> {
        let response = self
            .http
            .post(self.url(&self.endpoints.session_submit(session_id)))
            .json(&submission)
            .send()
            .await
            .context("failed to submit operation")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("failed to submit operation ({status}): {body}");
        }
        Ok(())
    }

    async fn get_json(&self, path: String) -> Result<serde_json::Value> {
        let response = self
            .http
            .get(self.url(&path))
            .send()
            .await
            .with_context(|| format!("failed to read {path}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("failed to read {path} ({status}): {body}");
        }
        response.json().await.context("failed to parse daemon JSON")
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[derive(Debug, Clone, Default)]
pub struct CreateSessionRequest {
    pub workspace_dir: Option<PathBuf>,
    pub agent_name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateSession {
    pub session_id: String,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub resolved_model: Option<String>,
    #[serde(default)]
    pub durability: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiEndpointPaths {
    pub health: String,
    pub sessions: String,
    pub session_read: String,
    pub session_reconnect_snapshot: String,
    pub session_history: String,
    pub session_events_read: String,
    pub session_events: String,
    pub session_submit: String,
    pub session_resume: String,
    pub session_rollback: String,
    pub session_compact: String,
    pub connections_current: String,
    pub skills_catalog: String,
}

pub fn spawn_event_stream(
    client: DaemonClient,
    session_id: String,
    tx: tokio::sync::mpsc::Sender<AppEvent>,
) {
    tokio::spawn(async move {
        loop {
            if !stream_events_once(&client, &session_id, &tx).await {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn stream_events_once(
    client: &DaemonClient,
    session_id: &str,
    tx: &tokio::sync::mpsc::Sender<AppEvent>,
) -> bool {
    match client.events(session_id).await {
        Ok(mut events) => {
            while let Some(event) = events.next().await {
                match event {
                    Ok(envelope) => {
                        if tx.send(AppEvent::Daemon(Box::new(envelope))).await.is_err() {
                            return false;
                        }
                    }
                    Err(err) => {
                        return tx
                            .send(AppEvent::Error(format!(
                                "event stream error: {err:#}; reconnecting"
                            )))
                            .await
                            .is_ok();
                    }
                }
            }
            tx.send(AppEvent::Status(
                "event stream ended; reconnecting".to_string(),
            ))
            .await
            .is_ok()
        }
        Err(err) => tx
            .send(AppEvent::Error(format!(
                "event stream unavailable: {err:#}; reconnecting"
            )))
            .await
            .is_ok(),
    }
}

type ByteStream =
    Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static>>;

pub struct NdjsonEventStream {
    bytes: ByteStream,
    parser: NdjsonLineParser,
    queued: VecDeque<Result<EventEnvelope>>,
}

impl NdjsonEventStream {
    fn new(bytes: ByteStream) -> Self {
        Self {
            bytes,
            parser: NdjsonLineParser::default(),
            queued: VecDeque::new(),
        }
    }
}

impl Stream for NdjsonEventStream {
    type Item = Result<EventEnvelope>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        if let Some(item) = self.queued.pop_front() {
            return Poll::Ready(Some(item));
        }

        loop {
            match self.bytes.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    for line in self.parser.push(&chunk) {
                        self.queued.push_back(parse_event_line(line));
                    }
                    if let Some(item) = self.queued.pop_front() {
                        return Poll::Ready(Some(item));
                    }
                }
                Poll::Ready(Some(Err(err))) => {
                    return Poll::Ready(Some(Err(err).context("event stream chunk failed")));
                }
                Poll::Ready(None) => {
                    if let Some(line) = self.parser.finish() {
                        return Poll::Ready(Some(parse_event_line(line)));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

fn parse_event_line(line: String) -> Result<EventEnvelope> {
    serde_json::from_str(&line).with_context(|| format!("invalid event envelope: {line}"))
}

fn active_workspace_session_id_from_body(body: &str) -> Option<String> {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.to_string());
    let prefix = "Workspace already has an active session runtime:";
    let (_, session_id) = message.split_once(prefix)?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return None;
    }
    session_id.split_whitespace().next().map(str::to_string)
}

#[derive(Default)]
struct NdjsonLineParser {
    buffer: Vec<u8>,
}

impl NdjsonLineParser {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut lines = Vec::new();
        let mut consumed = 0usize;
        while let Some(rel_pos) = self.buffer[consumed..]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let end = consumed + rel_pos;
            let line = self.buffer[consumed..end]
                .strip_suffix(b"\r")
                .unwrap_or(&self.buffer[consumed..end])
                .to_vec();
            consumed = end + 1;
            if !line.is_empty()
                && let Ok(line) = String::from_utf8(line)
            {
                lines.push(line);
            }
        }
        if consumed > 0 {
            self.buffer.drain(..consumed);
        }
        lines
    }

    fn finish(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        let line = std::mem::take(&mut self.buffer);
        let line = line.strip_suffix(b"\r").unwrap_or(&line).to_vec();
        if line.is_empty() {
            return None;
        }
        String::from_utf8(line).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeEndpoints;

    impl EndpointContract for FakeEndpoints {
        fn health(&self) -> &'static str {
            "/healthz"
        }
        fn sessions(&self) -> &'static str {
            "/sessions-root"
        }
        fn session_read(&self, session_id: &str) -> String {
            format!("/read/{session_id}")
        }
        fn session_reconnect_snapshot(&self, session_id: &str) -> String {
            format!("/reconnect/{session_id}")
        }
        fn session_history(&self, session_id: &str) -> String {
            format!("/history/{session_id}")
        }
        fn session_events_read(&self, session_id: &str) -> String {
            format!("/events-read/{session_id}")
        }
        fn session_events(&self, session_id: &str) -> String {
            format!("/events/{session_id}")
        }
        fn session_submit(&self, session_id: &str) -> String {
            format!("/submit/{session_id}")
        }
        fn session_resume(&self, session_id: &str) -> String {
            format!("/resume/{session_id}")
        }
        fn session_rollback(&self, session_id: &str) -> String {
            format!("/rollback/{session_id}")
        }
        fn session_compact(&self, session_id: &str) -> String {
            format!("/compact/{session_id}")
        }
        fn connections_current(&self) -> &'static str {
            "/connections/current"
        }
        fn skills_catalog(&self) -> &'static str {
            "/skills/catalog"
        }
    }

    #[test]
    fn endpoint_paths_are_supplied_by_contract() {
        let client = DaemonClient::new("http://daemon", Arc::new(FakeEndpoints));
        let paths = client.endpoint_paths("s-1");
        assert_eq!(paths.session_submit, "/submit/s-1");
        assert_eq!(paths.session_events, "/events/s-1");
        assert_eq!(paths.sessions, "/sessions-root");
    }

    #[test]
    fn ndjson_parser_handles_split_lines() {
        let mut parser = NdjsonLineParser::default();
        assert!(parser.push(br#"{"a":1}"#).is_empty());
        assert_eq!(parser.push(b"\n"), vec![r#"{"a":1}"#.to_string()]);
        assert_eq!(
            parser.push(b"{\"b\":2}\r\n"),
            vec![r#"{"b":2}"#.to_string()]
        );
    }

    #[test]
    fn parses_active_workspace_conflict_session_id() {
        let body = serde_json::json!({
            "error": "Workspace already has an active session runtime: sess-existing"
        })
        .to_string();
        assert_eq!(
            active_workspace_session_id_from_body(&body).as_deref(),
            Some("sess-existing")
        );
    }

    #[test]
    fn create_session_shape_accepts_session_read_payload() {
        let session: CreateSession = serde_json::from_value(serde_json::json!({
            "session_id": "sess-existing",
            "workspace_id": "abc123",
            "active": true,
            "profile_id": "chatgpt-main",
            "resolved_model": "gpt-5.3-codex",
            "durability": { "durable": false, "required": false },
            "messages": []
        }))
        .unwrap();

        assert_eq!(session.session_id, "sess-existing");
        assert_eq!(session.profile_id.as_deref(), Some("chatgpt-main"));
        assert_eq!(session.resolved_model.as_deref(), Some("gpt-5.3-codex"));
    }
}
