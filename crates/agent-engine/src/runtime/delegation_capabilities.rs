//! Delegated-task requirement classification and namespace eligibility checks.
//!
//! Requirements use namespace vocabulary. Decisions are derived from the
//! assembled namespace summary and are persisted only as bounded audit data;
//! they are not an authority registry.

use alan_agent_protocol::{
    DelegatedCapabilityDecision, DelegatedCapabilityRecovery, DelegatedCapabilityRequirement,
    DelegatedNamespaceSummary, DelegatedWorkspaceAccess,
};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

const READ_TOOL_BINDINGS: &[&str] = &["read_file", "grep", "glob", "list_dir", "bash"];
const WRITE_TOOL_BINDINGS: &[&str] = &["write_file", "edit_file", "bash"];
const SHELL_TOOL_BINDINGS: &[&str] = &["bash", "shell", "exec_command"];
const NETWORK_TOOL_BINDINGS: &[&str] = &["gh", "github", "curl", "browser", "agent-browser", "web"];
const GITHUB_TOOL_BINDINGS: &[&str] = &["gh", "github"];
const BROWSER_TOOL_BINDINGS: &[&str] = &["browser", "agent-browser", "web"];

/// Error returned before `/proc/clone` when the original delegated task cannot
/// run in the assembled namespace and no explicit narrowing was selected.
#[derive(Debug, Clone)]
pub(crate) struct DelegatedSpawnRejected {
    pub decision: DelegatedCapabilityDecision,
}

impl fmt::Display for DelegatedSpawnRejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let missing = self
            .decision
            .unsatisfied
            .iter()
            .map(DelegatedCapabilityRequirement::label)
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            formatter,
            "delegated namespace cannot satisfy the original task; missing: {missing}; recovery: {:?}",
            self.decision.recovery
        )
    }
}

impl std::error::Error for DelegatedSpawnRejected {}

/// Mechanically classify the first bounded requirement vocabulary.
pub(crate) fn classify_delegated_task_requirements(
    task: &str,
    workspace_root: Option<&Path>,
) -> Vec<DelegatedCapabilityRequirement> {
    let normalized = task.to_ascii_lowercase();
    let words = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<BTreeSet<_>>();
    let workspace_path = workspace_root.map(Path::to_path_buf);
    let mut requirements = BTreeSet::from([
        DelegatedCapabilityRequirement::WorkspaceRead {
            path: workspace_path.clone(),
        },
        DelegatedCapabilityRequirement::LlmConnection,
    ]);

    let github = contains_any_phrase(
        &normalized,
        &["github", "pull request", "merge request", "github issue"],
    ) || words.contains("pr")
        || words.contains("gh");
    let browser = contains_any_phrase(
        &normalized,
        &["browser", "web page", "webpage", "website", "open url"],
    ) || normalized.contains("http://")
        || normalized.contains("https://");
    let network = github
        || browser
        || contains_any_word(&words, &["network", "online", "remote", "curl", "download"]);
    let workspace_write = contains_any_word(
        &words,
        &[
            "edit",
            "modify",
            "implement",
            "fix",
            "write",
            "create",
            "delete",
            "rename",
            "format",
            "commit",
            "apply",
        ],
    );
    let shell = workspace_write
        || contains_any_word(
            &words,
            &[
                "shell", "command", "build", "test", "lint", "cargo", "just", "bash",
            ],
        );

    if workspace_write {
        requirements.insert(DelegatedCapabilityRequirement::WorkspaceWrite {
            path: workspace_path,
        });
        requirements.insert(DelegatedCapabilityRequirement::SideEffects);
    }
    if shell {
        requirements.insert(DelegatedCapabilityRequirement::Shell);
    }
    if network {
        requirements.insert(DelegatedCapabilityRequirement::Network);
    }
    if github {
        requirements.insert(DelegatedCapabilityRequirement::Github);
    }
    if browser {
        requirements.insert(DelegatedCapabilityRequirement::Browser);
    }

    requirements.into_iter().collect()
}

/// Compare requirements with the assembled child namespace and choose a
/// visible recovery path without silently substituting unrelated context.
pub(crate) fn evaluate_delegated_namespace(
    original_task: &str,
    requirements: &[DelegatedCapabilityRequirement],
    child_namespace: DelegatedNamespaceSummary,
    parent_namespace: &DelegatedNamespaceSummary,
) -> DelegatedCapabilityDecision {
    let unsatisfied = unsatisfied_requirements(requirements, &child_namespace);
    if unsatisfied.is_empty() {
        return DelegatedCapabilityDecision {
            requirements: requirements.to_vec(),
            namespace: child_namespace,
            unsatisfied,
            recovery: DelegatedCapabilityRecovery::Satisfied,
            narrowed_task: None,
        };
    }

    if unsatisfied_requirements(&unsatisfied, parent_namespace).is_empty() {
        return DelegatedCapabilityDecision {
            requirements: requirements.to_vec(),
            namespace: child_namespace,
            unsatisfied,
            recovery: DelegatedCapabilityRecovery::ParentPath,
            narrowed_task: None,
        };
    }

    if can_narrow_to_local_inspection(&unsatisfied, &child_namespace) {
        let withheld = unsatisfied
            .iter()
            .map(DelegatedCapabilityRequirement::label)
            .collect::<Vec<_>>()
            .join(", ");
        let narrowed_task = format!(
            "[NARROWED DELEGATION SCOPE]\n\
             Work only from the mounted local workspace and return inspection or guidance; do not perform withheld operations.\n\
             Withheld capabilities: {withheld}.\n\
             Do not infer or substitute unrelated local context for missing external input. The parent remains responsible for the withheld part.\n\
             Original task: {original_task}"
        );
        return DelegatedCapabilityDecision {
            requirements: requirements.to_vec(),
            namespace: child_namespace,
            unsatisfied,
            recovery: DelegatedCapabilityRecovery::Narrowed,
            narrowed_task: Some(narrowed_task),
        };
    }

    let recovery = if unsatisfied
        .iter()
        .any(|requirement| matches!(requirement, DelegatedCapabilityRequirement::LlmConnection))
    {
        DelegatedCapabilityRecovery::Limitation
    } else {
        DelegatedCapabilityRecovery::AskUser
    };
    DelegatedCapabilityDecision {
        requirements: requirements.to_vec(),
        namespace: child_namespace,
        unsatisfied,
        recovery,
        narrowed_task: None,
    }
}

pub(crate) fn namespace_summary_from_bindings(
    mounts: Vec<String>,
    bin_bindings: Vec<String>,
    workspace_root: Option<PathBuf>,
    llm_connection: Option<String>,
) -> DelegatedNamespaceSummary {
    let tool_names = bin_bindings
        .iter()
        .map(|binding| binding.strip_prefix("/bin/").unwrap_or(binding))
        .collect::<BTreeSet<_>>();
    let workspace_access = if workspace_root.is_none() {
        None
    } else if WRITE_TOOL_BINDINGS
        .iter()
        .any(|name| tool_names.contains(name))
    {
        Some(DelegatedWorkspaceAccess::ReadWrite)
    } else if READ_TOOL_BINDINGS
        .iter()
        .any(|name| tool_names.contains(name))
    {
        Some(DelegatedWorkspaceAccess::ReadOnly)
    } else {
        None
    };

    DelegatedNamespaceSummary {
        mounts,
        bin_bindings,
        workspace_root,
        workspace_access,
        workspace_projection: workspace_access
            .map(|_| "host_tool_binding_compatibility".to_string()),
        llm_connection,
    }
}

fn unsatisfied_requirements(
    requirements: &[DelegatedCapabilityRequirement],
    namespace: &DelegatedNamespaceSummary,
) -> Vec<DelegatedCapabilityRequirement> {
    requirements
        .iter()
        .filter(|requirement| !namespace_satisfies(requirement, namespace))
        .cloned()
        .collect()
}

fn namespace_satisfies(
    requirement: &DelegatedCapabilityRequirement,
    namespace: &DelegatedNamespaceSummary,
) -> bool {
    let tool_names = namespace
        .bin_bindings
        .iter()
        .map(|binding| binding.strip_prefix("/bin/").unwrap_or(binding))
        .collect::<BTreeSet<_>>();
    match requirement {
        DelegatedCapabilityRequirement::WorkspaceRead { path } => {
            workspace_path_is_covered(path.as_deref(), namespace.workspace_root.as_deref())
                && namespace.workspace_access.is_some()
        }
        DelegatedCapabilityRequirement::WorkspaceWrite { path } => {
            workspace_path_is_covered(path.as_deref(), namespace.workspace_root.as_deref())
                && matches!(
                    namespace.workspace_access,
                    Some(DelegatedWorkspaceAccess::ReadWrite)
                )
        }
        DelegatedCapabilityRequirement::Shell => contains_binding(&tool_names, SHELL_TOOL_BINDINGS),
        DelegatedCapabilityRequirement::Network => {
            contains_binding(&tool_names, NETWORK_TOOL_BINDINGS)
        }
        DelegatedCapabilityRequirement::Github => {
            contains_binding(&tool_names, GITHUB_TOOL_BINDINGS)
        }
        DelegatedCapabilityRequirement::Browser => {
            contains_binding(&tool_names, BROWSER_TOOL_BINDINGS)
        }
        DelegatedCapabilityRequirement::LlmConnection => namespace.llm_connection.is_some(),
        DelegatedCapabilityRequirement::SideEffects => {
            matches!(
                namespace.workspace_access,
                Some(DelegatedWorkspaceAccess::ReadWrite)
            ) || contains_binding(&tool_names, NETWORK_TOOL_BINDINGS)
        }
    }
}

fn workspace_path_is_covered(required: Option<&Path>, mounted: Option<&Path>) -> bool {
    match (required, mounted) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(required), Some(mounted)) => required.starts_with(mounted),
    }
}

fn contains_binding(bindings: &BTreeSet<&str>, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| bindings.contains(candidate))
}

fn can_narrow_to_local_inspection(
    unsatisfied: &[DelegatedCapabilityRequirement],
    namespace: &DelegatedNamespaceSummary,
) -> bool {
    namespace_satisfies(
        &DelegatedCapabilityRequirement::WorkspaceRead { path: None },
        namespace,
    ) && namespace_satisfies(&DelegatedCapabilityRequirement::LlmConnection, namespace)
        && unsatisfied.iter().all(|requirement| {
            matches!(
                requirement,
                DelegatedCapabilityRequirement::WorkspaceWrite { .. }
                    | DelegatedCapabilityRequirement::Shell
                    | DelegatedCapabilityRequirement::Network
                    | DelegatedCapabilityRequirement::Github
                    | DelegatedCapabilityRequirement::Browser
                    | DelegatedCapabilityRequirement::SideEffects
            )
        })
}

fn contains_any_phrase(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| text.contains(phrase))
}

fn contains_any_word(words: &BTreeSet<&str>, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| words.contains(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(tools: &[&str], access_root: bool) -> DelegatedNamespaceSummary {
        namespace_summary_from_bindings(
            vec!["/agent".to_string(), "/mnt/llm".to_string()],
            tools.iter().map(|tool| format!("/bin/{tool}")).collect(),
            access_root.then(|| PathBuf::from("/tmp/repo")),
            Some("default".to_string()),
        )
    }

    #[test]
    fn classifies_github_review_in_namespace_vocabulary() {
        let requirements = classify_delegated_task_requirements(
            "Review GitHub issue #42 and the target repository",
            Some(Path::new("/tmp/repo")),
        );

        assert!(
            requirements.contains(&DelegatedCapabilityRequirement::WorkspaceRead {
                path: Some(PathBuf::from("/tmp/repo")),
            })
        );
        assert!(requirements.contains(&DelegatedCapabilityRequirement::Github));
        assert!(requirements.contains(&DelegatedCapabilityRequirement::Network));
    }

    #[test]
    fn classifies_local_inspection_without_network_or_write() {
        let requirements = classify_delegated_task_requirements(
            "Inspect the local architecture and report findings",
            Some(Path::new("/tmp/repo")),
        );

        assert!(
            requirements.contains(&DelegatedCapabilityRequirement::WorkspaceRead {
                path: Some(PathBuf::from("/tmp/repo")),
            })
        );
        assert!(requirements.contains(&DelegatedCapabilityRequirement::LlmConnection));
        assert!(!requirements.contains(&DelegatedCapabilityRequirement::Network));
        assert!(!requirements.iter().any(|requirement| matches!(
            requirement,
            DelegatedCapabilityRequirement::WorkspaceWrite { .. }
        )));
    }

    #[test]
    fn classifies_mixed_remote_edit_task() {
        let requirements = classify_delegated_task_requirements(
            "Inspect GitHub PR 7, implement the fix, and run tests",
            Some(Path::new("/tmp/repo")),
        );

        assert!(requirements.contains(&DelegatedCapabilityRequirement::Github));
        assert!(requirements.contains(&DelegatedCapabilityRequirement::Network));
        assert!(requirements.contains(&DelegatedCapabilityRequirement::Shell));
        assert!(
            requirements.contains(&DelegatedCapabilityRequirement::WorkspaceWrite {
                path: Some(PathBuf::from("/tmp/repo")),
            })
        );
        assert!(requirements.contains(&DelegatedCapabilityRequirement::SideEffects));
    }

    #[test]
    fn satisfied_spawn_passes_without_rewriting_task() {
        let child = summary(&["read_file"], true);
        let requirements = vec![
            DelegatedCapabilityRequirement::WorkspaceRead {
                path: Some(PathBuf::from("/tmp/repo")),
            },
            DelegatedCapabilityRequirement::LlmConnection,
        ];

        let decision = evaluate_delegated_namespace(
            "Inspect local files",
            &requirements,
            child,
            &DelegatedNamespaceSummary::default(),
        );

        assert_eq!(decision.recovery, DelegatedCapabilityRecovery::Satisfied);
        assert!(decision.narrowed_task.is_none());
    }

    #[test]
    fn parent_path_recovery_is_explicit() {
        let child = summary(&["read_file"], true);
        let parent = summary(&["read_file", "gh"], true);
        let requirements = vec![DelegatedCapabilityRequirement::Github];

        let decision =
            evaluate_delegated_namespace("Review GitHub issue", &requirements, child, &parent);

        assert_eq!(decision.recovery, DelegatedCapabilityRecovery::ParentPath);
        assert_eq!(decision.unsatisfied, requirements);
    }

    #[test]
    fn narrowed_spawn_names_scope_and_withheld_capability() {
        let child = summary(&["read_file"], true);
        let requirements = vec![
            DelegatedCapabilityRequirement::WorkspaceRead {
                path: Some(PathBuf::from("/tmp/repo")),
            },
            DelegatedCapabilityRequirement::Github,
        ];

        let decision = evaluate_delegated_namespace(
            "Review GitHub issue against local code",
            &requirements,
            child,
            &DelegatedNamespaceSummary::default(),
        );

        assert_eq!(decision.recovery, DelegatedCapabilityRecovery::Narrowed);
        let task = decision.narrowed_task.unwrap();
        assert!(task.contains("NARROWED DELEGATION SCOPE"));
        assert!(task.contains("Withheld capabilities: github"));
        assert!(task.contains("parent remains responsible"));
    }

    #[test]
    fn missing_workspace_declines_with_ask_user_recovery() {
        let requirements = vec![DelegatedCapabilityRequirement::WorkspaceRead {
            path: Some(PathBuf::from("/tmp/repo")),
        }];

        let decision = evaluate_delegated_namespace(
            "Inspect local files",
            &requirements,
            summary(&["read_file"], false),
            &DelegatedNamespaceSummary::default(),
        );

        assert_eq!(decision.recovery, DelegatedCapabilityRecovery::AskUser);
        assert_eq!(decision.unsatisfied, requirements);
    }
}
