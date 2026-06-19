use alan_shell_core::{
    FixtureCase, FixtureCorpus, FixtureKind, FixtureSource, ShellContentWorkspaceManifest,
    ShellWorkspaceManifest,
};
use serde_json::{Value, json};
use std::path::PathBuf;

const SWIFT_MANIFEST_FIXTURES: &[&str] = &[
    "manifest/default-manifest-materialize",
    "manifest/materialize-empty-selected-space",
    "manifest/pruning-expired-tabs",
    "manifest/materialize-pinned-snapshot",
    "manifest/migrate-legacy-terminal-manifest",
    "manifest/materialize-quick-terminal",
    "manifest/materialize-missing-profile-reference",
    "manifest/decode-corrupt-input",
    "manifest/decode-malformed-content-manifest",
];

#[test]
fn rust_manifest_behavior_matches_swift_exported_fixtures() {
    let corpus = FixtureCorpus::load(fixtures_root()).expect("fixtures load");

    for fixture_id in SWIFT_MANIFEST_FIXTURES {
        let case = corpus
            .case(fixture_id)
            .unwrap_or_else(|| panic!("missing Swift manifest fixture {fixture_id}"));
        assert_eq!(case.kind, FixtureKind::Manifest);
        assert_eq!(case.source, FixtureSource::Swift);

        let actual = apply_manifest_fixture(case);
        case.assert_expected_matches(&actual)
            .unwrap_or_else(|error| panic!("{fixture_id}: {error}"));
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn apply_manifest_fixture(case: &FixtureCase) -> Value {
    let operation_type = case
        .operation
        .get("type")
        .and_then(Value::as_str)
        .expect("manifest operation type");

    match operation_type {
        "default_manifest" => {
            let window_id = operation_string(&case.operation, "window_id");
            let default_working_directory =
                operation_string(&case.operation, "default_working_directory");
            let now = operation_string(&case.operation, "now");
            let materialize_default_working_directory =
                operation_string(&case.operation, "materialize_default_working_directory");
            let manifest = ShellContentWorkspaceManifest::default_manifest(
                window_id,
                default_working_directory,
                now,
            );
            let state = manifest.materialize(materialize_default_working_directory, now);
            json!({
                "status": "ok",
                "manifest": manifest,
                "state": state,
            })
        }
        "materialize" => {
            let manifest: ShellContentWorkspaceManifest =
                serde_json::from_value(case.input.clone()).expect("manifest fixture input");
            let default_working_directory =
                operation_string(&case.operation, "default_working_directory");
            let now = operation_string(&case.operation, "now");
            json!({
                "status": "ok",
                "state": manifest.materialize(default_working_directory, now),
            })
        }
        "pruning_expired_tabs" => {
            let manifest: ShellContentWorkspaceManifest =
                serde_json::from_value(case.input.clone()).expect("manifest fixture input");
            let now = operation_string(&case.operation, "now");
            let ttl_seconds = case
                .operation
                .get("ttl_seconds")
                .and_then(Value::as_i64)
                .expect("ttl seconds");
            json!({
                "status": "ok",
                "manifest": manifest.pruning_expired_tabs(now, ttl_seconds),
            })
        }
        "migrate_legacy_terminal_manifest" => {
            let manifest: ShellWorkspaceManifest =
                serde_json::from_value(case.input.clone()).expect("legacy manifest fixture input");
            json!({
                "status": "ok",
                "manifest": manifest.migrating_terminal_restore_snapshots_to_content_containers(),
            })
        }
        "decode_content_manifest_json" => {
            let manifest_json = case
                .input
                .as_str()
                .expect("decode content manifest JSON input");
            match serde_json::from_str::<ShellContentWorkspaceManifest>(manifest_json) {
                Ok(manifest) => json!({
                    "status": "ok",
                    "manifest": manifest,
                }),
                Err(_) => json!({
                    "status": "error",
                    "error_code": "decode_error",
                }),
            }
        }
        other => panic!("unsupported manifest operation {other}"),
    }
}

fn operation_string<'a>(operation: &'a Value, key: &str) -> &'a str {
    operation
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing manifest operation field {key}"))
}
