//! Transport-neutral projection of OpenAI Responses streaming events.
//!
//! HTTP SSE `data:` payloads and WebSocket text frames carry the same event
//! JSON. Both transports feed this projector so they yield identical
//! [`StreamChunk`] sequences.

use serde_json::Value;
use tracing::debug;

use crate::openai_chat_completions::{
    OpenAiResponsesResponse, convert_openai_responses_usage,
    extract_responses_output_reasoning_signature, responses_finish_reason,
};
use crate::{StreamChunk, TokenUsage, ToolCallDelta};

/// How a projected event affects the response lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventOutcome {
    /// More events are expected.
    Continue,
    /// The response reached a protocol terminal (completed, incomplete,
    /// cancelled, failed or `[DONE]`).
    Terminal,
    /// The transport reported an `error` event; the response is over.
    Error,
}

/// Chunks produced by one event plus its lifecycle outcome.
#[derive(Debug)]
pub(crate) struct ProjectedEvent {
    pub(crate) chunks: Vec<StreamChunk>,
    pub(crate) outcome: EventOutcome,
}

impl ProjectedEvent {
    fn new(chunks: Vec<StreamChunk>, outcome: EventOutcome) -> Self {
        Self { chunks, outcome }
    }

    fn none() -> Self {
        Self::new(Vec::new(), EventOutcome::Continue)
    }

    /// Whether the response is over after this event.
    pub(crate) fn is_terminal(&self) -> bool {
        self.outcome != EventOutcome::Continue
    }
}

/// Stateful projector for one Responses stream.
#[derive(Debug, Default)]
pub(crate) struct ResponsesStreamProjector {
    latest_usage: Option<TokenUsage>,
    emitted_payload: bool,
    saw_tool_calls: bool,
}

fn chunk() -> StreamChunk {
    StreamChunk {
        text: None,
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        usage: None,
        provider_response_id: None,
        provider_response_status: None,
        sequence_number: None,
        tool_call_delta: None,
        is_finished: false,
        finish_reason: None,
    }
}

impl ResponsesStreamProjector {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Project one raw event payload (`[DONE]` or event JSON).
    pub(crate) fn project_data(&mut self, data: &str) -> ProjectedEvent {
        if data == "[DONE]" {
            return ProjectedEvent::new(
                self.finish_chunk(None).into_iter().collect(),
                EventOutcome::Terminal,
            );
        }
        match serde_json::from_str::<Value>(data) {
            Ok(event) => self.project_event(&event),
            Err(_) => {
                debug!(data, "Failed to parse Responses stream event");
                ProjectedEvent::none()
            }
        }
    }

    /// Terminal chunk for a byte stream that ended without a protocol terminal.
    pub(crate) fn finish(&mut self) -> Option<StreamChunk> {
        self.finish_chunk(None)
    }

    fn finish_chunk(&self, sequence_number: Option<u64>) -> Option<StreamChunk> {
        self.emitted_payload.then(|| StreamChunk {
            usage: self.latest_usage,
            sequence_number,
            is_finished: true,
            finish_reason: Some(responses_finish_reason(self.saw_tool_calls).to_string()),
            ..chunk()
        })
    }

    fn error_chunk(
        &self,
        event: &Value,
        response_id: Option<String>,
        response_status: Option<String>,
    ) -> Vec<StreamChunk> {
        if !self.emitted_payload {
            return Vec::new();
        }
        vec![StreamChunk {
            usage: self.latest_usage,
            provider_response_id: response_id,
            provider_response_status: response_status,
            sequence_number: sequence_number(event),
            is_finished: true,
            finish_reason: Some("stream_error".to_string()),
            ..chunk()
        }]
    }

    /// Project one parsed event.
    pub(crate) fn project_event(&mut self, event: &Value) -> ProjectedEvent {
        let Some(event_type) = event.get("type").and_then(Value::as_str) else {
            return ProjectedEvent::none();
        };
        let sequence_number = sequence_number(event);
        match event_type {
            "response.output_text.delta" | "response.refusal.delta" => {
                let Some(text) = responses_stream_text_delta(event) else {
                    return ProjectedEvent::none();
                };
                self.emitted_payload = true;
                let chunk = StreamChunk {
                    text: Some(text.to_string()),
                    sequence_number,
                    ..chunk()
                };
                ProjectedEvent::new(vec![chunk], EventOutcome::Continue)
            }
            "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
                let Some(thinking) = event
                    .get("delta")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                else {
                    return ProjectedEvent::none();
                };
                self.emitted_payload = true;
                let chunk = StreamChunk {
                    thinking: Some(thinking.to_string()),
                    sequence_number,
                    ..chunk()
                };
                ProjectedEvent::new(vec![chunk], EventOutcome::Continue)
            }
            "response.function_call_arguments.delta" => {
                let delta = event
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if delta.is_empty() {
                    return ProjectedEvent::none();
                }
                self.emitted_payload = true;
                let chunk = StreamChunk {
                    sequence_number,
                    tool_call_delta: Some(ToolCallDelta {
                        index: stream_index(event),
                        id: stream_tool_id(event.get("item"), event),
                        name: stream_tool_name(event.get("item"), event),
                        arguments_delta: Some(delta.to_string()),
                        arguments: None,
                    }),
                    ..chunk()
                };
                ProjectedEvent::new(vec![chunk], EventOutcome::Continue)
            }
            "response.output_item.done" => self.project_output_item_done(event),
            "response.completed" => self.project_completed(event),
            "response.incomplete" | "response.cancelled" => {
                let (response_id, response_status) = event
                    .get("response")
                    .cloned()
                    .and_then(|response| {
                        serde_json::from_value::<OpenAiResponsesResponse>(response).ok()
                    })
                    .map(|response| (response.id, response.status))
                    .unwrap_or((None, None));
                ProjectedEvent::new(
                    self.error_chunk(event, response_id, response_status),
                    EventOutcome::Terminal,
                )
            }
            "response.failed" => {
                ProjectedEvent::new(self.error_chunk(event, None, None), EventOutcome::Terminal)
            }
            "error" => {
                ProjectedEvent::new(self.error_chunk(event, None, None), EventOutcome::Error)
            }
            _ => ProjectedEvent::none(),
        }
    }

    fn project_output_item_done(&mut self, event: &Value) -> ProjectedEvent {
        let Some(item) = event.get("item") else {
            return ProjectedEvent::none();
        };
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            return ProjectedEvent::none();
        }
        let arguments = item
            .get("arguments")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let name = stream_tool_name(Some(item), event);
        let (Some(arguments), Some(name)) = (arguments, name) else {
            return ProjectedEvent::none();
        };
        self.emitted_payload = true;
        self.saw_tool_calls = true;
        let chunk = StreamChunk {
            sequence_number: sequence_number(event),
            tool_call_delta: Some(ToolCallDelta {
                index: stream_index(event),
                id: stream_tool_id(Some(item), event),
                name: Some(name),
                arguments_delta: None,
                arguments: Some(arguments.to_string()),
            }),
            ..chunk()
        };
        ProjectedEvent::new(vec![chunk], EventOutcome::Continue)
    }

    fn project_completed(&mut self, event: &Value) -> ProjectedEvent {
        let sequence_number = sequence_number(event);
        let mut chunks = Vec::new();
        let mut response_id = None;
        let mut response_status = None;
        if let Some(response) = event.get("response").cloned() {
            match serde_json::from_value::<OpenAiResponsesResponse>(response) {
                Ok(parsed) => {
                    response_id = parsed.id.clone();
                    response_status = parsed.status.clone();
                    self.latest_usage = parsed.usage.map(convert_openai_responses_usage);
                    if !self.saw_tool_calls {
                        self.saw_tool_calls = output_contains_tool_call(&parsed.output);
                    }
                    if let Some(signature) =
                        extract_responses_output_reasoning_signature(&parsed.output)
                    {
                        chunks.push(StreamChunk {
                            thinking_signature: Some(signature),
                            sequence_number,
                            ..chunk()
                        });
                    }
                }
                Err(error) => debug!(?error, "Failed to parse response.completed payload"),
            }
        }
        chunks.push(StreamChunk {
            usage: self.latest_usage,
            provider_response_id: response_id,
            provider_response_status: response_status,
            sequence_number,
            is_finished: true,
            finish_reason: Some(responses_finish_reason(self.saw_tool_calls).to_string()),
            ..chunk()
        });
        ProjectedEvent::new(chunks, EventOutcome::Terminal)
    }
}

fn stream_index(event: &Value) -> usize {
    event
        .get("output_index")
        .or_else(|| event.get("item_index"))
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize
}

fn stream_tool_id(item: Option<&Value>, event: &Value) -> Option<String> {
    item.and_then(|value| {
        value
            .get("call_id")
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
    .or_else(|| {
        event
            .get("call_id")
            .or_else(|| event.get("item_id"))
            .or_else(|| event.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn stream_tool_name(item: Option<&Value>, event: &Value) -> Option<String> {
    let name_of = |value: &Value| {
        value
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    };
    item.and_then(name_of).or_else(|| name_of(event))
}

fn output_contains_tool_call(output: &[Value]) -> bool {
    output
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
}

fn sequence_number(event: &Value) -> Option<u64> {
    event.get("sequence_number").and_then(Value::as_u64)
}

/// Streamed text can arrive as standalone spaces/newlines that preserve formatting.
pub(crate) fn responses_stream_text_delta(event: &Value) -> Option<&str> {
    event
        .get("delta")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}
