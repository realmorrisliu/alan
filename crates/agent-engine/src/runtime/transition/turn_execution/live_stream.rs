//! Live publication of one generation's text and reasoning deltas.
//!
//! The llmfs `events` stream is read incrementally; every delta is published to the Agent's
//! files as it arrives instead of after the whole response settles:
//!
//! - assistant text is appended to `io/output` (and forwarded as [`Event::TextDelta`]);
//! - reasoning updates `machine/ui/thinking` as a `Streaming` snapshot, then a `Complete` one.
//!
//! Planning is synchronous (state behind a `std` mutex, never held across an await) and the
//! resulting file operations are applied afterwards, so the planner can sit inside the
//! `FnMut(Event) -> Future` callback the generation reader expects.
//!
//! ## Guardrail hold
//!
//! Response guardrails judge the *complete* draft and may discard it for regeneration, while
//! `io/output` is append-only. A bounded prefix of the text is therefore held back whenever a
//! guardrail could still fire. If the prefix already trips the guardrail the whole draft stays
//! held until the generation ends, so a rejected draft never reaches `io/output`; otherwise the
//! prefix is released and the rest streams live. A contradiction that only appears after the
//! prefix has been released cannot be retracted, and is reported instead of retried.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use alan_agent_protocol::{Event, UiEvent, UiThinkingSnapshot};
use anyhow::{Context, Result};
use tracing::warn;

use crate::runtime::{
    response_guardrails::{
        AssistantDraft, GuardrailDecision, ResponseGuardrailContext, ResponseGuardrails,
    },
    transition::NamespaceAgentFiles,
};

/// Minimum spacing between `Streaming` reasoning snapshots.
const THINKING_FLUSH_INTERVAL: Duration = Duration::from_millis(80);
/// Trailing window carried by `Streaming` snapshots; the stream retains every event.
const THINKING_WINDOW_BYTES: usize = 2 * 1024;
/// Upper bound for the `Complete` snapshot, well under the AgentFS document limit.
const THINKING_COMPLETE_MAX_BYTES: usize = 256 * 1024;
/// Characters of assistant text held back while a guardrail could still reject the draft.
const GUARDRAIL_HOLD_CHARS: usize = 80;
/// Draft that trips every guardrail rule; used to learn whether any rule is armed.
const ARMED_PROBE: &str = "don't have access to tools. can't access the internet.";

/// Decides whether a draft prefix would be rejected by a response guardrail.
pub(super) type GuardrailProbe = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// One file operation produced by planning a live delta.
#[derive(Debug)]
enum Op {
    AppendOutput(String),
    PublishThinking(UiThinkingSnapshot),
}

/// What to forward and write for one delta, in order.
#[derive(Debug, Default)]
pub(super) struct Plan {
    forward: Vec<Event>,
    ops: Vec<Op>,
}

impl Plan {
    /// Hand the events to forward over, leaving only file operations to apply.
    pub(super) fn take_forward(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.forward)
    }
}

#[derive(Debug, PartialEq)]
enum Hold {
    /// No guardrail can fire; text passes straight through.
    Off,
    /// Buffering the draft prefix until it can be judged.
    Prefix,
    /// The prefix trips a guardrail: hold everything until the generation settles.
    Rejected,
    /// The prefix was judged acceptable and released.
    Released,
}

#[derive(Default)]
struct Thinking {
    started: Option<Instant>,
    text: String,
    last_flush: Option<Instant>,
    open: bool,
    published: bool,
}

impl Thinking {
    fn push(&mut self, chunk: &str, plan: &mut Plan, now: Instant) {
        self.started.get_or_insert(now);
        self.text.push_str(chunk);
        self.open = true;
        if self
            .last_flush
            .is_none_or(|last| now.duration_since(last) >= THINKING_FLUSH_INTERVAL)
        {
            self.last_flush = Some(now);
            self.published = true;
            plan.ops
                .push(Op::PublishThinking(UiThinkingSnapshot::streaming(
                    tail(&self.text, THINKING_WINDOW_BYTES).to_string(),
                )));
        }
    }

    fn complete(&mut self, plan: &mut Plan, now: Instant) {
        if !self.open {
            return;
        }
        self.open = false;
        self.published = true;
        let elapsed = self
            .started
            .map_or(0, |start| now.duration_since(start).as_secs());
        plan.ops
            .push(Op::PublishThinking(UiThinkingSnapshot::complete(
                tail(&self.text, THINKING_COMPLETE_MAX_BYTES).to_string(),
                elapsed,
            )));
        plan.forward.push(Event::ThinkingDelta {
            chunk: String::new(),
            is_final: true,
        });
    }
}

struct State {
    thinking: Thinking,
    hold: Hold,
    buffer: String,
    /// Text appended (or planned to be appended) to `io/output` so far.
    written: String,
}

/// Cloneable handle shared between the generation callback and the turn loop.
#[derive(Clone)]
pub(super) struct LiveTurnStream {
    files: NamespaceAgentFiles,
    state: Arc<Mutex<State>>,
    probe: Option<GuardrailProbe>,
    visible: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<anyhow::Error>>>,
}

impl LiveTurnStream {
    /// `probe` is the guardrail oracle; `None` (or a probe that can never fire) disables the hold.
    pub(super) fn new(files: NamespaceAgentFiles, probe: Option<GuardrailProbe>) -> Self {
        let probe = probe.filter(|probe| probe(ARMED_PROBE));
        Self {
            files,
            state: Arc::new(Mutex::new(State {
                thinking: Thinking::default(),
                hold: if probe.is_some() {
                    Hold::Prefix
                } else {
                    Hold::Off
                },
                buffer: String::new(),
                written: String::new(),
            })),
            probe,
            visible: Arc::new(AtomicBool::new(false)),
            failure: Arc::new(Mutex::new(None)),
        }
    }

    /// Whether any assistant text has reached `io/output` (and so cannot be retracted).
    pub(super) fn text_visible(&self) -> bool {
        self.visible.load(Ordering::Acquire)
    }

    /// Whether reasoning was published live for this generation.
    pub(super) fn thinking_published(&self) -> bool {
        self.state
            .lock()
            .expect("live stream state")
            .thinking
            .published
    }

    /// Plan one delta. Non-delta events plan nothing.
    pub(super) fn plan_event(&self, event: Event) -> Plan {
        let mut plan = Plan::default();
        let now = Instant::now();
        let mut state = self.state.lock().expect("live stream state");
        match event {
            Event::ThinkingDelta { chunk, is_final } => {
                if !is_final && !chunk.is_empty() {
                    state.thinking.push(&chunk, &mut plan, now);
                    plan.forward.push(Event::ThinkingDelta {
                        chunk,
                        is_final: false,
                    });
                }
            }
            Event::TextDelta {
                chunk,
                is_final: false,
            } if !chunk.is_empty() => {
                state.thinking.complete(&mut plan, now);
                self.plan_text(&mut state, chunk, &mut plan);
            }
            _ => {}
        }
        plan
    }

    fn plan_text(&self, state: &mut State, chunk: String, plan: &mut Plan) {
        match state.hold {
            Hold::Off | Hold::Released => self.release(state, chunk, plan),
            Hold::Rejected => state.buffer.push_str(&chunk),
            Hold::Prefix => {
                state.buffer.push_str(&chunk);
                if state.buffer.chars().count() < GUARDRAIL_HOLD_CHARS {
                    return;
                }
                let rejected = self
                    .probe
                    .as_ref()
                    .is_some_and(|probe| probe(&state.buffer));
                if rejected {
                    state.hold = Hold::Rejected;
                } else {
                    state.hold = Hold::Released;
                    let prefix = std::mem::take(&mut state.buffer);
                    self.release(state, prefix, plan);
                }
            }
        }
    }

    fn release(&self, state: &mut State, text: String, plan: &mut Plan) {
        if text.is_empty() {
            return;
        }
        self.visible.store(true, Ordering::Release);
        state.written.push_str(&text);
        plan.forward.push(Event::TextDelta {
            chunk: text.clone(),
            is_final: false,
        });
        plan.ops.push(Op::AppendOutput(text));
    }

    /// The generation settled and the draft was accepted: finish reasoning, release held text.
    pub(super) fn plan_accept(&self) -> Plan {
        let mut plan = Plan::default();
        let mut state = self.state.lock().expect("live stream state");
        state.thinking.complete(&mut plan, Instant::now());
        let held = std::mem::take(&mut state.buffer);
        state.hold = Hold::Released;
        self.release(&mut state, held, &mut plan);
        plan
    }

    /// The generation settled and the draft was rejected before any text was released.
    pub(super) fn plan_discard(&self) -> Plan {
        let mut plan = Plan::default();
        let mut state = self.state.lock().expect("live stream state");
        state.thinking.complete(&mut plan, Instant::now());
        state.buffer.clear();
        plan
    }

    /// A failed attempt is about to be retried: forget partial reasoning and held text.
    pub(super) fn plan_retry(&self) -> Plan {
        let mut plan = Plan::default();
        let mut state = self.state.lock().expect("live stream state");
        let had_reasoning = state.thinking.published;
        state.thinking = Thinking::default();
        state.buffer.clear();
        if self.probe.is_some() {
            state.hold = Hold::Prefix;
        }
        if had_reasoning {
            plan.ops
                .push(Op::PublishThinking(UiThinkingSnapshot::idle()));
        }
        plan
    }

    /// Text of `content` that this stream has not appended to `io/output`.
    ///
    /// `Some` only when part of the draft was streamed and the rest is missing, which keeps
    /// `io/output` equal to the recorded assistant message. Nothing streamed yields `None`: the
    /// caller then publishes the whole message itself.
    pub(super) fn unstreamed_remainder(&self, content: &str) -> Option<String> {
        let state = self.state.lock().expect("live stream state");
        if state.written.is_empty() {
            return None;
        }
        match content.strip_prefix(state.written.as_str()) {
            Some("") => None,
            Some(rest) => Some(rest.to_string()),
            None => {
                warn!("Streamed assistant text diverged from the settled response");
                None
            }
        }
    }

    /// Apply `plan`, forward its events in order, and surface any publication failure.
    pub(super) async fn settle<E, F>(&self, mut plan: Plan, emit: &mut E) -> Result<()>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        let events = plan.take_forward();
        self.apply(plan).await;
        for event in events {
            emit(event).await;
        }
        self.check()
    }

    /// Apply a plan's file operations. The first failure is remembered; later operations are
    /// skipped so a broken surface is reported once, by [`Self::check`].
    pub(super) async fn apply(&self, plan: Plan) {
        for op in plan.ops {
            if self.failure.lock().expect("live stream failure").is_some() {
                return;
            }
            if let Err(error) = self.apply_op(op).await {
                warn!(error = %error, "Failed to publish live generation output");
                *self.failure.lock().expect("live stream failure") = Some(error);
                return;
            }
        }
    }

    async fn apply_op(&self, op: Op) -> Result<()> {
        match op {
            Op::AppendOutput(text) => self
                .files
                .write_assistant_output(&text)
                .await
                .context("stream namespace assistant output"),
            Op::PublishThinking(snapshot) => {
                self.files
                    .write_ui_thinking_snapshot(&snapshot)
                    .await
                    .context("write thinking UI state")?;
                self.files
                    .append_ui_event(&UiEvent::Thinking { snapshot })
                    .await
                    .context("publish thinking UI event")
            }
        }
    }

    /// Surface the first publication failure, if any.
    pub(super) fn check(&self) -> Result<()> {
        match self.failure.lock().expect("live stream failure").take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// Guardrail oracle over a draft prefix, or `None` when no guardrail can fire any more.
pub(super) fn guardrail_probe(
    context: Arc<ResponseGuardrailContext>,
    budget_left: bool,
) -> Option<GuardrailProbe> {
    budget_left.then(|| -> GuardrailProbe {
        Arc::new(move |prefix: &str| {
            matches!(
                ResponseGuardrails::default()
                    .evaluate(&context, &AssistantDraft::new(prefix, false)),
                GuardrailDecision::Recover { .. }
            )
        })
    })
}

/// Trailing `max` bytes of `text`, cut on a character boundary.
fn tail(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

#[cfg(test)]
#[path = "live_stream_tests.rs"]
mod tests;
