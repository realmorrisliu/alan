use alan_shell_core::{
    FixtureCase, FixtureCorpus, FixtureKind, FixtureSource, ManagedTerminalAccountSettingsSummary,
    ShellSettingsCapabilitiesSummary, ShellSettingsDiagnosticsSummary, ShellSettingsLocalSummary,
    ShellSettingsSummaryRows, TerminalProfileSettingsSummary,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

const SWIFT_SETTINGS_SUMMARY_FIXTURES: &[&str] = &[
    "settings-summary/terminal-profile-rows",
    "settings-summary/managed-account-rows",
    "settings-summary/capability-rows",
    "settings-summary/local-diagnostics-rows",
];

#[test]
fn rust_settings_summaries_match_swift_exported_fixtures() {
    let corpus = FixtureCorpus::load(fixtures_root()).expect("fixtures load");

    for fixture_id in SWIFT_SETTINGS_SUMMARY_FIXTURES {
        let case = corpus
            .case(fixture_id)
            .unwrap_or_else(|| panic!("missing Swift settings summary fixture {fixture_id}"));
        assert_eq!(case.kind, FixtureKind::SettingsSummary);
        assert_eq!(case.source, FixtureSource::Swift);

        let actual = apply_settings_summary_fixture(case);
        case.assert_expected_matches(&actual)
            .unwrap_or_else(|error| panic!("{fixture_id}: {error}"));
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn apply_settings_summary_fixture(case: &FixtureCase) -> Value {
    let operation: SettingsSummaryFixtureOperation =
        serde_json::from_value(case.operation.clone()).expect("settings summary operation");

    match operation.operation_type.as_str() {
        "terminal_profile_rows" => {
            let summary: TerminalProfileSettingsSummary =
                serde_json::from_value(case.input.clone()).expect("terminal profile summary");
            json!({
                "rows": ShellSettingsSummaryRows::terminal_profile_rows(&summary),
            })
        }
        "managed_terminal_account_rows" => {
            let summary: ManagedTerminalAccountSettingsSummary =
                serde_json::from_value(case.input.clone()).expect("managed account summary");
            json!({
                "rows": ShellSettingsSummaryRows::managed_terminal_account_rows(&summary),
            })
        }
        "capability_rows" => {
            let summary: ShellSettingsCapabilitiesSummary =
                serde_json::from_value(case.input.clone()).expect("capabilities summary");
            json!({
                "rows": ShellSettingsSummaryRows::capability_rows(&summary),
            })
        }
        "local_rows" => {
            let input: LocalRowsFixtureInput =
                serde_json::from_value(case.input.clone()).expect("local rows input");
            json!({
                "rows": ShellSettingsSummaryRows::local_rows(&input.local, &input.diagnostics),
            })
        }
        other => panic!("unsupported settings summary operation {other}"),
    }
}

#[derive(Debug, Deserialize)]
struct SettingsSummaryFixtureOperation {
    #[serde(rename = "type")]
    operation_type: String,
}

#[derive(Debug, Deserialize)]
struct LocalRowsFixtureInput {
    local: ShellSettingsLocalSummary,
    diagnostics: ShellSettingsDiagnosticsSummary,
}
