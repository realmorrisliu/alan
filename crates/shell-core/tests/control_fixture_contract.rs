use alan_shell_core::{
    FixtureCase, FixtureCorpus, FixtureKind, FixtureSource, ShellControlCommand,
    ShellControlResponse, WorkspaceState,
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

const SWIFT_CONTROL_FIXTURES: &[&str] = &[
    "control-command/state",
    "control-command/tab-open",
    "control-command/pane-split",
    "control-command/pane-split-missing-direction",
    "control-command/pane-focus",
];

#[test]
fn rust_control_reducer_matches_swift_exported_command_fixtures() {
    let corpus = FixtureCorpus::load(fixtures_root()).expect("fixtures load");

    for fixture_id in SWIFT_CONTROL_FIXTURES {
        let case = corpus
            .case(fixture_id)
            .unwrap_or_else(|| panic!("missing Swift control fixture {fixture_id}"));
        assert_eq!(case.kind, FixtureKind::ControlCommand);
        assert_eq!(case.source, FixtureSource::Swift);

        let actual = apply_control_fixture(case);
        case.assert_expected_matches(&actual)
            .unwrap_or_else(|error| panic!("{fixture_id}: {error}"));
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn apply_control_fixture(case: &FixtureCase) -> Value {
    let state: WorkspaceState =
        serde_json::from_value(case.input.clone()).expect("control fixture state input");
    let command: ShellControlCommand =
        serde_json::from_value(case.operation.clone()).expect("control fixture command");
    let result = state.reduce_control(command);

    let mut output = Map::new();
    output.insert("status".to_string(), json!("handled"));
    output.insert(
        "response".to_string(),
        normalized_response(&result.response),
    );
    insert_some(&mut output, "updated_state", result.updated_state.as_ref());
    Value::Object(output)
}

fn normalized_response(response: &ShellControlResponse) -> Value {
    let mut output = Map::new();
    output.insert("request_id".to_string(), json!(response.request_id));
    output.insert(
        "contract_version".to_string(),
        json!(response.contract_version),
    );
    insert_some(&mut output, "applied", response.applied.as_ref());
    insert_some(&mut output, "state_snapshot", response.state.as_ref());
    insert_some(
        &mut output,
        "focused_pane_slot_id",
        response.focused_pane_slot_id.as_ref(),
    );
    insert_some(&mut output, "space_id", response.space_id.as_ref());
    insert_some(&mut output, "tab_id", response.tab_id.as_ref());
    insert_some(
        &mut output,
        "pane_slot_id",
        response.pane_slot_id.as_ref().or(response.pane_id.as_ref()),
    );
    insert_some(&mut output, "content_id", response.content_id.as_ref());
    insert_some(&mut output, "content_kind", response.content_kind.as_ref());
    insert_some(&mut output, "error_code", response.error_code.as_ref());
    insert_some(
        &mut output,
        "error_message",
        response.error_message.as_ref(),
    );
    Value::Object(output)
}

fn insert_some<T: Serialize>(output: &mut Map<String, Value>, key: &str, value: Option<&T>) {
    if let Some(value) = value {
        output.insert(
            key.to_string(),
            serde_json::to_value(value).expect("control fixture projection serializes"),
        );
    }
}
