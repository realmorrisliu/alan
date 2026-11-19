use std::collections::{HashMap, HashSet, VecDeque};

use alan_agent_protocol::{
    UiActivityState, UiEvent, UiInputStatus, UiQueueSnapshot, UiThinkingState, UiUsageSnapshot,
};
use alan_shell::{Shell, Tail};
use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Structured runtime events streamed from an Agent Process namespace tree.
///
/// Streams opened via [`crate::embed::RootAgentClient::event_stream`] tail:
/// - `io/output` for live incremental [`Self::TextDelta`] chunks (`is_final: false`),
/// - `actions/events` & `actions/<id>/*` for [`Self::ToolStarted`], [`Self::ToolProgress`],
///   and [`Self::ToolFinished`],
/// - `requests/events` & `requests/<id>/*` for rich [`Self::PendingRequest`] metadata, and
/// - `machine/ui/events`, `machine/ui/queue`, `machine/ui/usage`, and `machine/tape` for
///   [`Self::ThinkingDelta`], [`Self::Usage`], [`Self::Error`], and [`Self::TurnFinished`].
///
/// Completion order per submission: once generation completes and any trailing bytes/events
/// have flushed, the stream emits a final [`Self::TextDelta`] with `is_final: true` (and empty
/// `chunk`), optionally followed by [`Self::Usage`], and concludes with [`Self::TurnFinished`].
#[derive(Debug, Clone, PartialEq)]
pub enum EmbedEvent {
    /// Assistant output text chunk (`is_final: false`), followed by a terminal `is_final: true`
    /// marker once the turn completes.
    TextDelta {
        submission_id: Option<String>,
        chunk: String,
        is_final: bool,
    },
    /// Incremental assistant reasoning chunk (`is_final: false`), followed by a terminal
    /// `is_final: true` marker when the reasoning block completes or goes idle.
    ThinkingDelta {
        submission_id: Option<String>,
        chunk: String,
        is_final: bool,
    },
    /// A tool execution action started under `actions/<id>`.
    ToolStarted {
        call_id: String,
        name: String,
        title: String,
    },
    /// Live preview progress update for a running streaming tool action.
    ToolProgress { call_id: String, preview: String },
    /// Terminal state for a tool action (`completed`, `failed`, `rejected`, etc.).
    ToolFinished {
        call_id: String,
        name: String,
        status: String,
        preview: Option<String>,
    },
    /// A pending yield request under `requests/<request_id>` with decoded metadata.
    PendingRequest {
        request_id: String,
        kind: String,
        title: Option<String>,
        prompt: String,
        options: Vec<String>,
        default_option: Option<String>,
        questions: Option<Vec<serde_json::Value>>,
        capability: Option<String>,
        reason: Option<String>,
        presentation: Option<serde_json::Value>,
    },
    /// Usage snapshot captured at turn completion from `machine/ui/usage`.
    Usage {
        submission_id: Option<String>,
        usage: UiUsageSnapshot,
    },
    /// A `UiEvent::Error` notification.
    Error { message: String, recoverable: bool },
    /// End of one submission, from `UiEvent::InputCompleted`, in listed order.
    TurnFinished {
        submission_id: String,
        status: UiInputStatus,
        /// Error string from that `InputCompleted`, when the engine set one.
        error: Option<String>,
    },
}

#[derive(Debug, Default)]
struct ActionTrack {
    call_id: String,
    name: String,
    title: String,
    started_emitted: bool,
    last_preview: Option<String>,
    finished_emitted: bool,
}

#[derive(Debug)]
struct TapeAssistantTurn {
    submission_id: Option<String>,
    bytes_len: usize,
}

/// Live decoder over `io/output`, `actions/events`, `requests/events`, `machine/ui/events`, and `machine/tape`.
///
/// One stream stays valid across turns. Open it once, before the first
/// `submit_text`, and keep calling [`Self::next_event`] after each
/// [`EmbedEvent::TurnFinished`].
pub struct EmbedEventStream {
    shell: Shell,
    agent_path: String,
    output: Tail,
    actions: Tail,
    requests: Tail,
    ui_events: Tail,
    tape: Tail,
    output_utf8_carry: Vec<u8>,
    pending_output_bytes: Vec<u8>,
    action_buf: Vec<u8>,
    request_buf: Vec<u8>,
    ui_buf: Vec<u8>,
    tape_buf: Vec<u8>,
    queued: VecDeque<EmbedEvent>,
    tape_assistant_turns: VecDeque<TapeAssistantTurn>,
    current_submission_id: Option<String>,
    emitted_output_bytes_in_turn: usize,
    active_thinking: String,
    thinking_open: bool,
    actions_state: HashMap<String, ActionTrack>,
    open_requests: VecDeque<String>,
    emitted_requests: HashSet<String>,
}

impl EmbedEventStream {
    pub(super) async fn open(shell: Shell, agent_path: String) -> Result<Self> {
        let output = shell
            .tail(&format!("{agent_path}/io/output"))
            .await
            .map_err(|code| anyhow::anyhow!("tail io/output: {code}"))?;
        let actions = shell
            .tail(&format!("{agent_path}/actions/events"))
            .await
            .map_err(|code| anyhow::anyhow!("tail actions/events: {code}"))?;
        let requests = shell
            .tail(&format!("{agent_path}/requests/events"))
            .await
            .map_err(|code| anyhow::anyhow!("tail requests/events: {code}"))?;
        let ui_events = shell
            .tail(&format!("{agent_path}/machine/ui/events"))
            .await
            .map_err(|code| anyhow::anyhow!("tail machine/ui/events: {code}"))?;
        let tape = shell
            .tail(&format!("{agent_path}/machine/tape"))
            .await
            .map_err(|code| anyhow::anyhow!("tail machine/tape: {code}"))?;
        Ok(Self {
            shell,
            agent_path,
            output,
            actions,
            requests,
            ui_events,
            tape,
            output_utf8_carry: Vec::new(),
            pending_output_bytes: Vec::new(),
            action_buf: Vec::new(),
            request_buf: Vec::new(),
            ui_buf: Vec::new(),
            tape_buf: Vec::new(),
            queued: VecDeque::new(),
            tape_assistant_turns: VecDeque::new(),
            current_submission_id: None,
            emitted_output_bytes_in_turn: 0,
            active_thinking: String::new(),
            thinking_open: false,
            actions_state: HashMap::new(),
            open_requests: VecDeque::new(),
            emitted_requests: HashSet::new(),
        })
    }

    /// Read the next [`EmbedEvent`] in causal order.
    pub async fn next_event(&mut self) -> Result<EmbedEvent> {
        loop {
            if let Some(event) = self.queued.pop_front() {
                return Ok(event);
            }
            self.process_buffered().await?;
            if let Some(event) = self.queued.pop_front() {
                return Ok(event);
            }
            tokio::select! {
                chunk = self.output.read(4096) => {
                    let bytes = chunk.map_err(|code| anyhow::anyhow!("read embedded io/output: {code}"))?;
                    if !bytes.is_empty() {
                        self.push_output_chunk(&bytes);
                    } else if self.streams_drained().await? {
                        bail!("embedded event stream closed");
                    }
                }
                chunk = self.actions.read(4096) => {
                    let bytes = chunk.map_err(|code| anyhow::anyhow!("read embedded actions/events: {code}"))?;
                    self.action_buf.extend(bytes);
                }
                chunk = self.requests.read(4096) => {
                    let bytes = chunk.map_err(|code| anyhow::anyhow!("read embedded requests/events: {code}"))?;
                    self.request_buf.extend(bytes);
                }
                chunk = self.ui_events.read(4096) => {
                    let bytes = chunk.map_err(|code| anyhow::anyhow!("read embedded ui/events: {code}"))?;
                    self.ui_buf.extend(bytes);
                }
                chunk = self.tape.read(4096) => {
                    let bytes = chunk.map_err(|code| anyhow::anyhow!("read embedded tape: {code}"))?;
                    self.tape_buf.extend(bytes);
                }
            }
        }
    }

    async fn streams_drained(&mut self) -> Result<bool> {
        let ui = self
            .ui_events
            .read(1)
            .await
            .map_err(|code| anyhow::anyhow!("read embedded ui/events: {code}"))?;
        if !ui.is_empty() {
            self.ui_buf.extend(ui);
            return Ok(false);
        }
        Ok(true)
    }

    fn push_output_chunk(&mut self, bytes: &[u8]) {
        self.output_utf8_carry.extend_from_slice(bytes);
        let valid_len = match std::str::from_utf8(&self.output_utf8_carry) {
            Ok(_) => self.output_utf8_carry.len(),
            Err(err) => err.valid_up_to(),
        };
        if valid_len > 0 {
            self.pending_output_bytes
                .extend_from_slice(&self.output_utf8_carry[..valid_len]);
            self.output_utf8_carry.drain(..valid_len);
        }
    }

    async fn drain_ready_tail(
        shell: &Shell,
        path: &str,
        tail: &mut Tail,
    ) -> Result<Option<Vec<u8>>> {
        let Ok(stat) = shell.stat(path).await else {
            return Ok(None);
        };
        let mut collected = Vec::new();
        while tail.offset() < stat.length {
            let remaining = (stat.length - tail.offset()).min(65536) as u32;
            let bytes = tail
                .read(remaining)
                .await
                .map_err(|code| anyhow::anyhow!("drain embedded {path}: {code}"))?;
            if bytes.is_empty() {
                break;
            }
            collected.extend_from_slice(&bytes);
        }
        Ok((!collected.is_empty()).then_some(collected))
    }

    async fn drain_non_ui_sources(&mut self) -> Result<()> {
        if let Some(bytes) = Self::drain_ready_tail(
            &self.shell,
            &format!("{}/machine/tape", self.agent_path),
            &mut self.tape,
        )
        .await?
        {
            self.tape_buf.extend(bytes);
        }
        self.ingest_tape_lines()?;

        if let Some(bytes) = Self::drain_ready_tail(
            &self.shell,
            &format!("{}/actions/events", self.agent_path),
            &mut self.actions,
        )
        .await?
        {
            self.action_buf.extend(bytes);
        }
        self.ingest_action_lines().await?;

        if let Some(bytes) = Self::drain_ready_tail(
            &self.shell,
            &format!("{}/requests/events", self.agent_path),
            &mut self.requests,
        )
        .await?
        {
            self.request_buf.extend(bytes);
        }
        self.ingest_request_lines().await?;

        if let Some(bytes) = Self::drain_ready_tail(
            &self.shell,
            &format!("{}/io/output", self.agent_path),
            &mut self.output,
        )
        .await?
        {
            self.push_output_chunk(&bytes);
        }
        Ok(())
    }

    async fn drain_all_ready(&mut self) -> Result<()> {
        if let Some(bytes) = Self::drain_ready_tail(
            &self.shell,
            &format!("{}/machine/ui/events", self.agent_path),
            &mut self.ui_events,
        )
        .await?
        {
            self.ui_buf.extend(bytes);
        }
        self.drain_non_ui_sources().await
    }

    async fn process_buffered(&mut self) -> Result<()> {
        self.drain_all_ready().await?;
        self.ingest_tape_lines()?;
        self.ingest_action_lines().await?;
        self.ingest_request_lines().await?;

        // Process non-terminal UI events (Activity/Thinking/Error) before flushing live output
        // so thinking deltas and submission attribution lead assistant text in causal order.
        while ui_buf_next_is_non_terminal(&self.ui_buf) {
            if let Some(line) = next_line(&mut self.ui_buf) {
                self.process_ui_line(&line).await?;
            }
        }

        if !ui_buf_has_input_completed(&self.ui_buf) {
            self.emit_live_output_bytes().await;
        }

        while let Some(line) = next_line(&mut self.ui_buf) {
            let completed = self.process_ui_line(&line).await?;
            if completed {
                break;
            }
        }
        Ok(())
    }

    async fn process_ui_line(&mut self, line: &[u8]) -> Result<bool> {
        if line.is_empty() {
            return Ok(false);
        }
        let event: UiEvent =
            serde_json::from_slice(line).context("embedded ui/events record is not a UiEvent")?;
        match event {
            UiEvent::Activity { snapshot } => match snapshot.state {
                UiActivityState::Running => {
                    self.refresh_current_submission_id().await;
                }
                UiActivityState::Paused => {
                    self.drain_non_ui_sources().await?;
                    self.flush_open_requests().await;
                }
                UiActivityState::Idle => {}
            },
            UiEvent::Thinking { snapshot } => {
                if self.current_submission_id.is_none() {
                    self.refresh_current_submission_id().await;
                }
                self.handle_thinking_snapshot(snapshot.state, snapshot.text);
            }
            UiEvent::Error {
                message,
                recoverable,
            } => {
                self.queued.push_back(EmbedEvent::Error {
                    message,
                    recoverable,
                });
            }
            UiEvent::InputCompleted {
                submission_ids,
                status,
                error,
            } => {
                self.drain_non_ui_sources().await?;
                let more_finishes = ui_buf_has_input_completed(&self.ui_buf);
                for submission_id in submission_ids {
                    self.finish_submission(submission_id, status, error.clone(), more_finishes)
                        .await;
                }
                return Ok(true);
            }
            UiEvent::Plan { .. } | UiEvent::Notice { .. } => {}
        }
        Ok(false)
    }

    fn ingest_tape_lines(&mut self) -> Result<()> {
        while let Some(line) = next_line(&mut self.tape_buf) {
            if line.is_empty() {
                continue;
            }
            let msg: TapeMessage =
                serde_json::from_slice(&line).context("embedded tape record is not a message")?;
            if msg.role == "assistant" && !msg.content.is_empty() {
                self.tape_assistant_turns.push_back(TapeAssistantTurn {
                    submission_id: msg.submission_id,
                    bytes_len: msg.content.len(),
                });
            }
        }
        Ok(())
    }

    async fn emit_live_output_bytes(&mut self) {
        if self.pending_output_bytes.is_empty() {
            return;
        }
        if self.current_submission_id.is_none() {
            self.refresh_current_submission_id().await;
        }
        let chunk = take_valid_utf8_prefix(&mut self.pending_output_bytes, usize::MAX);
        if !chunk.is_empty() {
            self.emitted_output_bytes_in_turn += chunk.len();
            self.queued.push_back(EmbedEvent::TextDelta {
                submission_id: self.current_submission_id.clone(),
                chunk,
                is_final: false,
            });
        }
    }

    async fn finish_submission(
        &mut self,
        submission_id: String,
        status: UiInputStatus,
        error: Option<String>,
        more_finishes_pending: bool,
    ) {
        let expected_for_submission = self.take_tape_bytes_for_submission(&submission_id);
        let emit_limit = match expected_for_submission {
            Some(total_len) => total_len.saturating_sub(self.emitted_output_bytes_in_turn),
            None => {
                if matches!(status, UiInputStatus::Cancelled | UiInputStatus::Failed)
                    || more_finishes_pending
                {
                    0
                } else {
                    usize::MAX
                }
            }
        };
        if emit_limit > 0 && !self.pending_output_bytes.is_empty() {
            let chunk = take_valid_utf8_prefix(&mut self.pending_output_bytes, emit_limit);
            if !chunk.is_empty() {
                self.emitted_output_bytes_in_turn += chunk.len();
                self.queued.push_back(EmbedEvent::TextDelta {
                    submission_id: Some(submission_id.clone()),
                    chunk,
                    is_final: false,
                });
            }
        }

        if self.thinking_open {
            self.queued.push_back(EmbedEvent::ThinkingDelta {
                submission_id: Some(submission_id.clone()),
                chunk: String::new(),
                is_final: true,
            });
            self.active_thinking.clear();
            self.thinking_open = false;
        }

        self.queued.push_back(EmbedEvent::TextDelta {
            submission_id: Some(submission_id.clone()),
            chunk: String::new(),
            is_final: true,
        });

        if let Some(usage) = self.read_usage_snapshot().await {
            self.queued.push_back(EmbedEvent::Usage {
                submission_id: Some(submission_id.clone()),
                usage,
            });
        }

        self.queued.push_back(EmbedEvent::TurnFinished {
            submission_id,
            status,
            error,
        });
        self.emitted_output_bytes_in_turn = 0;
        self.current_submission_id = None;
        self.refresh_current_submission_id().await;
    }

    fn take_tape_bytes_for_submission(&mut self, submission_id: &str) -> Option<usize> {
        let mut total = 0usize;
        let mut matched = false;
        let mut idx = 0;
        while idx < self.tape_assistant_turns.len() {
            let matches_submission = match &self.tape_assistant_turns[idx].submission_id {
                Some(id) => id == submission_id,
                None => idx == 0 && !matched,
            };
            if matches_submission {
                let entry = self.tape_assistant_turns.remove(idx).expect("valid index");
                total += entry.bytes_len;
                matched = true;
            } else {
                idx += 1;
            }
        }
        matched.then_some(total)
    }

    fn handle_thinking_snapshot(&mut self, state: UiThinkingState, text: String) {
        let delta = thinking_delta(&self.active_thinking, &text);
        match state {
            UiThinkingState::Streaming => {
                if !delta.is_empty() {
                    self.queued.push_back(EmbedEvent::ThinkingDelta {
                        submission_id: self.current_submission_id.clone(),
                        chunk: delta,
                        is_final: false,
                    });
                }
                self.active_thinking = text;
                self.thinking_open = true;
            }
            UiThinkingState::Complete => {
                if !delta.is_empty() {
                    self.queued.push_back(EmbedEvent::ThinkingDelta {
                        submission_id: self.current_submission_id.clone(),
                        chunk: delta,
                        is_final: false,
                    });
                }
                if self.thinking_open || !text.is_empty() {
                    self.queued.push_back(EmbedEvent::ThinkingDelta {
                        submission_id: self.current_submission_id.clone(),
                        chunk: String::new(),
                        is_final: true,
                    });
                }
                self.active_thinking.clear();
                self.thinking_open = false;
            }
            UiThinkingState::Idle => {
                if self.thinking_open {
                    self.queued.push_back(EmbedEvent::ThinkingDelta {
                        submission_id: self.current_submission_id.clone(),
                        chunk: String::new(),
                        is_final: true,
                    });
                }
                self.active_thinking.clear();
                self.thinking_open = false;
            }
        }
    }

    async fn refresh_current_submission_id(&mut self) {
        let path = format!("{}/machine/ui/queue", self.agent_path);
        let Ok(bytes) = self.shell.cat(&path).await else {
            return;
        };
        if let Ok(queue) = serde_json::from_slice::<UiQueueSnapshot>(&bytes)
            && let Some(active) = queue.active_submission_ids.first()
        {
            self.current_submission_id = Some(active.clone());
        }
    }

    async fn read_usage_snapshot(&self) -> Option<UiUsageSnapshot> {
        let path = format!("{}/machine/ui/usage", self.agent_path);
        let bytes = self.shell.cat(&path).await.ok()?;
        if bytes.is_empty() {
            return None;
        }
        serde_json::from_slice::<UiUsageSnapshot>(&bytes).ok()
    }

    async fn ingest_action_lines(&mut self) -> Result<()> {
        while let Some(line) = next_line(&mut self.action_buf) {
            if line.is_empty() {
                continue;
            }
            let line = std::str::from_utf8(&line)
                .context("embedded actions/events record is not valid UTF-8")?;
            if let Some(action_id) = line.strip_prefix("created:") {
                self.actions_state.entry(action_id.to_string()).or_default();
                continue;
            }
            let Some((action_id, field)) = line.split_once(':') else {
                continue;
            };
            if matches!(field, "result" | "status" | "name") {
                self.handle_action_update(action_id, field).await;
            }
        }
        Ok(())
    }

    async fn handle_action_update(&mut self, action_id: &str, field: &str) {
        let name = self
            .read_action_string(action_id, "name")
            .await
            .unwrap_or_default();
        let status = self
            .read_action_string(action_id, "status")
            .await
            .unwrap_or_default();
        let result_json = self.read_action_json(action_id, "result").await;

        let call_id = result_json
            .as_ref()
            .and_then(|v| v.get("call_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let title = result_json
            .as_ref()
            .and_then(|v| v.get("title"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let preview = result_json
            .as_ref()
            .and_then(|v| v.get("result_preview"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        let track = self.actions_state.entry(action_id.to_string()).or_default();
        if track.name.is_empty() && !name.is_empty() {
            track.name = name;
        }
        if let Some(call_id) = call_id {
            track.call_id = call_id;
        } else if track.call_id.is_empty() {
            track.call_id = action_id.to_string();
        }
        if let Some(title) = title {
            track.title = title;
        } else if track.title.is_empty() {
            track.title = track.name.clone();
        }

        let is_terminal = !status.is_empty() && status != "running";
        if !track.started_emitted && (field == "result" || is_terminal || status == "running") {
            track.started_emitted = true;
            if let Some(preview_val) = &preview
                && !preview_val.is_empty()
                && status == "running"
            {
                track.last_preview = Some(preview_val.clone());
            }
            self.queued.push_back(EmbedEvent::ToolStarted {
                call_id: track.call_id.clone(),
                name: track.name.clone(),
                title: track.title.clone(),
            });
            if status == "running"
                && let Some(preview_val) = track.last_preview.clone()
            {
                self.queued.push_back(EmbedEvent::ToolProgress {
                    call_id: track.call_id.clone(),
                    preview: preview_val,
                });
            }
        } else if track.started_emitted
            && !track.finished_emitted
            && field == "result"
            && !is_terminal
            && let Some(preview_val) = &preview
            && !preview_val.is_empty()
            && track.last_preview.as_deref() != Some(preview_val.as_str())
        {
            track.last_preview = Some(preview_val.clone());
            self.queued.push_back(EmbedEvent::ToolProgress {
                call_id: track.call_id.clone(),
                preview: preview_val.clone(),
            });
        }

        if is_terminal && !track.finished_emitted {
            track.finished_emitted = true;
            self.queued.push_back(EmbedEvent::ToolFinished {
                call_id: track.call_id.clone(),
                name: track.name.clone(),
                status,
                preview,
            });
        }
    }

    async fn read_action_string(&self, action_id: &str, field: &str) -> Option<String> {
        let path = format!("{}/actions/{action_id}/{field}", self.agent_path);
        let bytes = self.shell.cat(&path).await.ok()?;
        let text = String::from_utf8(bytes).ok()?;
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    async fn read_action_json(&self, action_id: &str, field: &str) -> Option<serde_json::Value> {
        let path = format!("{}/actions/{action_id}/{field}", self.agent_path);
        let bytes = self.shell.cat(&path).await.ok()?;
        if bytes.is_empty() {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    }

    async fn ingest_request_lines(&mut self) -> Result<()> {
        while let Some(line) = next_line(&mut self.request_buf) {
            if line.is_empty() {
                continue;
            }
            let line =
                std::str::from_utf8(&line).context("embedded request event is not valid UTF-8")?;
            if let Some(request_id) = line.strip_prefix("created:") {
                if !self.emitted_requests.contains(request_id)
                    && !self.open_requests.iter().any(|id| id == request_id)
                {
                    self.open_requests.push_back(request_id.to_string());
                }
                continue;
            }
            let Some((request_id, field)) = line.split_once(':') else {
                continue;
            };
            if field == "options" && !self.emitted_requests.contains(request_id) {
                self.emit_request(request_id).await;
            }
        }
        Ok(())
    }

    async fn flush_open_requests(&mut self) {
        let pending = std::mem::take(&mut self.open_requests);
        for request_id in pending {
            if !self.emitted_requests.contains(&request_id) {
                self.emit_request(&request_id).await;
            }
        }
    }

    async fn emit_request(&mut self, request_id: &str) {
        if !self.emitted_requests.insert(request_id.to_string()) {
            return;
        }
        self.open_requests.retain(|id| id != request_id);
        let base = format!("{}/requests/{request_id}", self.agent_path);
        let kind = self
            .shell
            .cat(&format!("{base}/kind"))
            .await
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let prompt = self
            .shell
            .cat(&format!("{base}/prompt"))
            .await
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default();
        let options_doc = self
            .shell
            .cat(&format!("{base}/options"))
            .await
            .ok()
            .filter(|b| !b.is_empty())
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());

        let parsed = parse_request_options(options_doc.as_ref());

        self.queued.push_back(EmbedEvent::PendingRequest {
            request_id: request_id.to_string(),
            kind,
            title: parsed.title,
            prompt,
            options: parsed.options,
            default_option: parsed.default_option,
            questions: parsed.questions,
            capability: parsed.capability,
            reason: parsed.reason,
            presentation: parsed.presentation,
        });
    }
}

#[derive(Default)]
struct ParsedRequestOptions {
    title: Option<String>,
    options: Vec<String>,
    default_option: Option<String>,
    questions: Option<Vec<serde_json::Value>>,
    capability: Option<String>,
    reason: Option<String>,
    presentation: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct TapeMessage {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    submission_id: Option<String>,
}

fn parse_request_options(doc: Option<&serde_json::Value>) -> ParsedRequestOptions {
    let Some(doc) = doc else {
        return ParsedRequestOptions::default();
    };
    let details = doc.get("details");
    let title = doc
        .get("title")
        .or_else(|| details.and_then(|d| d.get("title")))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let options = doc
        .get("options")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let default_option = doc
        .get("default_option")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let questions = doc
        .get("questions")
        .and_then(serde_json::Value::as_array)
        .cloned();
    let capability = doc
        .get("capability")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            details
                .and_then(|d| d.get("capability"))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_string);
    let reason = doc
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            details
                .and_then(|d| d.get("reason"))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_string);
    let presentation = doc
        .get("presentation")
        .cloned()
        .or_else(|| details.and_then(|d| d.get("presentation")).cloned());

    ParsedRequestOptions {
        title,
        options,
        default_option,
        questions,
        capability,
        reason,
        presentation,
    }
}

fn thinking_delta(previous: &str, current: &str) -> String {
    if let Some(suffix) = current.strip_prefix(previous) {
        return suffix.to_string();
    }
    if previous.is_empty() {
        return current.to_string();
    }
    // Streaming snapshots keep a trailing 2 KiB window; align overlap when the head shifts.
    for (idx, _) in current.char_indices() {
        let prefix = &current[..idx];
        if !prefix.is_empty() && previous.ends_with(prefix) {
            return current[idx..].to_string();
        }
    }
    if previous.ends_with(current) {
        return String::new();
    }
    current.to_string()
}

fn take_valid_utf8_prefix(buf: &mut Vec<u8>, max_bytes: usize) -> String {
    let cap = buf.len().min(max_bytes);
    if cap == 0 {
        return String::new();
    }
    let slice = &buf[..cap];
    let take_len = match std::str::from_utf8(slice) {
        Ok(_) => cap,
        Err(err) => err.valid_up_to(),
    };
    if take_len == 0 {
        return String::new();
    }
    let out = String::from_utf8(buf[..take_len].to_vec()).expect("validated utf8 prefix");
    buf.drain(..take_len);
    out
}

fn ui_buf_has_input_completed(buf: &[u8]) -> bool {
    let text = String::from_utf8_lossy(buf);
    text.lines().any(|line| {
        !line.is_empty()
            && matches!(
                serde_json::from_str::<UiEvent>(line),
                Ok(UiEvent::InputCompleted { .. })
            )
    })
}

fn ui_buf_next_is_non_terminal(buf: &[u8]) -> bool {
    let Some(pos) = buf.iter().position(|byte| *byte == b'\n') else {
        return false;
    };
    let line = std::str::from_utf8(&buf[..pos]).unwrap_or("");
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    !matches!(
        serde_json::from_str::<UiEvent>(trimmed),
        Ok(UiEvent::InputCompleted { .. })
    )
}

fn next_line(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let pos = buf.iter().position(|byte| *byte == b'\n')?;
    let mut line: Vec<u8> = buf.drain(..=pos).collect();
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Some(line)
}
