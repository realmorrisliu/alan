use crate::ReasoningEffort;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Material capability required by a delegated task, expressed in the same
/// mount-and-binding vocabulary used to assemble the child namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DelegatedCapabilityRequirement {
    MountRead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
    },
    MountWrite {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
    },
    Shell,
    Network,
    Github,
    Browser,
    LlmConnection,
    SideEffects,
}

impl DelegatedCapabilityRequirement {
    /// Stable human-readable label used in launch and mismatch records.
    pub fn label(&self) -> &'static str {
        match self {
            Self::MountRead { .. } => "mount_read",
            Self::MountWrite { .. } => "mount_write",
            Self::Shell => "shell",
            Self::Network => "network",
            Self::Github => "github",
            Self::Browser => "browser",
            Self::LlmConnection => "llm_connection",
            Self::SideEffects => "side_effects",
        }
    }
}

/// Bounded historical summary derived from the actual child namespace plan.
///
/// This is audit metadata, not a second authority registry. While a child is
/// alive, `/proc/<pid>/namespace` remains the source of truth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DelegatedNamespaceSummary {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writable_mounts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bin_bindings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_connection: Option<String>,
}

/// Visible recovery selected when a delegated namespace cannot satisfy the
/// original task requirements.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedCapabilityRecovery {
    Satisfied,
    ParentPath,
    Narrowed,
    AskUser,
    Limitation,
}

/// Auditable outcome of comparing classified task requirements with the
/// assembled child namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DelegatedCapabilityDecision {
    pub requirements: Vec<DelegatedCapabilityRequirement>,
    pub namespace: DelegatedNamespaceSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsatisfied: Vec<DelegatedCapabilityRequirement>,
    pub recovery: DelegatedCapabilityRecovery,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrowed_task: Option<String>,
}

/// Requirement input attached to a delegated spawn before namespace assembly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DelegatedSpawnContext {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<DelegatedCapabilityRequirement>,
}

/// Explicit launch target for a child agent instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpawnTarget {
    /// Launch from an Agent Definition descriptor already present in the parent Process.
    DefinitionDescriptor { descriptor: String },
    /// Launch from a package-exported child-agent handle in the parent's capability view.
    PackageChildAgent {
        package_id: String,
        export_name: String,
    },
}

/// Shared parent-side handle that may be explicitly bound into a child launch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SpawnHandle {
    Artifacts,
    Memory,
    Plan,
    ConversationSnapshot,
    ToolResults,
    ApprovalScope,
}

/// Launch inputs supplied for a child runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct SpawnLaunchInputs {
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
}

/// First-version tool profile override for a child launch.
///
/// alan does not have stable named host profiles yet, so the initial contract
/// models a profile override as an explicit tool allowlist.
///
/// The allowlist is strict: child launch must fail instead of silently dropping
/// requested entries that cannot be bound into the child runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SpawnToolProfileOverride {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
}

impl SpawnToolProfileOverride {
    pub fn is_empty(&self) -> bool {
        self.allowed_tools.is_empty()
    }
}

/// Runtime overrides applied to the child runtime at launch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SpawnRuntimeOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_reasoning_effort: Option<ReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_profile: Option<SpawnToolProfileOverride>,
}

impl SpawnRuntimeOverrides {
    pub fn is_empty(&self) -> bool {
        self.model.is_none()
            && self.model_reasoning_effort.is_none()
            && self.policy_path.is_none()
            && self
                .tool_profile
                .as_ref()
                .is_none_or(SpawnToolProfileOverride::is_empty)
    }
}

/// Explicit child-agent launch contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpawnSpec {
    pub target: SpawnTarget,
    pub launch: SpawnLaunchInputs,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<SpawnHandle>,
    #[serde(default, skip_serializing_if = "SpawnRuntimeOverrides::is_empty")]
    pub runtime_overrides: SpawnRuntimeOverrides,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated: Option<DelegatedSpawnContext>,
}

impl SpawnSpec {
    pub fn has_handle(&self, handle: SpawnHandle) -> bool {
        self.handles.contains(&handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_spec_round_trips_with_handles_and_overrides() {
        let spec = SpawnSpec {
            target: SpawnTarget::DefinitionDescriptor {
                descriptor: "agent-definition".to_string(),
            },
            launch: SpawnLaunchInputs {
                task: "Review the repository".to_string(),
                cwd: Some(PathBuf::from("/mnt/source")),
                timeout_secs: Some(120),
                output_dir: Some(PathBuf::from("/mnt/source/out")),
            },
            handles: vec![SpawnHandle::ConversationSnapshot, SpawnHandle::ToolResults],
            runtime_overrides: SpawnRuntimeOverrides {
                model: Some("gpt-5.4".to_string()),
                model_reasoning_effort: Some(ReasoningEffort::High),
                policy_path: Some("policy.yaml".to_string()),
                tool_profile: Some(SpawnToolProfileOverride {
                    allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
                }),
            },
            delegated: Some(DelegatedSpawnContext {
                requirements: vec![
                    DelegatedCapabilityRequirement::MountRead {
                        path: Some(PathBuf::from("/mnt/source")),
                    },
                    DelegatedCapabilityRequirement::LlmConnection,
                ],
            }),
        };

        let value = serde_json::to_value(&spec).unwrap();
        assert_eq!(value["target"]["kind"], "definition_descriptor");
        assert_eq!(value["handles"][0], "conversation_snapshot");
        assert_eq!(value["runtime_overrides"]["model"], "gpt-5.4");
        assert_eq!(value["runtime_overrides"]["model_reasoning_effort"], "high");
        assert_eq!(value["delegated"]["requirements"][0]["kind"], "mount_read");

        let parsed: SpawnSpec = serde_json::from_value(value).unwrap();
        assert!(parsed.has_handle(SpawnHandle::ConversationSnapshot));
        assert_eq!(
            parsed.runtime_overrides.tool_profile.unwrap().allowed_tools,
            vec!["read_file".to_string(), "grep".to_string()]
        );
    }

    #[test]
    fn spawn_spec_round_trips_package_child_agent_target() {
        let spec = SpawnSpec {
            target: SpawnTarget::PackageChildAgent {
                package_id: "skill:repo-review".to_string(),
                export_name: "reviewer".to_string(),
            },
            launch: SpawnLaunchInputs {
                task: "Review the repository".to_string(),
                ..SpawnLaunchInputs::default()
            },
            handles: Vec::new(),
            runtime_overrides: SpawnRuntimeOverrides::default(),
            delegated: None,
        };

        let value = serde_json::to_value(&spec).unwrap();
        assert_eq!(value["target"]["kind"], "package_child_agent");
        assert_eq!(value["target"]["package_id"], "skill:repo-review");
        assert_eq!(value["target"]["export_name"], "reviewer");

        let parsed: SpawnSpec = serde_json::from_value(value).unwrap();
        assert_eq!(
            parsed.target,
            SpawnTarget::PackageChildAgent {
                package_id: "skill:repo-review".to_string(),
                export_name: "reviewer".to_string(),
            }
        );
    }

    #[test]
    fn spawn_launch_inputs_reject_legacy_budget_tokens() {
        let payload = serde_json::json!({
            "task": "Review the repository",
            "budget_tokens": 2048
        });

        let err = serde_json::from_value::<SpawnLaunchInputs>(payload).unwrap_err();
        assert!(err.to_string().contains("budget_tokens"));
    }
}
