use alan_shell_core::{FixtureCase, FixtureCorpus, FixtureKind, FixtureSource};
use alan_shell_core::{ReducerOperation, WorkspaceState};
use serde_json::{Value, json};
use std::path::PathBuf;

const SWIFT_REDUCER_FIXTURES: &[&str] = &[
    "reducer/open-terminal-tab",
    "reducer/create-terminal-space",
    "reducer/split-pane-right",
    "reducer/focus-adjacent-right",
    "reducer/close-selected-pane",
    "reducer/close-tab",
    "reducer/rename-tab",
    "reducer/pin-tab",
    "reducer/unpin-tab",
    "reducer/set-attention",
    "reducer/update-terminal-metadata",
    "reducer/apply-agent-activity",
    "reducer/duplicate-tab",
    "reducer/move-tab-to-space",
    "reducer/move-tab-within-section",
    "reducer/clear-inactive-temporary-tabs",
    "reducer/clear-inactive-temporary-tabs-active-task-metadata",
    "reducer/move-pane-within-tab",
    "reducer/move-pane-to-new-tab",
    "reducer/move-pane-to-tab",
    "reducer/zoom-pane",
    "reducer/unzoom-tab",
    "reducer/close-zoomed-pane-prunes-zoom",
    "reducer/zoom-single-pane-error",
    "reducer/unzoom-unzoomed-tab-error",
    "reducer/move-pane-to-new-tab-last-pane-error",
    "reducer/move-pane-within-tab-invalid-target-error",
    "reducer/split-missing-pane-error",
];

#[test]
fn rust_reducer_matches_swift_exported_state_fixtures() {
    let corpus = FixtureCorpus::load(fixtures_root()).expect("fixtures load");

    for fixture_id in SWIFT_REDUCER_FIXTURES {
        let case = corpus
            .case(fixture_id)
            .unwrap_or_else(|| panic!("missing Swift reducer fixture {fixture_id}"));
        assert_eq!(case.kind, FixtureKind::Reducer);
        assert_eq!(case.source, FixtureSource::Swift);

        let actual = apply_reducer_fixture(case);
        case.assert_expected_matches(&actual)
            .unwrap_or_else(|error| panic!("{fixture_id}: {error}"));
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn apply_reducer_fixture(case: &FixtureCase) -> Value {
    let state: WorkspaceState =
        serde_json::from_value(case.input.clone()).expect("reducer fixture state input");
    let operation: ReducerOperation =
        serde_json::from_value(case.operation.clone()).expect("reducer fixture operation");

    match state.reduce(operation) {
        Ok(result) => json!({
            "status": "ok",
            "state": result.state,
        }),
        Err(error) => json!({
            "status": "error",
            "error_code": error.code,
            "state": state,
        }),
    }
}
