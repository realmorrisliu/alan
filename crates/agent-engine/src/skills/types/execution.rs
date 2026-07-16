use super::{AlanSkillExecutionMode, SkillMetadata, name_to_id};
use serde::{Deserialize, Serialize};

/// Why a delegated or inline execution state resolved the way it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillExecutionResolutionSource {
    ExplicitMetadata,
    NoChildAgentExports,
    SameNameSkillAndChildAgent,
    SingleSkillSingleChildAgent,
}

impl SkillExecutionResolutionSource {
    pub fn render_label(&self) -> &'static str {
        match self {
            Self::ExplicitMetadata => "explicit_metadata",
            Self::NoChildAgentExports => "no_child_agent_exports",
            Self::SameNameSkillAndChildAgent => "same_name_skill_and_child_agent",
            Self::SingleSkillSingleChildAgent => "single_skill_single_child_agent",
        }
    }
}

/// Why delegated execution could not be resolved for a skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillExecutionUnresolvedReason {
    NotResolved,
    MissingChildAgentExports,
    DelegateTargetNotFound {
        target: String,
        available_targets: Vec<String>,
    },
    AmbiguousPackageShape {
        skill_id: String,
        child_agent_exports: Vec<String>,
    },
}

impl SkillExecutionUnresolvedReason {
    pub fn render_label(&self) -> String {
        match self {
            Self::NotResolved => "not_resolved".to_string(),
            Self::MissingChildAgentExports => "missing_child_agent_exports".to_string(),
            Self::DelegateTargetNotFound { target, .. } => {
                format!("delegate_target_not_found({target})")
            }
            Self::AmbiguousPackageShape { .. } => "ambiguous_package_shape".to_string(),
        }
    }
}

/// Resolved execution state for a skill after package-local inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedSkillExecution {
    Inline {
        source: SkillExecutionResolutionSource,
    },
    Delegate {
        target: String,
        source: SkillExecutionResolutionSource,
    },
    Unresolved {
        reason: SkillExecutionUnresolvedReason,
    },
}

impl Default for ResolvedSkillExecution {
    fn default() -> Self {
        Self::Unresolved {
            reason: SkillExecutionUnresolvedReason::NotResolved,
        }
    }
}

impl ResolvedSkillExecution {
    pub fn render_label(&self) -> String {
        match self {
            Self::Inline { source } => format!("inline({})", source.render_label()),
            Self::Delegate { target, source } => {
                format!(
                    "delegate(target={target}, source={})",
                    source.render_label()
                )
            }
            Self::Unresolved { reason } => format!("unresolved({})", reason.render_label()),
        }
    }

    pub fn delegate_target(&self) -> Option<&str> {
        match self {
            Self::Delegate { target, .. } => Some(target.as_str()),
            _ => None,
        }
    }

    pub fn renders_inline_body(&self) -> bool {
        matches!(self, Self::Inline { .. })
            || matches!(
                self,
                Self::Unresolved {
                    reason: SkillExecutionUnresolvedReason::NotResolved,
                }
            )
    }
}

/// Status returned to the parent for a delegated skill invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedSkillResultStatus {
    Completed,
    Failed,
}

/// Bounded delegated-skill result returned to the parent runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegatedSkillResult {
    pub status: DelegatedSkillResultStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_run: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_ref: Option<DelegatedSkillOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output_ref: Option<DelegatedSkillOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<DelegatedSkillResultTruncation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_decision: Option<alan_agent_protocol::DelegatedCapabilityDecision>,
}

/// Inspectable reference for omitted delegated child output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedSkillOutputRef {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<DelegatedSkillOutputDebugMetadata>,
}

/// Optional host-debug metadata that is never used to resolve delegated output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedSkillOutputDebugMetadata {
    pub process_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_path: Option<String>,
    pub field: String,
}

/// Explicit truncation metadata for delegated child handoff fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DelegatedSkillResultTruncation {
    #[serde(default)]
    pub summary: bool,
    #[serde(default)]
    pub output_text: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub child_run: bool,
    #[serde(default)]
    pub warnings: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_summary_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_output_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_child_run_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_warning_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl DelegatedSkillResult {
    pub fn completed(
        summary: impl Into<String>,
        structured_output: Option<serde_json::Value>,
    ) -> Self {
        Self {
            status: DelegatedSkillResultStatus::Completed,
            summary: summary.into(),
            summary_preview: None,
            child_run: None,
            output_text: None,
            output_ref: None,
            structured_output,
            structured_output_ref: None,
            truncation: None,
            warnings: Vec::new(),
            error_kind: None,
            error_message: None,
            capability_decision: None,
        }
    }

    pub fn failed(
        summary: impl Into<String>,
        structured_output: Option<serde_json::Value>,
    ) -> Self {
        Self {
            status: DelegatedSkillResultStatus::Failed,
            summary: summary.into(),
            summary_preview: None,
            child_run: None,
            output_text: None,
            output_ref: None,
            structured_output,
            structured_output_ref: None,
            truncation: None,
            warnings: Vec::new(),
            error_kind: None,
            error_message: None,
            capability_decision: None,
        }
    }
}

/// Parent-side record of a delegated invocation and its bounded result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegatedSkillInvocationRecord {
    pub skill_id: String,
    pub target: String,
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    pub result: DelegatedSkillResult,
}

pub fn resolve_skill_execution(
    metadata: &SkillMetadata,
    child_agent_exports: &[String],
) -> ResolvedSkillExecution {
    match metadata.alan_metadata.execution.mode {
        Some(AlanSkillExecutionMode::Inline) => ResolvedSkillExecution::Inline {
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        },
        Some(AlanSkillExecutionMode::Delegate) => {
            resolve_explicit_delegate_execution(metadata, child_agent_exports)
        }
        None => infer_skill_execution(metadata, child_agent_exports),
    }
}

fn resolve_explicit_delegate_execution(
    metadata: &SkillMetadata,
    child_agent_exports: &[String],
) -> ResolvedSkillExecution {
    if let Some(target) = metadata.alan_metadata.execution.target.as_ref() {
        if child_agent_exports.iter().any(|name| name == target) {
            return ResolvedSkillExecution::Delegate {
                target: target.clone(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            };
        }

        return ResolvedSkillExecution::Unresolved {
            reason: SkillExecutionUnresolvedReason::DelegateTargetNotFound {
                target: target.clone(),
                available_targets: child_agent_exports.to_vec(),
            },
        };
    }

    if child_agent_exports.is_empty() {
        return ResolvedSkillExecution::Unresolved {
            reason: SkillExecutionUnresolvedReason::MissingChildAgentExports,
        };
    }

    match same_name_child_agent_target(&metadata.id, child_agent_exports) {
        SameNameChildAgentTarget::Matched(target) => {
            return ResolvedSkillExecution::Delegate {
                target,
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            };
        }
        SameNameChildAgentTarget::Ambiguous => {
            return ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: metadata.id.clone(),
                    child_agent_exports: child_agent_exports.to_vec(),
                },
            };
        }
        SameNameChildAgentTarget::NotFound => {}
    }

    if child_agent_exports.len() == 1 {
        return ResolvedSkillExecution::Delegate {
            target: child_agent_exports[0].clone(),
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        };
    }

    ResolvedSkillExecution::Unresolved {
        reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
            skill_id: metadata.id.clone(),
            child_agent_exports: child_agent_exports.to_vec(),
        },
    }
}

fn infer_skill_execution(
    metadata: &SkillMetadata,
    child_agent_exports: &[String],
) -> ResolvedSkillExecution {
    if child_agent_exports.is_empty() {
        return ResolvedSkillExecution::Inline {
            source: SkillExecutionResolutionSource::NoChildAgentExports,
        };
    }

    match same_name_child_agent_target(&metadata.id, child_agent_exports) {
        SameNameChildAgentTarget::Matched(target) => {
            return ResolvedSkillExecution::Delegate {
                target,
                source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            };
        }
        SameNameChildAgentTarget::Ambiguous => {
            return ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: metadata.id.clone(),
                    child_agent_exports: child_agent_exports.to_vec(),
                },
            };
        }
        SameNameChildAgentTarget::NotFound => {}
    }

    if child_agent_exports.len() == 1 {
        return ResolvedSkillExecution::Delegate {
            target: child_agent_exports[0].clone(),
            source: SkillExecutionResolutionSource::SingleSkillSingleChildAgent,
        };
    }

    ResolvedSkillExecution::Unresolved {
        reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
            skill_id: metadata.id.clone(),
            child_agent_exports: child_agent_exports.to_vec(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SameNameChildAgentTarget {
    Matched(String),
    Ambiguous,
    NotFound,
}

fn same_name_child_agent_target(
    skill_id: &str,
    child_agent_exports: &[String],
) -> SameNameChildAgentTarget {
    let normalized_skill_id = name_to_id(skill_id);
    let mut matching_target = None;

    for export_name in child_agent_exports {
        if name_to_id(export_name) != normalized_skill_id {
            continue;
        }

        if matching_target.is_some() {
            return SameNameChildAgentTarget::Ambiguous;
        }

        matching_target = Some(export_name.clone());
    }

    matching_target
        .map(SameNameChildAgentTarget::Matched)
        .unwrap_or(SameNameChildAgentTarget::NotFound)
}
