use std::path::PathBuf;

use crate::llm::ToolDefinition;

pub(super) const MAX_DELEGATED_SKILL_ID_CHARS: usize = 120;
pub(super) const MAX_DELEGATED_TARGET_CHARS: usize = 120;
pub(super) const MAX_DELEGATED_TASK_CHARS: usize = 1_000;
pub(super) const MAX_DELEGATED_PATH_CHARS: usize = 1_000;
pub(super) const DEFAULT_DELEGATED_TIMEOUT_SECS: u64 = 900;
pub(super) const MAX_DELEGATED_TIMEOUT_SECS: u64 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DelegatedSkillInvocationRequest {
    pub(super) skill_id: String,
    pub(super) target: String,
    pub(super) task: String,
    pub(super) cwd: Option<PathBuf>,
    pub(super) timeout_secs: Option<u64>,
}

impl DelegatedSkillInvocationRequest {
    pub(super) fn with_effective_launch_inputs(
        &self,
        cwd: Option<PathBuf>,
        timeout_secs: Option<u64>,
    ) -> Self {
        Self {
            skill_id: self.skill_id.clone(),
            target: self.target.clone(),
            task: self.task.clone(),
            cwd,
            timeout_secs,
        }
    }
}

pub(super) fn parse_delegated_skill_invocation_request(
    arguments: &serde_json::Value,
) -> Option<DelegatedSkillInvocationRequest> {
    let skill_id = arguments.get("skill_id")?.as_str()?.trim().to_string();
    let target = arguments.get("target")?.as_str()?.trim().to_string();
    let task = arguments.get("task")?.as_str()?.trim().to_string();
    let cwd = parse_optional_path_argument(arguments, "cwd")?;
    let timeout_secs = parse_optional_timeout_secs_argument(arguments, "timeout_secs")?;
    if skill_id.is_empty() || target.is_empty() || task.is_empty() {
        return None;
    }
    Some(DelegatedSkillInvocationRequest {
        skill_id,
        target,
        task,
        cwd,
        timeout_secs,
    })
}

fn parse_optional_path_argument(
    arguments: &serde_json::Value,
    key: &str,
) -> Option<Option<PathBuf>> {
    match arguments.get(key) {
        None => Some(None),
        Some(value) => {
            let path = value.as_str()?.trim();
            if path.is_empty() {
                return Some(None);
            }
            Some(Some(PathBuf::from(path)))
        }
    }
}

fn parse_optional_timeout_secs_argument(
    arguments: &serde_json::Value,
    key: &str,
) -> Option<Option<u64>> {
    match arguments.get(key) {
        None => Some(None),
        Some(value) => {
            let timeout_secs = value.as_u64()?;
            if timeout_secs == 0 || timeout_secs > MAX_DELEGATED_TIMEOUT_SECS {
                return None;
            }
            Some(Some(timeout_secs))
        }
    }
}

pub(super) fn invoke_delegated_skill_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "invoke_delegated_skill".to_string(),
        description: "Invoke a delegated skill through alan's runtime-owned delegated launch path. Use this for delegated skills listed in the skills catalog or in active-skill runtime context.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "skill_id": {
                    "type": "string",
                    "description": "Resolved delegated skill id exposed in the skills catalog or active-skill runtime context.",
                    "maxLength": MAX_DELEGATED_SKILL_ID_CHARS
                },
                "target": {
                    "type": "string",
                    "description": "Resolved package-local launch target for this delegated skill.",
                    "maxLength": MAX_DELEGATED_TARGET_CHARS
                },
                "task": {
                    "type": "string",
                    "description": "A concise bounded task for the delegated runtime.",
                    "maxLength": MAX_DELEGATED_TASK_CHARS
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional Alan OS namespace cwd for the delegated Process. When omitted, the child inherits the parent Process cwd.",
                    "maxLength": MAX_DELEGATED_PATH_CHARS
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional bounded runtime timeout for the delegated child. When omitted, alan applies a default bounded child timeout.",
                    "minimum": 1,
                    "maximum": MAX_DELEGATED_TIMEOUT_SECS
                }
            },
            "required": ["skill_id", "target", "task"]
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_valid_invocation_request() {
        let args = json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        });

        let result = parse_delegated_skill_invocation_request(&args).unwrap();
        assert_eq!(result.skill_id, "repo-review");
        assert_eq!(result.target, "reviewer");
        assert_eq!(result.task, "Review the current diff and summarize risks.");
    }

    #[test]
    fn treats_an_empty_optional_cwd_as_absent() {
        let args = json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks.",
            "cwd": "   "
        });

        let result = parse_delegated_skill_invocation_request(&args).unwrap();
        assert_eq!(result.cwd, None);
    }

    #[test]
    fn rejects_missing_or_empty_required_fields() {
        let missing = json!({
            "skill_id": "repo-review",
            "target": "reviewer"
        });
        assert!(parse_delegated_skill_invocation_request(&missing).is_none());

        let empty = json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "   "
        });
        assert!(parse_delegated_skill_invocation_request(&empty).is_none());
    }

    #[test]
    fn accepts_a_bounded_timeout() {
        let request = parse_delegated_skill_invocation_request(&json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff.",
            "timeout_secs": 600
        }))
        .expect("expected delegated request");

        assert_eq!(request.timeout_secs, Some(600));
    }

    #[test]
    fn rejects_an_invalid_timeout() {
        assert!(
            parse_delegated_skill_invocation_request(&json!({
                "skill_id": "repo-review",
                "target": "reviewer",
                "task": "Review the current diff.",
                "timeout_secs": 0
            }))
            .is_none()
        );
        assert!(
            parse_delegated_skill_invocation_request(&json!({
                "skill_id": "repo-review",
                "target": "reviewer",
                "task": "Review the current diff.",
                "timeout_secs": (MAX_DELEGATED_TIMEOUT_SECS + 1)
            }))
            .is_none()
        );
    }

    #[test]
    fn tool_definition_exposes_bounded_invocation_contract() {
        let def = invoke_delegated_skill_tool_definition();
        assert_eq!(def.name, "invoke_delegated_skill");
        assert!(def.description.contains("delegated skill"));
        assert_eq!(def.parameters["type"], "object");
        assert_eq!(def.parameters["properties"]["skill_id"]["type"], "string");
        assert_eq!(
            def.parameters["properties"]["skill_id"]["maxLength"],
            MAX_DELEGATED_SKILL_ID_CHARS
        );
        assert_eq!(def.parameters["properties"]["target"]["type"], "string");
        assert_eq!(
            def.parameters["properties"]["target"]["maxLength"],
            MAX_DELEGATED_TARGET_CHARS
        );
        assert_eq!(def.parameters["properties"]["task"]["type"], "string");
        assert_eq!(
            def.parameters["properties"]["task"]["maxLength"],
            MAX_DELEGATED_TASK_CHARS
        );
        assert!(def.parameters["properties"].get("workspace_root").is_none());
        assert_eq!(def.parameters["properties"]["cwd"]["type"], "string");
    }
}
