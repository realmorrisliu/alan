use alan_shell_core::{
    FixtureCase, FixtureCorpus, FixtureKind, FixtureSource, ShellActionId, ShellActionRegistry,
    ShellActionShortcut, ShellActionTarget, WorkspaceState,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

const SWIFT_ACTION_FIXTURES: &[&str] = &[
    "actions/standard-shortcuts",
    "actions/keyboard-pane-zoom",
    "actions/context-tab-close",
    "actions/quick-terminal-promote",
    "actions/pane-move-left",
];

#[test]
fn rust_action_registry_matches_swift_exported_fixtures() {
    let corpus = FixtureCorpus::load(fixtures_root()).expect("fixtures load");

    for fixture_id in SWIFT_ACTION_FIXTURES {
        let case = corpus
            .case(fixture_id)
            .unwrap_or_else(|| panic!("missing Swift action fixture {fixture_id}"));
        assert_eq!(case.kind, FixtureKind::ActionRegistry);
        assert_eq!(case.source, FixtureSource::Swift);

        let actual = apply_action_fixture(case);
        case.assert_expected_matches(&actual)
            .unwrap_or_else(|error| panic!("{fixture_id}: {error}"));
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn apply_action_fixture(case: &FixtureCase) -> Value {
    let operation_type = case
        .operation
        .get("type")
        .and_then(Value::as_str)
        .expect("action operation type");
    let registry = ShellActionRegistry::standard();

    match operation_type {
        "standard_shortcuts" => {
            let operation: StandardShortcutsOperation =
                serde_json::from_value(case.operation.clone()).expect("standard shortcuts op");
            let shortcuts = operation
                .requests
                .into_iter()
                .map(|request| {
                    json!({
                        "id": request.id,
                        "shortcut": registry.default_shortcut(request.id, &request.target),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "status": "ok",
                "shortcuts": shortcuts,
            })
        }
        "keyboard_action" => {
            let operation: KeyboardActionOperation =
                serde_json::from_value(case.operation.clone()).expect("keyboard action op");
            json!({
                "status": "ok",
                "keyboard_action": registry.keyboard_action(&operation.shortcut),
            })
        }
        "execute" => {
            let state: WorkspaceState =
                serde_json::from_value(case.input.clone()).expect("workspace state input");
            let operation: ExecuteActionOperation =
                serde_json::from_value(case.operation.clone()).expect("execute action op");
            json!({
                "status": "ok",
                "result": registry.execute(operation.id, &operation.target, &state),
            })
        }
        other => panic!("unsupported action operation {other}"),
    }
}

#[derive(Debug, Deserialize)]
struct StandardShortcutsOperation {
    requests: Vec<ShortcutRequest>,
}

#[derive(Debug, Deserialize)]
struct ShortcutRequest {
    id: ShellActionId,
    target: ShellActionTarget,
}

#[derive(Debug, Deserialize)]
struct KeyboardActionOperation {
    shortcut: ShellActionShortcut,
}

#[derive(Debug, Deserialize)]
struct ExecuteActionOperation {
    id: ShellActionId,
    target: ShellActionTarget,
}
