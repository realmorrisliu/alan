use alan_shell_core::{
    ManagedTerminalAccountPlan, ManagedTerminalAccountPlanStatus, ManagedTerminalAccountPlanner,
    ManagedTerminalAccountProfileState, ManagedTerminalAccountRecord,
    ManagedTerminalAccountRequest, ManagedTerminalAccountSettingsSummary,
    ManagedTerminalAccountState, ManagedTerminalAccountVerificationStatus,
    ShellSettingsDiagnosticsSummary, ShellSettingsLocalSummary, ShellSettingsRowMutability,
    ShellSettingsSummaryRows, TerminalProfileDefinition, TerminalProfileLaunch,
    TerminalProfilePresentation, TerminalProfileSettingsSummary,
};

#[test]
fn terminal_profile_rows_match_swift_summary_semantics() {
    let summary = TerminalProfileSettingsSummary {
        default_profile_id: "login_shell".to_string(),
        recovery_message: Some("Recovered local store.".to_string()),
        profiles: vec![
            TerminalProfileDefinition::login_shell_fallback(),
            TerminalProfileDefinition {
                id: "alan".to_string(),
                title: "Alan".to_string(),
                launch: TerminalProfileLaunch::SudoUser {
                    unix_user: "alan".to_string(),
                },
                default_working_directory: Some("/Users/alan".to_string()),
                presentation: Some(TerminalProfilePresentation {
                    symbol_name: Some("person.crop.circle".to_string()),
                    color_name: None,
                }),
                managed_terminal_account_id: Some("alan".to_string()),
            },
        ],
    };

    let rows = ShellSettingsSummaryRows::terminal_profile_rows(&summary);

    assert_eq!(rows[0].id, "terminalProfilesDefault");
    assert_eq!(rows[0].value.as_deref(), Some("Login shell"));
    assert_eq!(rows[1].value.as_deref(), Some("Create…"));
    assert!(rows.iter().any(|row| row.id == "terminalProfilesRecovery"));
    let alan = rows
        .iter()
        .find(|row| row.id == "terminalProfile.alan")
        .expect("managed profile row");
    assert_eq!(alan.system_name, "person.crop.circle");
    assert_eq!(alan.detail.as_deref(), Some("Sudo user alan"));
    assert_eq!(alan.value.as_deref(), Some("Managed"));
    assert_eq!(alan.mutability, ShellSettingsRowMutability::ReadOnly);
}

#[test]
fn managed_account_rows_project_plan_status_and_detail() {
    let request = managed_account_request();
    let state = ManagedTerminalAccountState {
        account: ManagedTerminalAccountRecord::Missing,
        ownership: Default::default(),
        terminal_profile: ManagedTerminalAccountProfileState::Missing,
        verification: ManagedTerminalAccountVerificationStatus::NotRun,
        home_directory_exists: false,
    };
    let plan = ManagedTerminalAccountPlanner::plan(request, &state);
    let summary = ManagedTerminalAccountSettingsSummary { plans: vec![plan] };

    let rows = ShellSettingsSummaryRows::managed_terminal_account_rows(&summary);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "terminalAccount.alan_smoke");
    assert_eq!(rows[0].system_name, "person.crop.circle.badge.plus");
    assert_eq!(rows[0].value.as_deref(), Some("Preview"));
    assert_eq!(
        rows[0].detail.as_deref(),
        Some("alan_smoke terminal entry plan is ready for explicit confirmation.")
    );
    assert_eq!(rows[0].mutability, ShellSettingsRowMutability::ActionOnly);

    let empty_rows = ShellSettingsSummaryRows::managed_terminal_account_rows(
        &ManagedTerminalAccountSettingsSummary::default(),
    );
    assert_eq!(
        empty_rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["terminalAccountProvision", "terminalAccountLoginBoundary"]
    );
    assert_eq!(empty_rows[0].value.as_deref(), Some("Preview…"));

    let unmanaged_rows = ShellSettingsSummaryRows::managed_terminal_account_rows(
        &ManagedTerminalAccountSettingsSummary {
            plans: vec![ManagedTerminalAccountPlan {
                request: managed_account_request(),
                status: ManagedTerminalAccountPlanStatus::AccountNotAlanManaged,
                steps: vec![],
            }],
        },
    );
    assert_eq!(unmanaged_rows[0].value.as_deref(), Some("Not managed"));
    assert_eq!(
        unmanaged_rows[0].detail.as_deref(),
        Some("alan_smoke is an existing local account outside Alan management.")
    );

    let pty_failure_rows = ShellSettingsSummaryRows::managed_terminal_account_rows(
        &ManagedTerminalAccountSettingsSummary {
            plans: vec![ManagedTerminalAccountPlan {
                request: managed_account_request(),
                status: ManagedTerminalAccountPlanStatus::PtySpawnFailed,
                steps: vec![],
            }],
        },
    );
    assert_eq!(pty_failure_rows[0].value.as_deref(), Some("PTY failed"));
    assert_eq!(
        pty_failure_rows[0].detail.as_deref(),
        Some("alan_smoke account exists, but helper-managed PTY startup failed.")
    );
}

#[test]
fn local_rows_match_compact_settings_copy() {
    let local = ShellSettingsLocalSummary {
        bundle_identifier: "app.alanworks.macos.dev".to_string(),
        channel_label: "Dev".to_string(),
        cli_tool_name: "alan-dev".to_string(),
        update_summary: "Manual local build".to_string(),
        update_detail: "Use manual updates.".to_string(),
        system_store_display_path: "~/Library/Application Support/Alan/System Store/dev"
            .to_string(),
        application_support_display_path: "~/Library/Application Support/Alan Dev".to_string(),
        shell_control_namespace: "alan-dev-shell-control".to_string(),
    };
    let diagnostics = ShellSettingsDiagnosticsSummary {
        is_enabled: true,
        retained_event_count: 7,
        stutter_marker_count: 1,
        last_export_url: None,
    };
    let local_rows = ShellSettingsSummaryRows::local_rows(&local, &diagnostics);
    let diagnostics_export = local_rows
        .iter()
        .find(|row| row.id == "performanceDiagnosticsExport")
        .expect("diagnostics export row");
    assert_eq!(
        diagnostics_export.detail.as_deref(),
        Some("7 retained events, 1 stutter marker.")
    );
    assert_eq!(
        local_rows
            .iter()
            .find(|row| row.id == "performanceDiagnostics")
            .and_then(|row| row.value.as_deref()),
        Some("Enabled")
    );
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
