use super::{
    ResolvedSkillExecution, SkillCapabilities, SkillCompatibility, SkillExecutionUnresolvedReason,
    SkillMetadata, SkillTypedDependency, SkillsError,
};
use semver::{BuildMetadata, Version};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

/// Host/runtime capability context used to decide if a skill is runnable now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillHostCapabilities {
    pub alan_version: String,
    pub tools: BTreeSet<String>,
    pub executables: BTreeSet<String>,
    pub env_vars: BTreeSet<String>,
    pub delegated_skill_invocation_supported: bool,
}

impl Default for SkillHostCapabilities {
    fn default() -> Self {
        Self {
            alan_version: env!("CARGO_PKG_VERSION").to_string(),
            tools: BTreeSet::new(),
            executables: BTreeSet::new(),
            env_vars: BTreeSet::new(),
            delegated_skill_invocation_supported: false,
        }
    }
}

impl SkillHostCapabilities {
    pub fn with_tools<I, S>(tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            tools: tools.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    pub fn extend_tools<I, S>(&mut self, tools: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tools.extend(tools.into_iter().map(Into::into));
    }

    pub fn with_executables<I, S>(mut self, executables: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extend_executables(executables);
        self
    }

    pub fn extend_executables<I, S>(&mut self, executables: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.executables.extend(
            executables
                .into_iter()
                .map(Into::into)
                .map(|name: String| normalize_executable_name_for_host(&name)),
        );
    }

    pub fn with_path_executables<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.extend_executables_from_path_dirs(paths);
        self
    }

    pub fn with_env_vars<I, S>(mut self, env_vars: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extend_env_vars(env_vars);
        self
    }

    pub fn extend_env_vars<I, S>(&mut self, env_vars: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.env_vars.extend(
            env_vars
                .into_iter()
                .map(Into::into)
                .map(|name: String| normalize_env_var_name_for_host(&name)),
        );
    }

    pub(super) fn extend_env_var_values<I, K, V>(&mut self, env_vars: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env_vars
            .extend(env_vars.into_iter().filter_map(|(name, value)| {
                let value = value.into();
                if value.is_empty() {
                    None
                } else {
                    let name: String = name.into();
                    Some(normalize_env_var_name_for_host(&name))
                }
            }));
    }

    pub fn with_process_env(mut self) -> Self {
        self.extend_env_var_values(std::env::vars_os().map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        }));
        self
    }

    pub fn with_process_path_executables(mut self) -> Self {
        if let Some(path) = std::env::var_os("PATH") {
            self = self.with_path_executables(std::env::split_paths(&path));
        }
        self
    }

    pub fn supports_delegated_skill_invocation(&self) -> bool {
        self.delegated_skill_invocation_supported
    }

    pub fn supports_required_tool(&self, tool: &str) -> bool {
        match tool {
            "invoke_delegated_skill" => self.supports_delegated_skill_invocation(),
            _ if self.tools.contains(tool) => true,
            _ if is_reserved_runtime_tool_name(tool) => false,
            _ => self
                .executables
                .contains(&normalize_executable_name_for_host(tool)),
        }
    }

    pub fn supports_env_var(&self, name: &str) -> bool {
        self.env_vars
            .contains(&normalize_env_var_name_for_host(name))
    }

    pub fn supports_runtime_capability(&self, name: &str) -> bool {
        match name {
            "delegated_skill_invocation" => self.supports_delegated_skill_invocation(),
            _ => false,
        }
    }

    pub fn with_delegated_skill_invocation(mut self) -> Self {
        self.delegated_skill_invocation_supported = true;
        self.tools.insert("invoke_delegated_skill".to_string());
        self
    }

    pub fn with_runtime_defaults(mut self) -> Self {
        self.extend_tools([
            "request_confirmation",
            "request_mount",
            "request_user_input",
            "update_plan",
        ]);
        self
    }

    pub(super) fn extend_executables_from_path_dirs<I, P>(&mut self, paths: I)
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        for dir in paths {
            let Ok(entries) = std::fs::read_dir(dir.as_ref()) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = host_executable_name(&path) {
                    self.executables.insert(name);
                }
            }
        }
    }
}

/// Build the canonical skill-availability surface shared by runtime prompt
/// assembly and host catalog inspection.
pub fn build_skill_host_capabilities<I, S>(
    tools: I,
    delegated_skill_invocation_supported: bool,
) -> SkillHostCapabilities
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let path_dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    build_skill_host_capabilities_with_path_dirs(
        tools,
        path_dirs,
        delegated_skill_invocation_supported,
    )
}

pub fn build_skill_host_capabilities_with_path_dirs<I, S, J, P>(
    tools: I,
    path_dirs: J,
    delegated_skill_invocation_supported: bool,
) -> SkillHostCapabilities
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    J: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut capabilities = SkillHostCapabilities::with_tools(tools)
        .with_process_env()
        .with_path_executables(path_dirs)
        .with_runtime_defaults();
    if delegated_skill_invocation_supported {
        capabilities = capabilities.with_delegated_skill_invocation();
    }
    capabilities
}

fn normalize_env_var_name_for_host(name: &str) -> String {
    normalize_env_var_name(name, cfg!(windows))
}

pub(super) fn normalize_env_var_name(name: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        name.to_ascii_uppercase()
    } else {
        name.to_string()
    }
}

fn host_executable_name(path: &Path) -> Option<String> {
    if !path.is_file() || !is_host_executable(path) {
        return None;
    }

    #[cfg(windows)]
    {
        let allowed_extensions = allowed_windows_executable_extensions();
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_uppercase())?;
        if !allowed_extensions.contains(&extension) {
            return None;
        }

        return path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| normalize_executable_name_for_host(stem));
    }

    #[cfg(not(windows))]
    {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(normalize_executable_name_for_host)
    }
}

pub(super) fn normalize_executable_name_for_host(name: &str) -> String {
    normalize_executable_name(name, cfg!(windows))
}

pub(super) fn normalize_executable_name(name: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

fn is_reserved_runtime_tool_name(tool: &str) -> bool {
    matches!(
        tool,
        "read_file"
            | "write_file"
            | "edit_file"
            | "bash"
            | "grep"
            | "glob"
            | "list_dir"
            | "request_confirmation"
            | "request_mount"
            | "request_user_input"
            | "update_plan"
            | "invoke_delegated_skill"
    )
}

#[cfg(unix)]
fn is_host_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_host_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(windows)]
fn allowed_windows_executable_extensions() -> BTreeSet<String> {
    let pathext = std::env::var_os("PATHEXT")
        .unwrap_or_else(|| std::ffi::OsString::from(".COM;.EXE;.BAT;.CMD"));
    pathext
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .map(|extension| extension.trim_start_matches('.'))
        .filter(|extension: &&str| !extension.is_empty())
        .map(|extension| extension.to_ascii_uppercase())
        .collect()
}

/// Reason a skill is not currently runnable in the active host/runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillAvailabilityIssue {
    MissingDependencies(Vec<SkillDependencyIssue>),
    UnresolvedExecution(String),
    MinVersionNotMet { required: String, current: String },
    InvalidMinVersion(String),
}

impl std::fmt::Display for SkillAvailabilityIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillAvailabilityIssue::MissingDependencies(dependencies) => {
                write!(
                    f,
                    "missing dependencies: {}",
                    dependencies
                        .iter()
                        .map(SkillDependencyIssue::render_label)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            SkillAvailabilityIssue::UnresolvedExecution(detail) => {
                write!(f, "unresolved execution: {detail}")
            }
            SkillAvailabilityIssue::MinVersionNotMet { required, current } => {
                write!(f, "requires alan >= {required} (current: {current})")
            }
            SkillAvailabilityIssue::InvalidMinVersion(version) => {
                write!(f, "invalid compatibility.min_version: {version}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillDependencyIssue {
    MissingEnvVar {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    MissingTool {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    MissingRuntimeCapability {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl SkillDependencyIssue {
    pub fn render_label(&self) -> String {
        match self {
            Self::MissingEnvVar { name, .. } => format!("env_var:{name}"),
            Self::MissingTool { name, .. } => format!("tool:{name}"),
            Self::MissingRuntimeCapability { name, .. } => format!("runtime_capability:{name}"),
        }
    }
}

/// Runtime-facing availability state for a selected skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SkillAvailabilityState {
    Available,
    Unavailable { issues: Vec<SkillAvailabilityIssue> },
}

impl SkillAvailabilityState {
    pub fn available() -> Self {
        Self::Available
    }

    pub fn from_issues(issues: Vec<SkillAvailabilityIssue>) -> Self {
        if issues.is_empty() {
            Self::Available
        } else {
            Self::Unavailable { issues }
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn render_label(&self) -> String {
        match self {
            Self::Available => "available".to_string(),
            Self::Unavailable { issues } => {
                format!("unavailable ({})", format_skill_availability_issues(issues))
            }
        }
    }
}

/// Validates skill capabilities declaration.
/// Returns Ok(()) if valid, Err otherwise.
pub fn validate_capabilities(cap: &SkillCapabilities) -> Result<(), SkillsError> {
    // Validate tool names (should not contain spaces or special chars)
    for tool in &cap.required_tools {
        validate_tool_name(tool)?;
    }

    Ok(())
}

pub fn validate_skill_compatibility(compatibility: &SkillCompatibility) -> Result<(), SkillsError> {
    for dependency in &compatibility.dependencies {
        validate_skill_dependency(dependency)?;
    }
    Ok(())
}

fn validate_skill_dependency(dependency: &SkillTypedDependency) -> Result<(), SkillsError> {
    match dependency {
        SkillTypedDependency::EnvVar { name, .. } => {
            validate_non_empty_dependency_name("environment variable", name)?;
            if name.contains('=') {
                return Err(SkillsError::InvalidCapabilities(format!(
                    "Invalid environment variable name: {}",
                    name
                )));
            }
        }
        SkillTypedDependency::Tool { name, .. } => validate_tool_name(name)?,
        SkillTypedDependency::RuntimeCapability { name, .. } => {
            validate_non_empty_dependency_name("dependency", name)?;
        }
    }

    Ok(())
}

fn validate_tool_name(name: &str) -> Result<(), SkillsError> {
    validate_non_empty_dependency_name("tool", name)?;
    if name.contains(' ') || name.contains('<') || name.contains('>') {
        return Err(SkillsError::InvalidCapabilities(format!(
            "Invalid tool name: {}",
            name
        )));
    }
    Ok(())
}

fn validate_non_empty_dependency_name(kind: &str, name: &str) -> Result<(), SkillsError> {
    if name.trim().is_empty() || name.chars().any(char::is_whitespace) {
        return Err(SkillsError::InvalidCapabilities(format!(
            "Invalid {kind} name: {}",
            name
        )));
    }
    Ok(())
}

/// Collect every dependency declared by portable Skill metadata, including
/// legacy `capabilities.required_tools`, with duplicate identities removed.
pub fn skill_declared_dependencies(metadata: &SkillMetadata) -> Vec<SkillTypedDependency> {
    let mut dependencies = Vec::new();
    let mut seen = BTreeSet::new();

    let mut push_dependency = |dependency: SkillTypedDependency| {
        if seen.insert(dependency.identity_key()) {
            dependencies.push(dependency);
        }
    };

    if let Some(capabilities) = metadata.capabilities.as_ref() {
        for tool in &capabilities.required_tools {
            push_dependency(SkillTypedDependency::Tool {
                name: tool.clone(),
                description: None,
            });
        }
    }

    for dependency in &metadata.compatibility.dependencies {
        push_dependency(dependency.clone());
    }

    dependencies
}

pub fn skill_availability_issues(
    metadata: &SkillMetadata,
    host_capabilities: &SkillHostCapabilities,
) -> Vec<SkillAvailabilityIssue> {
    let mut issues = Vec::new();

    let missing_dependencies: Vec<SkillDependencyIssue> = skill_declared_dependencies(metadata)
        .into_iter()
        .filter_map(|dependency| match dependency {
            SkillTypedDependency::EnvVar { name, description }
                if !host_capabilities.supports_env_var(&name) =>
            {
                Some(SkillDependencyIssue::MissingEnvVar { name, description })
            }
            SkillTypedDependency::Tool { name, description }
                if !host_capabilities.supports_required_tool(&name) =>
            {
                Some(SkillDependencyIssue::MissingTool { name, description })
            }
            SkillTypedDependency::RuntimeCapability { name, description }
                if !host_capabilities.supports_runtime_capability(&name) =>
            {
                Some(SkillDependencyIssue::MissingRuntimeCapability { name, description })
            }
            _ => None,
        })
        .collect();
    if !missing_dependencies.is_empty() {
        issues.push(SkillAvailabilityIssue::MissingDependencies(
            missing_dependencies,
        ));
    }

    if let ResolvedSkillExecution::Unresolved { reason } = &metadata.execution
        && !matches!(reason, SkillExecutionUnresolvedReason::NotResolved)
    {
        issues.push(SkillAvailabilityIssue::UnresolvedExecution(
            metadata.execution.render_label(),
        ));
    }

    if let Some(required) = metadata.compatibility.min_version.as_deref() {
        match (
            parse_semver_version(required),
            parse_semver_version(&host_capabilities.alan_version),
        ) {
            (Some(required_version), Some(current_version)) => {
                if current_version < required_version {
                    issues.push(SkillAvailabilityIssue::MinVersionNotMet {
                        required: required.to_string(),
                        current: host_capabilities.alan_version.clone(),
                    });
                }
            }
            _ => issues.push(SkillAvailabilityIssue::InvalidMinVersion(
                required.to_string(),
            )),
        }
    }

    issues
}

pub fn is_skill_available(
    metadata: &SkillMetadata,
    host_capabilities: &SkillHostCapabilities,
) -> bool {
    skill_availability_issues(metadata, host_capabilities).is_empty()
}

pub fn format_skill_availability_issues(issues: &[SkillAvailabilityIssue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Structured remediation guidance for an unavailable skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRemediation {
    pub reasons: Vec<String>,
    pub next_steps: Vec<String>,
}

pub fn skill_remediation(
    metadata: &SkillMetadata,
    host_capabilities: &SkillHostCapabilities,
) -> Option<SkillRemediation> {
    let issues = skill_availability_issues(metadata, host_capabilities);
    skill_remediation_from_issues(metadata, &issues)
}

pub fn skill_remediation_from_issues(
    metadata: &SkillMetadata,
    issues: &[SkillAvailabilityIssue],
) -> Option<SkillRemediation> {
    if issues.is_empty() {
        return None;
    }

    let reasons = issues.iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut next_steps = BTreeSet::new();

    for issue in issues {
        match issue {
            SkillAvailabilityIssue::MissingDependencies(dependencies) => {
                for dependency in dependencies {
                    match dependency {
                        SkillDependencyIssue::MissingEnvVar { name, .. } => {
                            next_steps
                                .insert(format!("Set the required environment variable: {name}."));
                        }
                        SkillDependencyIssue::MissingTool { name, .. } => {
                            next_steps
                                .insert(format!("Enable or register the required tool: {name}."));
                        }
                        SkillDependencyIssue::MissingRuntimeCapability { name, .. } => {
                            next_steps.insert(format!(
                                "Run this skill in a runtime that supports the required capability: {name}."
                            ));
                        }
                    }
                }
            }
            SkillAvailabilityIssue::UnresolvedExecution(_) => {
                next_steps.insert(
                    "Fix delegated execution metadata so this skill resolves to inline execution or a valid package-local delegate target.".to_string(),
                );
                next_steps.insert(
                    "If the skill should delegate, ensure the target launch entry exists under agents/ and matches any explicit target configuration.".to_string(),
                );
            }
            SkillAvailabilityIssue::MinVersionNotMet { required, .. } => {
                next_steps.insert(format!("Upgrade alan to version {required} or newer."));
            }
            SkillAvailabilityIssue::InvalidMinVersion(version) => {
                next_steps.insert(format!(
                    "Fix compatibility.min_version '{version}' in SKILL.md."
                ));
            }
        }
    }

    if let Some(requirements) = metadata.compatibility.requirements.as_deref()
        && !requirements.trim().is_empty()
    {
        next_steps.insert(format!("Review additional requirements: {requirements}."));
    }

    Some(SkillRemediation {
        reasons,
        next_steps: next_steps.into_iter().collect(),
    })
}

fn parse_semver_version(version: &str) -> Option<Version> {
    let mut version = Version::parse(version).ok()?;
    version.build = BuildMetadata::EMPTY;
    Some(version)
}
