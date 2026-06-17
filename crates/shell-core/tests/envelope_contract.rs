use alan_shell_core::{
    EnvelopeVersion, ShellCoreErrorCode, ShellCoreRequestEnvelope, ShellCoreResponseEnvelope,
};
use serde_json::json;

#[test]
fn request_envelope_rejects_incompatible_major_schema_version() {
    let mut request = ShellCoreRequestEnvelope::new("model.describe", json!({"probe": true}));
    request.schema_version = EnvelopeVersion {
        major: EnvelopeVersion::CURRENT.major + 1,
        minor: 0,
    };

    let error = request.ensure_supported().unwrap_err();

    assert_eq!(error.code, ShellCoreErrorCode::SchemaVersionMismatch);
    assert_eq!(error.details["supported"], json!(EnvelopeVersion::CURRENT));
    assert_eq!(error.details["received"], json!(request.schema_version));
}

#[test]
fn response_envelope_serializes_success_and_error_without_losing_request_id() {
    let request = ShellCoreRequestEnvelope::new("model.describe", json!({"probe": true}));
    let success = ShellCoreResponseEnvelope::success(request.id, json!({"ok": true}));
    let success_json = serde_json::to_value(&success).unwrap();

    assert_eq!(success_json["request_id"], json!(request.id));
    assert_eq!(success_json["payload"], json!({"ok": true}));
    assert!(success_json.get("error").unwrap().is_null());

    let error = ShellCoreResponseEnvelope::error(
        request.id,
        ShellCoreErrorCode::SchemaVersionMismatch
            .envelope("unsupported shell-core schema version")
            .with_detail("supported", json!(EnvelopeVersion::CURRENT))
            .with_detail("received", json!({"major": 99, "minor": 0})),
    );
    let round_trip: ShellCoreResponseEnvelope =
        serde_json::from_value(serde_json::to_value(error).unwrap()).unwrap();

    assert_eq!(round_trip.request_id, request.id);
    assert_eq!(
        round_trip.error.unwrap().code,
        ShellCoreErrorCode::SchemaVersionMismatch
    );
}
