use std::path::Path;

use alan_agent_protocol::{AgentExecutableResult, AgentExecutableStatus};

use super::ObservedChildTerminalEvent;
use crate::runtime::delegated_child_run::{ChildRuntimePause, ChildRuntimeStatus, InheritedPause};
use crate::runtime::{ChildRunStatus, ChildRunTerminationRequest};

const MAX_OBSERVED_CHILD_WARNINGS: usize = 32;
const MAX_OBSERVED_CHILD_WARNING_CHARS: usize = 512;

pub(super) fn agent_result_matches_exit_code(
    exit_code: Option<i32>,
    result: &AgentExecutableResult,
) -> bool {
    matches!(
        (exit_code, result.status),
        (Some(0), AgentExecutableStatus::Completed)
            | (
                Some(1),
                AgentExecutableStatus::Paused | AgentExecutableStatus::Failed
            )
    )
}

pub(super) fn push_bounded_child_warning(warnings: &mut Vec<String>, warning: String) {
    while warnings.len() >= MAX_OBSERVED_CHILD_WARNINGS {
        warnings.remove(0);
    }
    warnings.push(truncate_child_text_with_suffix(
        &warning,
        MAX_OBSERVED_CHILD_WARNING_CHARS,
        "...",
    ));
}

fn truncate_child_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_len))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
}

pub(super) fn parse_child_structured_output(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .or_else(|| parse_last_json_fenced_block(trimmed))
}

pub(super) fn file_terminal_observation(
    output_text: String,
    warnings: Vec<String>,
    status: ChildRuntimeStatus,
    error_message: Option<String>,
    pause: Option<ChildRuntimePause>,
) -> ObservedChildTerminalEvent {
    ObservedChildTerminalEvent {
        structured_output: parse_child_structured_output(&output_text),
        output_text,
        turn_summary: None,
        warnings,
        error_message,
        pause,
        status,
    }
}

pub(super) fn invalid_agent_result_observation(
    exit_code: i32,
    output_text: String,
    warnings: Vec<String>,
) -> ObservedChildTerminalEvent {
    file_terminal_observation(
        output_text,
        warnings,
        ChildRuntimeStatus::Failed,
        Some(format!(
            "Child Agent Process exited with code {exit_code} without a valid terminal result"
        )),
        None,
    )
}

pub(super) fn timed_out_observation(
    message: &str,
    output_text: &str,
    warnings: &[String],
) -> ObservedChildTerminalEvent {
    ObservedChildTerminalEvent {
        output_text: output_text.to_string(),
        turn_summary: None,
        structured_output: parse_child_structured_output(output_text),
        warnings: warnings.to_vec(),
        error_message: Some(message.to_string()),
        pause: None,
        status: ChildRuntimeStatus::TimedOut,
    }
}

pub(super) fn terminated_observation(
    request: ChildRunTerminationRequest,
    output_text: &str,
    warnings: &[String],
) -> ObservedChildTerminalEvent {
    ObservedChildTerminalEvent {
        output_text: output_text.to_string(),
        turn_summary: None,
        structured_output: parse_child_structured_output(output_text),
        warnings: warnings.to_vec(),
        error_message: Some(format!(
            "Delegated Child Run terminated by {} with {:?} mode: {}",
            request.actor, request.mode, request.reason
        )),
        pause: None,
        status: ChildRuntimeStatus::Terminated,
    }
}

pub(super) fn child_run_status_for_runtime_status(status: ChildRuntimeStatus) -> ChildRunStatus {
    match status {
        ChildRuntimeStatus::Completed => ChildRunStatus::Completed,
        // A paused child is blocked, not failed: its decision may still be
        // resolved by whoever owns it.
        ChildRuntimeStatus::Paused => ChildRunStatus::Blocked,
        ChildRuntimeStatus::Cancelled => ChildRunStatus::Cancelled,
        ChildRuntimeStatus::TimedOut => ChildRunStatus::TimedOut,
        ChildRuntimeStatus::Terminated => ChildRunStatus::Terminated,
        ChildRuntimeStatus::Failed => ChildRunStatus::Failed,
    }
}

pub(super) async fn read_latest_assistant_text_from_rollout(
    rollout_path: Option<&Path>,
) -> Option<String> {
    let rollout_path = rollout_path?;
    let contents = tokio::fs::read_to_string(rollout_path).await.ok()?;
    extract_latest_assistant_text_from_rollout(contents.as_str())
}

fn extract_latest_assistant_text_from_rollout(contents: &str) -> Option<String> {
    let mut last_text = None;

    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(object) = value.as_object() else {
            continue;
        };
        if object.get("type").and_then(serde_json::Value::as_str) != Some("message") {
            continue;
        }
        if object.get("role").and_then(serde_json::Value::as_str) != Some("assistant") {
            continue;
        }

        let direct_content = object
            .get("content")
            .and_then(serde_json::Value::as_str)
            .and_then(non_empty_trimmed);
        if direct_content.is_some() {
            last_text = direct_content;
            continue;
        }

        let nested_parts = object
            .get("message")
            .and_then(|message| message.get("parts"))
            .and_then(serde_json::Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| {
                        if part.get("type").and_then(serde_json::Value::as_str) == Some("text") {
                            part.get("text")
                                .and_then(serde_json::Value::as_str)
                                .map(str::trim)
                                .filter(|text| !text.is_empty())
                                .map(ToOwned::to_owned)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|parts| !parts.is_empty())
            .map(|parts| parts.join("\n"));
        if nested_parts.is_some() {
            last_text = nested_parts;
        }
    }

    last_text
}

fn non_empty_trimmed(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_last_json_fenced_block(text: &str) -> Option<serde_json::Value> {
    let mut remainder = text;
    let mut last_match = None;

    while let Some(start_idx) = remainder.find("```") {
        let fence_remainder = &remainder[start_idx + 3..];
        let Some(newline_idx) = fence_remainder.find('\n') else {
            break;
        };
        let info_string = fence_remainder[..newline_idx].trim().to_ascii_lowercase();
        let content_start = start_idx + 3 + newline_idx + 1;
        let content_remainder = &remainder[content_start..];
        let Some(end_idx) = content_remainder.find("```") else {
            break;
        };
        if info_string.is_empty() || info_string == "json" {
            last_match = Some(content_remainder[..end_idx].trim().to_string());
        }
        remainder = &content_remainder[end_idx + 3..];
    }

    last_match.and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
}

/// The inherited identity a paused child reported in its decision details.
pub(super) fn inherited_pause_from(details: &serde_json::Value) -> InheritedPause {
    InheritedPause {
        ancestor_task_id: details["ancestor_task_id"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        dimension: details["dimension"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        checkpoint_id: details["checkpoint_id"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        rollout_path: details["rollout_path"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    }
}
