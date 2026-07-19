use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::evidence::redact_durable_evidence_text;
use crate::skills::{
    DelegatedSkillInvocationRecord, DelegatedSkillOutputDebugMetadata, DelegatedSkillOutputRef,
    DelegatedSkillResult, DelegatedSkillResultTruncation,
};

use super::delegated_child_run::{
    ChildRuntimeResult, DelegatedChildRunReference, MAX_DELEGATED_RESULT_SUMMARY_CHARS,
};
use super::delegated_skill_tool::{
    DelegatedSkillInvocationRequest, MAX_DELEGATED_PATH_CHARS, MAX_DELEGATED_SKILL_ID_CHARS,
    MAX_DELEGATED_TARGET_CHARS, MAX_DELEGATED_TASK_CHARS,
};
use super::transition::{NamespaceActionRecord, RuntimeLoopState};

pub(super) const MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS: usize = 4_000;
pub(super) const MAX_DELEGATED_CHILD_RUN_METADATA_CHARS: usize = 2_000;
pub(super) const MAX_DELEGATED_RESULT_WARNINGS: usize = 16;
pub(super) const MAX_DELEGATED_RESULT_WARNING_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct DelegatedSkillRolloutRecord {
    #[serde(flatten)]
    invocation: DelegatedSkillInvocationRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    child_run: Option<DelegatedChildRunReference>,
}

pub(super) async fn persist_delegated_child_evidence(
    state: &RuntimeLoopState,
    request: &DelegatedSkillInvocationRequest,
    result: &ChildRuntimeResult,
) -> Option<DelegatedSkillOutputRef> {
    if result.output_text.trim().is_empty() {
        return None;
    }

    let redacted = redact_durable_evidence_text(&result.output_text);
    if !result.requires_output_reference(redacted.text.chars().count()) {
        return None;
    }
    let mut result_doc = json!({
        "child_process_path": result.process_path,
        "child_run_id": result.child_run_id,
        "terminal_status": result.terminal_status_label(),
        "redactions": redacted.markers,
    });
    if let Some(agent_path) = result
        .child_run
        .as_ref()
        .and_then(|record| record.agent_path.as_deref())
    {
        result_doc["child_agent_path"] = json!(agent_path);
    }
    let action_id = state
        .namespace_environment()
        .write_action(
            NamespaceActionRecord::new(
                format!("delegate:{}", request.skill_id),
                result.terminal_status_label(),
            )
            .with_output(redacted.text)
            .with_result(result_doc.to_string())
            .with_approval("not_required"),
        )
        .await
        .ok()?;
    let path = format!(
        "{}/actions/{action_id}/output",
        state.namespace_environment().agent_path()
    );
    let reference = state
        .namespace_environment()
        .evidence_reference(path)
        .await?;
    state
        .namespace_environment()
        .resolve_evidence_reference(&reference, None, result.child_run_value())
        .await
        .ok()?;

    Some(DelegatedSkillOutputRef {
        path: reference.path,
        offset: reference.offset,
        length: reference.length,
        debug: Some(DelegatedSkillOutputDebugMetadata {
            process_path: result.process_path.clone(),
            rollout_path: result
                .rollout_path
                .as_ref()
                .map(|path| path.display().to_string()),
            field: "output_text".to_string(),
        }),
    })
}

pub(super) fn build_bounded_delegated_invocation_persistence(
    request: &DelegatedSkillInvocationRequest,
    result: DelegatedSkillResult,
    child_run: Option<DelegatedChildRunReference>,
) -> (
    serde_json::Value,
    DelegatedSkillInvocationRecord,
    DelegatedSkillRolloutRecord,
) {
    let (arguments, record) = build_bounded_delegated_tape_record(request, result);
    let rollout_record = DelegatedSkillRolloutRecord {
        invocation: record.clone(),
        child_run,
    };
    (arguments, record, rollout_record)
}

fn build_bounded_delegated_tape_record(
    request: &DelegatedSkillInvocationRequest,
    result: DelegatedSkillResult,
) -> (serde_json::Value, DelegatedSkillInvocationRecord) {
    let skill_id =
        truncate_text_with_suffix(&request.skill_id, MAX_DELEGATED_SKILL_ID_CHARS, "...");
    let target = truncate_text_with_suffix(&request.target, MAX_DELEGATED_TARGET_CHARS, "...");
    let task = truncate_text_with_suffix(&request.task, MAX_DELEGATED_TASK_CHARS, "...");
    let mut result = result;
    let summary_chars = result.summary.chars().count();
    if summary_chars > MAX_DELEGATED_RESULT_SUMMARY_CHARS {
        let preview =
            truncate_text_with_suffix(&result.summary, MAX_DELEGATED_RESULT_SUMMARY_CHARS, "...");
        result.summary = preview.clone();
        result.summary_preview = Some(preview);
        let mut truncation = result.truncation.take().unwrap_or_default();
        truncation.summary = true;
        truncation.original_summary_chars = Some(summary_chars);
        result.truncation = Some(truncation);
    }
    if let Some(value) = result.structured_output.take() {
        let serialized_size = serde_json::to_string(&value)
            .map(|text| text.chars().count())
            .unwrap_or(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS + 1);
        result.structured_output = Some(truncate_structured_output(
            value,
            MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS,
        ));
        if serialized_size > MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS {
            let mut truncation = result.truncation.take().unwrap_or_default();
            truncation.structured_output = true;
            result.truncation = Some(truncation);
        }
    }
    bound_delegated_result_sidecars(&mut result);

    let record = DelegatedSkillInvocationRecord {
        skill_id,
        target,
        task,
        cwd: request.cwd.as_ref().map(|path| {
            truncate_text_with_suffix(&path.to_string_lossy(), MAX_DELEGATED_PATH_CHARS, "...")
        }),
        timeout_secs: request.timeout_secs,
        result,
    };
    let mut arguments = json!({
        "skill_id": record.skill_id,
        "target": record.target,
        "task": record.task,
    });
    if let Some(cwd) = record.cwd.as_ref() {
        arguments["cwd"] = json!(cwd);
    }
    if let Some(timeout_secs) = record.timeout_secs {
        arguments["timeout_secs"] = json!(timeout_secs);
    }

    (arguments, record)
}

fn bound_delegated_result_sidecars(result: &mut DelegatedSkillResult) {
    if let Some(value) = result.child_run.take() {
        let serialized_size = serde_json::to_string(&value)
            .map(|text| text.chars().count())
            .unwrap_or(MAX_DELEGATED_CHILD_RUN_METADATA_CHARS + 1);
        result.child_run = Some(truncate_structured_output(
            value,
            MAX_DELEGATED_CHILD_RUN_METADATA_CHARS,
        ));
        if serialized_size > MAX_DELEGATED_CHILD_RUN_METADATA_CHARS {
            let truncation = result.truncation.get_or_insert_with(Default::default);
            truncation.child_run = true;
            truncation.original_child_run_chars = Some(serialized_size);
            append_truncation_note(truncation, "Child-run metadata was truncated.");
        }
    }

    let original_warning_count = result.warnings.len();
    let (warnings, truncated) = bounded_delegated_warnings(std::mem::take(&mut result.warnings));
    result.warnings = warnings;
    if truncated {
        let truncation = result.truncation.get_or_insert_with(Default::default);
        truncation.warnings = true;
        truncation.original_warning_count = Some(original_warning_count);
        append_truncation_note(truncation, "Warnings were truncated to recent entries.");
    }
}

fn bounded_delegated_warnings(warnings: Vec<String>) -> (Vec<String>, bool) {
    let original_count = warnings.len();
    let skip_count = original_count.saturating_sub(MAX_DELEGATED_RESULT_WARNINGS);
    let mut truncated = skip_count > 0;
    let warnings = warnings
        .into_iter()
        .skip(skip_count)
        .map(|warning| {
            let bounded =
                truncate_text_with_suffix(&warning, MAX_DELEGATED_RESULT_WARNING_CHARS, "...");
            if bounded != warning {
                truncated = true;
            }
            bounded
        })
        .collect();
    (warnings, truncated)
}

fn append_truncation_note(truncation: &mut DelegatedSkillResultTruncation, note: &str) {
    match truncation.note.as_mut() {
        Some(existing) if !existing.contains(note) => {
            existing.push(' ');
            existing.push_str(note);
        }
        Some(_) => {}
        None => truncation.note = Some(note.to_string()),
    }
}

fn is_critical_structured_output_key(key: &str) -> bool {
    matches!(
        key,
        "status"
            | "summary"
            | "overall_status"
            | "verification_attempted"
            | "attempted_count"
            | "passed_count"
            | "failed_count"
            | "environment_blocked_count"
            | "blocked_count"
            | "not_run_count"
            | "all_passed"
    )
}

fn truncate_structured_output(value: serde_json::Value, max_size: usize) -> serde_json::Value {
    let rendered = value.to_string();
    if rendered.len() <= max_size {
        return value;
    }

    match value {
        serde_json::Value::Object(map) => {
            let mut truncated = serde_json::Map::new();
            let mut current_size = 0usize;

            for (key, value) in map {
                let is_critical = is_critical_structured_output_key(key.as_str());
                let processed_value = if is_critical {
                    truncate_structured_output(value, (max_size / 4).max(64))
                } else {
                    truncate_structured_output(value, (max_size / 2).max(64))
                };
                let value_size = key.len() + processed_value.to_string().len();
                if current_size + value_size < max_size * 3 / 4 || is_critical {
                    truncated.insert(key, processed_value);
                    current_size += value_size;
                } else {
                    truncated.insert(
                        "_truncated".to_string(),
                        serde_json::Value::String("Additional fields omitted".to_string()),
                    );
                    break;
                }
            }

            serde_json::Value::Object(truncated)
        }
        serde_json::Value::Array(items) => {
            let item_budget = (max_size / items.len().max(1)).max(32);
            let mut truncated = Vec::new();
            let mut current_size = 0usize;

            for item in items {
                let processed = truncate_structured_output(item, item_budget);
                let item_size = processed.to_string().len();
                if current_size + item_size < max_size * 3 / 4 {
                    truncated.push(processed);
                    current_size += item_size;
                } else {
                    truncated.push(json!({
                        "_note": "Additional array items omitted"
                    }));
                    break;
                }
            }

            serde_json::Value::Array(truncated)
        }
        serde_json::Value::String(text) => {
            serde_json::Value::String(truncate_text_with_suffix(&text, max_size, "..."))
        }
        other => other,
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
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::runtime::delegated_skill_tool::DEFAULT_DELEGATED_TIMEOUT_SECS;

    fn request() -> DelegatedSkillInvocationRequest {
        DelegatedSkillInvocationRequest {
            skill_id: "repo-review".to_string(),
            target: "reviewer".to_string(),
            task: "Review the current diff and summarize risks.".to_string(),
            cwd: Some(PathBuf::from("/mnt/source/src")),
            timeout_secs: None,
        }
    }

    #[test]
    fn persistence_truncates_invocation_fields() {
        let request = DelegatedSkillInvocationRequest {
            skill_id: "s".repeat(MAX_DELEGATED_SKILL_ID_CHARS + 40),
            target: "t".repeat(MAX_DELEGATED_TARGET_CHARS + 40),
            task: "x".repeat(MAX_DELEGATED_TASK_CHARS + 200),
            cwd: Some(PathBuf::from(format!(
                "/tmp/{}",
                "c".repeat(MAX_DELEGATED_PATH_CHARS + 20)
            ))),
            timeout_secs: Some(DEFAULT_DELEGATED_TIMEOUT_SECS),
        };
        let result = DelegatedSkillResult::failed(
            format!(
                "Delegated skill '{}' resolved to delegated target '{}', but delegated launch support is not yet available in this runtime.",
                request.skill_id, request.target
            ),
            Some(json!({
                "error_kind": "runtime_child_launch_unavailable"
            })),
        );
        let child_run = Some(DelegatedChildRunReference {
            process_path: "/proc/42".to_string(),
            child_run_id: None,
            state_ref: None,
            rollout_debug_path: None,
            terminal_status: "completed".to_string(),
        });

        let (arguments, record, rollout_record) =
            build_bounded_delegated_invocation_persistence(&request, result, child_run);

        let skill_id = arguments["skill_id"].as_str().unwrap();
        let target = arguments["target"].as_str().unwrap();
        let task = arguments["task"].as_str().unwrap();
        assert!(skill_id.chars().count() <= MAX_DELEGATED_SKILL_ID_CHARS);
        assert!(target.chars().count() <= MAX_DELEGATED_TARGET_CHARS);
        assert!(task.chars().count() <= MAX_DELEGATED_TASK_CHARS);
        assert!(skill_id.ends_with("..."));
        assert!(target.ends_with("..."));
        assert!(task.ends_with("..."));
        assert!(arguments.get("workspace_root").is_none());
        assert!(arguments["cwd"].as_str().unwrap().chars().count() <= MAX_DELEGATED_PATH_CHARS);
        assert_eq!(
            arguments["timeout_secs"].as_u64(),
            Some(DEFAULT_DELEGATED_TIMEOUT_SECS)
        );
        assert!(record.result.summary.chars().count() <= MAX_DELEGATED_RESULT_SUMMARY_CHARS);
        assert!(record.result.summary.ends_with("..."));
        assert_eq!(
            rollout_record.child_run.as_ref().unwrap().process_path,
            "/proc/42"
        );
    }

    #[test]
    fn child_run_is_present_only_in_the_rollout_record() {
        let result = DelegatedSkillResult::completed("Delegated review completed.", None);
        let child_run = Some(DelegatedChildRunReference {
            process_path: "/proc/42".to_string(),
            child_run_id: None,
            state_ref: None,
            rollout_debug_path: Some("/tmp/inline-child.jsonl".to_string()),
            terminal_status: "completed".to_string(),
        });

        let (_, tape_record, rollout_record) =
            build_bounded_delegated_invocation_persistence(&request(), result, child_run);
        let tape_payload = serde_json::to_value(&tape_record).unwrap();
        let rollout_payload = serde_json::to_value(&rollout_record).unwrap();

        assert!(tape_payload.get("child_run").is_none());
        assert_eq!(
            rollout_payload["child_run"]["process_path"],
            json!("/proc/42")
        );
        assert_eq!(
            rollout_payload["child_run"]["rollout_debug_path"],
            json!("/tmp/inline-child.jsonl")
        );
        assert!(rollout_payload["child_run"].get("rollout_path").is_none());
        assert!(tape_payload.get("workspace_root").is_none());
        assert_eq!(tape_payload["cwd"], json!("/mnt/source/src"));
    }

    #[test]
    fn persistence_bounds_result_sidecars() {
        let mut result = DelegatedSkillResult::failed("Delegated review failed.", None);
        result.child_run = Some(json!({
            "id": "child-run-1",
            "status": "failed",
            "warnings": vec!["child-warning".repeat(200); MAX_DELEGATED_RESULT_WARNINGS + 8],
            "large_metadata": "x".repeat(MAX_DELEGATED_CHILD_RUN_METADATA_CHARS * 2)
        }));
        result.warnings = (0..(MAX_DELEGATED_RESULT_WARNINGS + 3))
            .map(|index| {
                format!(
                    "warning-{index:03}-{}",
                    "x".repeat(MAX_DELEGATED_RESULT_WARNING_CHARS)
                )
            })
            .collect();

        let (_, tape_record, _) =
            build_bounded_delegated_invocation_persistence(&request(), result, None);

        assert_eq!(
            tape_record.result.warnings.len(),
            MAX_DELEGATED_RESULT_WARNINGS
        );
        assert!(tape_record.result.warnings[0].starts_with("warning-003-"));
        assert!(
            tape_record
                .result
                .warnings
                .iter()
                .all(|warning| warning.chars().count() <= MAX_DELEGATED_RESULT_WARNING_CHARS)
        );
        assert!(tape_record.result.warnings.last().unwrap().ends_with("..."));
        assert!(
            tape_record
                .result
                .child_run
                .as_ref()
                .unwrap()
                .to_string()
                .len()
                <= MAX_DELEGATED_CHILD_RUN_METADATA_CHARS
        );
        let truncation = tape_record.result.truncation.unwrap();
        assert!(truncation.child_run);
        assert!(truncation.warnings);
        assert!(
            truncation.original_child_run_chars.unwrap() > MAX_DELEGATED_CHILD_RUN_METADATA_CHARS
        );
        assert_eq!(
            truncation.original_warning_count,
            Some(MAX_DELEGATED_RESULT_WARNINGS + 3)
        );
    }

    #[test]
    fn persistence_truncates_structured_output() {
        let result = DelegatedSkillResult::completed(
            "Delegated review completed.",
            Some(json!({
                "status": "completed",
                "summary": "Delegated review completed.",
                "details": "x".repeat(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS * 2)
            })),
        );

        let (_, tape_record, _) =
            build_bounded_delegated_invocation_persistence(&request(), result, None);
        let structured = tape_record.result.structured_output.unwrap();
        assert!(structured.to_string().len() <= MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS);
        assert_eq!(structured["status"], json!("completed"));
    }

    #[test]
    fn rollout_record_flattens_invocation_result() {
        let mut result = DelegatedSkillResult::completed("Review completed.", None);
        result.output_ref = Some(DelegatedSkillOutputRef {
            path: "/agent/1/actions/a0/output".to_string(),
            offset: Some(0),
            length: Some(42),
            debug: Some(DelegatedSkillOutputDebugMetadata {
                process_path: "child-machine".to_string(),
                rollout_path: Some("/tmp/child.jsonl".to_string()),
                field: "output_text".to_string(),
            }),
        });

        let (_, _, rollout_record) =
            build_bounded_delegated_invocation_persistence(&request(), result, None);
        let serialized = serde_json::to_value(rollout_record).unwrap();

        assert_eq!(
            serialized.pointer("/result/output_ref/debug/rollout_path"),
            Some(&json!("/tmp/child.jsonl"))
        );
        assert!(serialized.get("invocation").is_none());
    }

    #[test]
    fn persistence_truncates_an_oversized_summary() {
        let result = DelegatedSkillResult::completed(
            "Delegated review completed.",
            Some(json!({
                "status": "completed",
                "summary": "y".repeat(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS * 2),
                "details": "x".repeat(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS * 2)
            })),
        );

        let (_, tape_record, _) =
            build_bounded_delegated_invocation_persistence(&request(), result, None);
        let structured = tape_record.result.structured_output.unwrap();
        let summary = structured["summary"]
            .as_str()
            .expect("summary should remain string");
        assert!(structured.to_string().len() <= MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS);
        assert!(summary.len() < MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS);
        assert!(summary.ends_with("..."));
        assert_eq!(structured["status"], json!("completed"));
    }
}
