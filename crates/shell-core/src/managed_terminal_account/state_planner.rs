use super::{
    ManagedTerminalAccountIdentifierValidator, ManagedTerminalAccountOwnershipState,
    ManagedTerminalAccountPlan, ManagedTerminalAccountPlanStatus,
    ManagedTerminalAccountPlanStepKind, ManagedTerminalAccountProfileState,
    ManagedTerminalAccountRecord, ManagedTerminalAccountRequest, ManagedTerminalAccountState,
    ManagedTerminalAccountVerificationStatus, managed_account_step,
};

pub(super) fn plan(
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

    let requires_ownership = matches!(
        state.account,
        ManagedTerminalAccountRecord::Standard { .. } | ManagedTerminalAccountRecord::Admin { .. }
    );
    if requires_ownership
        && !matches!(
            state.ownership,
            ManagedTerminalAccountOwnershipState::AlanManaged { .. }
        )
    {
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
        ManagedTerminalAccountProfileState::ExistingUnmanaged { .. } => unreachable!(),
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
