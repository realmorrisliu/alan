use alan_shell_core::{
    EnvelopeVersion, ShellCoreErrorCode, ShellCoreRequestEnvelope, ShellCoreResponseEnvelope,
};
use alan_shell_core_ffi::{ABI_VERSION, handle_request_bytes};
use serde_json::{Value, json};

#[test]
fn facade_describe_reports_c_abi_byte_envelope_boundary() {
    let response = request("facade.describe", json!({}));

    assert!(response.error.is_none());
    let payload = response.payload.expect("describe payload");
    assert_eq!(payload["abi_version"], json!(ABI_VERSION));
    assert_eq!(payload["binding"], json!("c_abi_bytes"));
    assert_eq!(payload["generated_bindings"], json!(false));
    assert!(
        payload["supported_operations"]
            .as_array()
            .expect("supported operations")
            .contains(&json!("reducer.apply"))
    );
    assert!(
        payload["supported_operations"]
            .as_array()
            .expect("supported operations")
            .contains(&json!("terminal_profile.upsert"))
    );
    assert!(
        payload["supported_operations"]
            .as_array()
            .expect("supported operations")
            .contains(&json!("managed_terminal_account.validate_request"))
    );
    assert!(
        payload["supported_operations"]
            .as_array()
            .expect("supported operations")
            .contains(&json!("manifest.validate"))
    );
}

#[test]
fn facade_reports_schema_mismatch_as_response_envelope() {
    let mut envelope = ShellCoreRequestEnvelope::new("facade.describe", json!({}));
    envelope.schema_version = EnvelopeVersion {
        major: EnvelopeVersion::CURRENT.major + 1,
        minor: 0,
    };

    let response = round_trip(&envelope);

    assert!(response.payload.is_none());
    let error = response.error.expect("schema mismatch error");
    assert_eq!(error.code, ShellCoreErrorCode::SchemaVersionMismatch);
    assert_eq!(error.details["supported"], json!(EnvelopeVersion::CURRENT));
}

#[test]
fn facade_reports_unknown_operation_without_panicking() {
    let response = request("does.not.exist", json!({}));

    let error = response.error.expect("unknown operation error");
    assert_eq!(error.code, ShellCoreErrorCode::UnknownOperation);
    assert_eq!(error.details["operation"], json!("does.not.exist"));
}

#[test]
fn facade_dispatches_manifest_materialize_and_reducer_apply() {
    let manifest = request(
        "manifest.default_manifest",
        json!({
            "window_id": "window_main",
            "default_working_directory": "/repo/app",
            "now": "2026-06-17T12:00:00Z"
        }),
    )
    .payload
    .expect("default manifest payload")["manifest"]
        .clone();

    let state = request(
        "manifest.materialize",
        json!({
            "manifest": manifest,
            "default_working_directory": "/fallback",
            "now": "2026-06-17T12:00:00Z"
        }),
    )
    .payload
    .expect("materialize payload")["state"]
        .clone();

    let response = request(
        "reducer.apply",
        json!({
            "state": state,
            "operation": {
                "type": "open_terminal_tab",
                "space_id": null,
                "title": "Logs",
                "working_directory": "/repo/app/logs",
                "terminal_profile_id": null
            }
        }),
    );

    assert!(response.error.is_none());
    let payload = response.payload.expect("reducer payload");
    assert_eq!(payload["status"], json!("ok"));
    assert_eq!(
        payload["result"]["changed_ids"]["created_tab_ids"]
            .as_array()
            .expect("created tab ids")
            .len(),
        1
    );
    assert_eq!(
        payload["result"]["runtime_intents"][0]["type"],
        json!("start_terminal")
    );
}

#[test]
fn manifest_validate_rejects_unknown_nested_fields() {
    let mut manifest = request(
        "manifest.default_manifest",
        json!({
            "window_id": "window_main",
            "default_working_directory": "/repo/app",
            "now": "2026-06-17T12:00:00Z"
        }),
    )
    .payload
    .expect("default manifest payload")["manifest"]
        .clone();
    manifest["spaces"][0]["tabs"][0]["future_field"] = json!(true);

    let response = request(
        "manifest.validate",
        json!({ "manifest_json": manifest.to_string() }),
    );

    assert_eq!(
        response.error.expect("unknown nested field must fail").code,
        ShellCoreErrorCode::InvalidPayload
    );
}

#[test]
fn facade_dispatches_control_and_action_registry_calls() {
    let state = default_state();

    let control = request(
        "control.handle",
        json!({
            "state": state,
            "command": {
                "request_id": "req-state",
                "command": "state"
            }
        }),
    );
    assert!(control.error.is_none());
    assert_eq!(
        control.payload.unwrap()["result"]["response"]["request_id"],
        json!("req-state")
    );

    let shortcut = request(
        "actions.default_shortcut",
        json!({
            "id": "shell.tab.new_terminal",
            "target": {
                "type": "current_selection"
            }
        }),
    );
    assert!(shortcut.error.is_none());
    assert_eq!(
        shortcut.payload.unwrap()["shortcut"],
        json!({
            "key": "t",
            "modifiers": ["command"],
            "context": "shell"
        })
    );
}

#[test]
fn facade_dispatches_terminal_profile_launch_intent() {
    let capture = request(
        "terminal_profile.should_capture_global_default",
        json!({
            "id": "alan",
            "title": "Alan",
            "launch": {
                "kind": "sudo_user",
                "unix_user": "alan"
            },
            "default_working_directory": "/Users/alan"
        }),
    );
    assert!(capture.error.is_none());
    assert_eq!(capture.payload.unwrap()["capture"], json!(false));

    let upsert = request(
        "terminal_profile.upsert",
        json!({
            "draft": {
                "id": " custom ",
                "title": " Custom ",
                "launch_kind": "custom_command",
                "unix_user": "",
                "custom_command": "  echo hi  ",
                "default_working_directory": " /repo/custom "
            },
            "document": {
                "default_profile_id": "",
                "profiles": []
            }
        }),
    );
    assert!(upsert.error.is_none());
    let document = &upsert.payload.expect("upsert payload")["document"];
    assert_eq!(document["default_profile_id"], json!("custom"));
    assert_eq!(document["profiles"][0]["id"], json!("custom"));
    assert_eq!(
        document["profiles"][0]["default_working_directory"],
        json!("/repo/custom")
    );

    let response = request(
        "terminal_profile.resolve_launch_intent",
        json!({
            "terminal_profile_reference": "alan",
            "terminal_profiles": {
                "default_profile_id": "login_shell",
                "profiles": [
                    {
                        "id": "login_shell",
                        "title": "Login shell",
                        "launch": {
                            "kind": "login_shell"
                        },
                        "presentation": {
                            "symbol_name": "terminal"
                        }
                    },
                    {
                        "id": "alan",
                        "title": "Alan",
                        "launch": {
                            "kind": "sudo_user",
                            "unix_user": "alan"
                        },
                        "default_working_directory": "/Users/alan"
                    }
                ]
            },
            "availability": {
                "executable_paths": ["/usr/bin/sudo", "/bin/zsh"],
                "enforce": true
            },
            "environment": {
                "values": {
                    "SHELL": "/bin/zsh"
                }
            }
        }),
    );

    assert!(response.error.is_none());
    let intent = &response.payload.expect("launch intent payload")["intent"];
    assert_eq!(intent["strategy"], json!("terminal_profile_sudo_user"));
    assert_eq!(intent["launch_path"], json!("/usr/bin/sudo"));
    assert_eq!(intent["arguments"], json!(["-iu", "alan"]));
    assert_eq!(intent["working_directory"], json!("/Users/alan"));
    assert_eq!(
        intent["profile_environment"]["ALAN_TERMINAL_PROFILE_KIND"],
        json!("sudo_user")
    );
}

#[test]
fn facade_dispatches_managed_terminal_account_validation() {
    let invalid = request(
        "managed_terminal_account.validate_request",
        json!({
            "account_name": "root",
            "full_name": null,
            "shell": "/bin/zsh",
            "home_directory": "/Users/root",
            "hide_from_login_window": true
        }),
    );
    assert!(invalid.error.is_none());
    assert_eq!(
        invalid.payload.expect("validation payload")["errors"][0]["type"],
        json!("reserved_account_name")
    );
}

#[test]
fn facade_invalid_json_uses_nil_request_id_error_envelope() {
    let response: ShellCoreResponseEnvelope =
        serde_json::from_slice(&handle_request_bytes(b"{not-json")).expect("error response");

    assert!(response.payload.is_none());
    assert_eq!(
        response.error.expect("invalid json error").code,
        ShellCoreErrorCode::InvalidPayload
    );
}

fn request(operation: &str, payload: Value) -> ShellCoreResponseEnvelope {
    round_trip(&ShellCoreRequestEnvelope::new(operation, payload))
}

fn round_trip(envelope: &ShellCoreRequestEnvelope) -> ShellCoreResponseEnvelope {
    let request_bytes = serde_json::to_vec(envelope).expect("request serializes");
    let response_bytes = handle_request_bytes(&request_bytes);
    serde_json::from_slice(&response_bytes).expect("response envelope")
}

fn default_state() -> Value {
    let manifest = request(
        "manifest.default_manifest",
        json!({
            "window_id": "window_main",
            "default_working_directory": "/repo/app",
            "now": "2026-06-17T12:00:00Z"
        }),
    )
    .payload
    .expect("default manifest payload")["manifest"]
        .clone();

    request(
        "manifest.materialize",
        json!({
            "manifest": manifest,
            "default_working_directory": "/fallback",
            "now": "2026-06-17T12:00:00Z"
        }),
    )
    .payload
    .expect("materialize payload")["state"]
        .clone()
}
