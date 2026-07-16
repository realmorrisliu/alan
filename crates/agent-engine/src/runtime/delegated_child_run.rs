use std::path::PathBuf;

use alan_agent_protocol::YieldKind;
use serde::{Deserialize, Serialize};

use crate::evidence::redact_durable_evidence_text;
use crate::skills::{
    DelegatedSkillOutputRef, DelegatedSkillResult, DelegatedSkillResultTruncation,
};

use super::child_runs::ChildRunRecord;

pub(super) const MAX_DELEGATED_RESULT_SUMMARY_CHARS: usize = 320;
pub(super) const MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChildRuntimeStatus {
    Completed,
    Paused,
    Cancelled,
    TimedOut,
    Terminated,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChildRuntimePause {
    pub(super) request_id: String,
    pub(super) kind: YieldKind,
}

/// Normalized lifecycle and handoff evidence for one delegated Child Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChildRuntimeResult {
    pub(super) status: ChildRuntimeStatus,
    pub(super) process_path: String,
    pub(super) child_run_id: Option<String>,
    pub(super) rollout_path: Option<PathBuf>,
    pub(super) output_text: String,
    pub(super) turn_summary: Option<String>,
    pub(super) structured_output: Option<serde_json::Value>,
    pub(super) warnings: Vec<String>,
    pub(super) error_message: Option<String>,
    pub(super) pause: Option<ChildRuntimePause>,
    pub(super) child_run: Option<ChildRunRecord>,
}

impl ChildRuntimeResult {
    pub(super) fn cancelled_before_launch() -> Self {
        Self {
            status: ChildRuntimeStatus::Cancelled,
            process_path: String::new(),
            child_run_id: None,
            rollout_path: None,
            output_text: String::new(),
            turn_summary: None,
            structured_output: None,
            warnings: Vec::new(),
            error_message: None,
            pause: None,
            child_run: None,
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        matches!(self.status, ChildRuntimeStatus::Cancelled)
    }

    pub(super) fn requires_output_reference(&self, redacted_output_chars: usize) -> bool {
        !matches!(self.status, ChildRuntimeStatus::Completed)
            || redacted_output_chars > MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS
    }

    pub(super) fn terminal_status_label(&self) -> &'static str {
        match self.status {
            ChildRuntimeStatus::Completed => "completed",
            ChildRuntimeStatus::Paused => "paused",
            ChildRuntimeStatus::Cancelled => "cancelled",
            ChildRuntimeStatus::TimedOut => "timed_out",
            ChildRuntimeStatus::Terminated => "terminated",
            ChildRuntimeStatus::Failed => "failed",
        }
    }

    pub(super) fn reference(&self) -> DelegatedChildRunReference {
        DelegatedChildRunReference {
            child_run_id: self.child_run_id.clone(),
            process_path: self.process_path.clone(),
            state_ref: self
                .child_run
                .as_ref()
                .and_then(|record| record.state_ref.clone()),
            rollout_debug_path: self
                .rollout_path
                .as_ref()
                .map(|path| path.display().to_string()),
            terminal_status: self.terminal_status_label().to_string(),
        }
    }

    pub(super) fn child_run_value(&self) -> Option<serde_json::Value> {
        self.child_run
            .as_ref()
            .and_then(|record| serde_json::to_value(record).ok())
            .or_else(|| {
                serde_json::to_value(self.reference())
                    .ok()
                    .filter(|value| !value.is_null())
            })
    }

    pub(super) fn delegated_result(
        &self,
        output_reference: Option<DelegatedSkillOutputRef>,
    ) -> DelegatedSkillResult {
        let mut delegated = match self.status {
            ChildRuntimeStatus::Completed => {
                self.completed_delegated_result(output_reference.as_ref())
            }
            ChildRuntimeStatus::Failed => self.failed_delegated_result(
                format!(
                    "Delegated runtime failed: {}",
                    self.error_message
                        .clone()
                        .or_else(|| non_empty_trimmed(&self.output_text))
                        .unwrap_or_else(|| "unknown failure".to_string())
                ),
                "child_failed",
                output_reference.as_ref(),
            ),
            ChildRuntimeStatus::TimedOut => self.failed_delegated_result(
                "Delegated runtime timed out.".to_string(),
                "child_timed_out",
                output_reference.as_ref(),
            ),
            ChildRuntimeStatus::Cancelled => self.failed_delegated_result(
                "Delegated runtime was cancelled.".to_string(),
                "child_cancelled",
                output_reference.as_ref(),
            ),
            ChildRuntimeStatus::Terminated => self.failed_delegated_result(
                self.error_message
                    .clone()
                    .unwrap_or_else(|| "Delegated runtime was terminated.".to_string()),
                "child_terminated",
                output_reference.as_ref(),
            ),
            ChildRuntimeStatus::Paused => self.paused_delegated_result(output_reference),
        };
        delegated.capability_decision = self
            .child_run
            .as_ref()
            .and_then(|record| record.delegation_capability_decision.clone());
        delegated
    }

    fn completed_delegated_result(
        &self,
        output_reference: Option<&DelegatedSkillOutputRef>,
    ) -> DelegatedSkillResult {
        let output_text = non_empty_trimmed(&self.output_text)
            .map(|text| redact_durable_evidence_text(&text).text);
        let structured_output = self
            .structured_output
            .as_ref()
            .map(crate::evidence::redact_evidence_payload);
        let mut delegated = DelegatedSkillResult::completed(
            self.completed_summary(output_text.as_deref(), structured_output.as_ref()),
            structured_output,
        );
        delegated.child_run = self.child_run_value();
        delegated.warnings.clone_from(&self.warnings);

        if let Some(output_text) = output_text {
            let output_chars = output_text.chars().count();
            if output_chars <= MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS {
                delegated.output_text = Some(output_text);
            } else {
                delegated.summary_preview = Some(truncate_text_with_suffix(
                    &output_text,
                    MAX_DELEGATED_RESULT_SUMMARY_CHARS,
                    "... [truncated; inspect output_ref]",
                ));
                delegated.output_ref = output_reference.cloned();
                delegated.truncation = Some(DelegatedSkillResultTruncation {
                    output_text: true,
                    original_output_chars: Some(output_chars),
                    note: Some(if delegated.output_ref.is_some() {
                        "Full child output is available from the namespace output_ref.".to_string()
                    } else {
                        "Child output was truncated, and no parent-resolvable evidence path could be emitted; the inline preview is the declared-complete retained record."
                            .to_string()
                    }),
                    ..DelegatedSkillResultTruncation::default()
                });
            }
        }

        delegated
    }

    fn failed_delegated_result(
        &self,
        summary: String,
        error_kind: &str,
        output_reference: Option<&DelegatedSkillOutputRef>,
    ) -> DelegatedSkillResult {
        let mut delegated = DelegatedSkillResult::failed(
            summary,
            Some(serde_json::json!({
                "error_kind": error_kind
            })),
        );
        delegated.error_kind = Some(error_kind.to_string());
        delegated.error_message.clone_from(&self.error_message);
        delegated.child_run = self.child_run_value();
        delegated.warnings.clone_from(&self.warnings);
        if !self.output_text.trim().is_empty() {
            delegated.output_ref = output_reference.cloned();
            delegated.truncation = Some(DelegatedSkillResultTruncation {
                output_text: true,
                original_output_chars: Some(self.output_text.chars().count()),
                note: Some(if delegated.output_ref.is_some() {
                    "Child produced output before terminal failure; inspect the namespace output_ref."
                        .to_string()
                } else {
                    "Child produced output before terminal failure, but no parent-resolvable evidence path could be emitted; only the marked preview is retained."
                        .to_string()
                }),
                ..DelegatedSkillResultTruncation::default()
            });
        }
        delegated
    }

    fn paused_delegated_result(
        &self,
        output_reference: Option<DelegatedSkillOutputRef>,
    ) -> DelegatedSkillResult {
        let (pause_kind, request_id) = self
            .pause
            .as_ref()
            .map(|pause| {
                (
                    yield_kind_label(&pause.kind),
                    Some(pause.request_id.clone()),
                )
            })
            .unwrap_or_else(|| ("unknown".to_string(), None));
        let mut delegated = DelegatedSkillResult::failed(
            format!(
                "Delegated runtime paused for {pause_kind} and cannot continue in v1 delegated execution."
            ),
            Some(serde_json::json!({
                "error_kind": "child_paused",
                "pause_kind": pause_kind,
                "request_id": request_id
            })),
        );
        delegated.error_kind = Some("child_paused".to_string());
        delegated.error_message.clone_from(&self.error_message);
        delegated.child_run = self.child_run_value();
        delegated.warnings.clone_from(&self.warnings);
        if !self.output_text.trim().is_empty() {
            delegated.output_ref = output_reference;
            delegated.truncation = Some(DelegatedSkillResultTruncation {
                output_text: true,
                original_output_chars: Some(self.output_text.chars().count()),
                note: Some(if delegated.output_ref.is_some() {
                    "Child produced output before pausing; inspect the namespace output_ref."
                        .to_string()
                } else {
                    "Child produced output before pausing, but no parent-resolvable evidence path could be emitted; only the marked preview is retained."
                        .to_string()
                }),
                ..DelegatedSkillResultTruncation::default()
            });
        }
        delegated
    }

    fn completed_summary(
        &self,
        redacted_output_text: Option<&str>,
        redacted_structured_output: Option<&serde_json::Value>,
    ) -> String {
        structured_output_summary(redacted_structured_output)
            .or_else(|| {
                redacted_output_text
                    .and_then(non_empty_trimmed)
                    .map(|text| {
                        truncate_text_with_suffix(
                            &text,
                            MAX_DELEGATED_RESULT_SUMMARY_CHARS,
                            "... [truncated; inspect output_text or output_ref]",
                        )
                    })
            })
            .or_else(|| {
                non_empty_trimmed(self.turn_summary.as_deref().unwrap_or_default())
                    .map(|summary| redact_durable_evidence_text(&summary).text)
            })
            .unwrap_or_else(|| "Delegated runtime completed without textual output.".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct DelegatedChildRunReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) child_run_id: Option<String>,
    pub(super) process_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) state_ref: Option<DelegatedSkillOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) rollout_debug_path: Option<String>,
    pub(super) terminal_status: String,
}

fn structured_output_summary(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|value| value.get("summary"))
        .and_then(serde_json::Value::as_str)
        .and_then(non_empty_trimmed)
}

fn non_empty_trimmed(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn yield_kind_label(kind: &YieldKind) -> String {
    match kind {
        YieldKind::Confirmation => "confirmation".to_string(),
        YieldKind::StructuredInput => "structured_input".to_string(),
        YieldKind::Custom(kind) => kind.clone(),
    }
}

fn truncate_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
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

#[cfg(test)]
#[path = "delegated_child_run_tests.rs"]
mod tests;
