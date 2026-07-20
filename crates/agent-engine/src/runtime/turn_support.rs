use alan_agent_protocol::Event;
use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;

use super::transition::{
    HostMountTerminalResult, HostMountTerminalStatus, NamespaceAgentFiles,
    NamespaceHostMountRequests,
};
use crate::agent_machine::{AgentMachine, NormalizedToolCall, PendingHostMountRequest};

pub(super) fn preserve_approved_host_mount(
    _pending: &PendingHostMountRequest,
    terminal: &HostMountTerminalResult,
) -> Result<()> {
    if terminal.status != HostMountTerminalStatus::Approved {
        return Ok(());
    }
    terminal
        .grant_reference
        .as_ref()
        .context("approved Host Mount request has no grant reference")?;
    Ok(())
}

pub(super) async fn reset_turn_after_cancelling_host_mounts(
    machine: &mut AgentMachine,
    host_mount_requests: &NamespaceHostMountRequests,
) -> Result<()> {
    let pending_host_mounts = machine
        .pending_request_ids()
        .into_iter()
        .filter_map(|request_id| machine.pending_host_mount(&request_id))
        .collect::<Vec<_>>();
    for pending in pending_host_mounts {
        let terminal = host_mount_requests.cancel(&pending.request_id).await?;
        preserve_approved_host_mount(&pending, &terminal)?;
    }
    machine.reset_turn();
    Ok(())
}

pub(super) async fn cancel_current_task<E, F>(
    machine: &mut AgentMachine,
    agent_files: &NamespaceAgentFiles,
    host_mount_requests: &NamespaceHostMountRequests,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    warn!("Cancelling current task");
    // Clear turn-scoped pending state, but preserve machine history so the user can
    // continue the same conversation after an interrupt/cancel.
    reset_turn_after_cancelling_host_mounts(machine, host_mount_requests).await?;
    machine.clear_plan_snapshot();
    machine.clear_active_task();
    super::ui_surfaces::turn_completed(agent_files, true).await?;
    emit(Event::TurnCompleted {
        summary: Some("Task cancelled by user".to_string()),
    })
    .await;
    Ok(())
}

pub(super) async fn emit_task_completed_success<E, F>(
    agent_files: &NamespaceAgentFiles,
    emit: &mut E,
    summary: impl Into<String>,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let summary = summary.into();
    super::ui_surfaces::turn_completed(agent_files, false).await?;
    emit(Event::TurnCompleted {
        summary: Some(summary),
    })
    .await;
    Ok(())
}

pub(super) fn normalize_tool_calls(
    tool_calls: Vec<crate::llm::ToolCall>,
) -> Vec<NormalizedToolCall> {
    let fallback_prefix = format!("tool_call_{}", Uuid::new_v4().simple());

    tool_calls
        .into_iter()
        .enumerate()
        .map(|(index, tc)| {
            let id = tc
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{fallback_prefix}_{index}"));

            NormalizedToolCall {
                id,
                name: tc.name,
                arguments: tc.arguments,
            }
        })
        .collect()
}

pub(super) fn tool_result_preview(value: &serde_json::Value) -> Option<String> {
    const MAX_PREVIEW_CHARS: usize = 160;

    let mut preview = match value {
        serde_json::Value::Null => return None,
        serde_json::Value::String(text) => text.trim().to_string(),
        serde_json::Value::Object(map) => {
            if let Some(error) = map.get("error").and_then(|v| v.as_str()) {
                format!("error: {}", error.trim())
            } else if let Some(status) = map.get("status").and_then(|v| v.as_str()) {
                status.trim().to_string()
            } else {
                value.to_string()
            }
        }
        _ => value.to_string(),
    };

    if preview.is_empty() {
        return None;
    }

    if preview.chars().count() > MAX_PREVIEW_CHARS {
        preview = preview.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        preview.push_str("...");
    }

    Some(preview)
}

pub(super) fn split_text_for_typing(text: &str) -> Vec<String> {
    const TARGET_CHUNK_CHARS: usize = 32;

    if text.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for ch in text.chars() {
        current.push(ch);
        current_len += 1;

        let boundary = ch.is_whitespace() || [',', '.', '!', '?', ';', ':'].contains(&ch);
        if current_len >= TARGET_CHUNK_CHARS && boundary {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

pub(super) async fn emit_streaming_chunks<E, F>(emit: &mut E, content: &str)
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let chunks = split_text_for_typing(content);
    for chunk in &chunks {
        emit(Event::TextDelta {
            chunk: chunk.clone(),
            is_final: false,
        })
        .await;
    }
    emit(Event::TextDelta {
        chunk: String::new(),
        is_final: true,
    })
    .await;
}

pub(super) async fn emit_thinking_chunks<E, F>(emit: &mut E, thinking: &str)
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let chunks = split_text_for_typing(thinking);
    for chunk in &chunks {
        emit(Event::ThinkingDelta {
            chunk: chunk.clone(),
            is_final: false,
        })
        .await;
    }
    emit(Event::ThinkingDelta {
        chunk: String::new(),
        is_final: true,
    })
    .await;
}

pub(super) async fn check_turn_cancelled<E, F>(
    machine: &mut AgentMachine,
    agent_files: &NamespaceAgentFiles,
    host_mount_requests: &NamespaceHostMountRequests,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if !cancel.is_cancelled() {
        return Ok(false);
    }
    if !machine.is_turn_active() && !machine.has_pending_interaction() {
        emit(Event::Error {
            message: "No active turn to cancel.".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(false);
    }
    cancel_current_task(machine, agent_files, host_mount_requests, emit).await?;
    Ok(true)
}
