use alan_shell_core::{
    ManagedTerminalAccountDiagnosis, ManagedTerminalAccountFakeExecutor,
    ManagedTerminalAccountOwnershipEvidence, ManagedTerminalAccountOwnershipKind,
    ManagedTerminalAccountOwnershipState, ManagedTerminalAccountPlanStatus,
    ManagedTerminalAccountPlanStepKind, ManagedTerminalAccountPlanner,
    ManagedTerminalAccountProfileHandoff, ManagedTerminalAccountProfileState,
    ManagedTerminalAccountReadinessState, ManagedTerminalAccountRecord,
    ManagedTerminalAccountRequest, ManagedTerminalAccountRollbackScope,
    ManagedTerminalAccountState, ManagedTerminalAccountVerificationStatus,
    ManagedTerminalAccountVerificationStep, TerminalExecutableAvailability,
    TerminalLaunchEnvironment, TerminalLaunchIntent, TerminalLaunchStrategy,
    TerminalProfileDefinition, TerminalProfileDocument, TerminalProfileEditor,
    TerminalProfileEditorDraft, TerminalProfileLaunch, TerminalProfileLaunchKind,
    TerminalProfilePresentation, TerminalProfileResolutionState, TerminalProfileValidationError,
    TerminalProfileValidator, shell_quoted, should_capture_global_default_terminal_profile,
};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn terminal_profile_launch_json_matches_swift_shape() {
    let launch = TerminalProfileLaunch::SudoUser {
        unix_user: "alan".to_string(),
    };

    assert_eq!(
        serde_json::to_value(&launch).unwrap(),
        json!({
            "kind": "sudo_user",
            "unix_user": "alan"
        })
    );
    assert_eq!(
        serde_json::from_value::<TerminalProfileLaunch>(json!({
            "kind": "custom_command",
            "command": "echo hello"
        }))
        .unwrap(),
        TerminalProfileLaunch::CustomCommand {
            command: "echo hello".to_string()
        }
    );
    assert_eq!(shell_quoted("it's ready"), "'it'\\''s ready'");
}

#[test]
fn terminal_profile_validation_reports_stable_errors_without_file_io() {
    let document = TerminalProfileDocument {
        default_profile_id: "missing-default".to_string(),
        profiles: vec![
            TerminalProfileDefinition {
                id: "bad".to_string(),
                title: " ".to_string(),
                launch: TerminalProfileLaunch::SudoUser {
                    unix_user: " ".to_string(),
                },
                default_working_directory: None,
                presentation: None,
                managed_terminal_account_id: None,
            },
            TerminalProfileDefinition {
                id: "bad".to_string(),
                title: "Duplicate".to_string(),
                launch: TerminalProfileLaunch::CustomCommand {
                    command: "".to_string(),
                },
                default_working_directory: None,
                presentation: None,
                managed_terminal_account_id: None,
            },
        ],
    };

    let result = TerminalProfileValidator::validate_with_availability(
        &document,
        &TerminalExecutableAvailability::enforcing(["/bin/zsh"]),
    );

    assert!(
        result
            .errors
            .contains(&TerminalProfileValidationError::MissingTitle {
                id: "bad".to_string()
            })
    );
    assert!(
        result
            .errors
            .contains(&TerminalProfileValidationError::MissingUnixUser {
                id: "bad".to_string()
            })
    );
    assert!(
        result
            .errors
            .contains(&TerminalProfileValidationError::DuplicateId {
                id: "bad".to_string()
            })
    );
    assert!(
        result
            .errors
            .contains(&TerminalProfileValidationError::MissingCustomCommand {
                id: "bad".to_string()
            })
    );
    assert!(
        result
            .errors
            .contains(&TerminalProfileValidationError::MissingDefaultProfile {
                id: "missing-default".to_string()
            })
    );
    assert!(
        result
            .errors
            .contains(&TerminalProfileValidationError::UnavailableExecutable {
                profile_id: "bad".to_string(),
                path: "/usr/bin/sudo".to_string()
            })
    );
}

#[test]
fn terminal_profile_editor_trims_and_upserts_definitions() {
    let draft = TerminalProfileEditorDraft {
        id: " alan ".to_string(),
        title: " Alan ".to_string(),
        launch_kind: TerminalProfileLaunchKind::SudoUser,
        unix_user: "alan".to_string(),
        custom_command: String::new(),
        default_working_directory: Some(" /Users/alan ".to_string()),
        presentation: Some(TerminalProfilePresentation {
            symbol_name: Some("person.crop.circle".to_string()),
            color_name: Some("green".to_string()),
        }),
        managed_terminal_account_id: Some(" alan ".to_string()),
    };

    let result = TerminalProfileEditor::make_definition(draft.clone());
    let definition = result.definition.expect("draft is valid");

    assert_eq!(definition.id, "alan");
    assert_eq!(definition.title, "Alan");
    assert_eq!(
        definition.default_working_directory.as_deref(),
        Some("/Users/alan")
    );
    assert_eq!(
        definition.managed_terminal_account_id.as_deref(),
        Some("alan")
    );

    let document = TerminalProfileDocument {
        default_profile_id: String::new(),
        profiles: vec![TerminalProfileDefinition::login_shell_fallback()],
    };
    let upserted = TerminalProfileEditor::upsert(draft, &document);

    assert!(upserted.is_valid());
    assert_eq!(
        upserted.document.unwrap().default_profile_id,
        "alan".to_string()
    );
}

#[test]
fn terminal_profile_editor_rejects_existing_managed_profile_updates() {
    let document = TerminalProfileDocument {
        default_profile_id: "alan".to_string(),
        profiles: vec![TerminalProfileDefinition {
            id: "alan".to_string(),
            title: "Alan".to_string(),
            launch: TerminalProfileLaunch::ManagedUser {
                unix_user: "alan".to_string(),
            },
            default_working_directory: Some("/Users/alan".to_string()),
            presentation: None,
            managed_terminal_account_id: Some("alan".to_string()),
        }],
    };
    let result = TerminalProfileEditor::upsert(
        TerminalProfileEditorDraft {
            id: " alan ".to_string(),
            title: "Alan Root".to_string(),
            launch_kind: TerminalProfileLaunchKind::SudoRoot,
            unix_user: String::new(),
            custom_command: String::new(),
            default_working_directory: Some("/var/root".to_string()),
            presentation: None,
            managed_terminal_account_id: Some("alan".to_string()),
        },
        &document,
    );

    assert!(!result.is_valid());
    assert!(result.document.is_none());
    assert_eq!(
        result.errors,
        vec![TerminalProfileValidationError::ManagedProfileReadOnly {
            id: "alan".to_string()
        }]
    );
}

#[test]
fn terminal_profile_editor_allows_matching_managed_profile_repair_upserts() {
    let document = TerminalProfileDocument {
        default_profile_id: "alan".to_string(),
        profiles: vec![TerminalProfileDefinition {
            id: "alan".to_string(),
            title: "Alan".to_string(),
            launch: TerminalProfileLaunch::SudoUser {
                unix_user: "alan".to_string(),
            },
            default_working_directory: Some("/tmp/stale".to_string()),
            presentation: None,
            managed_terminal_account_id: Some("alan".to_string()),
        }],
    };
    let result = TerminalProfileEditor::upsert(
        TerminalProfileEditorDraft {
            id: " alan ".to_string(),
            title: "Alan".to_string(),
            launch_kind: TerminalProfileLaunchKind::ManagedUser,
            unix_user: "alan".to_string(),
            custom_command: String::new(),
            default_working_directory: Some("/Users/alan".to_string()),
            presentation: None,
            managed_terminal_account_id: Some("alan".to_string()),
        },
        &document,
    );

    assert!(result.is_valid());
    let document = result.document.expect("matching repair upsert succeeds");
    assert_eq!(
        document.profiles[0].launch,
        TerminalProfileLaunch::ManagedUser {
            unix_user: "alan".to_string()
        }
    );
    assert_eq!(
        document.profiles[0].default_working_directory.as_deref(),
        Some("/Users/alan")
    );
}

#[test]
fn launch_intent_resolves_profiles_and_projected_environment() {
    let document = profile_document();
    let availability = TerminalExecutableAvailability::enforcing(["/usr/bin/sudo", "/bin/zsh"]);
    let environment = launch_environment([("SHELL", "/bin/zsh")]);

    let sudo =
        TerminalLaunchIntent::resolve(Some("alan"), Some(&document), &availability, &environment);
    assert_eq!(
        sudo.strategy,
        TerminalLaunchStrategy::TerminalProfileSudoUser
    );
    assert_eq!(sudo.launch_path, "/usr/bin/sudo");
    assert_eq!(sudo.arguments, vec!["-iu", "alan"]);
    assert_eq!(
        sudo.surface_command.as_deref(),
        Some("'/usr/bin/sudo' '-iu' 'alan'")
    );
    assert_eq!(
        sudo.profile_environment
            .get("ALAN_TERMINAL_PROFILE_ID")
            .map(String::as_str),
        Some("alan")
    );
    assert_eq!(
        sudo.profile_environment
            .get("ALAN_TERMINAL_PROFILE_KIND")
            .map(String::as_str),
        Some("sudo_user")
    );
    assert_eq!(sudo.working_directory.as_deref(), Some("/Users/alan"));

    let custom =
        TerminalLaunchIntent::resolve(Some("custom"), Some(&document), &availability, &environment);
    assert_eq!(
        custom.strategy,
        TerminalLaunchStrategy::TerminalProfileCustomCommand
    );
    assert_eq!(custom.launch_path, "/bin/zsh");
    assert_eq!(custom.arguments, vec!["-lc", "echo hello"]);
    assert_eq!(custom.surface_command.as_deref(), Some("echo hello"));
    assert_eq!(custom.detail.as_deref(), Some("Custom command"));

    let missing =
        TerminalLaunchIntent::resolve(Some("lab"), Some(&document), &availability, &environment);
    assert_eq!(missing.strategy, TerminalLaunchStrategy::LoginShellEnv);
    assert!(matches!(
        missing.terminal_profile_state,
        TerminalProfileResolutionState::Missing { ref requested_id } if requested_id == "lab"
    ));
    assert_eq!(
        missing
            .profile_environment
            .get("ALAN_TERMINAL_PROFILE_STATE")
            .map(String::as_str),
        Some("missing")
    );
}

#[test]
fn launch_intent_reports_unavailable_profile_and_honors_env_override() {
    let document = profile_document();
    let only_shell = TerminalExecutableAvailability::enforcing(["/bin/zsh"]);
    let environment = launch_environment([("SHELL", "/bin/zsh")]);

    let unavailable =
        TerminalLaunchIntent::resolve(Some("alan"), Some(&document), &only_shell, &environment);
    assert_eq!(unavailable.strategy, TerminalLaunchStrategy::LoginShellEnv);
    assert!(matches!(
        unavailable.terminal_profile_state,
        TerminalProfileResolutionState::Unavailable {
            ref requested_id,
            ref reason
        } if requested_id == "alan" && reason == "missing_executable"
    ));

    let override_environment = launch_environment([
        ("SHELL", "/bin/zsh"),
        ("ALAN_SHELL_BOOT_COMMAND", "echo override"),
    ]);
    let override_intent = TerminalLaunchIntent::resolve(
        Some("alan"),
        Some(&document),
        &only_shell,
        &override_environment,
    );
    assert_eq!(
        override_intent.strategy,
        TerminalLaunchStrategy::ShellCommandEnv
    );
    assert_eq!(
        override_intent.surface_command.as_deref(),
        Some("echo override")
    );
    assert!(matches!(
        override_intent.terminal_profile_state,
        TerminalProfileResolutionState::Absent
    ));
}

#[test]
fn global_default_capture_matches_swift_policy() {
    let login_shell = TerminalProfileDefinition::login_shell_fallback();
    let sudo = profile_document().profile(Some("alan")).unwrap().clone();
    let login_with_cwd = TerminalProfileDefinition {
        default_working_directory: Some("/tmp".to_string()),
        ..login_shell.clone()
    };

    assert!(!should_capture_global_default_terminal_profile(
        &login_shell
    ));
    assert!(!should_capture_global_default_terminal_profile(&sudo));
    assert!(!should_capture_global_default_terminal_profile(
        &login_with_cwd
    ));
}

#[test]
fn managed_terminal_account_dry_run_uses_helper_owned_steps() {
    let request = managed_account_request();
    let missing_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Missing,
        ownership: ManagedTerminalAccountOwnershipState::Missing,
        terminal_profile: ManagedTerminalAccountProfileState::Missing,
        verification: ManagedTerminalAccountVerificationStatus::NotRun,
        home_directory_exists: false,
    };

    let plan = ManagedTerminalAccountPlanner::plan(request.clone(), &missing_state);
    let step_kinds = plan.steps.iter().map(|step| step.kind).collect::<Vec<_>>();

    assert_eq!(plan.status, ManagedTerminalAccountPlanStatus::ReadyToApply);
    assert_eq!(
        step_kinds,
        vec![
            ManagedTerminalAccountPlanStepKind::CreateStandardAccount,
            ManagedTerminalAccountPlanStepKind::HideAccount,
            ManagedTerminalAccountPlanStepKind::WriteOwnershipMarker,
            ManagedTerminalAccountPlanStepKind::VerifyAccount,
            ManagedTerminalAccountPlanStepKind::VerifyManagedUserPty,
            ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
        ]
    );

    let partial_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Invalid {
            reason: "Local account record is incomplete.".to_string(),
        },
        ownership: ManagedTerminalAccountOwnershipState::Missing,
        terminal_profile: ManagedTerminalAccountProfileState::Missing,
        verification: ManagedTerminalAccountVerificationStatus::NotRun,
        home_directory_exists: false,
    };
    let partial_plan = ManagedTerminalAccountPlanner::plan(request.clone(), &partial_state);
    assert_eq!(
        partial_plan.steps.first().map(|step| step.kind),
        Some(ManagedTerminalAccountPlanStepKind::CreateStandardAccount)
    );

    let invalid_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Invalid {
            reason: "Local account record is unreadable.".to_string(),
        },
        ownership: ManagedTerminalAccountOwnershipState::Missing,
        terminal_profile: ManagedTerminalAccountProfileState::Missing,
        verification: ManagedTerminalAccountVerificationStatus::NotRun,
        home_directory_exists: true,
    };
    let invalid_plan = ManagedTerminalAccountPlanner::plan(request.clone(), &invalid_state);
    assert_eq!(
        invalid_plan.status,
        ManagedTerminalAccountPlanStatus::Repair
    );
    assert_eq!(
        invalid_plan
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            ManagedTerminalAccountPlanStepKind::RepairAccountType,
            ManagedTerminalAccountPlanStepKind::VerifyAccount,
            ManagedTerminalAccountPlanStepKind::VerifyManagedUserPty,
            ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
        ]
    );

    let ordinary_account_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Standard {
            home_directory: "/Users/alan_smoke".to_string(),
            shell: "/bin/zsh".to_string(),
            hidden: false,
        },
        ownership: ManagedTerminalAccountOwnershipState::Missing,
        terminal_profile: ManagedTerminalAccountProfileState::Missing,
        verification: ManagedTerminalAccountVerificationStatus::NotRun,
        home_directory_exists: true,
    };
    let ordinary_account_plan =
        ManagedTerminalAccountPlanner::plan(request.clone(), &ordinary_account_state);
    assert_eq!(
        ordinary_account_plan.status,
        ManagedTerminalAccountPlanStatus::AccountNotAlanManaged
    );
    assert!(ordinary_account_plan.steps.is_empty());

    let missing_home_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Standard {
            home_directory: "/Users/alan_smoke".to_string(),
            shell: "/bin/zsh".to_string(),
            hidden: true,
        },
        ownership: ManagedTerminalAccountOwnershipState::AlanManaged {
            evidence: ManagedTerminalAccountOwnershipEvidence::HelperMarker {
                path: "/Library/Application Support/alan-macos-dev/managed-users/alan_smoke/ownership.json".to_string(),
            },
        },
        terminal_profile: ManagedTerminalAccountProfileState::Missing,
        verification: ManagedTerminalAccountVerificationStatus::Failed {
            step: ManagedTerminalAccountVerificationStep::HomeDirectory,
            message: "Home directory is missing.".to_string(),
        },
        home_directory_exists: false,
    };
    let missing_home_plan =
        ManagedTerminalAccountPlanner::plan(request.clone(), &missing_home_state);
    assert!(
        missing_home_plan
            .steps
            .iter()
            .any(|step| step.kind == ManagedTerminalAccountPlanStepKind::RepairHomeDirectory)
    );

    let mismatched_home_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Standard {
            home_directory: "/Users/other".to_string(),
            shell: "/bin/zsh".to_string(),
            hidden: true,
        },
        ownership: ManagedTerminalAccountOwnershipState::AlanManaged {
            evidence: ManagedTerminalAccountOwnershipEvidence::HelperMarker {
                path: "/Library/Application Support/alan-macos-dev/managed-users/alan_smoke/ownership.json".to_string(),
            },
        },
        terminal_profile: ManagedTerminalAccountProfileState::ExistingManaged {
            profile_id: "alan_smoke".to_string(),
        },
        verification: ManagedTerminalAccountVerificationStatus::Passed,
        home_directory_exists: true,
    };
    let mismatched_home_plan =
        ManagedTerminalAccountPlanner::plan(request.clone(), &mismatched_home_state);
    assert_eq!(
        mismatched_home_plan.status,
        ManagedTerminalAccountPlanStatus::Repair
    );
    assert_eq!(
        mismatched_home_plan
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![ManagedTerminalAccountPlanStepKind::RepairHomeDirectory]
    );

    let unverified_managed_profile_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Standard {
            home_directory: request.home_directory.clone(),
            shell: request.shell.clone(),
            hidden: true,
        },
        ownership: ManagedTerminalAccountOwnershipState::AlanManaged {
            evidence: ManagedTerminalAccountOwnershipEvidence::HelperMarker {
                path: "/Library/Application Support/alan-macos-dev/managed-users/alan_smoke/ownership.json".to_string(),
            },
        },
        terminal_profile: ManagedTerminalAccountProfileState::ExistingManaged {
            profile_id: request.terminal_profile_id().to_string(),
        },
        verification: ManagedTerminalAccountVerificationStatus::NotRun,
        home_directory_exists: true,
    };
    let unverified_managed_profile_plan =
        ManagedTerminalAccountPlanner::plan(request.clone(), &unverified_managed_profile_state);
    assert_eq!(
        unverified_managed_profile_plan.status,
        ManagedTerminalAccountPlanStatus::Repair
    );
    assert_eq!(
        unverified_managed_profile_plan
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            ManagedTerminalAccountPlanStepKind::VerifyAccount,
            ManagedTerminalAccountPlanStepKind::VerifyManagedUserPty,
            ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
        ]
    );

    let outdated_profile_state = ManagedTerminalAccountState {
        terminal_profile: ManagedTerminalAccountProfileState::ExistingManagedOutdated {
            profile_id: request.terminal_profile_id().to_string(),
        },
        verification: ManagedTerminalAccountVerificationStatus::Passed,
        ..unverified_managed_profile_state
    };
    let outdated_profile_plan =
        ManagedTerminalAccountPlanner::plan(request.clone(), &outdated_profile_state);
    assert_eq!(
        outdated_profile_plan.status,
        ManagedTerminalAccountPlanStatus::Repair
    );
    assert_eq!(
        outdated_profile_plan
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile]
    );

    let cancelled = ManagedTerminalAccountFakeExecutor::apply(&plan, true, None);
    assert!(cancelled.cancelled);
    assert!(cancelled.completed_steps.is_empty());
    assert_eq!(
        cancelled.failed_step,
        Some(ManagedTerminalAccountPlanStepKind::CreateStandardAccount)
    );

    let ready_state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Standard {
            home_directory: "/Users/alan_smoke".to_string(),
            shell: "/bin/zsh".to_string(),
            hidden: true,
        },
        ownership: ManagedTerminalAccountOwnershipState::AlanManaged {
            evidence: ManagedTerminalAccountOwnershipEvidence::HelperMarker {
                path: "/Library/Application Support/alan-macos-dev/managed-users/alan_smoke/ownership.json".to_string(),
            },
        },
        terminal_profile: ManagedTerminalAccountProfileState::Missing,
        verification: ManagedTerminalAccountVerificationStatus::Passed,
        home_directory_exists: true,
    };
    let handoff = ManagedTerminalAccountProfileHandoff::profile_definition(&request, &ready_state)
        .expect("ready state produces profile handoff");

    assert_eq!(handoff.id, "alan_smoke");
    assert_eq!(
        handoff.launch,
        TerminalProfileLaunch::ManagedUser {
            unix_user: "alan_smoke".to_string()
        }
    );
    assert_eq!(
        handoff.managed_terminal_account_id.as_deref(),
        Some("alan_smoke")
    );
}

#[test]
fn managed_terminal_account_diagnosis_planner_preserves_helper_and_profile_semantics() {
    let request = managed_account_request();
    let missing = managed_account_diagnosis(
        ManagedTerminalAccountReadinessState::AccountMissing,
        ManagedTerminalAccountOwnershipKind::Missing,
        false,
    );
    let plan = ManagedTerminalAccountPlanner::plan_from_diagnosis(request.clone(), &missing, None);
    assert_eq!(plan.status, ManagedTerminalAccountPlanStatus::ReadyToApply);
    assert_eq!(
        plan.steps.iter().map(|step| step.kind).collect::<Vec<_>>(),
        vec![
            ManagedTerminalAccountPlanStepKind::CreateStandardAccount,
            ManagedTerminalAccountPlanStepKind::HideAccount,
            ManagedTerminalAccountPlanStepKind::WriteOwnershipMarker,
            ManagedTerminalAccountPlanStepKind::VerifyAccount,
            ManagedTerminalAccountPlanStepKind::VerifyManagedUserPty,
            ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
        ]
    );

    let unavailable = ManagedTerminalAccountDiagnosis {
        readiness_state: ManagedTerminalAccountReadinessState::HelperUnavailable,
        ..missing
    };
    let plan =
        ManagedTerminalAccountPlanner::plan_from_diagnosis(request.clone(), &unavailable, None);
    assert_eq!(
        plan.status,
        ManagedTerminalAccountPlanStatus::HelperUnavailable
    );
    assert!(plan.steps.is_empty());

    let retired_profile = TerminalProfileDocument {
        default_profile_id: request.terminal_profile_id().to_string(),
        profiles: vec![TerminalProfileDefinition {
            id: request.terminal_profile_id().to_string(),
            title: "Alan Smoke".to_string(),
            launch: TerminalProfileLaunch::SudoUser {
                unix_user: request.account_name.clone(),
            },
            default_working_directory: Some(request.home_directory.clone()),
            presentation: None,
            managed_terminal_account_id: Some(request.account_name.clone()),
        }],
    };
    let ready = managed_account_diagnosis(
        ManagedTerminalAccountReadinessState::Ready,
        ManagedTerminalAccountOwnershipKind::AlanManaged,
        true,
    );
    let plan = ManagedTerminalAccountPlanner::plan_from_diagnosis(
        request.clone(),
        &ready,
        Some(&retired_profile),
    );
    assert_eq!(plan.status, ManagedTerminalAccountPlanStatus::ReadyToApply);
    assert_eq!(
        plan.steps.iter().map(|step| step.kind).collect::<Vec<_>>(),
        vec![ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile]
    );

    let pty_failed = ManagedTerminalAccountDiagnosis {
        readiness_state: ManagedTerminalAccountReadinessState::PtySpawnFailed,
        pty_smoke_verified: false,
        ..ready
    };
    let plan = ManagedTerminalAccountPlanner::plan_from_diagnosis(
        request,
        &pty_failed,
        Some(&retired_profile),
    );
    assert_eq!(
        plan.status,
        ManagedTerminalAccountPlanStatus::PtySpawnFailed
    );
    assert!(
        plan.steps
            .iter()
            .any(|step| step.kind == ManagedTerminalAccountPlanStepKind::VerifyManagedUserPty)
    );
}

#[test]
fn managed_terminal_account_rollback_stays_conservative() {
    let request = managed_account_request();
    let diagnosis = managed_account_diagnosis(
        ManagedTerminalAccountReadinessState::Ready,
        ManagedTerminalAccountOwnershipKind::AlanManaged,
        true,
    );
    let managed_profile = TerminalProfileDocument {
        default_profile_id: request.terminal_profile_id().to_string(),
        profiles: vec![TerminalProfileDefinition {
            id: request.terminal_profile_id().to_string(),
            title: "Alan Smoke".to_string(),
            launch: TerminalProfileLaunch::ManagedUser {
                unix_user: request.account_name.clone(),
            },
            default_working_directory: Some(request.home_directory.clone()),
            presentation: None,
            managed_terminal_account_id: Some(request.account_name.clone()),
        }],
    };

    let integration_only = ManagedTerminalAccountPlanner::rollback_plan(
        request.clone(),
        &diagnosis,
        &ManagedTerminalAccountRollbackScope::AlanIntegrationOnly,
        Some(&managed_profile),
    );
    assert_eq!(
        integration_only
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            ManagedTerminalAccountPlanStepKind::RemoveManagedTerminalProfile,
            ManagedTerminalAccountPlanStepKind::RemoveManagedUserIntegration,
        ]
    );

    let missing_ownership = ManagedTerminalAccountDiagnosis {
        ownership_state: ManagedTerminalAccountOwnershipKind::Missing,
        readiness_state: ManagedTerminalAccountReadinessState::Repairable,
        ..diagnosis.clone()
    };
    let integration_without_marker = ManagedTerminalAccountPlanner::rollback_plan(
        request.clone(),
        &missing_ownership,
        &ManagedTerminalAccountRollbackScope::AlanIntegrationOnly,
        Some(&managed_profile),
    );
    assert_eq!(
        integration_without_marker.status,
        ManagedTerminalAccountPlanStatus::ReadyToApply
    );
    let destructive_without_marker = ManagedTerminalAccountPlanner::rollback_plan(
        request.clone(),
        &missing_ownership,
        &ManagedTerminalAccountRollbackScope::DeleteAccountAndHome {
            confirmation: Some(request.account_name.clone()),
        },
        Some(&managed_profile),
    );
    assert_eq!(
        destructive_without_marker.status,
        ManagedTerminalAccountPlanStatus::AccountNotAlanManaged
    );
    assert_eq!(destructive_without_marker.steps.len(), 1);

    let confirmation_required = ManagedTerminalAccountPlanner::rollback_plan(
        request.clone(),
        &diagnosis,
        &ManagedTerminalAccountRollbackScope::DeleteAccountAndHome { confirmation: None },
        Some(&managed_profile),
    );
    assert_eq!(
        confirmation_required.status,
        ManagedTerminalAccountPlanStatus::RequiresDestructiveConfirmation
    );
    assert_eq!(confirmation_required.steps.len(), 1);

    let confirmed = ManagedTerminalAccountPlanner::rollback_plan(
        request.clone(),
        &diagnosis,
        &ManagedTerminalAccountRollbackScope::DeleteAccountAndHome {
            confirmation: Some(request.account_name),
        },
        Some(&managed_profile),
    );
    assert_eq!(
        confirmed
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            ManagedTerminalAccountPlanStepKind::RemoveManagedTerminalProfile,
            ManagedTerminalAccountPlanStepKind::DeleteAccount,
            ManagedTerminalAccountPlanStepKind::DeleteHomeDirectory,
            ManagedTerminalAccountPlanStepKind::RemoveManagedUserIntegration,
        ]
    );
}

fn profile_document() -> TerminalProfileDocument {
    TerminalProfileDocument {
        default_profile_id: "alan".to_string(),
        profiles: vec![
            TerminalProfileDefinition {
                id: "alan".to_string(),
                title: "Alan".to_string(),
                launch: TerminalProfileLaunch::SudoUser {
                    unix_user: "alan".to_string(),
                },
                default_working_directory: Some("/Users/alan".to_string()),
                presentation: None,
                managed_terminal_account_id: None,
            },
            TerminalProfileDefinition {
                id: "custom".to_string(),
                title: "Custom".to_string(),
                launch: TerminalProfileLaunch::CustomCommand {
                    command: "echo hello".to_string(),
                },
                default_working_directory: Some("/tmp".to_string()),
                presentation: None,
                managed_terminal_account_id: None,
            },
        ],
    }
}

fn managed_account_request() -> ManagedTerminalAccountRequest {
    ManagedTerminalAccountRequest {
        account_name: "alan_smoke".to_string(),
        full_name: Some("Alan Smoke".to_string()),
        shell: "/bin/zsh".to_string(),
        home_directory: "/Users/alan_smoke".to_string(),
        hide_from_login_window: true,
    }
}

fn managed_account_diagnosis(
    readiness_state: ManagedTerminalAccountReadinessState,
    ownership_state: ManagedTerminalAccountOwnershipKind,
    ready: bool,
) -> ManagedTerminalAccountDiagnosis {
    ManagedTerminalAccountDiagnosis {
        ownership_state,
        readiness_state,
        account_exists: ready,
        is_admin: false,
        home_directory_exists: ready,
        home_directory_matches: true,
        shell_matches: ready,
        hidden_from_login_window: ready,
        terminal_profile_id: ready.then(|| "alan_smoke".to_string()),
        pty_smoke_verified: ready,
    }
}

fn launch_environment(
    values: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> TerminalLaunchEnvironment {
    TerminalLaunchEnvironment {
        values: values
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}
