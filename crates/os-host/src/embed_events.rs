use std::collections::VecDeque;

use alan_agent_protocol::{UiEvent, UiInputStatus};
use alan_shell::Tail;
use anyhow::{Context, Result, bail};
use serde::Deserialize;

const READ_CHUNK: u32 = 4096;

/// One record an embedder can observe for a running Agent Process.
///
/// [`Self::TurnFinished`] is the only end-of-turn signal. `TextDelta::is_final`
/// is currently always `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedEvent {
    /// Assistant text for one submission, from a `machine/tape` message.
    ///
    /// `is_final` is currently always `false`. The turn ends at [`Self::TurnFinished`].
    TextDelta { chunk: String, is_final: bool },
    /// End of one submission, from `UiEvent::InputCompleted`, in listed order.
    ///
    /// This is the only end-of-turn signal.
    TurnFinished {
        submission_id: String,
        status: UiInputStatus,
        /// Error string from that `InputCompleted`, when the engine set one.
        error: Option<String>,
    },
    /// A newly allocated yield, from a `created:{id}` request line.
    PendingRequest { request_id: String },
    /// A `UiEvent::Error` record.
    Error { message: String, recoverable: bool },
}

/// Live decoder over tape, request, and UI tails.
///
/// One stream stays valid across turns. Open it once, before the first
/// `submit_text`, and keep calling [`Self::next_event`] after each
/// [`EmbedEvent::TurnFinished`]. The same stream yields the next turn's text
/// and that submission's `TurnFinished`.
///
/// Offsets stay on this stream. [`crate::embed::RootAgentClient`] remains a cheap
/// clone of the shell and the agent path.
pub struct EmbedEventStream {
    tape: Tail,
    requests: Tail,
    ui_events: Tail,
    tape_buf: Vec<u8>,
    request_buf: Vec<u8>,
    ui_buf: Vec<u8>,
    immediate: VecDeque<EmbedEvent>,
    texts: VecDeque<QueuedText>,
    finishes: VecDeque<QueuedFinish>,
    open: Option<OpenTurn>,
}

struct QueuedText {
    submission_id: Option<String>,
    chunk: String,
    emitted: bool,
}

struct QueuedFinish {
    submission_id: String,
    status: UiInputStatus,
    error: Option<String>,
}

struct OpenTurn {
    submission_id: Option<String>,
}

#[derive(Deserialize)]
struct TapeMessage {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    submission_id: Option<String>,
}

impl EmbedEventStream {
    pub(super) fn from_tails(tape: Tail, requests: Tail, ui_events: Tail) -> Self {
        Self {
            tape,
            requests,
            ui_events,
            tape_buf: Vec::new(),
            request_buf: Vec::new(),
            ui_buf: Vec::new(),
            immediate: VecDeque::new(),
            texts: VecDeque::new(),
            finishes: VecDeque::new(),
            open: None,
        }
    }

    /// Block until the next mapped event.
    ///
    /// Skipped UI and request lines are consumed here. An empty tail read is a
    /// closed stream and returns an error. Assistant text for a later submission
    /// stays queued until the open submission's [`EmbedEvent::TurnFinished`].
    pub async fn next_event(&mut self) -> Result<EmbedEvent> {
        loop {
            self.ingest()?;
            if let Some(event) = self.immediate.pop_front() {
                return Ok(event);
            }
            if let Some(event) = self.take_text() {
                return Ok(event);
            }
            if self.needs_unread_tape() {
                if self.tape_caught_up().await? {
                    if let Some(event) = self.take_finish() {
                        return Ok(event);
                    }
                } else {
                    push_chunk(&mut self.tape_buf, self.tape.read(READ_CHUNK).await, "tape")?;
                    continue;
                }
            } else if let Some(event) = self.take_finish() {
                return Ok(event);
            }

            let wait_for_tape = !self.tape_caught_up().await?;
            tokio::select! {
                biased;
                chunk = self.tape.read(READ_CHUNK), if wait_for_tape => {
                    push_chunk(&mut self.tape_buf, chunk, "tape")?;
                }
                chunk = self.requests.read(READ_CHUNK) => {
                    push_chunk(&mut self.request_buf, chunk, "requests")?;
                }
                chunk = self.ui_events.read(READ_CHUNK) => {
                    push_chunk(&mut self.ui_buf, chunk, "ui events")?;
                }
            }
        }
    }

    fn ingest(&mut self) -> Result<()> {
        while let Some(line) = next_line(&mut self.tape_buf) {
            if let Some(text) = map_tape_line(&line)? {
                self.texts.push_back(text);
            }
        }
        while let Some(line) = next_line(&mut self.request_buf) {
            if let Some(event) = map_request_line(&line)? {
                self.immediate.push_back(event);
            }
        }
        while let Some(line) = next_line(&mut self.ui_buf) {
            for event in map_ui_line(&line)? {
                match event {
                    EmbedEvent::TurnFinished {
                        submission_id,
                        status,
                        error,
                    } => self.finishes.push_back(QueuedFinish {
                        submission_id,
                        status,
                        error,
                    }),
                    other => self.immediate.push_back(other),
                }
            }
        }
        Ok(())
    }

    fn take_text(&mut self) -> Option<EmbedEvent> {
        let index = if self.open.is_none() {
            self.texts.iter().position(|text| !text.emitted)?
        } else {
            self.texts
                .iter()
                .position(|text| !text.emitted && self.belongs_to_open(text))?
        };
        if self.open.is_none() {
            self.open = Some(OpenTurn {
                submission_id: self.texts[index].submission_id.clone(),
            });
        }
        self.texts[index].emitted = true;
        Some(EmbedEvent::TextDelta {
            chunk: self.texts[index].chunk.clone(),
            is_final: false,
        })
    }

    fn belongs_to_open(&self, text: &QueuedText) -> bool {
        let Some(open) = &self.open else {
            return false;
        };
        text.submission_id.is_none() || text.submission_id == open.submission_id
    }

    fn take_finish(&mut self) -> Option<EmbedEvent> {
        let waiting = self.finishes.front().is_some_and(|finish| {
            self.texts.iter().any(|text| {
                !text.emitted && text.submission_id.as_ref() == Some(&finish.submission_id)
            })
        });
        if waiting {
            return None;
        }
        let finish = self.finishes.pop_front()?;
        if self.open.as_ref().is_some_and(|open| {
            open.submission_id.is_none()
                || open.submission_id.as_ref() == Some(&finish.submission_id)
        }) {
            self.open = None;
        }
        Some(EmbedEvent::TurnFinished {
            submission_id: finish.submission_id,
            status: finish.status,
            error: finish.error,
        })
    }

    fn needs_unread_tape(&self) -> bool {
        let Some(finish) = self.finishes.front() else {
            return false;
        };
        if finish.status != UiInputStatus::Completed {
            return false;
        }
        !self
            .texts
            .iter()
            .any(|text| text.submission_id.as_ref() == Some(&finish.submission_id))
    }

    async fn tape_caught_up(&self) -> Result<bool> {
        if !self.tape_buf.is_empty() {
            return Ok(false);
        }
        let snap = self.tape.snapshot().await.context("stat embedded tape")?;
        Ok(snap.len() as u64 <= self.tape.offset())
    }
}

fn push_chunk(
    buf: &mut Vec<u8>,
    chunk: std::result::Result<Vec<u8>, alan_ap::ErrorCode>,
    stream: &str,
) -> Result<()> {
    let chunk = chunk.with_context(|| format!("read embedded {stream}"))?;
    if chunk.is_empty() {
        bail!("embedded {stream} stream closed");
    }
    buf.extend(chunk);
    Ok(())
}

fn map_tape_line(line: &[u8]) -> Result<Option<QueuedText>> {
    if line.is_empty() {
        return Ok(None);
    }
    let message: TapeMessage =
        serde_json::from_slice(line).context("embedded tape record is not a message")?;
    if message.role != "assistant" || message.content.is_empty() {
        return Ok(None);
    }
    Ok(Some(QueuedText {
        submission_id: message.submission_id,
        chunk: message.content,
        emitted: false,
    }))
}

fn map_request_line(line: &[u8]) -> Result<Option<EmbedEvent>> {
    if line.is_empty() {
        return Ok(None);
    }
    let line = std::str::from_utf8(line).context("embedded request event is not valid UTF-8")?;
    Ok(line
        .strip_prefix("created:")
        .map(|request_id| EmbedEvent::PendingRequest {
            request_id: request_id.to_string(),
        }))
}

fn map_ui_line(line: &[u8]) -> Result<Vec<EmbedEvent>> {
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let event: UiEvent =
        serde_json::from_slice(line).context("embedded ui event is not a UiEvent")?;
    Ok(match event {
        UiEvent::Activity { .. }
        | UiEvent::Plan { .. }
        | UiEvent::Thinking { .. }
        | UiEvent::Notice { .. } => Vec::new(),
        UiEvent::Error {
            message,
            recoverable,
        } => vec![EmbedEvent::Error {
            message,
            recoverable,
        }],
        UiEvent::InputCompleted {
            submission_ids,
            status,
            error,
        } => submission_ids
            .into_iter()
            .map(|submission_id| EmbedEvent::TurnFinished {
                submission_id,
                status,
                error: error.clone(),
            })
            .collect(),
    })
}

fn next_line(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let end = buf.iter().position(|byte| *byte == b'\n')?;
    let mut line: Vec<u8> = buf.drain(..=end).collect();
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Some(line)
}
