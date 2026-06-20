use crate::{
    ManagedTerminalAccountPlan, ManagedTerminalAccountPlanStatus, TerminalProfileDefinition,
    TerminalProfileLaunch, TerminalProfileLaunchKind,
};
use serde::{Deserialize, Serialize};

/// Mutability of a portable settings row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellSettingsRowMutability {
    /// The row can be edited directly.
    Editable,
    /// The row is read-only.
    #[default]
    ReadOnly,
    /// The row triggers a command.
    ActionOnly,
    /// The row is deferred to a later surface.
    Deferred,
}

/// Platform-neutral settings row summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsRowSummary {
    /// Stable row id.
    pub id: String,
    /// Symbolic icon name.
    pub system_name: String,
    /// Row title.
    pub title: String,
    /// Optional secondary text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Optional value text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Row mutability.
    #[serde(default)]
    pub mutability: ShellSettingsRowMutability,
    /// Whether freeform editing is offered.
    #[serde(default)]
    pub offers_freeform_editing: bool,
}

impl ShellSettingsRowSummary {
    fn read_only(
        id: impl Into<String>,
        system_name: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            system_name: system_name.into(),
            title: title.into(),
            detail: None,
            value: None,
            mutability: ShellSettingsRowMutability::ReadOnly,
            offers_freeform_editing: false,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn with_optional_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }

    fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    fn with_mutability(mut self, mutability: ShellSettingsRowMutability) -> Self {
        self.mutability = mutability;
        self
    }
}

/// Terminal Profile settings summary input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileSettingsSummary {
    /// Profiles to display.
    pub profiles: Vec<TerminalProfileDefinition>,
    /// Default profile id.
    pub default_profile_id: String,
    /// Optional recovery message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_message: Option<String>,
}

impl TerminalProfileSettingsSummary {
    /// Returns the default profile title, if known.
    pub fn default_profile_title(&self) -> Option<&str> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.default_profile_id)
            .map(|profile| profile.title.as_str())
    }
}

/// Managed terminal account settings summary input.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountSettingsSummary {
    /// Plans to display.
    #[serde(default)]
    pub plans: Vec<ManagedTerminalAccountPlan>,
}

/// Skill summary used by settings capability rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsSkillSummary {
    /// Skill id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Whether the skill is enabled.
    pub enabled: bool,
    /// Whether implicit invocation is allowed.
    pub allow_implicit_invocation: bool,
    /// Whether the skill is available.
    pub available: bool,
}

/// Capability settings summary input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsCapabilitiesSummary {
    /// Skill summaries.
    #[serde(default)]
    pub skills: Vec<ShellSettingsSkillSummary>,
    /// Unavailable reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

/// Local host settings summary input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsLocalSummary {
    /// Bundle identifier.
    pub bundle_identifier: String,
    /// Install channel label.
    pub channel_label: String,
    /// CLI tool name.
    pub cli_tool_name: String,
    /// Daemon URL.
    pub daemon_url: String,
    /// Daemon bind address.
    pub daemon_bind_address: String,
    /// Update summary.
    pub update_summary: String,
    /// Update detail.
    pub update_detail: String,
    /// Alan home display path.
    pub alan_home_display_path: String,
    /// Application support display path.
    pub application_support_display_path: String,
    /// Global skills display path.
    pub global_skills_display_path: String,
    /// Shell control namespace.
    pub shell_control_namespace: String,
}

/// Diagnostics settings summary input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsDiagnosticsSummary {
    /// Whether diagnostics are enabled.
    pub is_enabled: bool,
    /// Retained event count.
    pub retained_event_count: u32,
    /// Stutter marker count.
    pub stutter_marker_count: u32,
    /// Last export URL/path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_export_url: Option<String>,
}

impl ShellSettingsDiagnosticsSummary {
    /// Returns Swift-compatible export detail copy.
    pub fn export_detail(&self) -> String {
        if self.retained_event_count == 0 {
            if self.is_enabled {
                return "Exports the retained local trace after activity is captured.".to_string();
            }
            return "Enable diagnostics to retain recent local performance events.".to_string();
        }

        let marker_label = if self.stutter_marker_count == 1 {
            "marker"
        } else {
            "markers"
        };
        format!(
            "{} retained events, {} stutter {}.",
            self.retained_event_count, self.stutter_marker_count, marker_label
        )
    }
}

/// Registered workspace summary supplied by the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsWorkspaceRegistryEntry {
    /// Workspace root path.
    pub path: String,
    /// Optional catalog alias or id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_identifier: Option<String>,
}

/// Settings workspace context input without filesystem access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsWorkspaceContextInput {
    /// Active working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_working_directory: Option<String>,
    /// Registered workspaces supplied by the platform.
    #[serde(default)]
    pub registered_workspaces: Vec<ShellSettingsWorkspaceRegistryEntry>,
    /// Workspace root discovered by the platform from local files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovered_workspace_root: Option<String>,
    /// Optional agent name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

/// Portable settings workspace context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettingsWorkspaceContext {
    /// Workspace directory for connection requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_workspace_dir: Option<String>,
    /// Workspace id or alias for skill catalog requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_catalog_workspace_dir: Option<String>,
    /// Reason skill catalog is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_catalog_unavailable_reason: Option<String>,
    /// Optional agent name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

impl ShellSettingsWorkspaceContext {
    /// Resolves workspace context from platform-supplied path observations.
    pub fn resolve(input: &ShellSettingsWorkspaceContextInput) -> Self {
        let agent_name = normalized_settings_value(input.agent_name.as_deref());
        let Some(active_directory) =
            normalized_settings_value(input.active_working_directory.as_deref())
        else {
            return Self {
                connection_workspace_dir: None,
                skill_catalog_workspace_dir: None,
                skill_catalog_unavailable_reason: None,
                agent_name,
            };
        };

        if let Some(registered) =
            most_specific_workspace(&input.registered_workspaces, &active_directory)
        {
            return Self {
                connection_workspace_dir: Some(registered.path.clone()),
                skill_catalog_workspace_dir: registered.catalog_identifier.clone(),
                skill_catalog_unavailable_reason: None,
                agent_name,
            };
        }

        let discovered_workspace_root =
            normalized_settings_value(input.discovered_workspace_root.as_deref());
        Self {
            connection_workspace_dir: Some(
                discovered_workspace_root
                    .clone()
                    .unwrap_or(active_directory),
            ),
            skill_catalog_workspace_dir: None,
            skill_catalog_unavailable_reason: discovered_workspace_root
                .map(|_| "Register this workspace to show workspace skills.".to_string()),
            agent_name,
        }
    }
}

/// Reusable shell settings summary row builders.
pub struct ShellSettingsSummaryRows;

impl ShellSettingsSummaryRows {
    /// Builds Terminal Profile rows.
    pub fn terminal_profile_rows(
        summary: &TerminalProfileSettingsSummary,
    ) -> Vec<ShellSettingsRowSummary> {
        let mut rows = vec![
            ShellSettingsRowSummary::read_only(
                "terminalProfilesDefault",
                "terminal",
                "Default profile",
            )
            .with_detail("Used for new terminals.")
            .with_value(summary.default_profile_title().unwrap_or("Login shell"))
            .with_mutability(ShellSettingsRowMutability::Editable),
            ShellSettingsRowSummary::read_only(
                "terminalProfilesCreate",
                "plus.circle",
                "New profile",
            )
            .with_detail("Create a local startup profile.")
            .with_value("Create…")
            .with_mutability(ShellSettingsRowMutability::ActionOnly),
        ];

        if let Some(recovery_message) = &summary.recovery_message {
            rows.push(
                ShellSettingsRowSummary::read_only(
                    "terminalProfilesRecovery",
                    "exclamationmark.triangle",
                    "Profile store recovery",
                )
                .with_detail(recovery_message)
                .with_value("Fallback active"),
            );
        }

        rows.extend(summary.profiles.iter().map(|profile| {
            let value = if profile.id == summary.default_profile_id {
                "Default".to_string()
            } else {
                launch_kind_value(profile.launch.kind()).to_string()
            };
            ShellSettingsRowSummary::read_only(
                format!("terminalProfile.{}", profile.id),
                terminal_profile_system_name(profile),
                &profile.title,
            )
            .with_optional_detail(non_repeating_detail(
                Some(profile.redacted_display_detail()),
                &profile.title,
            ))
            .with_value(value)
            .with_mutability(ShellSettingsRowMutability::Editable)
        }));

        rows.push(
            ShellSettingsRowSummary::read_only(
                "terminalProfilesSudoGuidance",
                "lock.shield",
                "Sudo behavior",
            )
            .with_detail("Prompts and passwordless sudo are controlled by macOS sudo policy.")
            .with_value("System managed"),
        );
        rows
    }

    /// Builds managed terminal account rows.
    pub fn managed_terminal_account_rows(
        summary: &ManagedTerminalAccountSettingsSummary,
    ) -> Vec<ShellSettingsRowSummary> {
        if summary.plans.is_empty() {
            return vec![
                ShellSettingsRowSummary::read_only(
                    "terminalAccountProvision",
                    "person.crop.circle.badge.plus",
                    "Managed terminal account",
                )
                .with_detail("Create a terminal-only local user for passwordless terminal entry.")
                .with_value("Preview…")
                .with_mutability(ShellSettingsRowMutability::ActionOnly),
                ShellSettingsRowSummary::read_only(
                    "terminalAccountLoginBoundary",
                    "macwindow.badge.plus",
                    "Mac login session",
                )
                .with_detail("This flow leaves the Mac login session setting unchanged.")
                .with_value("Not changed"),
            ];
        }

        summary
            .plans
            .iter()
            .map(|plan| {
                ShellSettingsRowSummary::read_only(
                    format!("terminalAccount.{}", plan.request.account_name),
                    terminal_account_system_name(plan),
                    "Managed terminal account",
                )
                .with_detail(terminal_account_detail(plan))
                .with_value(terminal_account_status_label(plan))
                .with_mutability(ShellSettingsRowMutability::ActionOnly)
            })
            .collect()
    }

    /// Builds capability rows.
    pub fn capability_rows(
        summary: &ShellSettingsCapabilitiesSummary,
    ) -> Vec<ShellSettingsRowSummary> {
        if summary.unavailable_reason.is_some() {
            return vec![unavailable_row(
                "capabilitiesUnavailable",
                "puzzlepiece.extension",
                "Skill catalog",
            )];
        }

        let total = summary.skills.len();
        let enabled = summary.skills.iter().filter(|skill| skill.enabled).count();
        vec![
            ShellSettingsRowSummary::read_only(
                "capabilitiesAvailable",
                "puzzlepiece.extension",
                "Skill catalog",
            )
            .with_value(if total == 0 {
                "No skills".to_string()
            } else {
                format!("{enabled} of {total}")
            }),
        ]
    }

    /// Builds local host and diagnostics rows.
    pub fn local_rows(
        local: &ShellSettingsLocalSummary,
        diagnostics: &ShellSettingsDiagnosticsSummary,
    ) -> Vec<ShellSettingsRowSummary> {
        vec![
            ShellSettingsRowSummary::read_only("appIdentity", "app", "Bundle ID")
                .with_value(local.bundle_identifier.as_str()),
            ShellSettingsRowSummary::read_only("installChannel", "shippingbox", "Channel")
                .with_value(local.channel_label.as_str()),
            ShellSettingsRowSummary::read_only("cliTool", "terminal", "Command line tool")
                .with_value(local.cli_tool_name.as_str()),
            ShellSettingsRowSummary::read_only("daemonEndpoint", "server.rack", "Daemon endpoint")
                .with_value(local.daemon_url.as_str()),
            ShellSettingsRowSummary::read_only("updates", "arrow.down.circle", "Updates")
                .with_value(local.update_summary.as_str()),
            ShellSettingsRowSummary::read_only("dataRoot", "folder", "Alan home")
                .with_value(local.alan_home_display_path.as_str()),
            ShellSettingsRowSummary::read_only(
                "publicSkills",
                "folder.badge.gearshape",
                "Skill packages",
            )
            .with_value(local.global_skills_display_path.as_str()),
            ShellSettingsRowSummary::read_only(
                "applicationSupport",
                "externaldrive",
                "Shell state",
            )
            .with_value(local.application_support_display_path.as_str()),
            ShellSettingsRowSummary::read_only(
                "shellControl",
                "point.3.connected.trianglepath.dotted",
                "Control namespace",
            )
            .with_value(local.shell_control_namespace.as_str()),
            ShellSettingsRowSummary::read_only(
                "performanceDiagnostics",
                "speedometer",
                "Performance Trace",
            )
            .with_detail("Local performance trace. Terminal content is not recorded.")
            .with_value(if diagnostics.is_enabled {
                "Enabled"
            } else {
                "Disabled"
            })
            .with_mutability(ShellSettingsRowMutability::Editable),
            ShellSettingsRowSummary::read_only(
                "performanceDiagnosticsExport",
                "square.and.arrow.up",
                "Export Diagnostics",
            )
            .with_detail(diagnostics.export_detail())
            .with_value("Export")
            .with_mutability(ShellSettingsRowMutability::ActionOnly),
        ]
    }
}

fn terminal_profile_system_name(profile: &TerminalProfileDefinition) -> &'static str {
    match profile.launch {
        TerminalProfileLaunch::LoginShell => "terminal",
        TerminalProfileLaunch::SudoUser { .. } => "person.crop.circle",
        TerminalProfileLaunch::SudoRoot => "exclamationmark.triangle",
        TerminalProfileLaunch::ManagedUser { .. } => "checkmark.seal",
        TerminalProfileLaunch::CustomCommand { .. } => "chevron.left.forwardslash.chevron.right",
    }
}

fn terminal_account_system_name(plan: &ManagedTerminalAccountPlan) -> &'static str {
    match plan.status {
        ManagedTerminalAccountPlanStatus::AlreadyReady => "checkmark.seal",
        ManagedTerminalAccountPlanStatus::Repair => "wrench.and.screwdriver",
        ManagedTerminalAccountPlanStatus::RequiresDestructiveConfirmation
        | ManagedTerminalAccountPlanStatus::Invalid { .. }
        | ManagedTerminalAccountPlanStatus::SudoersConflict { .. }
        | ManagedTerminalAccountPlanStatus::TerminalProfileConflict { .. }
        | ManagedTerminalAccountPlanStatus::AccountNotAlanManaged
        | ManagedTerminalAccountPlanStatus::LegacySudoersPresent { .. }
        | ManagedTerminalAccountPlanStatus::PtySpawnFailed => "exclamationmark.triangle",
        ManagedTerminalAccountPlanStatus::HelperUnavailable => "puzzlepiece.extension",
        ManagedTerminalAccountPlanStatus::ReadyToApply => "person.crop.circle.badge.plus",
    }
}

fn terminal_account_status_label(plan: &ManagedTerminalAccountPlan) -> &'static str {
    match plan.status {
        ManagedTerminalAccountPlanStatus::AlreadyReady => "Ready",
        ManagedTerminalAccountPlanStatus::Repair => "Repairable",
        ManagedTerminalAccountPlanStatus::Invalid { .. } => "Invalid",
        ManagedTerminalAccountPlanStatus::RequiresDestructiveConfirmation => "Confirm",
        ManagedTerminalAccountPlanStatus::SudoersConflict { .. }
        | ManagedTerminalAccountPlanStatus::TerminalProfileConflict { .. } => "Conflict",
        ManagedTerminalAccountPlanStatus::HelperUnavailable => "Unavailable",
        ManagedTerminalAccountPlanStatus::AccountNotAlanManaged => "Unmanaged",
        ManagedTerminalAccountPlanStatus::LegacySudoersPresent { .. } => "Cleanup",
        ManagedTerminalAccountPlanStatus::PtySpawnFailed => "PTY failed",
        ManagedTerminalAccountPlanStatus::ReadyToApply => "Preview",
    }
}

fn terminal_account_detail(plan: &ManagedTerminalAccountPlan) -> String {
    let target = &plan.request.account_name;
    match &plan.status {
        ManagedTerminalAccountPlanStatus::AlreadyReady => {
            format!("{target} is ready for terminal entry and linked to its Terminal Profile.")
        }
        ManagedTerminalAccountPlanStatus::Repair => {
            format!("{target} needs repair before terminal entry is ready.")
        }
        ManagedTerminalAccountPlanStatus::Invalid { .. } => {
            format!("{target} needs a valid local account identifier.")
        }
        ManagedTerminalAccountPlanStatus::RequiresDestructiveConfirmation => {
            format!("{target} rollback needs separate destructive confirmation.")
        }
        ManagedTerminalAccountPlanStatus::SudoersConflict { path } => {
            format!("{target} has an existing non-Alan sudoers file at {path}.")
        }
        ManagedTerminalAccountPlanStatus::TerminalProfileConflict { profile_id } => {
            format!("{target} has an existing non-Alan Terminal Profile named {profile_id}.")
        }
        ManagedTerminalAccountPlanStatus::HelperUnavailable => {
            "Alan privileged helper is unavailable for Managed Users.".to_string()
        }
        ManagedTerminalAccountPlanStatus::AccountNotAlanManaged => {
            format!("{target} exists but is not Alan-managed.")
        }
        ManagedTerminalAccountPlanStatus::LegacySudoersPresent { path } => match path {
            Some(path) => format!("{target} has a legacy Alan sudoers file at {path}."),
            None => format!("{target} has a legacy Alan sudoers file."),
        },
        ManagedTerminalAccountPlanStatus::PtySpawnFailed => {
            format!("{target} account exists, but helper-managed PTY startup failed.")
        }
        ManagedTerminalAccountPlanStatus::ReadyToApply => {
            format!("{target} terminal entry plan is ready for explicit confirmation.")
        }
    }
}

fn unavailable_row(
    id: impl Into<String>,
    system_name: impl Into<String>,
    title: impl Into<String>,
) -> ShellSettingsRowSummary {
    ShellSettingsRowSummary::read_only(id, system_name, title).with_value("Unavailable")
}

fn non_repeating_detail(detail: Option<String>, title: &str) -> Option<String> {
    let normalized_title = title.trim();
    let normalized_detail = detail?.trim().to_string();
    if normalized_detail.is_empty() || normalized_detail.eq_ignore_ascii_case(normalized_title) {
        return None;
    }
    Some(normalized_detail)
}

fn launch_kind_value(kind: TerminalProfileLaunchKind) -> &'static str {
    match kind {
        TerminalProfileLaunchKind::LoginShell => "login_shell",
        TerminalProfileLaunchKind::SudoUser => "sudo_user",
        TerminalProfileLaunchKind::SudoRoot => "sudo_root",
        TerminalProfileLaunchKind::ManagedUser => "managed_user",
        TerminalProfileLaunchKind::CustomCommand => "custom_command",
    }
}

fn normalized_settings_value(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn most_specific_workspace<'a>(
    entries: &'a [ShellSettingsWorkspaceRegistryEntry],
    active_directory: &str,
) -> Option<&'a ShellSettingsWorkspaceRegistryEntry> {
    entries
        .iter()
        .filter(|entry| contains_path(active_directory, &entry.path))
        .max_by_key(|entry| entry.path.len())
}

fn contains_path(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}
