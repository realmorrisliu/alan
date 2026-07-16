use anyhow::{Context, Result};
use futures::StreamExt;
use serde::Deserialize;
use tracing::debug;

use super::{Content, PromptFeedback, UsageMetadata, is_blocking_finish_reason};
use crate::{SseEventParser, StreamChunk as UnifiedStreamChunk, TokenUsage, ToolCallDelta};

/// Stream chunk from Gemini streaming API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunk {
    /// Generated candidates.
    #[serde(default)]
    pub candidates: Vec<StreamCandidate>,
    /// Usage metadata, which is normally present only in the final chunk.
    pub usage_metadata: Option<UsageMetadata>,
    /// Prompt feedback, such as a blocked prompt.
    pub prompt_feedback: Option<PromptFeedback>,
}

/// A candidate in a Gemini streaming response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCandidate {
    /// Content of the response.
    pub content: Option<Content>,
    /// Why generation stopped, when this is the final candidate chunk.
    pub finish_reason: Option<String>,
    /// Index of this candidate.
    pub index: Option<i32>,
}

pub(super) async fn consume_response_stream(
    response: reqwest::Response,
    tx: tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut parser = SseEventParser::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("Failed to read stream chunk")?;
        for data in parser.push(&chunk) {
            if forward_event(&data, &tx).await {
                return Ok(());
            }
        }
    }

    for data in parser.finish() {
        if forward_event(&data, &tx).await {
            return Ok(());
        }
    }

    Ok(())
}

async fn forward_event(data: &str, tx: &tokio::sync::mpsc::Sender<StreamChunk>) -> bool {
    if data == "[DONE]" {
        debug!("Stream completed");
        return true;
    }

    match serde_json::from_str::<StreamChunk>(data) {
        Ok(stream_chunk) => {
            if tx.send(stream_chunk).await.is_err() {
                debug!("Receiver dropped, stopping stream");
                return true;
            }
        }
        Err(error) => {
            debug!(?error, data, "Failed to parse stream chunk");
        }
    }

    false
}

pub(super) async fn project_chunks(
    mut gemini_rx: tokio::sync::mpsc::Receiver<StreamChunk>,
    tx: tokio::sync::mpsc::Sender<UnifiedStreamChunk>,
) {
    let mut latest_usage: Option<TokenUsage> = None;
    let mut emitted_final = false;
    let mut emitted_payload = false;
    let mut selected_candidate_index: Option<i32> = None;
    let mut next_tool_call_index: usize = 0;

    while let Some(gemini_chunk) = gemini_rx.recv().await {
        if let Some(usage) = gemini_chunk.usage_metadata {
            latest_usage = Some(TokenUsage {
                prompt_tokens: usage.prompt_token_count.unwrap_or(0),
                cached_prompt_tokens: None,
                completion_tokens: usage.candidates_token_count.unwrap_or(0),
                total_tokens: usage.total_token_count.unwrap_or(0),
                reasoning_tokens: None,
            });
        }

        if let Some(feedback) = gemini_chunk.prompt_feedback
            && let Some(block_reason) = feedback.block_reason
        {
            let _ = tx
                .send(UnifiedStreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: latest_usage,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some(format!(
                        "stream_error:prompt_blocked:{}",
                        block_reason.to_ascii_lowercase()
                    )),
                    provider_response_id: None,
                    provider_response_status: None,
                })
                .await;
            emitted_final = true;
            break;
        }

        selected_candidate_index = select_stream_candidate_index(
            selected_candidate_index,
            emitted_payload,
            &gemini_chunk.candidates,
        );

        for (candidate_position, candidate) in gemini_chunk.candidates.into_iter().enumerate() {
            if !should_consume_stream_candidate(
                selected_candidate_index,
                candidate_position,
                candidate.index,
            ) {
                continue;
            }

            let finish_reason = candidate.finish_reason;
            if let Some(content) = candidate.content {
                for part in content.parts {
                    if let Some(text) = part.text {
                        emitted_payload = true;
                        if tx
                            .send(UnifiedStreamChunk {
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
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }

                    if let Some(function_call) = part.function_call {
                        let tool_call_index = next_tool_call_index;
                        next_tool_call_index = next_tool_call_index.saturating_add(1);
                        emitted_payload = true;
                        if tx
                            .send(UnifiedStreamChunk {
                                text: None,
                                thinking: None,
                                thinking_signature: None,
                                redacted_thinking: None,
                                usage: None,
                                sequence_number: None,
                                tool_call_delta: Some(ToolCallDelta {
                                    index: tool_call_index,
                                    id: None,
                                    name: Some(function_call.name),
                                    arguments_delta: Some(function_call.args.to_string()),
                                    arguments: None,
                                }),
                                is_finished: false,
                                finish_reason: None,
                                provider_response_id: None,
                                provider_response_status: None,
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            if let Some(finish_reason) = finish_reason {
                emitted_final = true;
                let _ = tx
                    .send(UnifiedStreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: latest_usage,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some(normalize_stream_finish_reason(finish_reason)),
                        provider_response_id: None,
                        provider_response_status: None,
                    })
                    .await;
            }
        }
    }

    // Empty streams remain silent so the runtime can fall back to non-streaming generation.
    if !emitted_final && emitted_payload {
        let _ = tx
            .send(UnifiedStreamChunk {
                text: None,
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: latest_usage,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some("stream_closed".to_string()),
                provider_response_id: None,
                provider_response_status: None,
            })
            .await;
    }
}

fn select_stream_candidate_index(
    selected_candidate_index: Option<i32>,
    emitted_payload: bool,
    candidates: &[StreamCandidate],
) -> Option<i32> {
    if candidates.is_empty() {
        return selected_candidate_index;
    }

    let has_index_zero = candidates
        .iter()
        .any(|candidate| candidate.index == Some(0));
    match selected_candidate_index {
        Some(0) => Some(0),
        Some(_current) if has_index_zero && !emitted_payload => Some(0),
        Some(current) => Some(current),
        None if has_index_zero => Some(0),
        None => candidates.first().and_then(|candidate| candidate.index),
    }
}

fn should_consume_stream_candidate(
    selected_candidate_index: Option<i32>,
    candidate_position: usize,
    candidate_index: Option<i32>,
) -> bool {
    match selected_candidate_index {
        Some(index) => candidate_index == Some(index),
        None => candidate_position == 0,
    }
}

fn normalize_stream_finish_reason(finish_reason: String) -> String {
    if is_blocking_finish_reason(&finish_reason) {
        format!("stream_error:{}", finish_reason.to_ascii_lowercase())
    } else {
        finish_reason
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_stream_candidate_index_prefers_zero_then_falls_back() {
        let non_zero_candidates = vec![
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(3),
            },
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(4),
            },
        ];
        assert_eq!(
            select_stream_candidate_index(None, false, &non_zero_candidates),
            Some(3)
        );

        let with_zero = vec![
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(2),
            },
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(0),
            },
        ];
        assert_eq!(
            select_stream_candidate_index(None, false, &with_zero),
            Some(0)
        );
        assert_eq!(
            select_stream_candidate_index(Some(3), false, &with_zero),
            Some(0)
        );
        assert_eq!(
            select_stream_candidate_index(Some(3), true, &with_zero),
            Some(3)
        );
    }

    #[test]
    fn selected_stream_candidate_uses_index_or_position() {
        assert!(should_consume_stream_candidate(Some(0), 1, Some(0)));
        assert!(!should_consume_stream_candidate(Some(0), 0, Some(1)));
        assert!(should_consume_stream_candidate(None, 0, None));
        assert!(!should_consume_stream_candidate(None, 1, None));
    }

    #[test]
    fn blocking_finish_reasons_map_to_stream_errors() {
        assert_eq!(
            normalize_stream_finish_reason("SAFETY".to_string()),
            "stream_error:safety"
        );
        assert_eq!(
            normalize_stream_finish_reason("RECITATION".to_string()),
            "stream_error:recitation"
        );
        assert_eq!(normalize_stream_finish_reason("STOP".to_string()), "STOP");
    }

    #[test]
    fn stream_chunk_deserializes() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello"}]
                },
                "finishReason": null,
                "index": 0
            }],
            "usageMetadata": null
        }"#;

        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.candidates.len(), 1);
        assert!(chunk.usage_metadata.is_none());
    }
}
