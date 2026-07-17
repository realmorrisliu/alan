use super::{
    TerminalExecutableAvailability, TerminalProfileDefinition, TerminalProfileDocument,
    TerminalProfileLaunch, TerminalProfileLaunchKind, normalized_non_empty,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Terminal Profile resolution state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TerminalProfileResolutionState {
    /// No profile was requested or applied.
    Absent,
    /// Profile resolved.
    Resolved,
    /// Requested profile is missing.
    Missing {
        /// Requested profile id.
        requested_id: String,
    },
    /// Requested profile is unavailable.
    Unavailable {
        /// Requested profile id.
        requested_id: String,
        /// Stable reason.
        reason: String,
    },
}

impl TerminalProfileResolutionState {
    /// Environment string value.
    pub fn environment_value(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Resolved => "resolved",
            Self::Missing { .. } => "missing",
            Self::Unavailable { .. } => "unavailable",
        }
    }
}

/// Launch strategy for terminal launch intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalLaunchStrategy {
    /// ALAN_SHELL_BOOT_COMMAND controls startup.
    ShellCommandEnv,
    /// ALAN_SHELL_LOGIN_SHELL controls startup.
    LoginShellOverride,
    /// SHELL environment controls startup.
    LoginShellEnv,
    /// Fallback shell controls startup.
    LoginShellFallback,
    /// Terminal Profile sudo user launch.
    TerminalProfileSudoUser,
    /// Terminal Profile sudo root launch.
    TerminalProfileSudoRoot,
    /// Terminal Profile helper-managed user launch.
    TerminalProfileManagedUser,
    /// Terminal Profile custom command launch.
    TerminalProfileCustomCommand,
}

/// Platform-supplied launch environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLaunchEnvironment {
    /// Environment values.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

impl TerminalLaunchEnvironment {
    fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

/// Platform-neutral launch intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLaunchIntent {
    /// Launch strategy.
    pub strategy: TerminalLaunchStrategy,
    /// Executable path, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    /// Process launch path.
    pub launch_path: String,
    /// Process arguments.
    pub arguments: Vec<String>,
    /// Boot command string.
    pub boot_command: String,
    /// Optional terminal surface command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_command: Option<String>,
    /// User-facing summary.
    pub summary: String,
    /// Redacted detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Resolved Terminal Profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_profile: Option<TerminalProfileDefinition>,
    /// Terminal Profile resolution state.
    pub terminal_profile_state: TerminalProfileResolutionState,
    /// Working directory provided by the profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// Environment projection for platform adapters.
    pub profile_environment: BTreeMap<String, String>,
}

struct TerminalLaunchIntentSeed {
    strategy: TerminalLaunchStrategy,
    executable_path: Option<String>,
    launch_path: String,
    arguments: Vec<String>,
    boot_command: String,
    surface_command: Option<String>,
    summary: String,
    detail: Option<String>,
    terminal_profile: Option<TerminalProfileDefinition>,
    terminal_profile_state: TerminalProfileResolutionState,
    working_directory: Option<String>,
}

impl TerminalLaunchIntent {
    /// Resolves a terminal launch intent.
    pub fn resolve(
        terminal_profile_reference: Option<&str>,
        terminal_profiles: Option<&TerminalProfileDocument>,
        availability: &TerminalExecutableAvailability,
        environment: &TerminalLaunchEnvironment,
    ) -> Self {
        let override_command = normalized_non_empty(environment.value("ALAN_SHELL_BOOT_COMMAND"));
        let override_shell =
            normalized_executable_path(environment.value("ALAN_SHELL_LOGIN_SHELL"), availability);
        if override_command.is_some() || override_shell.is_some() {
            return Self::resolve_shell(availability, environment);
        }

        let fallback;
        let document = match terminal_profiles {
            Some(document) => document,
            None => {
                fallback = TerminalProfileDocument::fallback();
                &fallback
            }
        };
        let requested_id = terminal_profile_reference.map(str::trim);
        let profile = requested_id
            .and_then(|id| document.profile(Some(id)))
            .or_else(|| {
                if requested_id.is_some_and(|id| !id.is_empty()) {
                    None
                } else {
                    document.default_profile()
                }
            });

        let Some(profile) = profile else {
            let state = requested_id
                .map(|requested_id| TerminalProfileResolutionState::Missing {
                    requested_id: requested_id.to_string(),
                })
                .unwrap_or(TerminalProfileResolutionState::Absent);
            return Self::resolve_shell(availability, environment)
                .with_terminal_profile(None, state);
        };

        match &profile.launch {
            TerminalProfileLaunch::LoginShell => Self::resolve_shell(availability, environment)
                .with_terminal_profile(
                    Some(profile.clone()),
                    TerminalProfileResolutionState::Resolved,
                ),
            TerminalProfileLaunch::SudoUser { unix_user } => Self::profile_command(
                profile,
                TerminalLaunchStrategy::TerminalProfileSudoUser,
                "/usr/bin/sudo",
                vec!["-iu".to_string(), unix_user.clone()],
                availability,
                environment,
            ),
            TerminalProfileLaunch::SudoRoot => Self::profile_command(
                profile,
                TerminalLaunchStrategy::TerminalProfileSudoRoot,
                "/usr/bin/sudo",
                vec!["-i".to_string()],
                availability,
                environment,
            ),
            TerminalProfileLaunch::ManagedUser { unix_user } => {
                let boot_command = shell_join(&["managed_user".to_string(), unix_user.clone()]);
                Self::from_seed(TerminalLaunchIntentSeed {
                    strategy: TerminalLaunchStrategy::TerminalProfileManagedUser,
                    executable_path: None,
                    launch_path: String::new(),
                    arguments: Vec::new(),
                    boot_command,
                    surface_command: None,
                    summary: format!("Launching pane with Managed User {}", profile.title),
                    detail: Some(profile.redacted_display_detail()),
                    terminal_profile: Some(profile.clone()),
                    terminal_profile_state: TerminalProfileResolutionState::Resolved,
                    working_directory: normalized_non_empty(
                        profile.default_working_directory.as_deref(),
                    ),
                })
            }
            TerminalProfileLaunch::CustomCommand { command } => {
                let executable_path = "/bin/zsh";
                if !availability.is_executable(executable_path) {
                    return Self::resolve_shell(availability, environment).with_terminal_profile(
                        Some(profile.clone()),
                        TerminalProfileResolutionState::Unavailable {
                            requested_id: profile.id.clone(),
                            reason: "missing_executable".to_string(),
                        },
                    );
                }
                Self::from_seed(TerminalLaunchIntentSeed {
                    strategy: TerminalLaunchStrategy::TerminalProfileCustomCommand,
                    executable_path: Some(executable_path.to_string()),
                    launch_path: executable_path.to_string(),
                    arguments: vec!["-lc".to_string(), command.clone()],
                    boot_command: command.clone(),
                    surface_command: Some(command.clone()),
                    summary: format!("Launching pane with Terminal Profile {}", profile.title),
                    detail: Some(profile.redacted_display_detail()),
                    terminal_profile: Some(profile.clone()),
                    terminal_profile_state: TerminalProfileResolutionState::Resolved,
                    working_directory: normalized_non_empty(
                        profile.default_working_directory.as_deref(),
                    ),
                })
            }
        }
    }

    fn profile_command(
        profile: &TerminalProfileDefinition,
        strategy: TerminalLaunchStrategy,
        executable_path: &str,
        arguments: Vec<String>,
        availability: &TerminalExecutableAvailability,
        environment: &TerminalLaunchEnvironment,
    ) -> Self {
        if !availability.is_executable(executable_path) {
            return Self::resolve_shell(availability, environment).with_terminal_profile(
                Some(profile.clone()),
                TerminalProfileResolutionState::Unavailable {
                    requested_id: profile.id.clone(),
                    reason: "missing_executable".to_string(),
                },
            );
        }
        let boot_command = shell_join(
            std::iter::once(executable_path.to_string())
                .chain(arguments.iter().cloned())
                .collect::<Vec<_>>()
                .as_slice(),
        );
        Self::from_seed(TerminalLaunchIntentSeed {
            strategy,
            executable_path: Some(executable_path.to_string()),
            launch_path: executable_path.to_string(),
            arguments,
            boot_command: boot_command.clone(),
            surface_command: Some(boot_command),
            summary: format!("Launching pane with Terminal Profile {}", profile.title),
            detail: Some(profile.redacted_display_detail()),
            terminal_profile: Some(profile.clone()),
            terminal_profile_state: TerminalProfileResolutionState::Resolved,
            working_directory: normalized_non_empty(profile.default_working_directory.as_deref()),
        })
    }

    fn resolve_shell(
        availability: &TerminalExecutableAvailability,
        environment: &TerminalLaunchEnvironment,
    ) -> Self {
        if let Some(command) = normalized_non_empty(environment.value("ALAN_SHELL_BOOT_COMMAND")) {
            return Self::from_seed(TerminalLaunchIntentSeed {
                strategy: TerminalLaunchStrategy::ShellCommandEnv,
                executable_path: None,
                launch_path: "/bin/zsh".to_string(),
                arguments: vec!["-lc".to_string(), command.clone()],
                boot_command: command.clone(),
                surface_command: Some(command.clone()),
                summary: "Launching pane from ALAN_SHELL_BOOT_COMMAND".to_string(),
                detail: Some(command),
                terminal_profile: None,
                terminal_profile_state: TerminalProfileResolutionState::Absent,
                working_directory: None,
            });
        }

        if let Some(shell) =
            normalized_executable_path(environment.value("ALAN_SHELL_LOGIN_SHELL"), availability)
        {
            return Self::direct_shell(
                TerminalLaunchStrategy::LoginShellOverride,
                shell,
                "Launching pane from ALAN_SHELL_LOGIN_SHELL",
                false,
            );
        }

        if let Some(shell) = normalized_executable_path(environment.value("SHELL"), availability) {
            return Self::direct_shell(
                TerminalLaunchStrategy::LoginShellEnv,
                shell,
                "Launching pane from SHELL",
                true,
            );
        }

        let fallback_shell = ["/bin/zsh", "/bin/bash", "/bin/sh"]
            .into_iter()
            .find(|path| availability.is_executable(path))
            .unwrap_or("/bin/zsh")
            .to_string();
        Self::direct_shell(
            TerminalLaunchStrategy::LoginShellFallback,
            fallback_shell,
            "Launching pane with the default login shell",
            true,
        )
    }

    fn direct_shell(
        strategy: TerminalLaunchStrategy,
        executable_path: String,
        summary: &str,
        inherit_surface_command: bool,
    ) -> Self {
        let arguments = vec!["-l".to_string()];
        let boot_command = shell_join(&[executable_path.clone(), "-l".to_string()]);
        Self::from_seed(TerminalLaunchIntentSeed {
            strategy,
            executable_path: Some(executable_path.clone()),
            launch_path: executable_path.clone(),
            arguments,
            boot_command: boot_command.clone(),
            surface_command: (!inherit_surface_command).then_some(boot_command),
            summary: summary.to_string(),
            detail: Some(executable_path),
            terminal_profile: None,
            terminal_profile_state: TerminalProfileResolutionState::Absent,
            working_directory: None,
        })
    }

    fn from_seed(seed: TerminalLaunchIntentSeed) -> Self {
        let mut profile_environment = BTreeMap::new();
        profile_environment.insert(
            "ALAN_TERMINAL_PROFILE_STATE".to_string(),
            seed.terminal_profile_state.environment_value().to_string(),
        );
        if let TerminalProfileResolutionState::Missing { requested_id }
        | TerminalProfileResolutionState::Unavailable { requested_id, .. } =
            &seed.terminal_profile_state
        {
            profile_environment.insert(
                "ALAN_TERMINAL_PROFILE_REQUESTED_ID".to_string(),
                requested_id.clone(),
            );
        }
        if let Some(profile) = &seed.terminal_profile {
            profile_environment.insert("ALAN_TERMINAL_PROFILE_ID".to_string(), profile.id.clone());
            profile_environment.insert(
                "ALAN_TERMINAL_PROFILE_KIND".to_string(),
                profile.launch.kind().environment_value().to_string(),
            );
            if let TerminalProfileLaunch::ManagedUser { unix_user } = &profile.launch {
                profile_environment
                    .insert("ALAN_MANAGED_USER_ACCOUNT".to_string(), unix_user.clone());
            }
        }

        Self {
            strategy: seed.strategy,
            executable_path: seed.executable_path,
            launch_path: seed.launch_path,
            arguments: seed.arguments,
            boot_command: seed.boot_command,
            surface_command: seed.surface_command,
            summary: seed.summary,
            detail: seed.detail,
            terminal_profile: seed.terminal_profile,
            terminal_profile_state: seed.terminal_profile_state,
            working_directory: seed.working_directory,
            profile_environment,
        }
    }

    fn with_terminal_profile(
        mut self,
        profile: Option<TerminalProfileDefinition>,
        state: TerminalProfileResolutionState,
    ) -> Self {
        let working_directory = profile
            .as_ref()
            .and_then(|profile| normalized_non_empty(profile.default_working_directory.as_deref()));
        self.terminal_profile = profile;
        self.terminal_profile_state = state;
        self.working_directory = working_directory;
        self.profile_environment = Self::from_seed(TerminalLaunchIntentSeed {
            strategy: self.strategy,
            executable_path: self.executable_path.clone(),
            launch_path: self.launch_path.clone(),
            arguments: self.arguments.clone(),
            boot_command: self.boot_command.clone(),
            surface_command: self.surface_command.clone(),
            summary: self.summary.clone(),
            detail: self.detail.clone(),
            terminal_profile: self.terminal_profile.clone(),
            terminal_profile_state: self.terminal_profile_state.clone(),
            working_directory: self.working_directory.clone(),
        })
        .profile_environment;
        self
    }
}

impl TerminalProfileLaunchKind {
    fn environment_value(self) -> &'static str {
        match self {
            TerminalProfileLaunchKind::LoginShell => "login_shell",
            TerminalProfileLaunchKind::SudoUser => "sudo_user",
            TerminalProfileLaunchKind::SudoRoot => "sudo_root",
            TerminalProfileLaunchKind::ManagedUser => "managed_user",
            TerminalProfileLaunchKind::CustomCommand => "custom_command",
        }
    }
}

fn normalized_executable_path(
    raw_path: Option<&str>,
    availability: &TerminalExecutableAvailability,
) -> Option<String> {
    let path = normalized_non_empty(raw_path)?;
    availability.is_executable(&path).then_some(path)
}

fn shell_join(values: &[String]) -> String {
    values
        .iter()
        .map(|value| shell_quoted(value))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Quotes a shell argument using the Swift boot-profile convention.
pub fn shell_quoted(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}
