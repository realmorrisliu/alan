use crate::terminal_profile::{
    TerminalProfileDefinition, TerminalProfileLaunch, TerminalProfilePresentation,
};
use serde::{Deserialize, Serialize};

/// Portable request to prepare a managed local terminal account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountRequest {
    /// Managed account short name.
    pub account_name: String,
    /// Optional display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// Login shell path.
    pub shell: String,
    /// Managed account home directory.
    pub home_directory: String,
    /// Whether the account should be hidden from login window lists.
    pub hide_from_login_window: bool,
}

impl ManagedTerminalAccountRequest {
    /// Returns the canonical home directory for a managed account name.
    pub fn canonical_home_directory(account_name: &str) -> String {
        format!("/Users/{account_name}")
    }

    /// Returns the Terminal Profile id associated with this managed account.
    pub fn terminal_profile_id(&self) -> &str {
        &self.account_name
    }
}

/// Stable validation error for managed terminal account requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ManagedTerminalAccountValidationError {
    /// Managed account name is invalid.
    InvalidAccountName {
        /// Invalid value.
        value: String,
    },
    /// Managed account name is reserved.
    ReservedAccountName {
        /// Reserved value.
        value: String,
    },
    /// Shell path is invalid.
    InvalidShell {
        /// Invalid value.
        value: String,
    },
}

/// Validator for managed terminal account request identifiers.
pub struct ManagedTerminalAccountIdentifierValidator;

impl ManagedTerminalAccountIdentifierValidator {
    /// Validates portable request fields without querying the host OS.
    pub fn validate(
        request: &ManagedTerminalAccountRequest,
    ) -> Vec<ManagedTerminalAccountValidationError> {
        let mut errors = Vec::new();
        if !matches_managed_account_identifier(&request.account_name) {
            errors.push(ManagedTerminalAccountValidationError::InvalidAccountName {
                value: request.account_name.clone(),
            });
        }
        if matches!(
            request.account_name.to_ascii_lowercase().as_str(),
            "root" | "daemon" | "nobody"
        ) {
            errors.push(ManagedTerminalAccountValidationError::ReservedAccountName {
                value: request.account_name.clone(),
            });
        }
        if request.shell.trim().is_empty() || request.shell.contains('\n') {
            errors.push(ManagedTerminalAccountValidationError::InvalidShell {
                value: request.shell.clone(),
            });
        }
        errors
    }
}

/// Discovered managed account state supplied by a platform adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountRecord {
    /// Account does not exist.
    Missing,
    /// Account exists as a standard user.
    Standard {
        /// Current home directory.
        home_directory: String,
        /// Current shell.
        shell: String,
        /// Whether the account is hidden.
        hidden: bool,
    },
    /// Account exists but is an administrator.
    Admin {
        /// Current home directory.
        home_directory: String,
        /// Current shell.
        shell: String,
        /// Whether the account is hidden.
        hidden: bool,
    },
    /// Account exists but has an invalid or unreadable state.
    Invalid {
        /// Redacted reason.
        reason: String,
    },
}

impl ManagedTerminalAccountRecord {
    fn requires_alan_managed_ownership(&self) -> bool {
        matches!(self, Self::Standard { .. } | Self::Admin { .. })
    }
}

/// Evidence that an existing account is owned by Alan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountOwnershipEvidence {
    /// Helper-owned marker file.
    HelperMarker {
        /// Marker path.
        path: String,
    },
}

/// Ownership state for an existing managed terminal account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountOwnershipState {
    /// No ownership evidence is present.
    #[default]
    Missing,
    /// Alan owns this account.
    AlanManaged {
        /// Ownership evidence.
        evidence: ManagedTerminalAccountOwnershipEvidence,
    },
    /// The account exists but is outside Alan management.
    NotAlanManaged {
        /// Redacted reason.
        reason: String,
    },
}

impl ManagedTerminalAccountOwnershipState {
    fn is_alan_managed(&self) -> bool {
        matches!(self, Self::AlanManaged { .. })
    }
}

/// Discovered managed Terminal Profile state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountProfileState {
    /// Profile is absent.
    Missing,
    /// Matching managed profile exists.
    ExistingManaged {
        /// Profile id.
        profile_id: String,
    },
    /// Matching managed profile exists but needs refresh.
    ExistingManagedOutdated {
        /// Profile id.
        profile_id: String,
    },
    /// Profile id exists but is not managed by this request.
    ExistingUnmanaged {
        /// Profile id.
        profile_id: String,
    },
}

/// Readiness verification step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedTerminalAccountVerificationStep {
    /// Account lookup.
    AccountLookup,
    /// Non-admin account requirement.
    NonAdminAccount,
    /// Home directory check.
    HomeDirectory,
    /// Shell check.
    Shell,
    /// Alan ownership check.
    Ownership,
    /// Helper-managed PTY startup check.
    ManagedUserPty,
}

/// Readiness verification status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ManagedTerminalAccountVerificationStatus {
    /// Verification has not run.
    NotRun,
    /// Verification passed.
    Passed,
    /// Verification failed.
    Failed {
        /// Failed step.
        step: ManagedTerminalAccountVerificationStep,
        /// Redacted message.
        message: String,
    },
}

/// Portable discovered managed terminal account state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountState {
    /// Account record.
    pub account: ManagedTerminalAccountRecord,
    /// Alan ownership evidence for existing accounts.
    #[serde(default)]
    pub ownership: ManagedTerminalAccountOwnershipState,
    /// Terminal Profile record.
    pub terminal_profile: ManagedTerminalAccountProfileState,
    /// Verification status.
    pub verification: ManagedTerminalAccountVerificationStatus,
    /// Whether the account home directory exists on disk.
    #[serde(default = "default_true")]
    pub home_directory_exists: bool,
}

/// Managed terminal account plan step kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedTerminalAccountPlanStepKind {
    /// Create a standard account.
    CreateStandardAccount,
    /// Repair account type.
    RepairAccountType,
    /// Repair home directory.
    RepairHomeDirectory,
    /// Repair shell.
    RepairShell,
    /// Hide account from login window lists.
    HideAccount,
    /// Create or update Terminal Profile handoff.
    CreateOrUpdateTerminalProfile,
    /// Remove managed Terminal Profile.
    RemoveManagedTerminalProfile,
    /// Delete the account.
    DeleteAccount,
    /// Delete the account home directory.
    DeleteHomeDirectory,
    /// Write the helper ownership marker.
    WriteOwnershipMarker,
    /// Verify the helper-managed account state.
    VerifyAccount,
    /// Verify helper-managed PTY startup.
    VerifyManagedUserPty,
    /// Remove helper-managed account integration.
    RemoveManagedUserIntegration,
}

/// Managed terminal account plan step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountPlanStep {
    /// Step kind.
    pub kind: ManagedTerminalAccountPlanStepKind,
    /// User-facing summary.
    pub summary: String,
    /// Whether the step requires privilege.
    pub requires_privilege: bool,
}

/// Managed terminal account plan status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ManagedTerminalAccountPlanStatus {
    /// Plan is ready to apply.
    ReadyToApply,
    /// Nothing needs to change.
    AlreadyReady,
    /// Existing state needs repair.
    Repair,
    /// Request is invalid.
    Invalid {
        /// Validation errors.
        errors: Vec<ManagedTerminalAccountValidationError>,
    },
    /// Destructive rollback requires confirmation.
    RequiresDestructiveConfirmation,
    /// Terminal Profile id is occupied by unmanaged content.
    TerminalProfileConflict {
        /// Conflicting profile id.
        profile_id: String,
    },
    /// Privileged helper is unavailable.
    HelperUnavailable,
    /// Account exists but is not Alan-managed.
    AccountNotAlanManaged,
    /// Helper-managed PTY smoke failed.
    PtySpawnFailed,
}

impl ManagedTerminalAccountPlanStatus {
    /// Stable label used by settings summaries and diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReadyToApply => "ready_to_apply",
            Self::AlreadyReady => "already_ready",
            Self::Repair => "repair",
            Self::Invalid { .. } => "invalid",
            Self::RequiresDestructiveConfirmation => "requires_destructive_confirmation",
            Self::TerminalProfileConflict { .. } => "terminal_profile_conflict",
            Self::HelperUnavailable => "helper_unavailable",
            Self::AccountNotAlanManaged => "account_not_alan_managed",
            Self::PtySpawnFailed => "pty_spawn_failed",
        }
    }
}

/// Managed terminal account plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountPlan {
    /// Original request.
    pub request: ManagedTerminalAccountRequest,
    /// Plan status.
    pub status: ManagedTerminalAccountPlanStatus,
    /// Plan steps.
    pub steps: Vec<ManagedTerminalAccountPlanStep>,
}

/// Planner for portable managed terminal account semantics.
pub struct ManagedTerminalAccountPlanner;

impl ManagedTerminalAccountPlanner {
    /// Builds a provisioning or repair plan from platform-supplied state.
    pub fn plan(
        request: ManagedTerminalAccountRequest,
        state: &ManagedTerminalAccountState,
    ) -> ManagedTerminalAccountPlan {
        let validation_errors = ManagedTerminalAccountIdentifierValidator::validate(&request);
        if !validation_errors.is_empty() {
            return ManagedTerminalAccountPlan {
                request,
                status: ManagedTerminalAccountPlanStatus::Invalid {
                    errors: validation_errors,
                },
                steps: Vec::new(),
            };
        }
        if state.account.requires_alan_managed_ownership() && !state.ownership.is_alan_managed() {
            return ManagedTerminalAccountPlan {
                request,
                status: ManagedTerminalAccountPlanStatus::AccountNotAlanManaged,
                steps: Vec::new(),
            };
        }
        if let ManagedTerminalAccountProfileState::ExistingUnmanaged { profile_id } =
            &state.terminal_profile
        {
            return ManagedTerminalAccountPlan {
                request,
                status: ManagedTerminalAccountPlanStatus::TerminalProfileConflict {
                    profile_id: profile_id.clone(),
                },
                steps: Vec::new(),
            };
        }

        let mut steps = Vec::new();
        let mut needs_create = false;
        let mut repair_needed = false;

        match &state.account {
            ManagedTerminalAccountRecord::Missing => {
                needs_create = true;
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::CreateStandardAccount,
                    "Create standard local terminal account",
                    true,
                ));
                if request.hide_from_login_window {
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::HideAccount,
                        "Hide terminal account from login window lists",
                        true,
                    ));
                }
            }
            ManagedTerminalAccountRecord::Admin { .. } => {
                repair_needed = true;
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::RepairAccountType,
                    "Repair account so it is not an administrator",
                    true,
                ));
            }
            ManagedTerminalAccountRecord::Invalid { reason } => {
                if reason.to_ascii_lowercase().contains("incomplete") {
                    needs_create = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::CreateStandardAccount,
                        format!("Complete local terminal account record: {reason}"),
                        true,
                    ));
                    if request.hide_from_login_window {
                        steps.push(managed_account_step(
                            ManagedTerminalAccountPlanStepKind::HideAccount,
                            "Hide terminal account from login window lists",
                            true,
                        ));
                    }
                } else {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::RepairAccountType,
                        format!("Repair account state: {reason}"),
                        true,
                    ));
                }
            }
            ManagedTerminalAccountRecord::Standard {
                home_directory,
                shell,
                hidden,
            } => {
                if home_directory != &request.home_directory || !state.home_directory_exists {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::RepairHomeDirectory,
                        "Repair terminal account home directory",
                        true,
                    ));
                }
                if shell != &request.shell {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::RepairShell,
                        "Repair terminal account shell",
                        true,
                    ));
                }
                if request.hide_from_login_window && !hidden {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::HideAccount,
                        "Hide terminal account from login window lists",
                        true,
                    ));
                }
            }
        }

        if needs_create {
            steps.push(managed_account_step(
                ManagedTerminalAccountPlanStepKind::WriteOwnershipMarker,
                "Write helper ownership marker",
                true,
            ));
        }

        if state.verification != ManagedTerminalAccountVerificationStatus::Passed {
            if !needs_create {
                repair_needed = true;
            }
            steps.push(managed_account_step(
                ManagedTerminalAccountPlanStepKind::VerifyAccount,
                "Verify helper-managed account state",
                true,
            ));
            steps.push(managed_account_step(
                ManagedTerminalAccountPlanStepKind::VerifyManagedUserPty,
                "Verify helper-managed PTY startup",
                true,
            ));
        }

        match &state.terminal_profile {
            ManagedTerminalAccountProfileState::Missing => {
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
                    "Create matching Terminal Profile",
                    false,
                ));
            }
            ManagedTerminalAccountProfileState::ExistingManaged { .. } => {
                if state.verification != ManagedTerminalAccountVerificationStatus::Passed {
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
                        "Refresh matching Terminal Profile",
                        false,
                    ));
                }
            }
            ManagedTerminalAccountProfileState::ExistingManagedOutdated { .. } => {
                repair_needed = true;
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
                    "Refresh matching Terminal Profile",
                    false,
                ));
            }
            ManagedTerminalAccountProfileState::ExistingUnmanaged { .. } => {}
        }

        let status = if steps.is_empty() {
            ManagedTerminalAccountPlanStatus::AlreadyReady
        } else if repair_needed {
            ManagedTerminalAccountPlanStatus::Repair
        } else {
            ManagedTerminalAccountPlanStatus::ReadyToApply
        };
        ManagedTerminalAccountPlan {
            request,
            status,
            steps,
        }
    }
}

/// Result projected by a managed account dry-run executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountApplyResult {
    /// Completed steps.
    pub completed_steps: Vec<ManagedTerminalAccountPlanStepKind>,
    /// Failed step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_step: Option<ManagedTerminalAccountPlanStepKind>,
    /// Whether the operation was cancelled.
    pub cancelled: bool,
    /// Redacted visible diagnostics.
    pub visible_diagnostics: Vec<String>,
}

impl ManagedTerminalAccountApplyResult {
    /// Returns the Swift-compatible dry-run cancellation result.
    pub fn cancelled(before: &[ManagedTerminalAccountPlanStepKind]) -> Self {
        Self {
            completed_steps: Vec::new(),
            failed_step: before.first().copied(),
            cancelled: true,
            visible_diagnostics: vec![
                "Provisioning cancelled before privileged changes.".to_string(),
            ],
        }
    }
}

/// Fake executor for deterministic managed-account dry-run projections.
pub struct ManagedTerminalAccountFakeExecutor;

impl ManagedTerminalAccountFakeExecutor {
    /// Applies a plan in memory, optionally cancelling before the first step.
    pub fn apply(
        plan: &ManagedTerminalAccountPlan,
        cancel_before_apply: bool,
        fail_at: Option<ManagedTerminalAccountPlanStepKind>,
    ) -> ManagedTerminalAccountApplyResult {
        let step_kinds = plan.steps.iter().map(|step| step.kind).collect::<Vec<_>>();
        if cancel_before_apply {
            return ManagedTerminalAccountApplyResult::cancelled(&step_kinds);
        }

        let mut completed_steps = Vec::new();
        for step in &plan.steps {
            if Some(step.kind) == fail_at {
                return ManagedTerminalAccountApplyResult {
                    completed_steps,
                    failed_step: Some(step.kind),
                    cancelled: false,
                    visible_diagnostics: vec![format!(
                        "Step failed: {}. Credentials redacted.",
                        step.summary
                    )],
                };
            }
            completed_steps.push(step.kind);
        }
        ManagedTerminalAccountApplyResult {
            completed_steps,
            failed_step: None,
            cancelled: false,
            visible_diagnostics: vec![
                "Provisioning plan applied. Credentials redacted.".to_string(),
            ],
        }
    }
}

/// Terminal Profile handoff helper for managed terminal accounts.
pub struct ManagedTerminalAccountProfileHandoff;

impl ManagedTerminalAccountProfileHandoff {
    /// Produces a managed Terminal Profile once readiness verification has passed.
    pub fn profile_definition(
        request: &ManagedTerminalAccountRequest,
        state: &ManagedTerminalAccountState,
    ) -> Option<TerminalProfileDefinition> {
        (state.verification == ManagedTerminalAccountVerificationStatus::Passed).then(|| {
            TerminalProfileDefinition {
                id: request.terminal_profile_id().to_string(),
                title: request
                    .full_name
                    .clone()
                    .unwrap_or_else(|| request.account_name.clone()),
                launch: TerminalProfileLaunch::ManagedUser {
                    unix_user: request.account_name.clone(),
                },
                default_working_directory: Some(request.home_directory.clone()),
                presentation: Some(TerminalProfilePresentation {
                    symbol_name: Some("person.crop.circle".to_string()),
                    color_name: None,
                }),
                managed_terminal_account_id: Some(request.account_name.clone()),
            }
        })
    }
}

fn matches_managed_account_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let mut len = 1;
    for ch in chars {
        len += 1;
        if len > 32 || !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
            return false;
        }
    }
    true
}

fn managed_account_step(
    kind: ManagedTerminalAccountPlanStepKind,
    summary: impl Into<String>,
    requires_privilege: bool,
) -> ManagedTerminalAccountPlanStep {
    ManagedTerminalAccountPlanStep {
        kind,
        summary: summary.into(),
        requires_privilege,
    }
}

fn default_true() -> bool {
    true
}
