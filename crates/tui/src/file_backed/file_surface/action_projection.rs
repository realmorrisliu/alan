//! Typed Action result projection; content never supplies grouping eligibility.
use super::*;

pub(in crate::file_backed) fn action_snapshot_to_history_cell(
    snapshot: &ActionSnapshot,
) -> Option<HistoryCell> {
    let status = match snapshot.status.trim() {
        "completed" => ToolStatus::Complete,
        "failed" => ToolStatus::Failed,
        "rejected" => ToolStatus::Rejected,
        "cancelled" => ToolStatus::Cancelled,
        _ => return None,
    };
    let body = if !snapshot.output.trim().is_empty() {
        Some(snapshot.output.clone())
    } else if !snapshot.result.trim().is_empty() {
        Some(snapshot.result.clone())
    } else {
        None
    };

    let metadata = serde_json::from_str::<Value>(&snapshot.result).ok();
    let title = metadata
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| action_title(snapshot));
    let preview = metadata
        .as_ref()
        .and_then(|v| v.get("result_preview"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let presentation = metadata
        .as_ref()
        .and_then(|v| v.get("presentation"))
        .and_then(|v| serde_json::from_value::<ToolResultPresentation>(v.clone()).ok())
        .or_else(|| {
            preview
                .as_ref()
                .filter(|text| !text.trim().is_empty())
                .cloned()
                .or(body)
                .map(|body| ToolResultPresentation::PlainText { body })
        });
    Some(HistoryCell::Tool {
        action: Some(crate::history::ActionHistory {
            owner: String::new(),
            id: snapshot.id.clone(),
            read_only: (status == ToolStatus::Complete)
                .then(|| {
                    metadata
                        .as_ref()
                        .and_then(|v| v.get("read_only_context"))
                        .and_then(|v| {
                            serde_json::from_value::<alan_agent_protocol::ActionReadOnlyContext>(
                                v.clone(),
                            )
                            .ok()
                        })
                        .filter(|context| context.is_valid())
                })
                .flatten(),
            frozen_rows: None,
        }),
        title,
        status,
        preview,
        presentation,
    })
}

pub(super) fn action_title(snapshot: &ActionSnapshot) -> String {
    let trimmed = snapshot.name.trim();
    if !trimmed.is_empty() {
        trimmed.to_string()
    } else {
        format!("tool {}", snapshot.id)
    }
}
