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
    if snapshot.name == "cd"
        && status == ToolStatus::Complete
        && metadata
            .as_ref()
            .and_then(|v| v.get("title"))
            .and_then(Value::as_str)
            == Some("Select Process directory")
    {
        return None;
    }
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

pub(in crate::file_backed) fn action_title(snapshot: &ActionSnapshot) -> String {
    let trimmed = snapshot.name.trim();
    if !trimmed.is_empty() {
        trimmed.to_string()
    } else {
        format!("tool {}", snapshot.id)
    }
}

#[derive(Debug, Clone)]
pub(in crate::file_backed) struct RequestSnapshot {
    pub(in crate::file_backed) id: String,
    pub(in crate::file_backed) kind: String,
    pub(in crate::file_backed) prompt: String,
    pub(in crate::file_backed) options: String,
    pub(in crate::file_backed) status: String,
}

#[derive(Debug, Clone)]
pub(in crate::file_backed) struct ActionSnapshot {
    pub(in crate::file_backed) id: String,
    pub(in crate::file_backed) name: String,
    pub(in crate::file_backed) status: String,
    pub(in crate::file_backed) output: String,
    pub(in crate::file_backed) result: String,
}

#[derive(Deserialize)]
pub(in crate::file_backed) struct TapeRecordV1 {
    #[serde(skip)]
    pub(in crate::file_backed) end_offset: usize,
    #[allow(
        dead_code,
        reason = "version is part of the persisted tape schema even though deserialization validates it elsewhere"
    )]
    pub(in crate::file_backed) version: u16,
    pub(in crate::file_backed) kind: String,
    pub(in crate::file_backed) role: String,
    pub(in crate::file_backed) content: String,
    #[serde(default)]
    pub(in crate::file_backed) input_intent: Option<alan_agent_protocol::InputIntent>,
    #[serde(default)]
    pub(in crate::file_backed) submission_id: Option<String>,
    #[serde(default)]
    pub(in crate::file_backed) related_submission_ids: Vec<String>,
}

impl TapeRecordV1 {
    pub(in crate::file_backed) fn into_user_cell(self) -> HistoryCell {
        if self.input_intent == Some(alan_agent_protocol::InputIntent::Command) {
            HistoryCell::Command(self.content)
        } else {
            HistoryCell::User(self.content)
        }
    }

    pub(in crate::file_backed) fn belongs_to(&self, submission_id: &str) -> bool {
        self.submission_id.as_deref() == Some(submission_id)
            || self
                .related_submission_ids
                .iter()
                .any(|id| id == submission_id)
    }
}
