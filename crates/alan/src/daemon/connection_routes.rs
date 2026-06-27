use super::connection_api::{
    CompleteDeviceLoginRequest, ConnectionLoginSuccessResponse, ConnectionLogoutResponse,
    StartBrowserLoginRequest, StartBrowserLoginResponse, StartDeviceLoginRequest,
    StartDeviceLoginResponse,
};
use super::connection_control::{
    ConnectionCredentialStatus, ConnectionCurrentState, ConnectionEventReplayPage,
    ConnectionPinScope, ConnectionProfileSummary,
};
use super::state::AppState;
use alan_agent_engine::{LlmProvider, ProviderDescriptor};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

#[derive(Debug, Serialize)]
pub struct ConnectionCatalogResponse {
    pub providers: Vec<ProviderDescriptorView>,
}

#[derive(Debug, Serialize)]
pub struct ProviderDescriptorView {
    pub provider_id: LlmProvider,
    pub display_name: String,
    pub credential_kind: alan_agent_engine::CredentialKind,
    pub supports_browser_login: bool,
    pub supports_device_login: bool,
    pub supports_secret_entry: bool,
    pub supports_logout: bool,
    pub supports_test: bool,
    pub required_settings: Vec<String>,
    pub optional_settings: Vec<String>,
    pub default_settings: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectionListResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    pub profiles: Vec<ConnectionProfileSummary>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GetConnectionCurrentQuery {
    pub workspace_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateConnectionRequest {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub provider: LlmProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(default)]
    pub settings: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub activate: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateConnectionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct DeleteConnectionResponse {
    pub removed: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetSecretRequest {
    pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct SetDefaultProfileRequest {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ClearDefaultProfileRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PinConnectionRequest {
    pub profile_id: String,
    pub scope: ConnectionPinScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UnpinConnectionRequest {
    #[serde(default = "default_connection_pin_scope")]
    pub scope: ConnectionPinScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestConnectionResponse {
    pub profile_id: String,
    pub ok: bool,
    pub provider: LlmProvider,
    pub resolved_model: String,
    pub message: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ReadConnectionEventsQuery {
    pub after_event_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ReadConnectionEventsResponse {
    pub gap: bool,
    pub oldest_event_id: Option<String>,
    pub latest_event_id: Option<String>,
    pub events: Vec<super::connection_control::ConnectionEventEnvelope>,
}

type ConnectionRouteError = (StatusCode, Json<serde_json::Value>);

pub async fn get_catalog(
    State(state): State<AppState>,
) -> Result<Json<ConnectionCatalogResponse>, ConnectionRouteError> {
    let providers = state
        .connection_control
        .catalog()
        .into_iter()
        .map(provider_descriptor_view)
        .collect();
    Ok(Json(ConnectionCatalogResponse { providers }))
}

pub async fn list_connections(
    State(state): State<AppState>,
) -> Result<Json<ConnectionListResponse>, ConnectionRouteError> {
    let (default_profile, profiles) = state
        .connection_control
        .list_profiles()
        .await
        .map_err(connection_error_response)?;
    Ok(Json(ConnectionListResponse {
        default_profile,
        profiles,
    }))
}

pub async fn get_connection_current(
    State(state): State<AppState>,
    Query(query): Query<GetConnectionCurrentQuery>,
) -> Result<Json<ConnectionCurrentState>, ConnectionRouteError> {
    let workspace_dir = normalize_workspace_dir(query.workspace_dir);
    state
        .connection_control
        .current_selection(workspace_dir.as_deref())
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn create_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ConnectionProfileSummary>, ConnectionRouteError> {
    let Some(payload) = parse_optional_json_body::<CreateConnectionRequest>(&headers, &body)
        .map_err(json_body_error_response)?
    else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid JSON request body" })),
        ));
    };

    state
        .connection_control
        .create_profile(
            &payload.profile_id,
            payload.label,
            payload.provider,
            payload.credential_id,
            payload.settings,
            payload.activate,
        )
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn get_connection(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<ConnectionProfileSummary>, ConnectionRouteError> {
    state
        .connection_control
        .get_profile(&profile_id)
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn update_connection(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ConnectionProfileSummary>, ConnectionRouteError> {
    let Some(payload) = parse_optional_json_body::<UpdateConnectionRequest>(&headers, &body)
        .map_err(json_body_error_response)?
    else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid JSON request body" })),
        ));
    };

    state
        .connection_control
        .update_profile(
            &profile_id,
            payload.label,
            payload.credential_id,
            payload.settings,
        )
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn delete_connection(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<DeleteConnectionResponse>, ConnectionRouteError> {
    let removed = state
        .connection_control
        .delete_profile(&profile_id)
        .await
        .map_err(connection_error_response)?;
    Ok(Json(DeleteConnectionResponse { removed }))
}

pub async fn activate_connection(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<ConnectionProfileSummary>, ConnectionRouteError> {
    state
        .connection_control
        .activate_profile(&profile_id)
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn set_connection_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ConnectionCurrentState>, ConnectionRouteError> {
    let Some(payload) = parse_optional_json_body::<SetDefaultProfileRequest>(&headers, &body)
        .map_err(json_body_error_response)?
    else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid JSON request body" })),
        ));
    };
    let workspace_dir = normalize_workspace_dir(payload.workspace_dir);
    state
        .connection_control
        .set_default_profile(&payload.profile_id, workspace_dir.as_deref())
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn clear_connection_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ConnectionCurrentState>, ConnectionRouteError> {
    let payload = parse_optional_json_body::<ClearDefaultProfileRequest>(&headers, &body)
        .map_err(json_body_error_response)?
        .unwrap_or_default();
    let workspace_dir = normalize_workspace_dir(payload.workspace_dir);
    state
        .connection_control
        .clear_default_profile(workspace_dir.as_deref())
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn pin_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ConnectionCurrentState>, ConnectionRouteError> {
    let Some(payload) = parse_optional_json_body::<PinConnectionRequest>(&headers, &body)
        .map_err(json_body_error_response)?
    else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid JSON request body" })),
        ));
    };
    let workspace_dir = normalize_workspace_dir(payload.workspace_dir);
    state
        .connection_control
        .pin_profile(&payload.profile_id, payload.scope, workspace_dir.as_deref())
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn unpin_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ConnectionCurrentState>, ConnectionRouteError> {
    let payload = parse_optional_json_body::<UnpinConnectionRequest>(&headers, &body)
        .map_err(json_body_error_response)?
        .unwrap_or_default();
    let workspace_dir = normalize_workspace_dir(payload.workspace_dir);
    state
        .connection_control
        .unpin_profile(payload.scope, workspace_dir.as_deref())
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn get_connection_credential_status(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<ConnectionCredentialStatus>, ConnectionRouteError> {
    state
        .connection_control
        .credential_status(&profile_id)
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn post_connection_secret(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ConnectionCredentialStatus>, ConnectionRouteError> {
    let Some(payload) = parse_optional_json_body::<SetSecretRequest>(&headers, &body)
        .map_err(json_body_error_response)?
    else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid JSON request body" })),
        ));
    };
    state
        .connection_control
        .set_secret(&profile_id, &payload.secret)
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn start_connection_browser_login(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Json(request): Json<StartBrowserLoginRequest>,
) -> Result<Json<StartBrowserLoginResponse>, ConnectionRouteError> {
    let timeout_secs = request.timeout_secs.unwrap_or(300).clamp(30, 1800);
    state
        .connection_control
        .start_browser_login(
            &profile_id,
            request.workspace_id,
            std::time::Duration::from_secs(timeout_secs),
        )
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn start_connection_device_login(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Json(request): Json<StartDeviceLoginRequest>,
) -> Result<Json<StartDeviceLoginResponse>, ConnectionRouteError> {
    state
        .connection_control
        .start_device_login(&profile_id, request.workspace_id)
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn complete_connection_device_login(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Json(request): Json<CompleteDeviceLoginRequest>,
) -> Result<Json<ConnectionLoginSuccessResponse>, ConnectionRouteError> {
    state
        .connection_control
        .complete_device_login(&profile_id, &request.login_id)
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn logout_connection_credential(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<ConnectionLogoutResponse>, ConnectionRouteError> {
    state
        .connection_control
        .logout(&profile_id)
        .await
        .map(Json)
        .map_err(connection_error_response)
}

pub async fn test_connection(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<TestConnectionResponse>, ConnectionRouteError> {
    let summary = state
        .connection_control
        .get_profile(&profile_id)
        .await
        .map_err(connection_error_response)?;
    let (resolved_model, message) = state
        .connection_control
        .test_connection(&profile_id)
        .await
        .map_err(connection_error_response)?;
    Ok(Json(TestConnectionResponse {
        profile_id,
        ok: true,
        provider: summary.provider,
        resolved_model,
        message,
    }))
}

pub async fn read_connection_events(
    State(state): State<AppState>,
    Query(query): Query<ReadConnectionEventsQuery>,
) -> Result<Json<ReadConnectionEventsResponse>, ConnectionRouteError> {
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let ConnectionEventReplayPage {
        events,
        gap,
        oldest_event_id,
        latest_event_id,
    } = state
        .connection_control
        .read_events(query.after_event_id.as_deref(), limit)
        .await;
    Ok(Json(ReadConnectionEventsResponse {
        gap,
        oldest_event_id,
        latest_event_id,
        events,
    }))
}

pub async fn stream_connection_events(
    State(state): State<AppState>,
) -> Result<Response<Body>, ConnectionRouteError> {
    let mut events_rx = state.connection_control.subscribe();
    let bootstrap_cursor = state.connection_control.replay_cursor().await;
    let (tx, rx) = mpsc::channel::<Result<Bytes, std::convert::Infallible>>(64);

    tokio::spawn(async move {
        let bootstrap = serde_json::json!({
            "event_id": bootstrap_cursor.event_id,
            "sequence": bootstrap_cursor.sequence,
            "timestamp_ms": 0,
            "profile_id": "",
            "provider": "chatgpt",
            "type": "bootstrap"
        });
        if send_json_event(&tx, &bootstrap).await.is_err() {
            return;
        }
        loop {
            match events_rx.recv().await {
                Ok(envelope) => {
                    if send_json_event(&tx, &envelope).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(skipped, "Connection event stream lagged");
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(response)
}

fn provider_descriptor_view(descriptor: ProviderDescriptor) -> ProviderDescriptorView {
    ProviderDescriptorView {
        provider_id: descriptor.provider_id,
        display_name: descriptor.display_name.to_string(),
        credential_kind: descriptor.credential_kind,
        supports_browser_login: descriptor.supports_browser_login,
        supports_device_login: descriptor.supports_device_login,
        supports_secret_entry: descriptor.supports_secret_entry,
        supports_logout: descriptor.supports_logout,
        supports_test: descriptor.supports_test,
        required_settings: descriptor
            .required_settings
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        optional_settings: descriptor
            .optional_settings
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        default_settings: descriptor
            .default_settings
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    }
}

fn default_connection_pin_scope() -> ConnectionPinScope {
    ConnectionPinScope::Global
}

fn normalize_workspace_dir(raw: Option<String>) -> Option<PathBuf> {
    raw.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    })
}

fn request_has_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let media_type = value.split(';').next().map(str::trim).unwrap_or_default();
            media_type.eq_ignore_ascii_case("application/json") || media_type.ends_with("+json")
        })
        .unwrap_or(false)
}

fn parse_optional_json_body<T: DeserializeOwned>(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Option<T>, StatusCode> {
    if body.is_empty() || body.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Ok(None);
    }

    if !request_has_json_content_type(headers) {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    serde_json::from_slice(body)
        .map(Some)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

fn json_body_error_response(status: StatusCode) -> ConnectionRouteError {
    let message = match status {
        StatusCode::BAD_REQUEST => "Invalid JSON request body",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "Expected application/json request body",
        _ => "Invalid request body",
    };
    (status, Json(serde_json::json!({ "error": message })))
}

fn connection_error_response(error: anyhow::Error) -> ConnectionRouteError {
    let message = format!("{:#}", error);
    let status = if message.contains("Unknown connection profile")
        || message.contains("Invalid profile id")
        || message.contains("Invalid credential id")
        || message.contains("does not support")
        || message.contains("missing a secret")
        || message.contains("not logged in")
        || message.contains("No connection profile selected")
    {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    (status, Json(serde_json::json!({ "error": message })))
}

async fn send_json_event(
    tx: &mpsc::Sender<Result<Bytes, std::convert::Infallible>>,
    value: &impl Serialize,
) -> Result<(), ()> {
    let mut line = serde_json::to_vec(value).map_err(|_| ())?;
    line.push(b'\n');
    tx.send(Ok(Bytes::from(line))).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_descriptor_view_includes_openrouter_catalog_metadata() {
        let descriptor =
            alan_agent_engine::ConnectionsFile::profile_descriptor(LlmProvider::OpenRouter);
        let view = provider_descriptor_view(descriptor.clone());

        assert_eq!(view.provider_id, LlmProvider::OpenRouter);
        assert_eq!(
            view.credential_kind,
            alan_agent_engine::CredentialKind::SecretString
        );
        assert!(view.supports_secret_entry);
        assert!(view.required_settings.contains(&"model".to_string()));
        assert!(view.optional_settings.contains(&"http_referer".to_string()));
        assert!(view.optional_settings.contains(&"x_title".to_string()));
        assert!(
            view.optional_settings
                .contains(&"app_categories".to_string())
        );
        assert_eq!(
            view.default_settings.get("model").map(String::as_str),
            Some("moonshotai/kimi-k2.6")
        );
    }
}
