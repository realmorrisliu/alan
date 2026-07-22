//! Hand-written C ABI facade for `alan-shell-core`.
//!
//! This crate is intentionally separate from the pure Rust core. It exposes a
//! small synchronous byte-envelope boundary for Swift and future platform
//! adapters without shaping the core API around binding-generator constraints.

use alan_shell_core::{
    EnvelopeVersion, ManagedTerminalAccountDiagnosis, ManagedTerminalAccountIdentifierValidator,
    ManagedTerminalAccountPlanner, ManagedTerminalAccountRequest,
    ManagedTerminalAccountRollbackScope, ManagedTerminalAccountSettingsSummary, ReducerError,
    ReducerOperation, ShellActionId, ShellActionRegistry, ShellActionShortcut, ShellActionTarget,
    ShellContentWorkspaceManifest, ShellControlCommand, ShellControlExecutionContext,
    ShellCoreErrorCode, ShellCoreErrorEnvelope, ShellCoreRequestEnvelope,
    ShellCoreResponseEnvelope, ShellSettingsDiagnosticsSummary, ShellSettingsLocalSummary,
    ShellSettingsSummaryRows, TerminalExecutableAvailability, TerminalLaunchEnvironment,
    TerminalLaunchIntent, TerminalProfileDefinition, TerminalProfileDocument,
    TerminalProfileEditor, TerminalProfileEditorDraft, TerminalProfileSettingsSummary,
    TerminalProfileValidator, WorkspaceState, should_capture_global_default_terminal_profile,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

/// Current hand-written C ABI version.
pub const ABI_VERSION: u32 = 1;

/// Byte buffer returned by the C ABI.
///
/// Callers must release non-null buffers with `alan_shell_core_ffi_free_buffer`.
#[repr(C)]
pub struct AlanShellCoreByteBuffer {
    /// Owned byte pointer.
    pub ptr: *mut u8,
    /// Byte length.
    pub len: usize,
}

/// Returns the ABI version implemented by this dynamic library.
#[unsafe(no_mangle)]
pub extern "C" fn alan_shell_core_ffi_abi_version() -> u32 {
    ABI_VERSION
}

/// Handles one JSON-encoded `ShellCoreRequestEnvelope` and returns a JSON
/// `ShellCoreResponseEnvelope`.
///
/// # Safety
///
/// When `len` is non-zero, `ptr` must point to `len` readable bytes for the
/// duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn alan_shell_core_ffi_handle_request(
    ptr: *const u8,
    len: usize,
) -> AlanShellCoreByteBuffer {
    if ptr.is_null() && len > 0 {
        return owned_buffer(invalid_request_response("request pointer is null"));
    }

    let request = if len == 0 {
        &[]
    } else {
        // SAFETY: The caller provides a pointer/length pair. The bytes are only
        // borrowed for the duration of this function and copied into the
        // response before returning.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    owned_buffer(handle_request_bytes(request))
}

/// Handles one JSON request and writes the owned response pointer/length to
/// out-parameters.
///
/// This exists for Swift dynamic loading, where C function pointers returning a
/// custom struct are not representable as `@convention(c)` function types.
///
/// # Safety
///
/// When `len` is non-zero, `ptr` must point to `len` readable bytes for the
/// duration of the call. `out_ptr` and `out_len` must be valid writable
/// pointers. The returned pointer must be released with
/// `alan_shell_core_ffi_free_bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn alan_shell_core_ffi_handle_request_out(
    ptr: *const u8,
    len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> u8 {
    if out_ptr.is_null() || out_len.is_null() {
        return 0;
    }

    // SAFETY: This function has the same pointer/length contract as
    // `alan_shell_core_ffi_handle_request`.
    let buffer = unsafe { alan_shell_core_ffi_handle_request(ptr, len) };
    // SAFETY: The caller provided valid writable out-pointers.
    unsafe {
        *out_ptr = buffer.ptr;
        *out_len = buffer.len;
    }
    1
}

/// Frees a byte buffer returned by `alan_shell_core_ffi_handle_request`.
#[unsafe(no_mangle)]
pub extern "C" fn alan_shell_core_ffi_free_buffer(buffer: AlanShellCoreByteBuffer) {
    if buffer.ptr.is_null() {
        return;
    }
    free_owned_bytes(buffer.ptr, buffer.len);
}

/// Frees a byte buffer returned by `alan_shell_core_ffi_handle_request_out`.
#[unsafe(no_mangle)]
pub extern "C" fn alan_shell_core_ffi_free_bytes(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    free_owned_bytes(ptr, len);
}

fn free_owned_bytes(ptr: *mut u8, len: usize) {
    // SAFETY: `owned_buffer` creates buffers from boxed `[u8]` values with this
    // exact pointer and length. Reconstructing the boxed slice drops it once.
    unsafe {
        let slice = std::ptr::slice_from_raw_parts_mut(ptr, len);
        drop(Box::from_raw(slice));
    }
}

/// Handles one request envelope as bytes.
pub fn handle_request_bytes(request: &[u8]) -> Vec<u8> {
    let request: ShellCoreRequestEnvelope = match serde_json::from_slice(request) {
        Ok(request) => request,
        Err(error) => {
            return invalid_request_response(format!("failed to decode request envelope: {error}"));
        }
    };

    let response = match request.ensure_supported() {
        Ok(()) => dispatch_request(request),
        Err(error) => ShellCoreResponseEnvelope::error(request.id, error),
    };
    serialize_response(&response)
}

fn dispatch_request(request: ShellCoreRequestEnvelope) -> ShellCoreResponseEnvelope {
    let request_id = request.id;
    let result = match request.operation.as_str() {
        "facade.describe" => Ok(json!({
            "abi_version": ABI_VERSION,
            "binding": "c_abi_bytes",
            "schema_version": EnvelopeVersion::CURRENT,
            "generated_bindings": false,
            "supported_operations": supported_operations(),
        })),
        "manifest.default_manifest" => default_manifest(request.payload),
        "manifest.validate" => validate_manifest(request.payload),
        "manifest.materialize" => materialize_manifest(request.payload),
        "manifest.pruning_expired_tabs" => pruning_expired_tabs(request.payload),
        "reducer.apply" => apply_reducer(request.payload),
        "control.handle" => handle_control(request.payload),
        "actions.standard_descriptors" => standard_action_descriptors(request.payload),
        "actions.default_shortcut" => default_action_shortcut(request.payload),
        "actions.keyboard_action" => keyboard_action(request.payload),
        "actions.execute" => execute_action(request.payload),
        "terminal_profile.validate" => validate_terminal_profile(request.payload),
        "terminal_profile.make_definition" => make_terminal_profile_definition(request.payload),
        "terminal_profile.upsert" => upsert_terminal_profile(request.payload),
        "terminal_profile.should_capture_global_default" => {
            should_capture_global_default(request.payload)
        }
        "terminal_profile.resolve_launch_intent" => resolve_terminal_launch_intent(request.payload),
        "managed_terminal_account.validate_request" => {
            validate_managed_terminal_account_request(request.payload)
        }
        "managed_terminal_account.plan" => plan_managed_terminal_account(request.payload),
        "settings.terminal_profile_rows" => terminal_profile_rows(request.payload),
        "settings.managed_terminal_account_rows" => managed_terminal_account_rows(request.payload),
        "settings.local_rows" => local_rows(request.payload),
        operation => Err(ShellCoreErrorCode::UnknownOperation
            .envelope("unknown shell-core facade operation")
            .with_detail("operation", json!(operation))),
    };

    match result {
        Ok(payload) => ShellCoreResponseEnvelope::success(request_id, payload),
        Err(error) => ShellCoreResponseEnvelope::error(request_id, error),
    }
}

fn supported_operations() -> &'static [&'static str] {
    &[
        "facade.describe",
        "manifest.default_manifest",
        "manifest.validate",
        "manifest.materialize",
        "manifest.pruning_expired_tabs",
        "reducer.apply",
        "control.handle",
        "actions.standard_descriptors",
        "actions.default_shortcut",
        "actions.keyboard_action",
        "actions.execute",
        "terminal_profile.validate",
        "terminal_profile.make_definition",
        "terminal_profile.upsert",
        "terminal_profile.should_capture_global_default",
        "terminal_profile.resolve_launch_intent",
        "managed_terminal_account.validate_request",
        "managed_terminal_account.plan",
        "settings.terminal_profile_rows",
        "settings.managed_terminal_account_rows",
        "settings.local_rows",
    ]
}

fn default_manifest(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: DefaultManifestInput = decode_payload(payload, "manifest.default_manifest")?;
    Ok(json!({
        "manifest": ShellContentWorkspaceManifest::default_manifest(
            &input.window_id,
            &input.default_working_directory,
            &input.now,
        ),
    }))
}

fn validate_manifest(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: ValidateManifestInput = decode_payload(payload, "manifest.validate")?;
    Ok(json!({
        "valid": serde_json::from_str::<ShellContentWorkspaceManifest>(&input.manifest_json)
            .is_ok(),
    }))
}

fn materialize_manifest(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: MaterializeManifestInput = decode_payload(payload, "manifest.materialize")?;
    Ok(json!({
        "state": input
            .manifest
            .materialize(&input.default_working_directory, &input.now),
    }))
}

fn pruning_expired_tabs(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: PruningExpiredTabsInput = decode_payload(payload, "manifest.pruning_expired_tabs")?;
    Ok(json!({
        "manifest": input.manifest.pruning_expired_tabs(&input.now, input.ttl_seconds),
    }))
}

fn apply_reducer(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: ReducerApplyInput = decode_payload(payload, "reducer.apply")?;
    Ok(match input.state.reduce(input.operation) {
        Ok(result) => json!({
            "status": "ok",
            "result": result,
        }),
        Err(error) => reducer_error_payload(error, input.state),
    })
}

fn handle_control(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: ControlHandleInput = decode_payload(payload, "control.handle")?;
    Ok(json!({
        "result": input
            .state
            .reduce_control_with_context(input.command, input.context),
    }))
}

fn standard_action_descriptors(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let _: EmptyInput = decode_payload(payload, "actions.standard_descriptors")?;
    let registry = ShellActionRegistry::standard();
    Ok(json!({
        "actions": registry.actions(),
    }))
}

fn default_action_shortcut(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: ActionDefaultShortcutInput = decode_payload(payload, "actions.default_shortcut")?;
    let registry = ShellActionRegistry::standard();
    Ok(json!({
        "shortcut": registry.default_shortcut(input.id, &input.target),
    }))
}

fn keyboard_action(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: ActionKeyboardInput = decode_payload(payload, "actions.keyboard_action")?;
    let registry = ShellActionRegistry::standard();
    Ok(json!({
        "keyboard_action": registry.keyboard_action(&input.shortcut),
    }))
}

fn execute_action(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: ActionExecuteInput = decode_payload(payload, "actions.execute")?;
    let registry = ShellActionRegistry::standard();
    Ok(json!({
        "result": registry.execute(input.id, &input.target, &input.state),
    }))
}

fn validate_terminal_profile(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: TerminalProfileValidateInput = decode_payload(payload, "terminal_profile.validate")?;
    let (document, availability) = input.into_parts();
    let result = TerminalProfileValidator::validate_with_availability(&document, &availability);
    Ok(json!({
        "is_valid": result.is_valid(),
        "errors": result.errors,
    }))
}

fn make_terminal_profile_definition(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let draft: TerminalProfileEditorDraft =
        decode_payload(payload, "terminal_profile.make_definition")?;
    let result = TerminalProfileEditor::make_definition(draft);
    Ok(json!({
        "is_valid": result.is_valid(),
        "definition": result.definition,
        "errors": result.errors,
    }))
}

fn upsert_terminal_profile(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: TerminalProfileUpsertInput = decode_payload(payload, "terminal_profile.upsert")?;
    let result = TerminalProfileEditor::upsert(input.draft, &input.document);
    Ok(json!({
        "is_valid": result.is_valid(),
        "document": result.document,
        "errors": result.errors,
    }))
}

fn should_capture_global_default(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let profile: TerminalProfileDefinition =
        decode_payload(payload, "terminal_profile.should_capture_global_default")?;
    Ok(json!({
        "capture": should_capture_global_default_terminal_profile(&profile),
    }))
}

fn resolve_terminal_launch_intent(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: TerminalLaunchIntentInput =
        decode_payload(payload, "terminal_profile.resolve_launch_intent")?;
    Ok(json!({
        "intent": TerminalLaunchIntent::resolve(
            input.terminal_profile_reference.as_deref(),
            input.terminal_profiles.as_ref(),
            &input.availability,
            &input.environment,
        ),
    }))
}

fn validate_managed_terminal_account_request(
    payload: Value,
) -> Result<Value, ShellCoreErrorEnvelope> {
    let request: ManagedTerminalAccountRequest =
        decode_payload(payload, "managed_terminal_account.validate_request")?;
    Ok(json!({
        "errors": ManagedTerminalAccountIdentifierValidator::validate(&request),
    }))
}

fn plan_managed_terminal_account(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: ManagedTerminalAccountPlanInput =
        decode_payload(payload, "managed_terminal_account.plan")?;
    let plan = match input {
        ManagedTerminalAccountPlanInput::Provision {
            request,
            diagnosis,
            terminal_profiles,
        } => ManagedTerminalAccountPlanner::plan_from_diagnosis(
            request,
            &diagnosis,
            terminal_profiles.as_ref(),
        ),
        ManagedTerminalAccountPlanInput::Rollback {
            request,
            diagnosis,
            scope,
            terminal_profiles,
        } => ManagedTerminalAccountPlanner::rollback_plan(
            request,
            &diagnosis,
            &scope,
            terminal_profiles.as_ref(),
        ),
    };
    Ok(json!({ "plan": plan }))
}

fn terminal_profile_rows(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let summary: TerminalProfileSettingsSummary =
        decode_payload(payload, "settings.terminal_profile_rows")?;
    Ok(json!({
        "rows": ShellSettingsSummaryRows::terminal_profile_rows(&summary),
    }))
}

fn managed_terminal_account_rows(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let summary: ManagedTerminalAccountSettingsSummary =
        decode_payload(payload, "settings.managed_terminal_account_rows")?;
    Ok(json!({
        "rows": ShellSettingsSummaryRows::managed_terminal_account_rows(&summary),
    }))
}

fn local_rows(payload: Value) -> Result<Value, ShellCoreErrorEnvelope> {
    let input: LocalRowsInput = decode_payload(payload, "settings.local_rows")?;
    Ok(json!({
        "rows": ShellSettingsSummaryRows::local_rows(&input.local, &input.diagnostics),
    }))
}

fn decode_payload<T: for<'de> Deserialize<'de>>(
    payload: Value,
    operation: &'static str,
) -> Result<T, ShellCoreErrorEnvelope> {
    serde_json::from_value(payload).map_err(|error| {
        ShellCoreErrorCode::InvalidPayload
            .envelope("invalid shell-core facade payload")
            .with_detail("operation", json!(operation))
            .with_detail("message", json!(error.to_string()))
    })
}

fn invalid_request_response(message: impl Into<String>) -> Vec<u8> {
    let response = ShellCoreResponseEnvelope::error(
        Uuid::nil(),
        ShellCoreErrorCode::InvalidPayload.envelope(message),
    );
    serialize_response(&response)
}

fn serialize_response(response: &ShellCoreResponseEnvelope) -> Vec<u8> {
    serde_json::to_vec(response).unwrap_or_else(|error| {
        let fallback = ShellCoreResponseEnvelope::error(
            response.request_id,
            ShellCoreErrorCode::InvalidPayload
                .envelope("failed to encode shell-core facade response")
                .with_detail("message", json!(error.to_string())),
        );
        serde_json::to_vec(&fallback).expect("fallback shell-core response envelope must serialize")
    })
}

fn owned_buffer(bytes: Vec<u8>) -> AlanShellCoreByteBuffer {
    let len = bytes.len();
    let ptr = Box::into_raw(bytes.into_boxed_slice()) as *mut u8;
    AlanShellCoreByteBuffer { ptr, len }
}

fn reducer_error_payload(error: ReducerError, state: WorkspaceState) -> Value {
    json!({
        "status": "error",
        "error_code": error.code,
        "error_message": error.message,
        "state": state,
    })
}

#[derive(Debug, Deserialize)]
struct EmptyInput {}

#[derive(Debug, Deserialize)]
struct DefaultManifestInput {
    window_id: String,
    default_working_directory: String,
    now: String,
}

#[derive(Debug, Deserialize)]
struct ValidateManifestInput {
    manifest_json: String,
}

#[derive(Debug, Deserialize)]
struct MaterializeManifestInput {
    manifest: ShellContentWorkspaceManifest,
    default_working_directory: String,
    now: String,
}

#[derive(Debug, Deserialize)]
struct PruningExpiredTabsInput {
    manifest: ShellContentWorkspaceManifest,
    now: String,
    ttl_seconds: i64,
}

#[derive(Debug, Deserialize)]
struct ReducerApplyInput {
    state: WorkspaceState,
    operation: ReducerOperation,
}

#[derive(Debug, Deserialize)]
struct ControlHandleInput {
    state: WorkspaceState,
    command: ShellControlCommand,
    #[serde(default)]
    context: ShellControlExecutionContext,
}

#[derive(Debug, Deserialize)]
struct ActionDefaultShortcutInput {
    id: ShellActionId,
    target: ShellActionTarget,
}

#[derive(Debug, Deserialize)]
struct ActionKeyboardInput {
    shortcut: ShellActionShortcut,
}

#[derive(Debug, Deserialize)]
struct ActionExecuteInput {
    state: WorkspaceState,
    id: ShellActionId,
    target: ShellActionTarget,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TerminalProfileValidateInput {
    WithAvailability {
        document: TerminalProfileDocument,
        #[serde(default)]
        availability: TerminalExecutableAvailability,
    },
    Document(TerminalProfileDocument),
}

impl TerminalProfileValidateInput {
    fn into_parts(self) -> (TerminalProfileDocument, TerminalExecutableAvailability) {
        match self {
            Self::WithAvailability {
                document,
                availability,
            } => (document, availability),
            Self::Document(document) => (document, TerminalExecutableAvailability::default()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TerminalProfileUpsertInput {
    draft: TerminalProfileEditorDraft,
    document: TerminalProfileDocument,
}

#[derive(Debug, Deserialize)]
struct TerminalLaunchIntentInput {
    terminal_profile_reference: Option<String>,
    terminal_profiles: Option<TerminalProfileDocument>,
    availability: TerminalExecutableAvailability,
    environment: TerminalLaunchEnvironment,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ManagedTerminalAccountPlanInput {
    Provision {
        request: ManagedTerminalAccountRequest,
        diagnosis: ManagedTerminalAccountDiagnosis,
        terminal_profiles: Option<TerminalProfileDocument>,
    },
    Rollback {
        request: ManagedTerminalAccountRequest,
        diagnosis: ManagedTerminalAccountDiagnosis,
        scope: ManagedTerminalAccountRollbackScope,
        terminal_profiles: Option<TerminalProfileDocument>,
    },
}

#[derive(Debug, Deserialize)]
struct LocalRowsInput {
    local: ShellSettingsLocalSummary,
    diagnostics: ShellSettingsDiagnosticsSummary,
}
