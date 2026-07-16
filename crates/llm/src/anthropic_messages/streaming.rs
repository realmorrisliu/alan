use std::collections::HashMap;

use tokio::sync::mpsc::{self, Receiver, Sender};

use super::{StreamEvent, convert_usage, is_non_empty};
use crate::{StreamChunk, TokenUsage, ToolCallDelta};

pub(super) fn project_events(mut event_rx: Receiver<StreamEvent>) -> Receiver<StreamChunk> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        let mut latest_usage: Option<TokenUsage> = None;
        let mut latest_response_id: Option<String> = None;
        let mut tool_call_meta: HashMap<usize, StreamedToolUseState> = HashMap::new();

        while let Some(event) = event_rx.recv().await {
            if let Some(message) = event.message.as_ref()
                && let Some(id) = message.id.clone()
            {
                latest_response_id = Some(id);
            }
            let usage_from_event = event.usage.clone().or_else(|| {
                event
                    .message
                    .as_ref()
                    .and_then(|message| message.usage.clone())
            });
            if let Some(usage) = usage_from_event {
                latest_usage = Some(convert_usage(usage));
            }

            match event.event_type.as_str() {
                "content_block_start" => {
                    if let Some(content_block) = event.content_block {
                        if content_block.block_type == "redacted_thinking"
                            && let Some(data) = content_block.data
                        {
                            let _ = tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: Some(data),
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: None,
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                })
                                .await;
                        }
                        if content_block.block_type == "tool_use" {
                            let index = event
                                .index
                                .unwrap_or_default()
                                .max(0)
                                .try_into()
                                .unwrap_or(0usize);
                            let id = content_block.id.clone();
                            let name = content_block.name.clone();
                            let start_input = content_block.input.map(|input| input.to_string());
                            tool_call_meta.insert(
                                index,
                                StreamedToolUseState::new(id.clone(), name.clone(), start_input),
                            );
                            if !send_stream_chunk(
                                &tx,
                                StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: Some(ToolCallDelta {
                                        index,
                                        id,
                                        name,
                                        arguments_delta: None,
                                        arguments: None,
                                    }),
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                },
                            )
                            .await
                            {
                                return;
                            }
                        }
                    }
                }
                "content_block_delta" => {
                    if let Some(delta) = event.delta {
                        if let Some(thinking) = delta.thinking {
                            let _ = tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: Some(thinking),
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: None,
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                })
                                .await;
                        }
                        if let Some(signature) = delta.signature {
                            let _ = tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: Some(signature),
                                    redacted_thinking: None,
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: None,
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                })
                                .await;
                        }
                        if let Some(text) = delta.text {
                            let _ = tx
                                .send(StreamChunk {
                                    text: Some(text),
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: None,
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                })
                                .await;
                        }
                        if let (Some(partial_json), Some(index)) = (delta.partial_json, event.index)
                        {
                            let index = index.max(0).try_into().unwrap_or(0usize);
                            let state = tool_call_meta
                                .entry(index)
                                .or_insert_with(|| StreamedToolUseState::new(None, None, None));
                            let arguments_delta =
                                state.merge_partial_json_for_preview(partial_json);
                            if !send_stream_chunk(
                                &tx,
                                StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: Some(ToolCallDelta {
                                        index,
                                        id: state.id.clone(),
                                        name: state.name.clone(),
                                        arguments_delta: Some(arguments_delta),
                                        arguments: None,
                                    }),
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                },
                            )
                            .await
                            {
                                return;
                            }
                        }
                    }
                }
                "content_block_stop" => {
                    if let Some(index) = event.index {
                        let index = index.max(0).try_into().unwrap_or(0usize);
                        if let Some(state) = tool_call_meta.remove(&index) {
                            for chunk in finalize_tool_use_chunks(index, state) {
                                if !send_stream_chunk(&tx, chunk).await {
                                    return;
                                }
                            }
                        }
                    }
                }
                "message_stop" => {
                    let mut indices: Vec<usize> = tool_call_meta.keys().copied().collect();
                    indices.sort_unstable();
                    for index in indices {
                        if let Some(state) = tool_call_meta.remove(&index) {
                            for chunk in finalize_tool_use_chunks(index, state) {
                                if !send_stream_chunk(&tx, chunk).await {
                                    return;
                                }
                            }
                        }
                    }
                    let _ = send_stream_chunk(
                        &tx,
                        StreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: latest_usage,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: true,
                            finish_reason: event.message.and_then(|message| message.stop_reason),
                            provider_response_id: latest_response_id.clone(),
                            provider_response_status: None,
                        },
                    )
                    .await;
                }
                _ => {}
            }
        }
    });

    rx
}

fn merge_initial_tool_arguments_delta(
    start_input: &mut Option<String>,
    saw_partial_json: &mut bool,
    partial_json: String,
) -> String {
    let merged = if !*saw_partial_json {
        start_input
            .take()
            .map(|prefix| format!("{prefix}{partial_json}"))
    } else {
        None
    };
    *saw_partial_json = true;
    merged.unwrap_or(partial_json)
}

#[derive(Debug, Clone)]
struct StreamedToolUseState {
    id: Option<String>,
    name: Option<String>,
    preview_start_input: Option<String>,
    accumulated_arguments: String,
    saw_partial_json: bool,
}

impl StreamedToolUseState {
    fn new(id: Option<String>, name: Option<String>, start_input: Option<String>) -> Self {
        Self {
            id,
            name,
            preview_start_input: start_input,
            accumulated_arguments: String::new(),
            saw_partial_json: false,
        }
    }

    fn merge_partial_json_for_preview(&mut self, partial_json: String) -> String {
        self.accumulated_arguments.push_str(&partial_json);
        merge_initial_tool_arguments_delta(
            &mut self.preview_start_input,
            &mut self.saw_partial_json,
            partial_json,
        )
    }

    fn finalized_arguments(&self) -> Option<String> {
        if self.saw_partial_json {
            (!self.accumulated_arguments.is_empty()).then(|| self.accumulated_arguments.clone())
        } else {
            self.preview_start_input
                .clone()
                .filter(|value| is_non_empty(value))
        }
    }
}

fn finalize_tool_use_chunks(index: usize, state: StreamedToolUseState) -> Vec<StreamChunk> {
    let StreamedToolUseState {
        id,
        name,
        preview_start_input,
        accumulated_arguments: _,
        saw_partial_json,
    } = state.clone();
    let mut chunks = Vec::new();

    if !saw_partial_json && let Some(arguments_delta) = preview_start_input {
        chunks.push(StreamChunk {
            text: None,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            usage: None,
            sequence_number: None,
            tool_call_delta: Some(ToolCallDelta {
                index,
                id: id.clone(),
                name: name.clone(),
                arguments_delta: Some(arguments_delta),
                arguments: None,
            }),
            is_finished: false,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
        });
    }

    if let Some(arguments) = state.finalized_arguments() {
        chunks.push(StreamChunk {
            text: None,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            usage: None,
            sequence_number: None,
            tool_call_delta: Some(ToolCallDelta {
                index,
                id,
                name,
                arguments_delta: None,
                arguments: Some(arguments),
            }),
            is_finished: false,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
        });
    }

    chunks
}

async fn send_stream_chunk(tx: &Sender<StreamChunk>, chunk: StreamChunk) -> bool {
    tx.send(chunk).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_initial_tool_arguments_delta_prefixes_first_partial_json() {
        let mut start_input = Some("{\"a\":".to_string());
        let mut saw_partial_json = false;

        let merged = merge_initial_tool_arguments_delta(
            &mut start_input,
            &mut saw_partial_json,
            "1}".to_string(),
        );

        assert_eq!(merged, "{\"a\":1}");
        assert!(saw_partial_json);
        assert!(start_input.is_none());
    }

    #[test]
    fn test_merge_initial_tool_arguments_delta_does_not_repeat_prefix() {
        let mut start_input = Some("{\"a\":".to_string());
        let mut saw_partial_json = false;

        let first = merge_initial_tool_arguments_delta(
            &mut start_input,
            &mut saw_partial_json,
            "1".to_string(),
        );
        let second = merge_initial_tool_arguments_delta(
            &mut start_input,
            &mut saw_partial_json,
            "}".to_string(),
        );

        assert_eq!(first, "{\"a\":1");
        assert_eq!(second, "}");
    }

    #[test]
    fn test_streamed_tool_use_state_finalizes_start_input_without_partial_json() {
        let state = StreamedToolUseState::new(
            Some("tool-1".to_string()),
            Some("bash".to_string()),
            Some("{\"command\":\"pwd\"}".to_string()),
        );

        let chunks = finalize_tool_use_chunks(0, state);
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks[0]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments_delta.as_deref()),
            Some("{\"command\":\"pwd\"}")
        );
        assert_eq!(
            chunks[1]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments.as_deref()),
            Some("{\"command\":\"pwd\"}")
        );
    }

    #[test]
    fn test_streamed_tool_use_state_finalizes_accumulated_partial_json() {
        let mut state =
            StreamedToolUseState::new(Some("tool-2".to_string()), Some("bash".to_string()), None);

        let first_preview =
            state.merge_partial_json_for_preview("{\"command\":\"echo ".to_string());
        let second_preview = state.merge_partial_json_for_preview("hello\"}".to_string());
        let chunks = finalize_tool_use_chunks(0, state);

        assert_eq!(first_preview, "{\"command\":\"echo ");
        assert_eq!(second_preview, "hello\"}");
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments.as_deref()),
            Some("{\"command\":\"echo hello\"}")
        );
    }

    #[test]
    fn test_streamed_tool_use_state_ignores_start_input_when_partial_json_arrives() {
        let mut state = StreamedToolUseState::new(
            Some("tool-3".to_string()),
            Some("bash".to_string()),
            Some("{\"command\":\"pwd\"}".to_string()),
        );

        let preview = state.merge_partial_json_for_preview("{\"command\":\"pwd\"}".to_string());
        let chunks = finalize_tool_use_chunks(0, state);

        assert_eq!(preview, "{\"command\":\"pwd\"}{\"command\":\"pwd\"}");
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments.as_deref()),
            Some("{\"command\":\"pwd\"}")
        );
    }
}
