//! Delegated-task requirement classification and namespace eligibility checks.
//!
//! Requirements use Alan OS namespace vocabulary. Decisions are derived from
//! the assembled namespace and are audit records, never a second authority
//! registry.

use alan_agent_protocol::{
    DelegatedCapabilityDecision, DelegatedCapabilityRecovery, DelegatedCapabilityRequirement,
    DelegatedNamespaceSummary,
};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

const SHELL_TOOL_BINDINGS: &[&str] = &["bash", "shell", "exec_command"];
const NETWORK_TOOL_BINDINGS: &[&str] = &[
    "bash",
    "gh",
    "github",
    "curl",
    "browser",
    "agent-browser",
    "web",
];
const GITHUB_TOOL_BINDINGS: &[&str] = &["bash", "gh", "github"];
const BROWSER_TOOL_BINDINGS: &[&str] = &["browser", "agent-browser", "web"];

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
    cwd: Option<&Path>,
) -> Vec<DelegatedCapabilityRequirement> {
    let normalized = task.to_ascii_lowercase();
    let words = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<BTreeSet<_>>();
    let mount_path = cwd.map(Path::to_path_buf);
    let mut requirements = BTreeSet::from([
        DelegatedCapabilityRequirement::MountRead {
            path: mount_path.clone(),
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
    let mount_write = contains_any_word(
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
    let shell = mount_write
        || contains_any_word(
            &words,
            &[
                "shell", "command", "build", "test", "lint", "cargo", "just", "bash",
            ],
        );

    if mount_write {
        requirements.insert(DelegatedCapabilityRequirement::MountWrite { path: mount_path });
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
        return DelegatedCapabilityDecision {
            requirements: requirements.to_vec(),
            namespace: child_namespace,
            unsatisfied,
            recovery: DelegatedCapabilityRecovery::Narrowed,
            narrowed_task: Some(format!(
                "[NARROWED DELEGATION SCOPE]\n\
                 Work only from mounted Alan OS paths and return inspection or guidance; do not perform withheld operations.\n\
                 Withheld capabilities: {withheld}.\n\
                 Do not infer or substitute unavailable Host resources. The parent remains responsible for the withheld part.\n\
                 Original task: {original_task}"
            )),
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
    writable_mounts: Vec<String>,
    bin_bindings: Vec<String>,
    cwd: Option<PathBuf>,
    llm_connection: Option<String>,
) -> DelegatedNamespaceSummary {
    DelegatedNamespaceSummary {
        mounts,
        writable_mounts,
        bin_bindings,
        cwd,
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
        DelegatedCapabilityRequirement::MountRead { path } => {
            mount_path_is_covered(path.as_deref(), &namespace.mounts)
        }
        DelegatedCapabilityRequirement::MountWrite { path } => {
            mount_path_is_covered(path.as_deref(), &namespace.writable_mounts)
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
            !namespace.writable_mounts.is_empty()
                || contains_binding(&tool_names, NETWORK_TOOL_BINDINGS)
        }
    }
}

fn mount_path_is_covered(required: Option<&Path>, mounts: &[String]) -> bool {
    let Some(required) = required.and_then(normalize_absolute_requirement_path) else {
        return required.is_none() && !mounts.is_empty();
    };
    mounts.iter().any(|mounted| {
        normalize_absolute_requirement_path(Path::new(mounted))
            .is_some_and(|mounted| required.starts_with(mounted))
    })
}

fn normalize_absolute_requirement_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Some(normalized)
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
        &DelegatedCapabilityRequirement::MountRead { path: None },
        namespace,
    ) && namespace_satisfies(&DelegatedCapabilityRequirement::LlmConnection, namespace)
        && unsatisfied.iter().all(|requirement| {
            matches!(
                requirement,
                DelegatedCapabilityRequirement::MountWrite { .. }
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

    fn summary(tools: &[&str], writable: bool) -> DelegatedNamespaceSummary {
        namespace_summary_from_bindings(
            vec!["/agent".into(), "/mnt/source".into(), "/mnt/llm".into()],
            writable.then(|| "/mnt/source".into()).into_iter().collect(),
            tools.iter().map(|tool| format!("/bin/{tool}")).collect(),
            Some("/mnt/source".into()),
            Some("default".into()),
        )
    }

    #[test]
    fn classifies_mount_authority_in_namespace_vocabulary() {
        let requirements = classify_delegated_task_requirements(
            "Inspect the repository, implement the fix, and run tests",
            Some(Path::new("/mnt/source")),
        );
        assert!(
            requirements.contains(&DelegatedCapabilityRequirement::MountRead {
                path: Some("/mnt/source".into()),
            })
        );
        assert!(
            requirements.contains(&DelegatedCapabilityRequirement::MountWrite {
                path: Some("/mnt/source".into()),
            })
        );
    }

    #[test]
    fn read_only_mount_cannot_satisfy_write() {
        let requirements = vec![DelegatedCapabilityRequirement::MountWrite {
            path: Some("/mnt/source".into()),
        }];
        let decision = evaluate_delegated_namespace(
            "edit source",
            &requirements,
            summary(&["read_file"], false),
            &summary(&["write_file"], true),
        );
        assert_eq!(decision.recovery, DelegatedCapabilityRecovery::ParentPath);
    }

    #[test]
    fn writable_mount_and_tool_satisfy_task() {
        let requirements = classify_delegated_task_requirements(
            "implement the fix",
            Some(Path::new("/mnt/source")),
        );
        let namespace = summary(&["read_file", "write_file", "bash"], true);
        let decision = evaluate_delegated_namespace(
            "implement the fix",
            &requirements,
            namespace.clone(),
            &namespace,
        );
        assert_eq!(decision.recovery, DelegatedCapabilityRecovery::Satisfied);
    }
}
