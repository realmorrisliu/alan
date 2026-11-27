//! SSE consumption and projection for Vertex AI `:streamGenerateContent?alt=sse`.

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Map, Value};
use tracing::debug;

use super::is_blocking_finish_reason;
use super::signatures::TurnSignatures;
use super::wire::{Content, PartialArg, PromptFeedback, UsageMetadata};
use crate::{SseEventParser, StreamChunk as UnifiedStreamChunk, TokenUsage, ToolCallDelta};

#[cfg(test)]
#[path = "cancellation_tests.rs"]
mod cancellation_tests;

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
    /// Provider response identifier.
    #[serde(default)]
    pub response_id: Option<String>,
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

/// Accumulator for a single Gemini `streamFunctionCallArguments` call.
struct ActivePartialCall {
    index: usize,
    id: Option<String>,
    args: Value,
}

impl ActivePartialCall {
    fn new(index: usize, id: Option<String>, initial_args: Value) -> Self {
        let args = if initial_args.is_object() {
            initial_args
        } else {
            Value::Object(Map::new())
        };
        Self { index, id, args }
    }

    /// Apply a batch of `partialArgs` and return a human-readable progress delta.
    fn apply_partial_args(&mut self, partial_args: &[PartialArg]) -> Option<String> {
        let mut delta = String::new();
        for arg in partial_args {
            apply_partial_arg(&mut self.args, arg);
            if let Some(s) = &arg.string_value {
                delta.push_str(s);
            } else if let Some(n) = arg.number_value {
                delta.push_str(&n.to_string());
            } else if let Some(b) = arg.bool_value {
                delta.push_str(if b { "true" } else { "false" });
            } else if arg.null_value.is_some() {
                delta.push_str("null");
            }
        }
        (!delta.is_empty()).then_some(delta)
    }

    fn finalize_delta(self) -> ToolCallDelta {
        ToolCallDelta {
            index: self.index,
            id: self.id,
            name: None,
            arguments_delta: None,
            arguments: Some(self.args.to_string()),
        }
    }
}

fn empty_chunk() -> UnifiedStreamChunk {
    UnifiedStreamChunk {
        text: None,
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
    }
}

pub(super) async fn consume_response_stream(
    response: reqwest::Response,
    tx: tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut parser = SseEventParser::new();

    loop {
        let chunk_result = tokio::select! {
            biased;
            _ = tx.closed() => return Ok(()),
            chunk = stream.next() => match chunk {
                Some(chunk) => chunk,
                None => break,
            },
        };
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
    let mut latest_response_id: Option<String> = None;
    let mut signatures = TurnSignatures::default();
    let mut active_partial_call: Option<ActivePartialCall> = None;
    let mut emitted_final = false;
    let mut emitted_payload = false;
    let mut selected_candidate_index: Option<i32> = None;
    let mut next_tool_call_index: usize = 0;

    loop {
        let gemini_chunk = tokio::select! {
            biased;
            _ = tx.closed() => return,
            chunk = gemini_rx.recv() => match chunk {
                Some(chunk) => chunk,
                None => break,
            },
        };

        if let Some(usage) = gemini_chunk.usage_metadata {
            latest_usage = Some(usage.to_token_usage());
        }
        if let Some(response_id) = gemini_chunk.response_id.filter(|id| !id.trim().is_empty()) {
            latest_response_id = Some(response_id);
        }

        if let Some(feedback) = gemini_chunk.prompt_feedback
            && let Some(block_reason) = feedback.block_reason
        {
            let _ = tx
                .send(UnifiedStreamChunk {
                    usage: latest_usage,
                    is_finished: true,
                    finish_reason: Some(format!(
                        "stream_error:prompt_blocked:{}",
                        block_reason.to_ascii_lowercase()
                    )),
                    provider_response_id: latest_response_id.clone(),
                    ..empty_chunk()
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
                    if part.is_thought() {
                        signatures.record_text(part.thought_signature.as_deref());
                        if let Some(thinking) = part.text.filter(|t| !t.is_empty()) {
                            emitted_payload = true;
                            if tx
                                .send(UnifiedStreamChunk {
                                    thinking: Some(thinking),
                                    ..empty_chunk()
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        continue;
                    }

                    if let Some(text) = part.text {
                        signatures.record_text(part.thought_signature.as_deref());
                        if !flush_partial_call(&mut active_partial_call, &tx).await {
                            return;
                        }
                        if !text.is_empty() {
                            emitted_payload = true;
                            if tx
                                .send(UnifiedStreamChunk {
                                    text: Some(text),
                                    ..empty_chunk()
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        continue;
                    }

                    if let Some(function_call) = part.function_call {
                        let will_continue = function_call.will_continue == Some(true);
                        let has_partial = function_call.partial_args.is_some();

                        if function_call.name.is_empty() {
                            if let Some(active) = active_partial_call.as_mut() {
                                signatures
                                    .record_call(active.index, part.thought_signature.as_deref());
                                if let Some(partial_args) = &function_call.partial_args
                                    && let Some(delta) = active.apply_partial_args(partial_args)
                                {
                                    emitted_payload = true;
                                    if tx
                                        .send(UnifiedStreamChunk {
                                            tool_call_delta: Some(ToolCallDelta {
                                                index: active.index,
                                                id: None,
                                                name: None,
                                                arguments_delta: Some(delta),
                                                arguments: None,
                                            }),
                                            ..empty_chunk()
                                        })
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                if !will_continue
                                    && !flush_partial_call(&mut active_partial_call, &tx).await
                                {
                                    return;
                                }
                            }
                            continue;
                        }

                        if !flush_partial_call(&mut active_partial_call, &tx).await {
                            return;
                        }

                        let tool_call_index = next_tool_call_index;
                        next_tool_call_index = next_tool_call_index.saturating_add(1);
                        signatures.record_call(tool_call_index, part.thought_signature.as_deref());
                        emitted_payload = true;

                        if will_continue || has_partial {
                            let mut active = ActivePartialCall::new(
                                tool_call_index,
                                function_call.id.clone(),
                                function_call.args,
                            );
                            let arguments_delta = function_call
                                .partial_args
                                .as_deref()
                                .and_then(|args| active.apply_partial_args(args));
                            if tx
                                .send(UnifiedStreamChunk {
                                    tool_call_delta: Some(ToolCallDelta {
                                        index: tool_call_index,
                                        id: function_call.id,
                                        name: Some(function_call.name),
                                        arguments_delta,
                                        arguments: None,
                                    }),
                                    ..empty_chunk()
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                            if will_continue {
                                active_partial_call = Some(active);
                            } else if tx
                                .send(UnifiedStreamChunk {
                                    tool_call_delta: Some(active.finalize_delta()),
                                    ..empty_chunk()
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        } else {
                            let args_json = if function_call.args.is_null() {
                                "{}".to_string()
                            } else {
                                function_call.args.to_string()
                            };
                            if tx
                                .send(UnifiedStreamChunk {
                                    tool_call_delta: Some(ToolCallDelta {
                                        index: tool_call_index,
                                        id: function_call.id,
                                        name: Some(function_call.name),
                                        arguments_delta: Some(args_json.clone()),
                                        arguments: Some(args_json),
                                    }),
                                    ..empty_chunk()
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        continue;
                    }

                    if part.thought_signature.is_some() {
                        signatures.record_text(part.thought_signature.as_deref());
                    }
                }
            }

            if let Some(finish_reason) = finish_reason {
                if !flush_partial_call(&mut active_partial_call, &tx).await {
                    return;
                }
                emitted_final = true;
                let _ = tx
                    .send(UnifiedStreamChunk {
                        thinking_signature: signatures.encode(),
                        usage: latest_usage,
                        is_finished: true,
                        finish_reason: Some(normalize_stream_finish_reason(finish_reason)),
                        provider_response_id: latest_response_id.clone(),
                        ..empty_chunk()
                    })
                    .await;
            }
        }
    }

    // Empty streams remain silent so the runtime can fall back to non-streaming generation.
    if !emitted_final && emitted_payload {
        let _ = flush_partial_call(&mut active_partial_call, &tx).await;
        let _ = tx
            .send(UnifiedStreamChunk {
                thinking_signature: signatures.encode(),
                usage: latest_usage,
                is_finished: true,
                finish_reason: Some("stream_closed".to_string()),
                provider_response_id: latest_response_id,
                ..empty_chunk()
            })
            .await;
    }
}

async fn flush_partial_call(
    active_partial_call: &mut Option<ActivePartialCall>,
    tx: &tokio::sync::mpsc::Sender<UnifiedStreamChunk>,
) -> bool {
    let Some(active) = active_partial_call.take() else {
        return true;
    };
    tx.send(UnifiedStreamChunk {
        tool_call_delta: Some(active.finalize_delta()),
        ..empty_chunk()
    })
    .await
    .is_ok()
}

#[derive(Debug, PartialEq, Eq)]
enum PathSegment {
    Key(String),
    Index(usize),
    ObjectMarker,
    ArrayMarker,
}

fn push_unquoted_key_or_marker(segments: &mut Vec<PathSegment>, raw: &str) {
    if raw.is_empty() {
        return;
    }
    if raw == "{}" {
        segments.push(PathSegment::ObjectMarker);
    } else {
        segments.push(PathSegment::Key(raw.to_string()));
    }
}

fn parse_json_path(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    let rest = path.trim().strip_prefix('$').unwrap_or(path.trim());
    let bytes = rest.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                push_unquoted_key_or_marker(&mut segments, &rest[start..i]);
            }
            b'[' => {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'\'' || bytes[i] == b'"') {
                    let quote = bytes[i];
                    i += 1;
                    let start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    segments.push(PathSegment::Key(rest[start..i].to_string()));
                    if i < bytes.len() {
                        i += 1; // closing quote
                    }
                    if i < bytes.len() && bytes[i] == b']' {
                        i += 1;
                    }
                } else {
                    let start = i;
                    while i < bytes.len() && bytes[i] != b']' {
                        i += 1;
                    }
                    let index_text = rest[start..i].trim();
                    if index_text.is_empty() {
                        segments.push(PathSegment::ArrayMarker);
                    } else if let Ok(index) = index_text.parse::<usize>() {
                        segments.push(PathSegment::Index(index));
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                }
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                push_unquoted_key_or_marker(&mut segments, &rest[start..i]);
            }
        }
    }

    segments
}

fn apply_partial_arg(root: &mut Value, arg: &PartialArg) {
    let segments = parse_json_path(&arg.json_path);
    if segments.is_empty() {
        return;
    }

    let has_value = arg.string_value.is_some()
        || arg.number_value.is_some()
        || arg.bool_value.is_some()
        || arg.null_value.is_some();

    let mut cursor = root;
    for (pos, segment) in segments.iter().enumerate() {
        let next = segments.get(pos + 1);
        match segment {
            PathSegment::ObjectMarker => {
                if !cursor.is_object() {
                    *cursor = Value::Object(Map::new());
                }
            }
            PathSegment::ArrayMarker => {
                if !cursor.is_array() {
                    *cursor = Value::Array(Vec::new());
                }
            }
            PathSegment::Key(key) => {
                if !cursor.is_object() {
                    *cursor = Value::Object(Map::new());
                }
                if next.is_none() && !has_value {
                    return;
                }
                let map = cursor.as_object_mut().expect("object");
                cursor = map.entry(key.clone()).or_insert_with(|| match next {
                    Some(PathSegment::Index(_) | PathSegment::ArrayMarker) => {
                        Value::Array(Vec::new())
                    }
                    Some(PathSegment::Key(_) | PathSegment::ObjectMarker) => {
                        Value::Object(Map::new())
                    }
                    None => Value::Null,
                });
            }
            PathSegment::Index(index) => {
                if !cursor.is_array() {
                    *cursor = Value::Array(Vec::new());
                }
                let arr = cursor.as_array_mut().expect("array");
                if arr.len() <= *index {
                    arr.resize(*index + 1, Value::Null);
                }
                cursor = &mut arr[*index];
                if cursor.is_null() {
                    match next {
                        Some(PathSegment::Key(_) | PathSegment::ObjectMarker) => {
                            *cursor = Value::Object(Map::new());
                        }
                        Some(PathSegment::Index(_) | PathSegment::ArrayMarker) => {
                            *cursor = Value::Array(Vec::new());
                        }
                        None => {}
                    }
                }
                if next.is_none() && !has_value {
                    return;
                }
            }
        }
    }

    if let Some(s) = &arg.string_value {
        match cursor {
            Value::String(existing) => existing.push_str(s),
            _ => *cursor = Value::String(s.clone()),
        }
    } else if let Some(n) = arg.number_value {
        if let Some(num) = serde_json::Number::from_f64(n) {
            *cursor = Value::Number(num);
        }
    } else if let Some(b) = arg.bool_value {
        *cursor = Value::Bool(b);
    } else if arg.null_value.is_some() {
        *cursor = Value::Null;
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
    use serde_json::json;

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
        for (raw, expected) in [
            ("SAFETY", "stream_error:safety"),
            ("RECITATION", "stream_error:recitation"),
            (
                "MALFORMED_FUNCTION_CALL",
                "stream_error:malformed_function_call",
            ),
            ("UNEXPECTED_TOOL_CALL", "stream_error:unexpected_tool_call"),
            ("BLOCKLIST", "stream_error:blocklist"),
            ("PROHIBITED_CONTENT", "stream_error:prohibited_content"),
            ("SPII", "stream_error:spii"),
            ("IMAGE_SAFETY", "stream_error:image_safety"),
            ("OTHER", "stream_error:other"),
        ] {
            assert!(is_blocking_finish_reason(raw), "{raw} should be blocking");
            let normalized = normalize_stream_finish_reason(raw.to_string());
            assert_eq!(normalized, expected, "{raw}");
            assert_eq!(
                crate::safe_finish_reason(&normalized),
                expected,
                "{raw} must survive safe_finish_reason without collapsing to unknown"
            );
        }
        assert!(!is_blocking_finish_reason("STOP"));
        assert!(!is_blocking_finish_reason("MAX_TOKENS"));
        assert_eq!(normalize_stream_finish_reason("STOP".to_string()), "STOP");
        assert_eq!(
            normalize_stream_finish_reason("MAX_TOKENS".to_string()),
            "MAX_TOKENS"
        );
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

    #[test]
    fn partial_args_assemble_nested_and_chunked_strings() {
        let mut active = ActivePartialCall::new(0, Some("call_1".into()), Value::Null);
        active.apply_partial_args(&[
            PartialArg {
                json_path: "$.path".into(),
                string_value: Some("src/lib.rs".into()),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.content".into(),
                string_value: Some("pub fn ".into()),
                will_continue: Some(true),
                ..Default::default()
            },
        ]);
        active.apply_partial_args(&[
            PartialArg {
                json_path: "$.content".into(),
                string_value: Some("hi() {}".into()),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.flags.overwrite".into(),
                bool_value: Some(true),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.lines[0]".into(),
                number_value: Some(42.0),
                ..Default::default()
            },
        ]);

        let finalized = active.finalize_delta();
        let parsed: Value = serde_json::from_str(finalized.arguments.as_deref().unwrap()).unwrap();
        assert_eq!(
            parsed,
            json!({
                "path": "src/lib.rs",
                "content": "pub fn hi() {}",
                "flags": { "overwrite": true },
                "lines": [42.0]
            })
        );
    }

    #[test]
    fn partial_args_ignore_empty_object_and_valueless_markers() {
        let mut active = ActivePartialCall::new(0, Some("call_mounts".into()), Value::Null);
        active.apply_partial_args(&[
            PartialArg {
                json_path: "$.host_mounts[0]{}".into(),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.host_mounts[0].grant".into(),
                string_value: Some("repo-wt".into()),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.host_mounts[0].target".into(),
                string_value: Some("/mnt/grants/repo-wt".into()),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.host_mounts[0].access".into(),
                string_value: Some("read_only".into()),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.host_mounts[0].{}".into(),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.empty_obj.{}".into(),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.empty_arr[]".into(),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.unused_termination_marker".into(),
                will_continue: Some(false),
                ..Default::default()
            },
            PartialArg {
                json_path: "$.explicit_null".into(),
                null_value: Some(Value::String("NULL_VALUE".into())),
                ..Default::default()
            },
        ]);

        let finalized = active.finalize_delta();
        let parsed: Value = serde_json::from_str(finalized.arguments.as_deref().unwrap()).unwrap();
        assert_eq!(
            parsed,
            json!({
                "host_mounts": [{
                    "grant": "repo-wt",
                    "target": "/mnt/grants/repo-wt",
                    "access": "read_only"
                }],
                "empty_obj": {},
                "empty_arr": [],
                "explicit_null": null
            })
        );
    }
}
