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
    let title = normalize_project_mount_paths(
        &metadata
            .as_ref()
            .and_then(|v| v.get("title"))
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| action_title(snapshot)),
    );
    let preview = metadata
        .as_ref()
        .and_then(|v| v.get("result_preview"))
        .and_then(Value::as_str)
        .map(normalize_project_mount_paths);
    let presentation = metadata
        .as_ref()
        .and_then(|v| v.get("presentation"))
        .and_then(|v| serde_json::from_value::<ToolResultPresentation>(v.clone()).ok())
        .or_else(|| {
            preview
                .as_ref()
                .filter(|text| !text.trim().is_empty())
                .cloned()
                .or_else(|| body.as_deref().map(normalize_project_mount_paths))
                .map(|body| ToolResultPresentation::PlainText { body })
        })
        .map(normalize_presentation_project_paths);
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

pub(in crate::file_backed) fn normalize_project_mount_paths(text: &str) -> String {
    const PREFIX: &str = "/mnt/project-request-";
    if !text.contains(PREFIX) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(rel) = text[cursor..].find(PREFIX) {
        let start = cursor + rel;
        out.push_str(&text[cursor..start]);
        let slug_start = start + PREFIX.len();
        let slug_len = text[slug_start..]
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
            .unwrap_or(text.len() - slug_start);
        if slug_len == 0 {
            out.push_str(PREFIX);
            cursor = slug_start;
            continue;
        }
        let after_slug = slug_start + slug_len;
        if text[after_slug..].starts_with('/') {
            // Strip `/mnt/project-request-<id>/` down to project-relative path.
            cursor = after_slug + 1;
        } else {
            out.push('.');
            cursor = after_slug;
        }
    }
    out.push_str(&text[cursor..]);
    out
}

fn normalize_presentation_project_paths(
    presentation: ToolResultPresentation,
) -> ToolResultPresentation {
    match presentation {
        ToolResultPresentation::Diff { path, hunks } => ToolResultPresentation::Diff {
            path: normalize_project_mount_paths(&path),
            hunks,
        },
        ToolResultPresentation::Listing { rows } => ToolResultPresentation::Listing {
            rows: rows
                .into_iter()
                .map(|row| normalize_project_mount_paths(&row))
                .collect(),
        },
        ToolResultPresentation::FileContent {
            path,
            lines,
            truncated,
        } => ToolResultPresentation::FileContent {
            path: normalize_project_mount_paths(&path),
            lines,
            truncated,
        },
        ToolResultPresentation::PlainText { body } => ToolResultPresentation::PlainText {
            body: normalize_project_mount_paths(&body),
        },
        other @ ToolResultPresentation::Command { .. } => other,
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
