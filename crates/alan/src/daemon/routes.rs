//! REST and streaming route handlers for agentd.

use std::{
    convert::Infallible,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use alan_protocol::{
    CompactionAttemptSnapshot, Event, EventEnvelope, MemoryFlushAttemptSnapshot, Submission,
};
use alan_runtime::{
    RolloutItem, RolloutRecorder, latest_compaction_attempt_from_rollout_items,
    latest_memory_flush_attempt_from_rollout_items,
};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, warn};

use super::api_contract::paths;
use super::remote_control::{RemoteRequestContext, required_scope_for_op};
use super::state::{AppState, CreateSessionFromRolloutOptions, SessionPlanSnapshot};
use super::task_store::{
    RunCheckpointRecord, RunResumeAction, RunStatus, ScheduleItemRecord, ScheduleStatus,
    ScheduleTriggerType,
};
use crate::skill_catalog::{SkillCatalogSnapshot, SkillCatalogTarget};

/// Health check response
pub async fn health() -> &'static str {
    "OK"
}

#[derive(Debug, Deserialize, Default)]
pub struct SkillCatalogQuery {
    pub workspace_dir: Option<PathBuf>,
    pub agent_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SkillCatalogChangedQuery {
    pub workspace_dir: Option<PathBuf>,
    pub agent_name: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteSkillOverrideRequest {
    pub workspace_dir: Option<PathBuf>,
    pub agent_name: Option<String>,
    pub skill_id: String,
    #[serde(default)]
    pub enabled: Option<Option<bool>>,
    #[serde(default, rename = "allowImplicitInvocation")]
    pub allow_implicit_invocation: Option<Option<bool>>,
}

#[derive(Debug, Serialize)]
pub struct SkillCatalogChangedResponse {
    pub changed: bool,
    pub cursor: String,
    pub package_count: usize,
    pub skill_count: usize,
}

#[derive(Debug, Serialize)]
pub struct WriteSkillOverrideResponse {
    pub skill_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "allowImplicitInvocation"
    )]
    pub allow_implicit_invocation: Option<bool>,
    pub config_path: String,
    pub snapshot: SkillCatalogSnapshot,
}

pub async fn get_skill_catalog(
    State(state): State<AppState>,
    Query(query): Query<SkillCatalogQuery>,
) -> Result<Json<SkillCatalogSnapshot>, (StatusCode, Json<serde_json::Value>)> {
    let target = SkillCatalogTarget {
        workspace_dir: normalized_skill_catalog_workspace_identifier(query.workspace_dir)?
            .map(PathBuf::from),
        agent_name: normalized_agent_name(query.agent_name),
    };
    state
        .resolve_skill_catalog_snapshot(&target)
        .map(Json)
        .map_err(skill_catalog_error_response)
}

pub async fn get_skill_catalog_changed(
    State(state): State<AppState>,
    Query(query): Query<SkillCatalogChangedQuery>,
) -> Result<Json<SkillCatalogChangedResponse>, (StatusCode, Json<serde_json::Value>)> {
    let target = SkillCatalogTarget {
        workspace_dir: normalized_skill_catalog_workspace_identifier(query.workspace_dir)?
            .map(PathBuf::from),
        agent_name: normalized_agent_name(query.agent_name),
    };
    let snapshot = state
        .resolve_skill_catalog_snapshot(&target)
        .map_err(skill_catalog_error_response)?;
    Ok(Json(SkillCatalogChangedResponse {
        changed: query.after.as_deref() != Some(snapshot.cursor.as_str()),
        cursor: snapshot.cursor,
        package_count: snapshot.packages.len(),
        skill_count: snapshot.skills.len(),
    }))
}

pub async fn write_skill_override_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<WriteSkillOverrideResponse>, (StatusCode, Json<serde_json::Value>)> {
    let Some(payload) = parse_optional_json_body::<WriteSkillOverrideRequest>(&headers, &body)
        .map_err(json_body_error_response)?
    else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Invalid JSON request body"
            })),
        ));
    };

    let skill_id = payload.skill_id;
    let target = SkillCatalogTarget {
        workspace_dir: normalized_skill_catalog_workspace_identifier(payload.workspace_dir)?
            .map(PathBuf::from),
        agent_name: validated_skill_override_agent_name(payload.agent_name)?,
    };
    let (config_path, snapshot) = state
        .write_skill_override(
            &target,
            &skill_id,
            payload.enabled,
            payload.allow_implicit_invocation,
        )
        .map_err(skill_catalog_error_response)?;
    Ok(Json(WriteSkillOverrideResponse {
        skill_id,
        enabled: payload.enabled.flatten(),
        allow_implicit_invocation: payload.allow_implicit_invocation.flatten(),
        config_path: config_path.display().to_string(),
        snapshot,
    }))
}

/// Response for session creation
#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub websocket_url: String,
    pub events_url: String,
    pub submit_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub governance: alan_protocol::GovernanceConfig,
    pub execution_backend: String,
    pub streaming_mode: alan_runtime::StreamingMode,
    pub partial_stream_recovery_mode: alan_runtime::PartialStreamRecoveryMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<alan_runtime::LlmProvider>,
    pub resolved_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    pub durability: SessionDurabilityInfo,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SessionDurabilityInfo {
    pub durable: bool,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct RollbackDurabilityInfo {
    pub durable: bool,
    pub scope: &'static str,
}

#[derive(Deserialize, Default)]
pub struct CreateSessionRequest {
    /// Optional workspace directory override for this agent session (agentd-local path)
    pub workspace_dir: Option<PathBuf>,
    /// Optional named agent override
    pub agent_name: Option<String>,
    /// Optional explicit connection profile id.
    pub profile_id: Option<String>,
    /// Optional session-scoped reasoning effort override.
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    /// Optional governance override
    pub governance: Option<alan_protocol::GovernanceConfig>,
    /// Optional streaming behavior override
    pub streaming_mode: Option<alan_runtime::StreamingMode>,
    /// Optional partial-stream recovery override
    pub partial_stream_recovery_mode: Option<alan_runtime::PartialStreamRecoveryMode>,
}

/// Create a new session
pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<CreateSessionResponse>, (StatusCode, Json<serde_json::Value>)> {
    let payload = parse_optional_json_body::<CreateSessionRequest>(&headers, &body)
        .map_err(json_body_error_response)?;
    let (
        workspace_dir,
        agent_name,
        profile_id,
        reasoning_effort,
        governance,
        streaming_mode,
        partial_stream_recovery_mode,
    ) = payload
        .map(|req| {
            (
                req.workspace_dir.filter(|p| !p.as_os_str().is_empty()),
                normalized_agent_name(req.agent_name),
                req.profile_id,
                req.reasoning_effort,
                req.governance,
                req.streaming_mode,
                req.partial_stream_recovery_mode,
            )
        })
        .unwrap_or((None, None, None, None, None, None, None));

    let session_id = state
        .create_session_from_rollout(CreateSessionFromRolloutOptions {
            workspace_dir,
            resume_rollout_path: None,
            agent_name,
            profile_id,
            reasoning_effort,
            governance: governance.clone(),
            streaming_mode,
            partial_stream_recovery_mode,
        })
        .await
        .map_err(|err| {
            warn!(error = %err, "Failed to create session");
            (
                status_for_session_creation_error(&err),
                Json(serde_json::json!({ "error": format!("{:#}", err) })),
            )
        })?;
    info!(%session_id, "Created new session");

    let (
        agent_name,
        governance,
        execution_backend,
        streaming_mode,
        partial_stream_recovery_mode,
        profile_id,
        provider,
        resolved_model,
        reasoning_effort,
        durability,
    ) = {
        let sessions = state.sessions.read().await;
        let Some(entry) = sessions.get(&session_id) else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Session {session_id} missing after creation")
                })),
            ));
        };
        (
            entry.agent_name.clone(),
            entry.governance.clone(),
            entry.execution_backend.clone(),
            entry.streaming_mode,
            entry.partial_stream_recovery_mode,
            entry.profile_id.clone(),
            entry.provider,
            entry.resolved_model.clone(),
            entry.reasoning_effort,
            session_durability_info(entry.durability_required, entry.durable),
        )
    };

    Ok(Json(CreateSessionResponse {
        websocket_url: paths::session_ws(&session_id),
        events_url: paths::session_events(&session_id),
        submit_url: paths::session_submit(&session_id),
        session_id,
        agent_name,
        governance,
        execution_backend,
        streaming_mode,
        partial_stream_recovery_mode,
        profile_id,
        provider,
        resolved_model,
        reasoning_effort,
        durability,
    }))
}

/// Get session info
#[derive(Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub governance: alan_protocol::GovernanceConfig,
    pub execution_backend: String,
    pub streaming_mode: alan_runtime::StreamingMode,
    pub partial_stream_recovery_mode: alan_runtime::PartialStreamRecoveryMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<alan_runtime::LlmProvider>,
    pub resolved_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    pub durability: SessionDurabilityInfo,
}

#[derive(Serialize)]
pub struct SessionListItem {
    pub session_id: String,
    pub workspace_id: String,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub governance: alan_protocol::GovernanceConfig,
    pub execution_backend: String,
    pub streaming_mode: alan_runtime::StreamingMode,
    pub partial_stream_recovery_mode: alan_runtime::PartialStreamRecoveryMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<alan_runtime::LlmProvider>,
    pub resolved_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    pub durability: SessionDurabilityInfo,
}

#[derive(Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionListItem>,
}

#[derive(Serialize)]
pub struct ChildRunListResponse {
    pub child_runs: Vec<alan_runtime::runtime::ChildRunRecord>,
}

#[derive(Serialize)]
pub struct ChildRunResponse {
    pub child_run: alan_runtime::runtime::ChildRunRecord,
}

#[derive(Deserialize, Default)]
pub struct TerminateChildRunRequest {
    pub reason: Option<String>,
    pub mode: Option<alan_runtime::runtime::ChildRunTerminationMode>,
    pub actor: Option<String>,
}

#[derive(Serialize)]
pub struct SessionReadResponse {
    pub session_id: String,
    pub workspace_id: String,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub governance: alan_protocol::GovernanceConfig,
    pub execution_backend: String,
    pub streaming_mode: alan_runtime::StreamingMode,
    pub partial_stream_recovery_mode: alan_runtime::PartialStreamRecoveryMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<alan_runtime::LlmProvider>,
    pub resolved_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    pub durability: SessionDurabilityInfo,
    pub rollout_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_compaction_attempt: Option<CompactionAttemptSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_memory_flush_attempt: Option<MemoryFlushAttemptSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_plan_snapshot: Option<SessionPlanSnapshot>,
    pub messages: Vec<SessionHistoryMessage>,
}

#[derive(Serialize)]
pub struct ReconnectSnapshotResponse {
    pub session_id: String,
    pub workspace_id: String,
    pub captured_at_ms: u64,
    pub replay: ReconnectReplayState,
    pub execution: ReconnectExecutionState,
    pub notifications: ReconnectNotificationState,
}

#[derive(Serialize)]
pub struct ReconnectReplayState {
    pub oldest_event_id: Option<String>,
    pub latest_event_id: Option<String>,
    pub latest_submission_id: Option<String>,
    pub buffered_event_count: usize,
}

#[derive(Serialize)]
pub struct ReconnectExecutionState {
    pub run_status: Option<RunStatus>,
    pub next_action: Option<RunResumeAction>,
    pub resume_required: bool,
    pub latest_checkpoint: Option<ReconnectCheckpoint>,
    pub execution_backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_compaction_attempt: Option<CompactionAttemptSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_memory_flush_attempt: Option<MemoryFlushAttemptSnapshot>,
}

#[derive(Serialize)]
pub struct ReconnectCheckpoint {
    pub checkpoint_id: String,
    pub checkpoint_type: String,
    pub summary: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ReconnectNotificationState {
    pub latest_signal_cursor: Option<String>,
    pub signals: Vec<ReconnectNotificationSignal>,
}

#[derive(Serialize)]
pub struct ReconnectNotificationSignal {
    pub signal_id: String,
    pub signal_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yield_kind: Option<String>,
    pub summary: String,
    pub created_at: String,
    pub informational: bool,
}

#[derive(Serialize)]
pub struct ResumeSessionResponse {
    pub session_id: String,
    pub resumed: bool,
}

#[derive(Deserialize, Default)]
pub struct ForkSessionRequest {
    pub workspace_dir: Option<PathBuf>,
    pub agent_name: Option<String>,
    pub profile_id: Option<String>,
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    pub governance: Option<alan_protocol::GovernanceConfig>,
    pub streaming_mode: Option<alan_runtime::StreamingMode>,
    pub partial_stream_recovery_mode: Option<alan_runtime::PartialStreamRecoveryMode>,
}

#[derive(Serialize)]
pub struct ForkSessionResponse {
    pub session_id: String,
    pub forked_from_session_id: String,
    pub websocket_url: String,
    pub events_url: String,
    pub submit_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub governance: alan_protocol::GovernanceConfig,
    pub streaming_mode: alan_runtime::StreamingMode,
    pub partial_stream_recovery_mode: alan_runtime::PartialStreamRecoveryMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<alan_runtime::LlmProvider>,
    pub resolved_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    pub durability: SessionDurabilityInfo,
}

#[derive(Deserialize)]
pub struct RollbackSessionRequest {
    pub turns: u32,
}

#[derive(Serialize)]
pub struct RollbackSessionResponse {
    pub submission_id: String,
    pub accepted: bool,
    pub durability: RollbackDurabilityInfo,
    pub warning: String,
}

#[derive(Serialize)]
pub struct CompactSessionResponse {
    pub submission_id: String,
    pub accepted: bool,
}

#[derive(Deserialize, Default)]
pub struct CompactSessionRequest {
    pub focus: Option<String>,
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
) -> Result<Option<T>, JsonBodyError> {
    if body.is_empty() || body.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Ok(None);
    }

    if !request_has_json_content_type(headers) {
        return Err(JsonBodyError::UnsupportedMediaType);
    }

    let value =
        serde_json::from_slice::<serde_json::Value>(body).map_err(|_| JsonBodyError::BadRequest)?;
    if object_has_removed_thinking_budget(&value) {
        return Err(JsonBodyError::RemovedThinkingBudget);
    }

    serde_json::from_value(value)
        .map(Some)
        .map_err(|_| JsonBodyError::BadRequest)
}

fn parse_required_json_body<T, F>(
    headers: &HeaderMap,
    body: &Bytes,
    contains_removed_thinking_budget: F,
) -> Result<T, JsonBodyError>
where
    T: DeserializeOwned,
    F: FnOnce(&serde_json::Value) -> bool,
{
    if body.is_empty() || body.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Err(JsonBodyError::BadRequest);
    }

    if !request_has_json_content_type(headers) {
        return Err(JsonBodyError::UnsupportedMediaType);
    }

    let value =
        serde_json::from_slice::<serde_json::Value>(body).map_err(|_| JsonBodyError::BadRequest)?;
    if contains_removed_thinking_budget(&value) {
        return Err(JsonBodyError::RemovedThinkingBudget);
    }

    serde_json::from_value(value).map_err(|_| JsonBodyError::BadRequest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsonBodyError {
    BadRequest,
    UnsupportedMediaType,
    RemovedThinkingBudget,
}

impl JsonBodyError {
    fn status(self) -> StatusCode {
        match self {
            Self::BadRequest | Self::RemovedThinkingBudget => StatusCode::BAD_REQUEST,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::BadRequest => "Invalid JSON request body",
            Self::UnsupportedMediaType => "Expected application/json request body",
            Self::RemovedThinkingBudget => {
                "Removed request field `thinking_budget_tokens`; use `model_reasoning_effort` in agent config or `reasoning_effort` on session/turn requests"
            }
        }
    }
}

fn object_has_removed_thinking_budget(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("thinking_budget_tokens"))
}

fn submit_request_has_removed_thinking_budget(value: &serde_json::Value) -> bool {
    if object_has_removed_thinking_budget(value) {
        return true;
    }

    let Some(op) = value.get("op") else {
        return false;
    };
    object_has_removed_thinking_budget(op)
        || op
            .get("context")
            .is_some_and(object_has_removed_thinking_budget)
}

fn json_body_error_status(error: JsonBodyError) -> StatusCode {
    error.status()
}

fn json_body_error_response(error: JsonBodyError) -> (StatusCode, Json<serde_json::Value>) {
    (
        error.status(),
        Json(serde_json::json!({ "error": error.message() })),
    )
}

fn skill_catalog_error_response(err: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    let message = format!("{:#}", err);
    let status = status_for_skill_catalog_error(&message);
    (status, Json(serde_json::json!({ "error": message })))
}

fn status_for_skill_catalog_error(message: &str) -> StatusCode {
    if message.contains("No writable agent root")
        || message.contains("skill id must not be empty")
        || message.contains("skill_id must not be empty")
        || message.contains("Invalid runtime skill id")
        || message.contains("Unknown skill_id")
        || message.contains("Failed to parse")
        || message.contains("Invalid skill_overrides")
        || message.contains("Invalid agent config")
        || message.contains("Unknown registered workspace identifier")
        || message.contains("initialized workspace")
    {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Deserialize)]
pub struct ScheduleAtRequest {
    pub wake_at: String,
}

#[derive(Deserialize)]
pub struct SleepUntilRequest {
    pub wake_at: String,
}

#[derive(Serialize)]
pub struct ScheduleResponse {
    pub session_id: String,
    pub schedule_id: String,
    pub run_id: String,
    pub trigger_type: String,
    pub status: String,
    pub wake_at: String,
    pub idempotency_key: String,
}

#[derive(Serialize)]
pub struct SessionHistoryMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct SessionHistoryResponse {
    pub session_id: String,
    pub messages: Vec<SessionHistoryMessage>,
}

#[derive(Deserialize, Default)]
pub struct ReadEventsQuery {
    pub after_event_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct ReadEventsResponse {
    pub session_id: String,
    pub gap: bool,
    pub oldest_event_id: Option<String>,
    pub latest_event_id: Option<String>,
    pub events: Vec<EventEnvelope>,
}

fn session_durability_info(required: bool, durable: bool) -> SessionDurabilityInfo {
    SessionDurabilityInfo { durable, required }
}

pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionInfo>, StatusCode> {
    debug!(%id, "Getting session info");

    let exists = state.get_session(&id).await.map_err(|err| {
        warn!(%id, error = %err, "Failed to recover sessions before get_session");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if exists {
        let (
            active,
            agent_name,
            governance,
            execution_backend,
            streaming_mode,
            partial_stream_recovery_mode,
            profile_id,
            provider,
            resolved_model,
            reasoning_effort,
            durability,
        ) = {
            let sessions = state.sessions.read().await;
            let Some(entry) = sessions.get(&id) else {
                return Err(StatusCode::NOT_FOUND);
            };
            (
                entry.active,
                entry.agent_name.clone(),
                entry.governance.clone(),
                entry.execution_backend.clone(),
                entry.streaming_mode,
                entry.partial_stream_recovery_mode,
                entry.profile_id.clone(),
                entry.provider,
                entry.resolved_model.clone(),
                entry.reasoning_effort,
                session_durability_info(entry.durability_required, entry.durable),
            )
        };
        Ok(Json(SessionInfo {
            session_id: id,
            active,
            agent_name,
            governance,
            execution_backend,
            streaming_mode,
            partial_stream_recovery_mode,
            profile_id,
            provider,
            resolved_model,
            reasoning_effort,
            durability,
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// List sessions known to agentd.
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<SessionListResponse>, StatusCode> {
    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(error = %err, "Failed to recover sessions before listing");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let sessions = state.sessions.read().await;
    let mut data: Vec<SessionListItem> = sessions
        .iter()
        .map(|(session_id, entry)| SessionListItem {
            session_id: session_id.clone(),
            workspace_id: entry.workspace_id.clone(),
            active: entry.active,
            agent_name: entry.agent_name.clone(),
            governance: entry.governance.clone(),
            execution_backend: entry.execution_backend.clone(),
            streaming_mode: entry.streaming_mode,
            partial_stream_recovery_mode: entry.partial_stream_recovery_mode,
            profile_id: entry.profile_id.clone(),
            provider: entry.provider,
            resolved_model: entry.resolved_model.clone(),
            reasoning_effort: entry.reasoning_effort,
            durability: session_durability_info(entry.durability_required, entry.durable),
        })
        .collect();
    data.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    Ok(Json(SessionListResponse { sessions: data }))
}

pub async fn list_child_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ChildRunListResponse>, StatusCode> {
    if !state.get_session(&id).await.map_err(|err| {
        warn!(%id, error = %err, "Failed to recover sessions before listing child runs");
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(ChildRunListResponse {
        child_runs: state.runtime_manager.list_child_runs(&id).await,
    }))
}

pub async fn get_child_run(
    State(state): State<AppState>,
    Path((id, child_run_id)): Path<(String, String)>,
) -> Result<Json<ChildRunResponse>, StatusCode> {
    if !state.get_session(&id).await.map_err(|err| {
        warn!(%id, error = %err, "Failed to recover sessions before reading child run");
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        return Err(StatusCode::NOT_FOUND);
    }

    state
        .runtime_manager
        .get_child_run(&id, &child_run_id)
        .await
        .map(|child_run| Json(ChildRunResponse { child_run }))
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn terminate_child_run(
    State(state): State<AppState>,
    Path((id, child_run_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ChildRunResponse>, (StatusCode, Json<serde_json::Value>)> {
    if !state.get_session(&id).await.map_err(|err| {
        warn!(%id, error = %err, "Failed to recover sessions before terminating child run");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{:#}", err) })),
        )
    })? {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Session not found" })),
        ));
    }

    let payload = parse_optional_json_body::<TerminateChildRunRequest>(&headers, &body)
        .map_err(json_body_error_response)?
        .unwrap_or_default();
    let reason = payload
        .reason
        .filter(|reason| !reason.trim().is_empty())
        .unwrap_or_else(|| "operator requested child termination".to_string());
    let actor = payload
        .actor
        .filter(|actor| !actor.trim().is_empty())
        .unwrap_or_else(|| "operator".to_string());
    let mode = payload
        .mode
        .unwrap_or(alan_runtime::runtime::ChildRunTerminationMode::Graceful);

    match state
        .runtime_manager
        .terminate_child_run(&id, &child_run_id, actor, mode, reason)
        .await
    {
        Ok(child_run) => Ok(Json(ChildRunResponse { child_run })),
        Err(alan_runtime::runtime::ChildRunRegistryError::AlreadyTerminal(child_run)) => {
            Ok(Json(ChildRunResponse {
                child_run: *child_run,
            }))
        }
        Err(alan_runtime::runtime::ChildRunRegistryError::NotFound) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Child run not found" })),
        )),
    }
}

/// Read session metadata and persisted message history in one response.
pub async fn read_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionReadResponse>, StatusCode> {
    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to recover sessions before read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (
        workspace_id,
        agent_name,
        governance,
        execution_backend,
        streaming_mode,
        partial_stream_recovery_mode,
        profile_id,
        provider,
        resolved_model,
        reasoning_effort,
        durability,
        stored_rollout_path,
        event_log,
        active,
    ) = {
        let sessions = state.sessions.read().await;
        let Some(entry) = sessions.get(&session_id) else {
            return Err(StatusCode::NOT_FOUND);
        };
        (
            entry.workspace_id.clone(),
            entry.agent_name.clone(),
            entry.governance.clone(),
            entry.execution_backend.clone(),
            entry.streaming_mode,
            entry.partial_stream_recovery_mode,
            entry.profile_id.clone(),
            entry.provider,
            entry.resolved_model.clone(),
            entry.reasoning_effort,
            session_durability_info(entry.durability_required, entry.durable),
            entry.rollout_path.clone(),
            Arc::clone(&entry.event_log),
            entry.active,
        )
    };

    state
        .touch_session_inbound(&session_id)
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to update inbound activity before read");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let (latest_compaction_from_replay, latest_memory_flush_from_replay, latest_plan_snapshot) = {
        let guard = event_log.read().await;
        (
            guard.latest_compaction_attempt(),
            guard.latest_memory_flush_attempt(),
            guard.latest_plan_snapshot(),
        )
    };
    let (resolved_rollout_path, rollout_items) = load_rollout_items_for_session(
        &state,
        &session_id,
        &workspace_id,
        stored_rollout_path,
        "read",
    )
    .await?;
    let latest_compaction_attempt = latest_compaction_from_replay
        .or_else(|| latest_compaction_attempt_from_rollout_items(&rollout_items));
    let latest_memory_flush_attempt = latest_memory_flush_from_replay
        .or_else(|| latest_memory_flush_attempt_from_rollout_items(&rollout_items));
    let messages = rollout_items_to_history_messages(rollout_items);

    Ok(Json(SessionReadResponse {
        session_id,
        workspace_id,
        active,
        agent_name,
        governance,
        execution_backend,
        streaming_mode,
        partial_stream_recovery_mode,
        profile_id,
        provider,
        resolved_model,
        reasoning_effort,
        durability,
        rollout_path: resolved_rollout_path.map(|path| path.to_string_lossy().to_string()),
        latest_compaction_attempt,
        latest_memory_flush_attempt,
        latest_plan_snapshot,
        messages,
    }))
}

/// Read reconnect snapshot for mobile/offline recovery without mutating execution state.
pub async fn reconnect_snapshot(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ReconnectSnapshotResponse>, StatusCode> {
    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(
            %session_id,
            error = %err,
            "Failed to recover sessions before reconnect snapshot read"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (workspace_id, execution_backend, event_log, rollout_path) = {
        let sessions = state.sessions.read().await;
        let Some(entry) = sessions.get(&session_id) else {
            return Err(StatusCode::NOT_FOUND);
        };
        (
            entry.workspace_id.clone(),
            entry.execution_backend.clone(),
            Arc::clone(&entry.event_log),
            entry.rollout_path.clone(),
        )
    };

    state
        .touch_session_inbound(&session_id)
        .await
        .map_err(|err| {
            warn!(
                %session_id,
                error = %err,
                "Failed to update inbound activity before reconnect snapshot read"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let (replay_summary, latest_compaction_from_replay, latest_memory_flush_from_replay) = {
        let guard = event_log.read().await;
        (
            guard.replay_summary(),
            guard.latest_compaction_attempt(),
            guard.latest_memory_flush_attempt(),
        )
    };
    let latest_compaction_attempt = if let Some(attempt) = latest_compaction_from_replay {
        Some(attempt)
    } else {
        best_effort_latest_compaction_attempt_for_reconnect(
            &state,
            &session_id,
            &workspace_id,
            rollout_path.clone(),
        )
        .await
    };
    let latest_memory_flush_attempt = if let Some(attempt) = latest_memory_flush_from_replay {
        Some(attempt)
    } else {
        best_effort_latest_memory_flush_attempt_for_reconnect(
            &state,
            &session_id,
            &workspace_id,
            rollout_path.clone(),
        )
        .await
    };

    let run_snapshot = match state.restore_run(&session_id) {
        Ok(snapshot) => Some(snapshot),
        Err(err) => {
            warn!(
                %session_id,
                error = %err,
                "Unable to restore run snapshot for reconnect read; continuing with replay data only"
            );
            None
        }
    };

    let (execution, notifications) = if let Some(snapshot) = run_snapshot {
        let checkpoint = snapshot
            .checkpoint
            .as_ref()
            .map(reconnect_checkpoint_from_record);
        let signal = snapshot.checkpoint.as_ref().and_then(|checkpoint| {
            reconnect_signal_from_checkpoint(checkpoint, snapshot.run.status, snapshot.next_action)
        });
        (
            ReconnectExecutionState {
                run_status: Some(snapshot.run.status),
                next_action: Some(snapshot.next_action),
                resume_required: matches!(snapshot.next_action, RunResumeAction::AwaitUserResume),
                latest_checkpoint: checkpoint,
                execution_backend: execution_backend.clone(),
                latest_compaction_attempt: latest_compaction_attempt.clone(),
                latest_memory_flush_attempt: latest_memory_flush_attempt.clone(),
            },
            ReconnectNotificationState {
                latest_signal_cursor: signal.as_ref().map(|s| s.signal_id.clone()),
                signals: signal.into_iter().collect(),
            },
        )
    } else {
        (
            ReconnectExecutionState {
                run_status: None,
                next_action: None,
                resume_required: false,
                latest_checkpoint: None,
                execution_backend: execution_backend.clone(),
                latest_compaction_attempt,
                latest_memory_flush_attempt,
            },
            ReconnectNotificationState {
                latest_signal_cursor: None,
                signals: vec![],
            },
        )
    };

    Ok(Json(ReconnectSnapshotResponse {
        session_id,
        workspace_id,
        captured_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        replay: ReconnectReplayState {
            oldest_event_id: replay_summary.oldest_event_id,
            latest_event_id: replay_summary.latest_event_id,
            latest_submission_id: replay_summary.latest_submission_id,
            buffered_event_count: replay_summary.buffered_event_count,
        },
        execution,
        notifications,
    }))
}

/// Resume (ensure running) the runtime for a known session.
pub async fn resume_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ResumeSessionResponse>, StatusCode> {
    let exists = state.get_session(&session_id).await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to recover sessions before resume");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .touch_session_inbound(&session_id)
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to update inbound activity before resume");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    state
        .resume_session_runtime(&session_id)
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to resume session runtime");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(ResumeSessionResponse {
        session_id,
        resumed: true,
    }))
}

/// Fork a session by starting a new runtime seeded from the source rollout.
pub async fn fork_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ForkSessionResponse>, (StatusCode, Json<serde_json::Value>)> {
    let payload = parse_optional_json_body::<ForkSessionRequest>(&headers, &body)
        .map_err(json_body_error_response)?;
    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to recover sessions before fork");
        status_error_response(StatusCode::INTERNAL_SERVER_ERROR)
    })?;
    let (
        source_workspace_id,
        source_agent_name,
        source_profile_id,
        inherited_reasoning_effort,
        source_governance,
        source_streaming_mode,
        source_partial_stream_recovery_mode,
        stored_rollout_path,
    ) = {
        let sessions = state.sessions.read().await;
        let Some(entry) = sessions.get(&session_id) else {
            return Err(status_error_response(StatusCode::NOT_FOUND));
        };
        (
            entry.workspace_id.clone(),
            entry.agent_name.clone(),
            entry.profile_id.clone(),
            entry.reasoning_effort,
            entry.governance.clone(),
            entry.streaming_mode,
            entry.partial_stream_recovery_mode,
            entry.rollout_path.clone(),
        )
    };

    state
        .touch_session_inbound(&session_id)
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to update inbound activity before fork");
            status_error_response(StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let JsonLikeFork {
        workspace_dir,
        agent_name,
        profile_id,
        reasoning_effort,
        governance,
        streaming_mode,
        partial_stream_recovery_mode,
    } = JsonLikeFork::from_payload(payload);
    let effective_agent_name = agent_name.or(source_agent_name);
    let effective_profile_id = profile_id.or(source_profile_id);
    let selected_reasoning_effort = match reasoning_effort {
        Some(effort) => Some(effort),
        None => inherited_reasoning_effort,
    };
    let effective_governance = governance.unwrap_or(source_governance);
    let effective_streaming_mode = streaming_mode.unwrap_or(source_streaming_mode);
    let effective_partial_stream_recovery_mode =
        partial_stream_recovery_mode.unwrap_or(source_partial_stream_recovery_mode);

    let rollout_path = if let Some(path) = stored_rollout_path {
        if path.exists() {
            Some(path)
        } else {
            latest_rollout_path_for_workspace(&state, &session_id, &source_workspace_id)
                .await
                .map_err(status_error_response)?
        }
    } else {
        latest_rollout_path_for_workspace(&state, &session_id, &source_workspace_id)
            .await
            .map_err(status_error_response)?
    };

    let Some(rollout_path) = rollout_path else {
        return Err(status_error_response(StatusCode::CONFLICT));
    };

    let new_session_id = state
        .create_session_from_rollout(CreateSessionFromRolloutOptions {
            workspace_dir,
            resume_rollout_path: Some(rollout_path),
            agent_name: effective_agent_name,
            profile_id: effective_profile_id,
            reasoning_effort: selected_reasoning_effort,
            governance: Some(effective_governance.clone()),
            streaming_mode: Some(effective_streaming_mode),
            partial_stream_recovery_mode: Some(effective_partial_stream_recovery_mode),
        })
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to fork session");
            status_error_response(status_for_session_creation_error(&err))
        })?;

    let (agent_name, profile_id, provider, resolved_model, reasoning_effort, durability) = {
        let sessions = state.sessions.read().await;
        let Some(entry) = sessions.get(&new_session_id) else {
            return Err(status_error_response(StatusCode::INTERNAL_SERVER_ERROR));
        };
        (
            entry.agent_name.clone(),
            entry.profile_id.clone(),
            entry.provider,
            entry.resolved_model.clone(),
            entry.reasoning_effort,
            session_durability_info(entry.durability_required, entry.durable),
        )
    };

    Ok(Json(ForkSessionResponse {
        websocket_url: paths::session_ws(&new_session_id),
        events_url: paths::session_events(&new_session_id),
        submit_url: paths::session_submit(&new_session_id),
        session_id: new_session_id,
        forked_from_session_id: session_id,
        agent_name,
        governance: effective_governance,
        streaming_mode: effective_streaming_mode,
        partial_stream_recovery_mode: effective_partial_stream_recovery_mode,
        profile_id,
        provider,
        resolved_model,
        reasoning_effort,
        durability,
    }))
}

/// Get persisted message history for a session (final messages only; no streaming deltas)
pub async fn get_session_history(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionHistoryResponse>, StatusCode> {
    debug!(%session_id, "Getting session message history");
    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to recover sessions before history read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (workspace_id, stored_rollout_path) = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => (session.workspace_id.clone(), session.rollout_path.clone()),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    state.touch_session_inbound(&session_id).await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to update inbound activity before history read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (_, items) = load_rollout_items_for_session(
        &state,
        &session_id,
        &workspace_id,
        stored_rollout_path,
        "history",
    )
    .await?;
    let messages = rollout_items_to_history_messages(items);

    Ok(Json(SessionHistoryResponse {
        session_id,
        messages,
    }))
}

async fn load_rollout_items_for_session(
    state: &AppState,
    session_id: &str,
    workspace_id: &str,
    stored_rollout_path: Option<PathBuf>,
    read_surface: &str,
) -> Result<(Option<PathBuf>, Vec<RolloutItem>), StatusCode> {
    let rollout_path = resolve_rollout_path_for_session(
        state,
        session_id,
        workspace_id,
        stored_rollout_path,
        read_surface,
    )
    .await?;
    let items = match rollout_path.as_ref() {
        Some(rollout_path) => RolloutRecorder::load_history(rollout_path)
            .await
            .map_err(|err| {
                warn!(
                    %session_id,
                    %workspace_id,
                    path = %rollout_path.display(),
                    error = %err,
                    "Failed to read rollout history"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?,
        None => Vec::new(),
    };
    Ok((rollout_path, items))
}

async fn resolve_rollout_path_for_session(
    state: &AppState,
    session_id: &str,
    workspace_id: &str,
    stored_rollout_path: Option<PathBuf>,
    read_surface: &str,
) -> Result<Option<PathBuf>, StatusCode> {
    let rollout_path = if let Some(path) = stored_rollout_path {
        if path.exists() {
            Some(path)
        } else {
            let refreshed =
                latest_rollout_path_for_workspace(state, session_id, workspace_id).await?;
            state
                .set_session_rollout_path(session_id, refreshed.clone())
                .await
                .map_err(|err| {
                    warn!(
                        %session_id,
                        error = %err,
                        "Failed to persist refreshed rollout path for session read"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            refreshed
        }
    } else {
        let refreshed = latest_rollout_path_for_workspace(state, session_id, workspace_id).await?;
        state
            .set_session_rollout_path(session_id, refreshed.clone())
            .await
            .map_err(|err| {
                warn!(
                    %session_id,
                    error = %err,
                    "Failed to persist refreshed rollout path for session read"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        refreshed
    };

    debug!(%session_id, %workspace_id, read_surface, ?rollout_path, "Resolved rollout path");
    Ok(rollout_path)
}

async fn best_effort_latest_compaction_attempt_for_reconnect(
    state: &AppState,
    session_id: &str,
    workspace_id: &str,
    rollout_path: Option<PathBuf>,
) -> Option<CompactionAttemptSnapshot> {
    let resolved_rollout_path = match rollout_path {
        Some(path) if path.exists() => Some(path),
        Some(_) | None => {
            match latest_rollout_path_for_workspace(state, session_id, workspace_id).await {
                Ok(path) => path,
                Err(status) => {
                    warn!(
                        %session_id,
                        %workspace_id,
                        ?status,
                        "Failed to inspect rollout path for reconnect compaction fallback"
                    );
                    return None;
                }
            }
        }
    };

    let rollout_path = resolved_rollout_path?;

    let items = match RolloutRecorder::load_history(&rollout_path).await {
        Ok(items) => items,
        Err(err) => {
            warn!(
                %session_id,
                %workspace_id,
                path = %rollout_path.display(),
                error = %err,
                "Failed to read rollout history for reconnect compaction fallback"
            );
            return None;
        }
    };

    latest_compaction_attempt_from_rollout_items(&items)
}

async fn best_effort_latest_memory_flush_attempt_for_reconnect(
    state: &AppState,
    session_id: &str,
    workspace_id: &str,
    rollout_path: Option<PathBuf>,
) -> Option<MemoryFlushAttemptSnapshot> {
    let items = match load_rollout_items_for_session(
        state,
        session_id,
        workspace_id,
        rollout_path,
        "reconnect metadata fallback",
    )
    .await
    {
        Ok((_, items)) => items,
        Err(status) => {
            if status != StatusCode::NOT_FOUND {
                warn!(
                    %session_id,
                    %workspace_id,
                    ?status,
                    "Failed to load rollout for reconnect memory-flush fallback"
                );
            }
            return None;
        }
    };

    latest_memory_flush_attempt_from_rollout_items(&items)
}

async fn latest_rollout_path_for_workspace(
    state: &AppState,
    session_id: &str,
    _workspace_id: &str,
) -> Result<Option<PathBuf>, StatusCode> {
    let sessions_dirs = match state
        .get_session_read_dirs(session_id)
        .await
        .map_err(|err| {
            warn!(
                %session_id,
                error = %err,
                "Failed to recover sessions before resolving session read directories"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
        Some(dirs) => dirs,
        None => {
            warn!(%session_id, "Session not found when looking up session read directories");
            return Err(StatusCode::NOT_FOUND);
        }
    };

    latest_rollout_path_in_dirs(&sessions_dirs).map_err(|err| {
        warn!(
            %session_id,
            paths = ?sessions_dirs,
            error = %err,
            "Failed to inspect session read directories for history"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// Delete (destroy) a session and its backing runtime
pub async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!(%id, "Deleting session");
    state
        .remove_session(&id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|err| {
            warn!(session_id = %id, error = %err, "Failed to delete session");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// Request body for submitting an operation
#[derive(Deserialize)]
pub struct SubmitRequest {
    pub op: alan_protocol::Op,
}

/// Response for operation submission
#[derive(Serialize)]
pub struct SubmitResponse {
    pub submission_id: String,
    pub accepted: bool,
}

/// HTTP route wrapper for submitting an operation to a session.
pub async fn submit_operation_route(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    remote_context: Option<Extension<RemoteRequestContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SubmitResponse>, (StatusCode, Json<serde_json::Value>)> {
    let request = parse_required_json_body::<SubmitRequest, _>(
        &headers,
        &body,
        submit_request_has_removed_thinking_budget,
    )
    .map_err(json_body_error_response)?;
    submit_operation(
        State(state),
        Path(session_id),
        remote_context,
        Json(request),
    )
    .await
    .map_err(status_error_response)
}

/// Submit an operation to a session
pub async fn submit_operation(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    remote_context: Option<Extension<RemoteRequestContext>>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<SubmitResponse>, StatusCode> {
    debug!(%session_id, "Submitting operation");
    let required_scope = required_scope_for_op(&request.op);
    if let Some(context) = remote_context.as_ref().map(|ext| &ext.0)
        && !context.allows_scope(required_scope)
    {
        warn!(
            %session_id,
            required_scope = ?required_scope,
            "Rejecting submit operation due to insufficient remote scope"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to recover sessions before submit");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let submission_tx = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => session.submission_tx.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    state
        .touch_session_inbound(&session_id)
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to update inbound activity before submit");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let submission = Submission::new(request.op);
    let submission_id = submission.id.clone();

    if submission_tx.send(submission.clone()).await.is_ok() {
        return Ok(Json(SubmitResponse {
            submission_id,
            accepted: true,
        }));
    }

    // Recovery path: placeholder channel after agentd restart. Resume runtime and retry once.
    if let Err(err) = state.resume_session_runtime(&session_id).await {
        warn!(%session_id, error = %err, "Failed to resume runtime after submit channel send failure");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let retry_tx = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => session.submission_tx.clone(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };
    if retry_tx.send(submission).await.is_ok() {
        Ok(Json(SubmitResponse {
            submission_id,
            accepted: true,
        }))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

fn status_error_response(status: StatusCode) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "error": status.canonical_reason().unwrap_or("Request failed")
        })),
    )
}

/// Request manual context compaction for a session.
pub async fn compact_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<CompactSessionResponse>, StatusCode> {
    let payload = parse_optional_json_body::<CompactSessionRequest>(&headers, &body)
        .map_err(json_body_error_status)?;
    let focus = payload
        .and_then(|req| req.focus)
        .map(|focus| focus.trim().to_string())
        .filter(|focus| !focus.is_empty());
    let Json(resp) = submit_operation(
        State(state),
        Path(session_id),
        None,
        Json(SubmitRequest {
            op: alan_protocol::Op::CompactWithOptions { focus },
        }),
    )
    .await?;
    Ok(Json(CompactSessionResponse {
        submission_id: resp.submission_id,
        accepted: resp.accepted,
    }))
}

/// Request in-memory, non-durable rollback of the latest N turns for a session.
pub async fn rollback_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<RollbackSessionRequest>,
) -> Result<Json<RollbackSessionResponse>, StatusCode> {
    if request.turns == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let Json(resp) = submit_operation(
        State(state),
        Path(session_id),
        None,
        Json(SubmitRequest {
            op: alan_protocol::Op::Rollback {
                turns: request.turns,
            },
        }),
    )
    .await?;
    Ok(Json(RollbackSessionResponse {
        submission_id: resp.submission_id,
        accepted: resp.accepted,
        durability: RollbackDurabilityInfo {
            durable: false,
            scope: "in_memory",
        },
        warning: alan_runtime::ROLLBACK_NON_DURABLE_WARNING.to_string(),
    }))
}

/// Persist a one-shot `schedule_at` wake for a session.
pub async fn schedule_session_at(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<ScheduleAtRequest>,
) -> Result<Json<ScheduleResponse>, StatusCode> {
    let wake_at = parse_wake_at_or_bad_request(&request.wake_at)?;
    let schedule = state
        .schedule_at(&session_id, wake_at)
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to register schedule_at");
            if err.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    Ok(Json(schedule_response(&session_id, schedule)))
}

/// Transition a session to sleeping and register wakeup.
pub async fn sleep_session_until(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<SleepUntilRequest>,
) -> Result<Json<ScheduleResponse>, StatusCode> {
    let wake_at = parse_wake_at_or_bad_request(&request.wake_at)?;
    let schedule = state
        .sleep_until(&session_id, wake_at)
        .await
        .map_err(|err| {
            warn!(%session_id, error = %err, "Failed to register sleep_until");
            if err.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    Ok(Json(schedule_response(&session_id, schedule)))
}

fn parse_wake_at_or_bad_request(raw: &str) -> Result<DateTime<Utc>, StatusCode> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| StatusCode::BAD_REQUEST)
}

fn schedule_response(session_id: &str, schedule: ScheduleItemRecord) -> ScheduleResponse {
    let trigger_type = match schedule.trigger_type {
        ScheduleTriggerType::At => "at",
        ScheduleTriggerType::Interval => "interval",
        ScheduleTriggerType::RetryBackoff => "retry_backoff",
    };
    let status = match schedule.status {
        ScheduleStatus::Waiting => "waiting",
        ScheduleStatus::Due => "due",
        ScheduleStatus::Dispatching => "dispatching",
        ScheduleStatus::Cancelled => "cancelled",
        ScheduleStatus::Completed => "completed",
        ScheduleStatus::Failed => "failed",
    };

    ScheduleResponse {
        session_id: session_id.to_string(),
        schedule_id: schedule.schedule_id,
        run_id: schedule.run_id,
        trigger_type: trigger_type.to_string(),
        status: status.to_string(),
        wake_at: schedule.next_wake_at,
        idempotency_key: schedule.idempotency_key,
    }
}

fn reconnect_checkpoint_from_record(checkpoint: &RunCheckpointRecord) -> ReconnectCheckpoint {
    ReconnectCheckpoint {
        checkpoint_id: checkpoint.checkpoint_id.clone(),
        checkpoint_type: checkpoint.checkpoint_type.clone(),
        summary: checkpoint.summary.clone(),
        created_at: checkpoint.created_at.clone(),
        payload: checkpoint.payload.clone(),
    }
}

fn reconnect_signal_from_checkpoint(
    checkpoint: &RunCheckpointRecord,
    run_status: RunStatus,
    next_action: RunResumeAction,
) -> Option<ReconnectNotificationSignal> {
    if checkpoint.checkpoint_type != "yield" {
        return None;
    }
    if !matches!(run_status, RunStatus::Yielded)
        || !matches!(next_action, RunResumeAction::AwaitUserResume)
    {
        return None;
    }

    let request_id = checkpoint
        .payload
        .as_ref()
        .and_then(|payload| payload.get("request_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let yield_kind = checkpoint
        .payload
        .as_ref()
        .and_then(|payload| payload.get("kind"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let signal_type = if yield_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("structured_input"))
    {
        "pending_structured_input".to_string()
    } else {
        "pending_yield".to_string()
    };

    Some(ReconnectNotificationSignal {
        signal_id: checkpoint.checkpoint_id.clone(),
        signal_type,
        request_id,
        yield_kind,
        summary: checkpoint.summary.clone(),
        created_at: checkpoint.created_at.clone(),
        informational: true,
    })
}

/// Read buffered transport events with cursor-based replay semantics.
pub async fn read_events(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    query: Query<ReadEventsQuery>,
) -> Result<Json<ReadEventsResponse>, StatusCode> {
    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to recover sessions before event replay read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Query(query) = query;
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);

    let event_log = {
        let sessions = state.sessions.read().await;
        let Some(entry) = sessions.get(&session_id) else {
            return Err(StatusCode::NOT_FOUND);
        };
        Arc::clone(&entry.event_log)
    };

    state
        .touch_session_inbound(&session_id)
        .await
        .map_err(|err| {
            warn!(
                %session_id,
                error = %err,
                "Failed to update inbound activity before event replay read"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let page = {
        let guard = event_log.read().await;
        guard.read_after(query.after_event_id.as_deref(), limit)
    };

    Ok(Json(ReadEventsResponse {
        session_id,
        gap: page.gap,
        oldest_event_id: page.oldest_event_id,
        latest_event_id: page.latest_event_id,
        events: page.events,
    }))
}

/// Stream agent events as NDJSON (`application/x-ndjson`)
pub async fn stream_events(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    state.ensure_sessions_recovered().await.map_err(|err| {
        warn!(%session_id, error = %err, "Failed to recover sessions before streaming events");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut events_rx = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => session.events_tx.subscribe(),
            None => return Err(StatusCode::NOT_FOUND),
        }
    };
    let resume_error_message = match state.resume_session_runtime(&session_id).await {
        Ok(()) => None,
        Err(err) => {
            warn!(
                %session_id,
                error = %err,
                "Failed to resume session runtime before streaming events"
            );
            Some(err.to_string())
        }
    };

    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(256);
    let state_clone = state.clone();
    let session_id_clone = session_id.clone();

    tokio::spawn(async move {
        if let Some(error_message) = resume_error_message {
            let resume_error = stream_resume_failed_envelope(&session_id_clone, error_message);
            let mut payload = match serde_json::to_vec(&resume_error) {
                Ok(bytes) => bytes,
                Err(err) => {
                    warn!(
                        session_id = %session_id_clone,
                        error = %err,
                        "Failed to serialize resume error event"
                    );
                    return;
                }
            };
            payload.push(b'\n');
            if tx.send(Ok(Bytes::from(payload))).await.is_err() {
                return;
            }
        }

        let mut last_event_id: Option<String> = None;
        loop {
            match events_rx.recv().await {
                Ok(envelope) => {
                    if let Err(err) = state_clone.touch_session_outbound(&session_id_clone).await {
                        warn!(
                            session_id = %session_id_clone,
                            error = %err,
                            "Failed to update outbound activity while streaming events"
                        );
                    }
                    last_event_id = Some(envelope.event_id.clone());
                    let mut payload = match serde_json::to_vec(&envelope) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            warn!(session_id = %session_id_clone, error = %err, "Failed to serialize event");
                            continue;
                        }
                    };
                    payload.push(b'\n');
                    if tx.send(Ok(Bytes::from(payload))).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    warn!(session_id = %session_id_clone, missed = count, "Event stream lagged");
                    let lagged =
                        stream_lagged_envelope(&session_id_clone, count, last_event_id.clone());
                    let mut payload = match serde_json::to_vec(&lagged) {
                        Ok(bytes) => bytes,
                        Err(_) => break,
                    };
                    payload.push(b'\n');
                    let _ = tx.send(Ok(Bytes::from(payload))).await;
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

fn stream_lagged_envelope(
    session_id: &str,
    skipped: u64,
    replay_from_event_id: Option<String>,
) -> EventEnvelope {
    let replay_hint = replay_from_event_id
        .as_deref()
        .map(|event_id| format!(" Replay from event_id={event_id}."))
        .unwrap_or_default();
    EventEnvelope {
        event_id: format!("control_lagged_{}", uuid::Uuid::new_v4()),
        sequence: 0,
        session_id: session_id.to_string(),
        submission_id: None,
        turn_id: "turn_control".to_string(),
        item_id: "item_control".to_string(),
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        event: Event::Error {
            message: format!(
                "Event stream lagged and skipped {skipped} event(s).{}",
                replay_hint
            ),
            recoverable: true,
        },
    }
}

fn stream_resume_failed_envelope(session_id: &str, error_message: String) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("control_resume_failed_{}", uuid::Uuid::new_v4()),
        sequence: 0,
        session_id: session_id.to_string(),
        submission_id: None,
        turn_id: "turn_control".to_string(),
        item_id: "item_control".to_string(),
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        event: Event::Error {
            message: format!(
                "Failed to resume session runtime before streaming events: {error_message}"
            ),
            recoverable: true,
        },
    }
}

fn latest_rollout_path(sessions_dir: &FsPath) -> std::io::Result<Option<PathBuf>> {
    if !sessions_dir.exists() {
        return Ok(None);
    }

    let root = sessions_dir.to_path_buf();
    let mut dirs = vec![root.clone()];
    let mut latest: Option<(SystemTime, PathBuf)> = None;
    while let Some(dir) = dirs.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) => {
                if dir == root {
                    return Err(err);
                }
                warn!(
                    path = %dir.display(),
                    error = %err,
                    "Failed to inspect nested sessions directory while scanning rollouts"
                );
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    warn!(
                        path = %dir.display(),
                        error = %err,
                        "Failed to inspect nested sessions entry while scanning rollouts"
                    );
                    continue;
                }
            };
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                dirs.push(path);
                continue;
            }

            let is_jsonl = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("jsonl"))
                .unwrap_or(false);
            if !is_jsonl {
                continue;
            }

            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);

            match &latest {
                Some((best_time, best_path))
                    if modified < *best_time || (modified == *best_time && path <= *best_path) => {}
                _ => latest = Some((modified, path)),
            }
        }
    }

    Ok(latest.map(|(_, path)| path))
}

fn latest_rollout_path_in_dirs(sessions_dirs: &[PathBuf]) -> std::io::Result<Option<PathBuf>> {
    for sessions_dir in sessions_dirs {
        if let Some(path) = latest_rollout_path(sessions_dir)? {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn status_for_session_creation_error(err: &anyhow::Error) -> StatusCode {
    let message = format!("{:#}", err);
    if message.contains("Workspace already has an active session runtime") {
        StatusCode::CONFLICT
    } else if message.contains("Workspace path must already exist before creating state directory")
        || message.contains("Workspace path must be a directory")
    {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

fn rollout_items_to_history_messages(items: Vec<RolloutItem>) -> Vec<SessionHistoryMessage> {
    items
        .into_iter()
        .filter_map(|item| match item {
            RolloutItem::Message(msg) => {
                if let Some(message) = msg.message {
                    let (role, content, tool_name) = match &message {
                        alan_runtime::tape::Message::User { .. } => {
                            ("user".to_string(), message.text_content(), None)
                        }
                        alan_runtime::tape::Message::Assistant { .. } => (
                            "assistant".to_string(),
                            message.non_thinking_text_content(),
                            None,
                        ),
                        alan_runtime::tape::Message::Tool { responses } => (
                            "tool".to_string(),
                            message.text_content(),
                            responses
                                .first()
                                .map(|response| response.id.trim().to_string())
                                .filter(|id| !id.is_empty()),
                        ),
                        alan_runtime::tape::Message::System { .. }
                        | alan_runtime::tape::Message::Context { .. } => return None,
                    };

                    if content.trim().is_empty() {
                        return None;
                    }

                    return Some(SessionHistoryMessage {
                        role,
                        content,
                        tool_name,
                        timestamp: msg.timestamp,
                    });
                }

                let role = msg.role;
                if role != "user" && role != "assistant" && role != "tool" {
                    return None;
                }

                let content = msg.content.unwrap_or_default();
                if content.trim().is_empty() {
                    return None;
                }

                Some(SessionHistoryMessage {
                    role,
                    content,
                    tool_name: msg.tool_name,
                    timestamp: msg.timestamp,
                })
            }
            _ => None,
        })
        .collect()
}

struct JsonLikeFork {
    workspace_dir: Option<PathBuf>,
    agent_name: Option<String>,
    profile_id: Option<String>,
    reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    governance: Option<alan_protocol::GovernanceConfig>,
    streaming_mode: Option<alan_runtime::StreamingMode>,
    partial_stream_recovery_mode: Option<alan_runtime::PartialStreamRecoveryMode>,
}

impl JsonLikeFork {
    fn from_payload(payload: Option<ForkSessionRequest>) -> Self {
        payload
            .map(|req| Self {
                workspace_dir: req.workspace_dir.filter(|p| !p.as_os_str().is_empty()),
                agent_name: normalized_agent_name(req.agent_name),
                profile_id: req.profile_id,
                reasoning_effort: req.reasoning_effort,
                governance: req.governance,
                streaming_mode: req.streaming_mode,
                partial_stream_recovery_mode: req.partial_stream_recovery_mode,
            })
            .unwrap_or(Self {
                workspace_dir: None,
                agent_name: None,
                profile_id: None,
                reasoning_effort: None,
                governance: None,
                streaming_mode: None,
                partial_stream_recovery_mode: None,
            })
    }
}

fn normalized_agent_name(agent_name: Option<String>) -> Option<String> {
    alan_runtime::normalize_agent_name(agent_name.as_deref()).map(str::to_owned)
}

fn normalized_skill_catalog_workspace_identifier(
    workspace_dir: Option<PathBuf>,
) -> Result<Option<String>, (StatusCode, Json<serde_json::Value>)> {
    let Some(workspace_dir) = workspace_dir else {
        return Ok(None);
    };
    if workspace_dir.as_os_str().is_empty() {
        return Ok(None);
    }
    let Some(raw_identifier) = workspace_dir.to_str() else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "workspace_dir must be a UTF-8 registered workspace identifier"
            })),
        ));
    };
    let identifier = raw_identifier.trim();
    if identifier.is_empty() {
        return Ok(None);
    }
    let mut components = std::path::Path::new(identifier).components();
    let valid_single_component = matches!(
        components.next(),
        Some(std::path::Component::Normal(component))
            if component == std::ffi::OsStr::new(identifier)
    ) && components.next().is_none();
    if !valid_single_component {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "workspace_dir must be a registered workspace alias or short id"
            })),
        ));
    }
    Ok(Some(identifier.to_owned()))
}

fn validated_skill_override_agent_name(
    agent_name: Option<String>,
) -> Result<Option<String>, (StatusCode, Json<serde_json::Value>)> {
    let Some(agent_name) = agent_name else {
        return Ok(None);
    };
    let normalized = alan_runtime::normalize_agent_name(Some(agent_name.as_str()))
        .map(str::to_owned)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "agent_name must be a single non-empty path component"
                })),
            )
        })?;
    Ok(Some(normalized))
}

#[cfg(test)]
mod tests {

    use super::super::state::{SessionEntry, SessionEventLog};
    use super::*;
    use alan_protocol::{Event, Op};
    use alan_runtime::{
        Config, MessageRecord,
        runtime::{
            ChildRunRecord, ChildRunStatus, RuntimeEventEnvelope, SessionDurabilityState,
            WorkspaceRuntimeConfig, global_child_run_registry,
        },
    };
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, header},
        routing::post,
    };
    #[cfg(unix)]
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    use tempfile::TempDir;
    use tower::ServiceExt;

    fn runtime_event_with_submission(
        event: Event,
        submission_id: Option<&str>,
    ) -> RuntimeEventEnvelope {
        RuntimeEventEnvelope {
            submission_id: submission_id.map(str::to_owned),
            event,
        }
    }

    fn runtime_event(event: Event) -> RuntimeEventEnvelope {
        runtime_event_with_submission(event, Some("sub-test"))
    }

    fn json_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers
    }

    fn test_runtime_config() -> Config {
        Config::for_openai_responses("sk-test", None, Some("gpt-5.4"))
    }

    fn test_state() -> AppState {
        test_state_with_runtime_limit(10)
    }

    fn test_state_with_runtime_limit(max_concurrent_runtimes: usize) -> AppState {
        test_state_with_runtime_limit_and_config(max_concurrent_runtimes, test_runtime_config())
    }

    fn test_state_with_runtime_source_and_config(
        max_concurrent_runtimes: usize,
        core_config_source: alan_runtime::ConfigSourceKind,
        config: Config,
    ) -> AppState {
        test_state_with_runtime_source_and_config_and_registry(
            max_concurrent_runtimes,
            core_config_source,
            config,
            crate::registry::WorkspaceRegistry {
                version: 1,
                workspaces: vec![],
            },
        )
    }

    fn test_state_with_runtime_source_and_config_and_registry(
        max_concurrent_runtimes: usize,
        core_config_source: alan_runtime::ConfigSourceKind,
        config: Config,
        registry: crate::registry::WorkspaceRegistry,
    ) -> AppState {
        let base_dir =
            std::env::temp_dir().join(format!("agentd-routes-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base_dir).unwrap();

        // Create test resolver and runtime manager
        let resolver = crate::daemon::workspace_resolver::WorkspaceResolver::with_registry(
            registry,
            base_dir.clone(),
        );
        let mut runtime_config_template = WorkspaceRuntimeConfig::from(config.clone());
        runtime_config_template.core_config_source = core_config_source;
        runtime_config_template.agent_home_paths =
            Some(alan_runtime::AlanHomePaths::from_home_dir(&base_dir));
        let runtime_manager = crate::daemon::runtime_manager::RuntimeManager::new(
            crate::daemon::runtime_manager::RuntimeManagerConfig {
                max_concurrent_runtimes,
                runtime_config_template,
            },
        );
        let store = std::sync::Arc::new(
            crate::daemon::session_store::SessionStore::with_dir(base_dir.join("sessions"))
                .unwrap(),
        );
        let task_store = std::sync::Arc::new(
            crate::daemon::task_store::TaskStore::new(
                crate::daemon::task_store::JsonFileTaskStoreBackend::with_storage_dir(
                    base_dir.join("tasks"),
                )
                .unwrap(),
            )
            .unwrap(),
        );

        AppState::from_parts_with_task_store(
            config,
            std::sync::Arc::new(resolver),
            std::sync::Arc::new(runtime_manager),
            store,
            task_store,
            3600,
        )
    }

    fn test_state_with_registered_workspace(
        alias: &str,
        workspace_path: &std::path::Path,
    ) -> AppState {
        let canonical_workspace = std::fs::canonicalize(workspace_path).unwrap();
        let registry = crate::registry::WorkspaceRegistry {
            version: 1,
            workspaces: vec![crate::registry::WorkspaceEntry {
                id: crate::registry::generate_workspace_id(&canonical_workspace),
                path: canonical_workspace,
                alias: alias.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }],
        };
        test_state_with_runtime_source_and_config_and_registry(
            10,
            alan_runtime::ConfigSourceKind::Default,
            test_runtime_config(),
            registry,
        )
    }

    fn test_state_with_runtime_limit_and_config(
        max_concurrent_runtimes: usize,
        config: Config,
    ) -> AppState {
        test_state_with_runtime_source_and_config(
            max_concurrent_runtimes,
            alan_runtime::ConfigSourceKind::Default,
            config,
        )
    }

    fn session_entry(
        workspace_path: &std::path::Path,
    ) -> (SessionEntry, mpsc::Receiver<Submission>) {
        session_entry_with_replay_capacity(workspace_path, 32)
    }

    fn create_test_skill(workspace_path: &std::path::Path, skill_name: &str) {
        let skill_dir = workspace_path
            .join(".alan/agents/default/skills")
            .join(skill_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {skill_name}
description: test skill
---

Body
"#
            ),
        )
        .unwrap();
    }

    fn session_entry_with_replay_capacity(
        workspace_path: &std::path::Path,
        replay_capacity: usize,
    ) -> (SessionEntry, mpsc::Receiver<Submission>) {
        let (submission_tx, submission_rx) = mpsc::channel(8);
        let (events_tx, _) = broadcast::channel(8);
        let event_log = Arc::new(tokio::sync::RwLock::new(SessionEventLog::new(
            replay_capacity,
        )));
        let entry = SessionEntry::new(
            workspace_path.to_path_buf(),
            workspace_path.join(".alan"),
            None,
            None,
            None,
            "gpt-5.4".to_string(),
            Some(alan_protocol::ReasoningEffort::Medium),
            alan_protocol::GovernanceConfig {
                profile: alan_protocol::GovernanceProfile::Conservative,
                policy_path: None,
            },
            alan_runtime::StreamingMode::Auto,
            alan_runtime::PartialStreamRecoveryMode::ContinueOnce,
            SessionDurabilityState {
                durable: true,
                required: false,
            },
            submission_tx,
            events_tx,
            event_log,
            None,
            None,
        );
        (entry, submission_rx)
    }

    fn test_child_run_record(child_run_id: &str, parent_session_id: &str) -> ChildRunRecord {
        ChildRunRecord::new(
            child_run_id.to_string(),
            parent_session_id.to_string(),
            format!("child-session-{child_run_id}"),
            Some("/tmp/alan-child-route-workspace".to_string()),
            Some(
                "/tmp/alan-child-route-workspace/.alan/runtime/stable/sessions/child.jsonl"
                    .to_string(),
            ),
            Some("repo-coding".to_string()),
        )
    }

    struct SessionsDirPermissionGuard {
        path: std::path::PathBuf,
    }

    impl SessionsDirPermissionGuard {
        fn new(path: std::path::PathBuf) -> Self {
            set_directory_writable(&path, false);
            Self { path }
        }
    }

    impl Drop for SessionsDirPermissionGuard {
        fn drop(&mut self) {
            set_directory_writable(&self.path, true);
        }
    }

    fn prepare_recorder_blocked_workspace(
        base_dir: &std::path::Path,
    ) -> (std::path::PathBuf, SessionsDirPermissionGuard) {
        let workspace_path = base_dir.join("workspace");
        let alan_dir = workspace_path.join(".alan");
        std::fs::create_dir_all(alan_dir.join("agents/default/skills")).unwrap();
        let sessions_dir = alan_runtime::workspace_runtime_sessions_dir_from_alan_dir(
            &alan_dir,
            alan_runtime::InstallChannel::Stable,
        );
        let memory_dir = alan_runtime::workspace_runtime_memory_dir_from_alan_dir(
            &alan_dir,
            alan_runtime::InstallChannel::Stable,
        );
        std::fs::create_dir_all(&sessions_dir).unwrap();
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::create_dir_all(alan_dir.join("agents/default/persona")).unwrap();
        std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

        let guard = SessionsDirPermissionGuard::new(sessions_dir);
        (workspace_path, guard)
    }

    fn set_directory_writable(path: &std::path::Path, writable: bool) {
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            permissions.set_mode(if writable { 0o755 } else { 0o555 });
        }
        #[cfg(not(unix))]
        {
            permissions.set_readonly(!writable);
        }
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[tokio::test]
    async fn health_returns_ok() {
        assert_eq!(health().await, "OK");
    }

    #[tokio::test]
    async fn get_skill_catalog_returns_resolved_snapshot() {
        let temp = TempDir::new().unwrap();
        let workspace_path = temp.path().join("workspace");
        create_test_skill(&workspace_path, "repo-review");
        let state = test_state_with_registered_workspace("repo", &workspace_path);

        let Json(snapshot) = get_skill_catalog(
            State(state),
            Query(SkillCatalogQuery {
                workspace_dir: Some(PathBuf::from("repo")),
                agent_name: None,
            }),
        )
        .await
        .unwrap();

        assert!(!snapshot.cursor.is_empty());
        assert!(
            snapshot
                .packages
                .iter()
                .any(|package| package.id == "skill:repo-review")
        );
        assert!(
            snapshot
                .skills
                .iter()
                .any(|skill| skill.id == "repo-review")
        );
    }

    #[tokio::test]
    async fn get_skill_catalog_changed_detects_filesystem_updates() {
        let temp = TempDir::new().unwrap();
        let workspace_path = temp.path().join("workspace");
        create_test_skill(&workspace_path, "repo-review");
        let state = test_state_with_registered_workspace("repo", &workspace_path);

        let Json(initial) = get_skill_catalog(
            State(state.clone()),
            Query(SkillCatalogQuery {
                workspace_dir: Some(PathBuf::from("repo")),
                agent_name: None,
            }),
        )
        .await
        .unwrap();

        create_test_skill(&workspace_path, "lint-summary");

        let Json(changed) = get_skill_catalog_changed(
            State(state),
            Query(SkillCatalogChangedQuery {
                workspace_dir: Some(PathBuf::from("repo")),
                agent_name: None,
                after: Some(initial.cursor),
            }),
        )
        .await
        .unwrap();

        assert!(changed.changed);
        assert!(changed.skill_count >= 2);
    }

    #[tokio::test]
    async fn write_skill_override_route_persists_override_and_returns_snapshot() {
        let temp = TempDir::new().unwrap();
        let workspace_path = temp.path().join("workspace");
        create_test_skill(&workspace_path, "repo-review");
        let state = test_state_with_registered_workspace("repo", &workspace_path);

        let Json(response) = write_skill_override_route(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "workspace_dir": "repo",
                    "skill_id": "repo-review",
                    "enabled": true,
                    "allowImplicitInvocation": false
                })
                .to_string(),
            ),
        )
        .await
        .unwrap();

        assert!(
            response
                .config_path
                .ends_with(".alan/agents/default/agent.toml")
        );
        assert!(
            std::fs::read_to_string(&response.config_path)
                .unwrap()
                .contains("skill = \"repo-review\"")
        );
        let skill = response
            .snapshot
            .skills
            .iter()
            .find(|skill| skill.id == "repo-review")
            .unwrap();
        assert!(skill.enabled);
        assert!(!skill.allow_implicit_invocation);
    }

    #[tokio::test]
    async fn write_skill_override_route_rejects_unknown_skill_id() {
        let temp = TempDir::new().unwrap();
        let workspace_path = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_path).unwrap();
        create_test_skill(&workspace_path, "repo-review");
        let state = test_state_with_registered_workspace("repo", &workspace_path);

        let err = write_skill_override_route(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "workspace_dir": "repo",
                    "skill_id": "builtin-alan-plan",
                    "enabled": true
                })
                .to_string(),
            ),
        )
        .await
        .unwrap_err();

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(
            err.1.0["error"]
                .as_str()
                .unwrap()
                .contains("Unknown skill_id"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn write_skill_override_route_rejects_noncanonical_skill_id() {
        let temp = TempDir::new().unwrap();
        let workspace_path = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_path).unwrap();
        create_test_skill(&workspace_path, "repo-review");
        let state = test_state_with_registered_workspace("repo", &workspace_path);

        let err = write_skill_override_route(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "workspace_dir": "repo",
                    "skill_id": "repo.review",
                    "enabled": true
                })
                .to_string(),
            ),
        )
        .await
        .unwrap_err();

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(
            err.1.0["error"]
                .as_str()
                .unwrap()
                .contains("canonical runtime skill id `repo-review`"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn write_skill_override_route_rejects_legacy_allow_implicit_invocation_key() {
        let temp = TempDir::new().unwrap();
        let workspace_path = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_path).unwrap();
        create_test_skill(&workspace_path, "repo-review");
        let state = test_state_with_registered_workspace("repo", &workspace_path);

        let err = write_skill_override_route(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "workspace_dir": "repo",
                    "skill_id": "repo-review",
                    "allow_implicit_invocation": false
                })
                .to_string(),
            ),
        )
        .await
        .unwrap_err();

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1.0["error"], "Invalid JSON request body");
    }

    #[tokio::test]
    async fn get_skill_catalog_rejects_workspace_path_inputs() {
        let state = test_state();
        let err = get_skill_catalog(
            State(state),
            Query(SkillCatalogQuery {
                workspace_dir: Some(PathBuf::from("/tmp/workspace")),
                agent_name: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            err.1.0["error"],
            serde_json::json!("workspace_dir must be a registered workspace alias or short id")
        );
    }

    #[tokio::test]
    async fn get_skill_catalog_rejects_unknown_workspace_identifiers_as_bad_request() {
        let state = test_state();
        let err = get_skill_catalog(
            State(state),
            Query(SkillCatalogQuery {
                workspace_dir: Some(PathBuf::from("unknown-workspace")),
                agent_name: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(
            err.1.0["error"]
                .as_str()
                .unwrap_or_default()
                .contains("Unknown registered workspace identifier")
        );
    }

    #[tokio::test]
    async fn write_skill_override_route_rejects_invalid_agent_names() {
        let state = test_state();
        let err = write_skill_override_route(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "skill_id": "repo-review",
                    "agent_name": "foo/bar",
                    "enabled": true
                })
                .to_string(),
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            err.1.0["error"],
            serde_json::json!("agent_name must be a single non-empty path component")
        );
    }

    #[tokio::test]
    async fn create_session_returns_500_when_runtime_cannot_start() {
        let state = test_state_with_runtime_source_and_config(
            10,
            alan_runtime::ConfigSourceKind::EnvOverride,
            Config::default(),
        );
        let (status, _body) = create_session(State(state), HeaderMap::new(), Bytes::new())
            .await
            .err()
            .unwrap();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn create_session_returns_400_for_missing_workspace_root() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = test_state();
        let missing_workspace = temp.path().join("missing-workspace");
        let (status, body) = create_session(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "workspace_dir": missing_workspace.to_string_lossy().to_string()
                })
                .to_string(),
            ),
        )
        .await
        .err()
        .unwrap();

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body.0["error"]
                .as_str()
                .unwrap_or_default()
                .contains("Workspace path must already exist before creating state directory")
        );
    }

    #[tokio::test]
    async fn create_session_accepts_empty_json_body_for_legacy_clients() {
        let state = test_state();
        let app = Router::new()
            .route("/api/v1/sessions", post(create_session))
            .with_state(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/sessions")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(state.sessions.read().await.len(), 1);
    }

    #[tokio::test]
    async fn create_session_returns_reasoning_effort_metadata() {
        let state = test_state();

        let Json(resp) = create_session(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "reasoning_effort": "high"
                })
                .to_string(),
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            resp.reasoning_effort,
            Some(alan_protocol::ReasoningEffort::High)
        );
    }

    #[tokio::test]
    async fn create_session_rejects_legacy_thinking_budget_control() {
        let state = test_state();

        let (status, body) = match create_session(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "thinking_budget_tokens": 2048
                })
                .to_string(),
            ),
        )
        .await
        {
            Ok(_) => panic!("legacy thinking budget control should be rejected"),
            Err(err) => err,
        };

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = body.0["error"].as_str().unwrap_or_default();
        assert!(message.contains("thinking_budget_tokens"));
        assert!(message.contains("model_reasoning_effort"));
        assert!(message.contains("reasoning_effort"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn create_session_reports_non_durable_mode_and_warning_when_recorder_is_unavailable() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (workspace_path, _guard) = prepare_recorder_blocked_workspace(temp.path());

        let Json(resp) = create_session(
            State(state.clone()),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "workspace_dir": workspace_path.to_string_lossy().to_string()
                })
                .to_string(),
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            resp.execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
        assert!(!resp.durability.durable);
        assert!(!resp.durability.required);

        let Json(events) = read_events(
            State(state.clone()),
            Path(resp.session_id.clone()),
            Query(ReadEventsQuery::default()),
        )
        .await
        .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                &event.event,
                Event::Warning { message } if message.contains("in-memory mode")
            )
        }));

        let Json(read_resp) = read_session(State(state.clone()), Path(resp.session_id.clone()))
            .await
            .unwrap();
        assert_eq!(
            read_resp.execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
        assert!(!read_resp.durability.durable);
        assert!(!read_resp.durability.required);

        state.remove_session(&resp.session_id).await.unwrap();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn create_session_fails_in_strict_durability_mode_when_recorder_is_unavailable() {
        let mut config = test_runtime_config();
        config.durability.required = true;
        let state = test_state_with_runtime_limit_and_config(10, config);
        let temp = tempfile::TempDir::new().unwrap();
        let (workspace_path, _guard) = prepare_recorder_blocked_workspace(temp.path());

        let (status, body) = create_session(
            State(state),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "workspace_dir": workspace_path.to_string_lossy().to_string()
                })
                .to_string(),
            ),
        )
        .await
        .err()
        .unwrap();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            body.0["error"]
                .as_str()
                .unwrap_or_default()
                .contains("Strict durability required")
        );
    }

    #[tokio::test]
    async fn get_session_returns_not_found_for_missing_session() {
        let state = test_state();
        let err = get_session(State(state), Path("missing".to_string()))
            .await
            .err()
            .unwrap();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_missing_session_is_idempotent() {
        let state = test_state();
        let status = delete_session(State(state), Path("missing".to_string()))
            .await
            .unwrap();
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_session_removes_session_even_if_workspace_id_is_stale() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-del".to_string(), entry);

        let status = delete_session(State(state.clone()), Path("sess-del".to_string()))
            .await
            .unwrap();
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!state.get_session("sess-del").await.unwrap());
    }

    #[tokio::test]
    async fn child_run_routes_list_and_read_registered_runs() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let session_id = format!("sess-child-{}", uuid::Uuid::new_v4());
        let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
        let (entry, _rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert(session_id.clone(), entry);
        global_child_run_registry().register(test_child_run_record(&child_run_id, &session_id));

        let Json(list) = list_child_runs(State(state.clone()), Path(session_id.clone()))
            .await
            .unwrap();
        assert_eq!(list.child_runs.len(), 1);
        assert_eq!(list.child_runs[0].id, child_run_id);

        let Json(read) = get_child_run(
            State(state.clone()),
            Path((session_id.clone(), child_run_id.clone())),
        )
        .await
        .unwrap();
        assert_eq!(read.child_run.id, child_run_id);
        assert_eq!(read.child_run.parent_session_id, session_id);
    }

    #[tokio::test]
    async fn child_run_routes_return_not_found_for_missing_session_or_child() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let session_id = format!("sess-child-{}", uuid::Uuid::new_v4());
        let (entry, _rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert(session_id.clone(), entry);

        let missing_session = list_child_runs(State(state.clone()), Path("missing".to_string()))
            .await
            .err()
            .unwrap();
        assert_eq!(missing_session, StatusCode::NOT_FOUND);

        let missing_child = get_child_run(
            State(state.clone()),
            Path((session_id, "missing".to_string())),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(missing_child, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn child_run_route_terminates_and_preserves_already_terminal_runs() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let session_id = format!("sess-child-{}", uuid::Uuid::new_v4());
        let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
        let (entry, _rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert(session_id.clone(), entry);
        global_child_run_registry().register(test_child_run_record(&child_run_id, &session_id));

        let body = Bytes::from(
            serde_json::json!({
                "actor": "tui",
                "reason": "operator stop",
                "mode": "forceful"
            })
            .to_string(),
        );
        let Json(terminated) = terminate_child_run(
            State(state.clone()),
            Path((session_id.clone(), child_run_id.clone())),
            json_headers(),
            body,
        )
        .await
        .unwrap();
        assert_eq!(terminated.child_run.status, ChildRunStatus::Terminating);
        let termination = terminated.child_run.termination.as_ref().unwrap();
        assert_eq!(termination.actor, "tui");
        assert_eq!(termination.reason, "operator stop");

        global_child_run_registry().mark_terminal(&child_run_id, ChildRunStatus::Terminated, None);
        let Json(already_terminal) = terminate_child_run(
            State(state.clone()),
            Path((session_id, child_run_id)),
            HeaderMap::new(),
            Bytes::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            already_terminal.child_run.status,
            ChildRunStatus::Terminated
        );
        assert_eq!(
            already_terminal
                .child_run
                .termination
                .as_ref()
                .map(|request| request.reason.as_str()),
            Some("operator stop")
        );
    }

    #[tokio::test]
    async fn child_run_route_terminate_unknown_child_returns_json_404() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let session_id = format!("sess-child-{}", uuid::Uuid::new_v4());
        let (entry, _rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert(session_id.clone(), entry);

        let (status, body) = terminate_child_run(
            State(state.clone()),
            Path((session_id, "missing-child-run".to_string())),
            HeaderMap::new(),
            Bytes::new(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body.0["error"], "Child run not found");
    }

    #[tokio::test]
    async fn get_session_history_prefers_stored_rollout_path_over_latest_file() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _submission_rx) = session_entry(temp.path());

        let temp = tempfile::TempDir::new().unwrap();
        let older = temp.path().join("older.jsonl");
        let newer = temp.path().join("newer.jsonl");

        let old_items = [
            alan_runtime::RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "runtime-old".to_string(),
                started_at: "2026-02-23T00:00:00Z".to_string(),
                cwd: ".".to_string(),
                model: "test-model".to_string(),
                reasoning_effort: None,
            }),
            alan_runtime::RolloutItem::Message(alan_runtime::MessageRecord {
                role: "assistant".to_string(),
                content: Some("expected-from-stored".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:01Z".to_string(),
            }),
        ];
        let new_items = [
            alan_runtime::RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "runtime-new".to_string(),
                started_at: "2026-02-23T00:01:00Z".to_string(),
                cwd: ".".to_string(),
                model: "test-model".to_string(),
                reasoning_effort: None,
            }),
            alan_runtime::RolloutItem::Message(alan_runtime::MessageRecord {
                role: "assistant".to_string(),
                content: Some("wrong-latest".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:01:01Z".to_string(),
            }),
        ];

        std::fs::write(
            &older,
            old_items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n",
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        std::fs::write(
            &newer,
            new_items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n",
        )
        .unwrap();

        entry.rollout_path = Some(older.clone());
        state
            .sessions
            .write()
            .await
            .insert("sess-history".to_string(), entry);

        let Json(resp) = get_session_history(State(state), Path("sess-history".to_string()))
            .await
            .unwrap();

        assert_eq!(resp.messages.len(), 1);
        assert_eq!(resp.messages[0].content, "expected-from-stored");
    }

    #[tokio::test]
    async fn get_session_history_falls_back_to_stable_legacy_sessions_dir() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _submission_rx) = session_entry(temp.path());

        let legacy_sessions_dir = temp.path().join(".alan").join("sessions");
        std::fs::create_dir_all(&legacy_sessions_dir).unwrap();
        let rollout = legacy_sessions_dir.join("legacy-read.jsonl");
        let items = [
            alan_runtime::RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "runtime-legacy-read".to_string(),
                started_at: "2026-02-23T00:00:00Z".to_string(),
                cwd: ".".to_string(),
                model: "test-model".to_string(),
                reasoning_effort: None,
            }),
            alan_runtime::RolloutItem::Message(alan_runtime::MessageRecord {
                role: "assistant".to_string(),
                content: Some("loaded-from-stable-legacy".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:01Z".to_string(),
            }),
        ];
        std::fs::write(
            &rollout,
            items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n",
        )
        .unwrap();

        state
            .sessions
            .write()
            .await
            .insert("sess-legacy-history".to_string(), entry);

        let Json(resp) = get_session_history(
            State(state.clone()),
            Path("sess-legacy-history".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(resp.messages.len(), 1);
        assert_eq!(resp.messages[0].content, "loaded-from-stable-legacy");
        let stored = state
            .sessions
            .read()
            .await
            .get("sess-legacy-history")
            .and_then(|entry| entry.rollout_path.clone());
        assert_eq!(stored, Some(rollout));
    }

    #[tokio::test]
    async fn submit_operation_returns_not_found_for_missing_session() {
        let state = test_state();
        let err = submit_operation(
            State(state),
            Path("missing".to_string()),
            None,
            Json(SubmitRequest { op: Op::Interrupt }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn schedule_session_at_returns_bad_request_for_invalid_wake_at() {
        let state = test_state();
        let err = schedule_session_at(
            State(state),
            Path("missing".to_string()),
            Json(ScheduleAtRequest {
                wake_at: "not-a-timestamp".to_string(),
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn schedule_session_at_returns_not_found_for_missing_session() {
        let state = test_state();
        let err = schedule_session_at(
            State(state),
            Path("missing".to_string()),
            Json(ScheduleAtRequest {
                wake_at: chrono::Utc::now().to_rfc3339(),
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sleep_session_until_returns_waiting_schedule_for_existing_session() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-sleep-route".to_string(), entry);

        let wake_at = chrono::Utc::now() + chrono::Duration::minutes(2);
        let Json(resp) = sleep_session_until(
            State(state.clone()),
            Path("sess-sleep-route".to_string()),
            Json(SleepUntilRequest {
                wake_at: wake_at.to_rfc3339(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(resp.session_id, "sess-sleep-route");
        assert_eq!(resp.status, "waiting");
        assert_eq!(resp.trigger_type, "at");

        let run = state
            .task_store
            .get_run("sess-sleep-route")
            .unwrap()
            .unwrap();
        assert_eq!(run.status, crate::daemon::task_store::RunStatus::Sleeping);
    }

    #[tokio::test]
    async fn stream_events_returns_not_found_for_missing_session() {
        let state = test_state();
        let err = stream_events(State(state), Path("missing".to_string()))
            .await
            .err()
            .unwrap();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_session_history_returns_not_found_for_missing_session() {
        let state = test_state();
        let err = get_session_history(State(state), Path("missing".to_string()))
            .await
            .err()
            .unwrap();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_sessions_returns_registered_sessions() {
        let state = test_state();
        let (entry1, _rx1) = session_entry(std::path::Path::new("/tmp/ws-a"));
        let (mut entry2, _rx2) = session_entry(std::path::Path::new("/tmp/ws-b"));
        entry2.governance = alan_protocol::GovernanceConfig {
            profile: alan_protocol::GovernanceProfile::Autonomous,
            policy_path: None,
        };
        entry2.streaming_mode = alan_runtime::StreamingMode::Off;
        {
            let mut sessions = state.sessions.write().await;
            sessions.insert("sess-b".to_string(), entry2);
            sessions.insert("sess-a".to_string(), entry1);
        }

        let Json(resp) = list_sessions(State(state)).await.unwrap();
        assert_eq!(resp.sessions.len(), 2);
        assert_eq!(resp.sessions[0].session_id, "sess-a");
        assert_eq!(resp.sessions[1].session_id, "sess-b");
        assert!(resp.sessions.iter().all(|s| s.active));
        assert_eq!(
            resp.sessions[0].governance.profile,
            alan_protocol::GovernanceProfile::Conservative
        );
        assert_eq!(
            resp.sessions[0].streaming_mode,
            alan_runtime::StreamingMode::Auto
        );
        assert!(resp.sessions[0].durability.durable);
        assert!(!resp.sessions[0].durability.required);
        assert_eq!(resp.sessions[0].governance.policy_path, None);
        assert_eq!(
            resp.sessions[0].execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
        assert_eq!(
            resp.sessions[1].governance.profile,
            alan_protocol::GovernanceProfile::Autonomous
        );
        assert_eq!(
            resp.sessions[1].streaming_mode,
            alan_runtime::StreamingMode::Off
        );
        assert!(resp.sessions[1].durability.durable);
        assert!(!resp.sessions[1].durability.required);
        assert_eq!(resp.sessions[1].governance.policy_path, None);
        assert_eq!(
            resp.sessions[1].execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
    }

    #[tokio::test]
    async fn read_session_returns_metadata_and_history() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _rx) = session_entry(temp.path());

        // Get the generated workspace_id from the entry
        let expected_workspace_id = entry.workspace_id.clone();

        let temp = tempfile::TempDir::new().unwrap();
        let rollout = temp.path().join("read.jsonl");
        let items = [
            alan_runtime::RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "runtime-read".to_string(),
                started_at: "2026-02-23T00:00:00Z".to_string(),
                cwd: ".".to_string(),
                model: "test-model".to_string(),
                reasoning_effort: None,
            }),
            alan_runtime::RolloutItem::CompactionAttempt(
                alan_protocol::CompactionAttemptSnapshot {
                    attempt_id: "attempt-read".to_string(),
                    submission_id: Some("sub-read".to_string()),
                    request: alan_protocol::CompactionRequestMetadata {
                        mode: alan_protocol::CompactionMode::Manual,
                        trigger: alan_protocol::CompactionTrigger::Manual,
                        reason: alan_protocol::CompactionReason::ExplicitRequest,
                        focus: Some("preserve tasks".to_string()),
                    },
                    result: alan_protocol::CompactionResult::Success,
                    pressure_level: None,
                    memory_flush_attempt_id: None,
                    input_messages: Some(8),
                    output_messages: Some(3),
                    input_prompt_tokens: Some(512),
                    output_prompt_tokens: Some(128),
                    retry_count: 0,
                    tape_mutated: true,
                    warning_message: None,
                    error_message: None,
                    failure_streak: None,
                    reference_context_revision_before: Some(1),
                    reference_context_revision_after: Some(2),
                    timestamp: "2026-02-23T00:00:00Z".to_string(),
                },
            ),
            alan_runtime::RolloutItem::MemoryFlushAttempt(
                alan_protocol::MemoryFlushAttemptSnapshot {
                    attempt_id: "flush-read".to_string(),
                    compaction_mode: alan_protocol::CompactionMode::AutoPreTurn,
                    pressure_level: alan_protocol::CompactionPressureLevel::Soft,
                    result: alan_protocol::MemoryFlushResult::Success,
                    skip_reason: None,
                    source_messages: Some(8),
                    output_path: Some(".alan/memory/daily/2026-02-23.md".to_string()),
                    warning_message: None,
                    error_message: None,
                    timestamp: "2026-02-23T00:00:00Z".to_string(),
                },
            ),
            alan_runtime::RolloutItem::Message(alan_runtime::MessageRecord {
                role: "user".to_string(),
                content: Some("hello".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:01Z".to_string(),
            }),
        ];
        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        let compacted = serde_json::json!({
            "type": "compacted",
            "message": "Summary for read session",
            "attempt_id": "attempt-read",
            "trigger": "manual",
            "reason": "explicit_request",
            "focus": "preserve tasks",
            "input_messages": 8,
            "output_messages": 3,
            "input_tokens": 512,
            "output_tokens": 128,
            "duration_ms": 20,
            "retry_count": 0,
            "result": "success",
            "reference_context_revision": 1,
            "timestamp": "2026-02-23T00:00:00Z"
        });
        std::fs::write(&rollout, format!("{content}\n{compacted}\n")).unwrap();
        entry.rollout_path = Some(rollout.clone());

        state
            .sessions
            .write()
            .await
            .insert("sess-read".to_string(), entry);

        let Json(resp) = read_session(State(state), Path("sess-read".to_string()))
            .await
            .unwrap();
        assert_eq!(resp.session_id, "sess-read");
        assert_eq!(resp.workspace_id, expected_workspace_id);
        assert_eq!(
            resp.governance.profile,
            alan_protocol::GovernanceProfile::Conservative
        );
        assert_eq!(
            resp.execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
        assert_eq!(resp.streaming_mode, alan_runtime::StreamingMode::Auto);
        assert!(resp.durability.durable);
        assert!(!resp.durability.required);
        assert_eq!(
            resp.latest_compaction_attempt
                .as_ref()
                .and_then(|attempt| attempt.submission_id.as_deref()),
            Some("sub-read")
        );
        assert_eq!(
            resp.latest_memory_flush_attempt
                .as_ref()
                .map(|attempt| attempt.attempt_id.as_str()),
            Some("flush-read")
        );
        assert_eq!(resp.messages.len(), 1);
        assert_eq!(resp.messages[0].content, "hello");
        assert!(resp.rollout_path.unwrap().ends_with("read.jsonl"));
    }

    #[test]
    #[cfg(unix)]
    fn non_utf8_rollout_path_lossy_string_roundtrip_corrupts_bytes() {
        let original = PathBuf::from(OsString::from_vec(b"rollout-\xFF.jsonl".to_vec()));
        let round_tripped = PathBuf::from(original.to_string_lossy().to_string());

        assert_ne!(round_tripped, original);
    }

    #[tokio::test]
    #[cfg(all(unix, not(target_os = "macos")))]
    async fn read_session_preserves_non_utf8_rollout_path_for_history_loading() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _rx) = session_entry(temp.path());

        let rollout_name = OsString::from_vec(b"read-\xFF.jsonl".to_vec());
        let rollout_path = temp.path().join(rollout_name);
        let items = [
            alan_runtime::RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "runtime-non-utf8-read".to_string(),
                started_at: "2026-02-23T00:00:00Z".to_string(),
                cwd: ".".to_string(),
                model: "test-model".to_string(),
                reasoning_effort: None,
            }),
            alan_runtime::RolloutItem::Message(alan_runtime::MessageRecord {
                role: "assistant".to_string(),
                content: Some("loaded-from-non-utf8-rollout".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:01Z".to_string(),
            }),
        ];
        std::fs::write(
            &rollout_path,
            items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n",
        )
        .unwrap();
        entry.rollout_path = Some(rollout_path);

        state
            .sessions
            .write()
            .await
            .insert("sess-read-non-utf8".to_string(), entry);

        let Json(resp) = read_session(State(state), Path("sess-read-non-utf8".to_string()))
            .await
            .unwrap();

        assert_eq!(resp.messages.len(), 1);
        assert_eq!(resp.messages[0].content, "loaded-from-non-utf8-rollout");
        assert!(resp.rollout_path.is_some());
    }

    #[tokio::test]
    async fn recovered_session_metadata_reports_non_durable_until_runtime_resumes() {
        let mut config = test_runtime_config();
        config.durability.required = true;
        let state = test_state_with_runtime_limit_and_config(10, config);
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_path = temp.path().join("workspace");
        let sessions_dir = workspace_path.join(".alan").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let rollout_path = sessions_dir.join("recovered.jsonl");
        let items = [
            alan_runtime::RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "sess-recovered".to_string(),
                started_at: "2026-02-23T00:00:00Z".to_string(),
                cwd: ".".to_string(),
                model: "test-model".to_string(),
                reasoning_effort: None,
            }),
            alan_runtime::RolloutItem::Message(alan_runtime::MessageRecord {
                role: "user".to_string(),
                content: Some("hello".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:01Z".to_string(),
            }),
        ];
        std::fs::write(
            &rollout_path,
            items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n",
        )
        .unwrap();

        state
            .session_store
            .save(crate::daemon::session_store::SessionBinding {
                session_id: "sess-recovered".to_string(),
                workspace_path: workspace_path.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
                governance: alan_protocol::GovernanceConfig::default(),
                agent_name: None,
                profile_id: None,
                provider: None,
                resolved_model: String::new(),
                reasoning_effort: None,
                streaming_mode: Some(alan_runtime::StreamingMode::Auto),
                partial_stream_recovery_mode: Some(
                    alan_runtime::PartialStreamRecoveryMode::ContinueOnce,
                ),
                rollout_path: Some(rollout_path),
                durability_required: Some(true),
                durable: Some(true),
            })
            .unwrap();

        let Json(info) = get_session(State(state.clone()), Path("sess-recovered".to_string()))
            .await
            .unwrap();
        assert!(!info.active);
        assert!(info.durability.required);
        assert!(!info.durability.durable);

        let Json(list) = list_sessions(State(state.clone())).await.unwrap();
        let listed = list
            .sessions
            .into_iter()
            .find(|session| session.session_id == "sess-recovered")
            .expect("recovered session should be listed");
        assert!(!listed.active);
        assert!(listed.durability.required);
        assert!(!listed.durability.durable);

        let Json(read) = read_session(State(state), Path("sess-recovered".to_string()))
            .await
            .unwrap();
        assert!(!read.active);
        assert!(read.durability.required);
        assert!(!read.durability.durable);
        assert_eq!(read.messages.len(), 1);
        assert_eq!(read.messages[0].content, "hello");
    }

    #[tokio::test]
    async fn reconnect_snapshot_returns_not_found_for_missing_session() {
        let state = test_state();
        let err = reconnect_snapshot(State(state), Path("missing".to_string()))
            .await
            .err()
            .unwrap();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn reconnect_snapshot_returns_replay_and_pending_yield_signal() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry(temp.path());
        {
            let mut log = entry.event_log.write().await;
            let _ = log.append_runtime_event(
                "sess-reconnect",
                runtime_event(Event::ThinkingDelta {
                    chunk: "plan".to_string(),
                    is_final: false,
                }),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect",
                runtime_event(Event::Yield {
                    request_id: "req-mobile".to_string(),
                    kind: alan_protocol::YieldKind::Confirmation,
                    payload: serde_json::json!({"reason": "approve"}),
                }),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect",
                runtime_event(Event::CompactionObserved {
                    attempt: alan_protocol::CompactionAttemptSnapshot {
                        attempt_id: "attempt-reconnect".to_string(),
                        submission_id: Some("sub-test".to_string()),
                        request: alan_protocol::CompactionRequestMetadata {
                            mode: alan_protocol::CompactionMode::Manual,
                            trigger: alan_protocol::CompactionTrigger::Manual,
                            reason: alan_protocol::CompactionReason::ExplicitRequest,
                            focus: Some("preserve context".to_string()),
                        },
                        result: alan_protocol::CompactionResult::Retry,
                        pressure_level: None,
                        memory_flush_attempt_id: None,
                        input_messages: Some(10),
                        output_messages: Some(4),
                        input_prompt_tokens: Some(800),
                        output_prompt_tokens: Some(300),
                        retry_count: 1,
                        tape_mutated: true,
                        warning_message: None,
                        error_message: None,
                        failure_streak: None,
                        reference_context_revision_before: Some(2),
                        reference_context_revision_after: Some(3),
                        timestamp: "2026-03-17T12:00:00Z".to_string(),
                    },
                }),
            );
        }
        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect".to_string(), entry);

        let mut run = crate::daemon::task_store::RunRecord::new("sess-reconnect", "task-1", 1);
        run.status = crate::daemon::task_store::RunStatus::Yielded;
        state.task_store.save_run(run).unwrap();
        state
            .task_store
            .record_run_checkpoint(
                "sess-reconnect",
                "yield",
                "runtime yielded awaiting external input",
                Some(serde_json::json!({
                    "request_id": "req-mobile",
                    "kind": "confirmation"
                })),
            )
            .unwrap();

        let Json(snapshot) = reconnect_snapshot(State(state), Path("sess-reconnect".to_string()))
            .await
            .unwrap();
        assert_eq!(snapshot.session_id, "sess-reconnect");
        assert_eq!(
            snapshot.replay.latest_submission_id.as_deref(),
            Some("sub-test")
        );
        assert_eq!(snapshot.replay.buffered_event_count, 3);
        assert_eq!(
            snapshot.execution.run_status,
            Some(crate::daemon::task_store::RunStatus::Yielded)
        );
        assert_eq!(
            snapshot.execution.execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
        assert_eq!(
            snapshot.execution.next_action,
            Some(crate::daemon::task_store::RunResumeAction::AwaitUserResume)
        );
        assert!(snapshot.execution.resume_required);
        assert_eq!(
            snapshot
                .execution
                .latest_compaction_attempt
                .as_ref()
                .map(|attempt| (attempt.attempt_id.as_str(), attempt.retry_count)),
            Some(("attempt-reconnect", 1))
        );
        assert_eq!(snapshot.notifications.signals.len(), 1);
        let signal = &snapshot.notifications.signals[0];
        assert_eq!(signal.signal_type, "pending_yield");
        assert_eq!(signal.request_id.as_deref(), Some("req-mobile"));
        assert_eq!(signal.yield_kind.as_deref(), Some("confirmation"));
        assert!(signal.informational);
    }

    #[tokio::test]
    async fn reconnect_snapshot_degrades_gracefully_when_run_snapshot_missing() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry(temp.path());
        {
            let mut log = entry.event_log.write().await;
            let _ = log.append_runtime_event(
                "sess-reconnect-empty",
                runtime_event(Event::TextDelta {
                    chunk: "hello".to_string(),
                    is_final: true,
                }),
            );
        }
        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect-empty".to_string(), entry);

        let Json(snapshot) =
            reconnect_snapshot(State(state), Path("sess-reconnect-empty".to_string()))
                .await
                .unwrap();
        assert_eq!(snapshot.execution.run_status, None);
        assert_eq!(snapshot.execution.next_action, None);
        assert_eq!(
            snapshot.execution.execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
        assert!(!snapshot.execution.resume_required);
        assert!(snapshot.notifications.signals.is_empty());
        assert_eq!(
            snapshot.replay.latest_submission_id.as_deref(),
            Some("sub-test")
        );
    }

    #[tokio::test]
    async fn reconnect_snapshot_prefers_replay_compaction_attempt_when_rollout_is_unreadable() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _rx) = session_entry(temp.path());
        entry.rollout_path = Some(temp.path().to_path_buf());
        {
            let mut log = entry.event_log.write().await;
            let _ = log.append_runtime_event(
                "sess-reconnect-replay-compaction",
                runtime_event(Event::CompactionObserved {
                    attempt: alan_protocol::CompactionAttemptSnapshot {
                        attempt_id: "attempt-replay".to_string(),
                        submission_id: Some("sub-replay".to_string()),
                        request: alan_protocol::CompactionRequestMetadata {
                            mode: alan_protocol::CompactionMode::Manual,
                            trigger: alan_protocol::CompactionTrigger::Manual,
                            reason: alan_protocol::CompactionReason::ExplicitRequest,
                            focus: Some("preserve replay".to_string()),
                        },
                        result: alan_protocol::CompactionResult::Success,
                        pressure_level: None,
                        memory_flush_attempt_id: None,
                        input_messages: Some(6),
                        output_messages: Some(2),
                        input_prompt_tokens: Some(320),
                        output_prompt_tokens: Some(128),
                        retry_count: 0,
                        tape_mutated: true,
                        warning_message: None,
                        error_message: None,
                        failure_streak: None,
                        reference_context_revision_before: Some(1),
                        reference_context_revision_after: Some(2),
                        timestamp: "2026-03-17T12:00:00Z".to_string(),
                    },
                }),
            );
        }
        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect-replay-compaction".to_string(), entry);

        let Json(snapshot) = reconnect_snapshot(
            State(state),
            Path("sess-reconnect-replay-compaction".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(
            snapshot
                .execution
                .latest_compaction_attempt
                .as_ref()
                .map(|attempt| attempt.attempt_id.as_str()),
            Some("attempt-replay")
        );
    }

    #[tokio::test]
    async fn reconnect_snapshot_ignores_unreadable_rollout_for_optional_compaction_fallback() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _rx) = session_entry(temp.path());
        entry.rollout_path = Some(temp.path().to_path_buf());
        {
            let mut log = entry.event_log.write().await;
            let _ = log.append_runtime_event(
                "sess-reconnect-rollout-fallback",
                runtime_event(Event::TextDelta {
                    chunk: "hello".to_string(),
                    is_final: true,
                }),
            );
        }
        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect-rollout-fallback".to_string(), entry);

        let Json(snapshot) = reconnect_snapshot(
            State(state),
            Path("sess-reconnect-rollout-fallback".to_string()),
        )
        .await
        .unwrap();

        assert!(snapshot.execution.latest_compaction_attempt.is_none());
        assert_eq!(
            snapshot.replay.latest_submission_id.as_deref(),
            Some("sub-test")
        );
    }

    #[tokio::test]
    async fn reconnect_snapshot_retains_compaction_attempt_after_replay_eviction() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry_with_replay_capacity(temp.path(), 3);
        {
            let mut log = entry.event_log.write().await;
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-compaction",
                runtime_event(Event::TurnStarted {}),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-compaction",
                runtime_event(Event::CompactionObserved {
                    attempt: alan_protocol::CompactionAttemptSnapshot {
                        attempt_id: "attempt-evicted".to_string(),
                        submission_id: Some("sub-compaction".to_string()),
                        request: alan_protocol::CompactionRequestMetadata {
                            mode: alan_protocol::CompactionMode::Manual,
                            trigger: alan_protocol::CompactionTrigger::Manual,
                            reason: alan_protocol::CompactionReason::ExplicitRequest,
                            focus: Some("persist reconnect state".to_string()),
                        },
                        result: alan_protocol::CompactionResult::Success,
                        pressure_level: None,
                        memory_flush_attempt_id: None,
                        input_messages: Some(9),
                        output_messages: Some(3),
                        input_prompt_tokens: Some(640),
                        output_prompt_tokens: Some(220),
                        retry_count: 0,
                        tape_mutated: true,
                        warning_message: None,
                        error_message: None,
                        failure_streak: None,
                        reference_context_revision_before: Some(2),
                        reference_context_revision_after: Some(3),
                        timestamp: "2026-03-17T12:00:00Z".to_string(),
                    },
                }),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-compaction",
                runtime_event(Event::TextDelta {
                    chunk: "chunk-1".to_string(),
                    is_final: true,
                }),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-compaction",
                runtime_event(Event::TextDelta {
                    chunk: "chunk-2".to_string(),
                    is_final: true,
                }),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-compaction",
                runtime_event(Event::TextDelta {
                    chunk: "chunk-3".to_string(),
                    is_final: true,
                }),
            );
        }
        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect-evicted-compaction".to_string(), entry);

        let Json(snapshot) = reconnect_snapshot(
            State(state),
            Path("sess-reconnect-evicted-compaction".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(
            snapshot.replay.oldest_event_id.as_deref(),
            Some("evt_0000000000000003")
        );
        assert_eq!(snapshot.replay.buffered_event_count, 3);
        assert_eq!(
            snapshot
                .execution
                .latest_compaction_attempt
                .as_ref()
                .map(|attempt| attempt.attempt_id.as_str()),
            Some("attempt-evicted")
        );
    }

    #[tokio::test]
    async fn reconnect_snapshot_retains_memory_flush_attempt_after_replay_eviction() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry_with_replay_capacity(temp.path(), 2);
        {
            let mut log = entry.event_log.write().await;
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-flush",
                runtime_event(Event::TurnStarted {}),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-flush",
                runtime_event(Event::MemoryFlushObserved {
                    attempt: alan_protocol::MemoryFlushAttemptSnapshot {
                        attempt_id: "flush-evicted".to_string(),
                        compaction_mode: alan_protocol::CompactionMode::AutoPreTurn,
                        pressure_level: alan_protocol::CompactionPressureLevel::Soft,
                        result: alan_protocol::MemoryFlushResult::Success,
                        skip_reason: None,
                        source_messages: Some(7),
                        output_path: Some(".alan/memory/daily/2026-03-17.md".to_string()),
                        warning_message: None,
                        error_message: None,
                        timestamp: "2026-03-17T12:00:00Z".to_string(),
                    },
                }),
            );
            let _ = log.append_runtime_event(
                "sess-reconnect-evicted-flush",
                runtime_event(Event::TextDelta {
                    chunk: "chunk-after-flush".to_string(),
                    is_final: true,
                }),
            );
        }

        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect-evicted-flush".to_string(), entry);

        let Json(snapshot) = reconnect_snapshot(
            State(state),
            Path("sess-reconnect-evicted-flush".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(
            snapshot
                .execution
                .latest_memory_flush_attempt
                .as_ref()
                .map(|attempt| attempt.attempt_id.as_str()),
            Some("flush-evicted")
        );
    }

    #[tokio::test]
    async fn reconnect_snapshot_uses_full_buffer_for_replay_metadata() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry_with_replay_capacity(temp.path(), 1105);
        {
            let mut log = entry.event_log.write().await;
            for index in 0..1101 {
                let event = Event::TextDelta {
                    chunk: format!("chunk-{index}"),
                    is_final: index == 1100,
                };
                let runtime_event = if index == 1100 {
                    runtime_event_with_submission(event, Some("sub-tail"))
                } else {
                    runtime_event_with_submission(event, None)
                };
                let _ = log.append_runtime_event("sess-reconnect-large", runtime_event);
            }
        }
        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect-large".to_string(), entry);

        let Json(snapshot) =
            reconnect_snapshot(State(state), Path("sess-reconnect-large".to_string()))
                .await
                .unwrap();
        assert_eq!(
            snapshot.replay.oldest_event_id.as_deref(),
            Some("evt_0000000000000001")
        );
        assert_eq!(
            snapshot.replay.latest_event_id.as_deref(),
            Some("evt_0000000000001101")
        );
        assert_eq!(
            snapshot.replay.latest_submission_id.as_deref(),
            Some("sub-tail")
        );
        assert_eq!(snapshot.replay.buffered_event_count, 1101);
    }

    #[tokio::test]
    async fn reconnect_snapshot_maps_structured_input_signal_type() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-reconnect-structured".to_string(), entry);

        let mut run =
            crate::daemon::task_store::RunRecord::new("sess-reconnect-structured", "task-1", 1);
        run.status = crate::daemon::task_store::RunStatus::Yielded;
        state.task_store.save_run(run).unwrap();
        state
            .task_store
            .record_run_checkpoint(
                "sess-reconnect-structured",
                "yield",
                "runtime yielded awaiting external input",
                Some(serde_json::json!({
                    "request_id": "req-structured",
                    "kind": "structured_input"
                })),
            )
            .unwrap();

        let Json(snapshot) =
            reconnect_snapshot(State(state), Path("sess-reconnect-structured".to_string()))
                .await
                .unwrap();
        assert_eq!(snapshot.notifications.signals.len(), 1);
        assert_eq!(
            snapshot.notifications.signals[0].signal_type,
            "pending_structured_input"
        );
        assert_eq!(
            snapshot.notifications.signals[0].request_id.as_deref(),
            Some("req-structured")
        );
    }

    #[tokio::test]
    async fn get_session_and_submit_operation_work_for_existing_session() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-1".to_string(), entry);

        let info = get_session(State(state.clone()), Path("sess-1".to_string()))
            .await
            .unwrap();
        assert_eq!(info.0.session_id, "sess-1");
        assert!(info.0.active);
        assert_eq!(
            info.0.execution_backend,
            alan_runtime::tools::Sandbox::backend_name_static()
        );
        assert_eq!(
            info.0.governance.profile,
            alan_protocol::GovernanceProfile::Conservative
        );
        assert_eq!(info.0.streaming_mode, alan_runtime::StreamingMode::Auto);
        assert!(info.0.durability.durable);
        assert!(!info.0.durability.required);

        let resp = submit_operation(
            State(state.clone()),
            Path("sess-1".to_string()),
            None,
            Json(SubmitRequest {
                op: Op::Input {
                    parts: vec![alan_protocol::ContentPart::text("hello")],
                    mode: alan_protocol::InputMode::Steer,
                },
            }),
        )
        .await
        .unwrap();
        assert!(resp.0.accepted);

        let submission =
            tokio::time::timeout(std::time::Duration::from_secs(2), submission_rx.recv())
                .await
                .unwrap()
                .unwrap();
        match submission.op {
            Op::Input { parts, mode } => {
                assert_eq!(alan_protocol::parts_to_text(&parts), "hello");
                assert_eq!(mode, alan_protocol::InputMode::Steer);
            }
            other => panic!("Unexpected op: {:?}", other),
        }
    }

    #[tokio::test]
    async fn submit_operation_forbids_privileged_op_with_write_only_scope() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-scope".to_string(), entry);

        let context = RemoteRequestContext {
            node_id: Some("node-1".to_string()),
            client_id: Some("client-1".to_string()),
            trace_id: Some("trace-1".to_string()),
            transport_mode: None,
            required_scope: Some(super::super::remote_control::SessionScope::Write),
            granted_scopes: Some(std::collections::HashSet::from([
                super::super::remote_control::SessionScope::Write,
            ])),
            auth_enabled: true,
            authenticated: true,
        };

        let err = submit_operation(
            State(state.clone()),
            Path("sess-scope".to_string()),
            Some(Extension(context)),
            Json(SubmitRequest {
                op: Op::Rollback { turns: 1 },
            }),
        )
        .await
        .err()
        .unwrap();

        assert_eq!(err, StatusCode::FORBIDDEN);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), submission_rx.recv())
                .await
                .is_err(),
            "forbidden op should not be forwarded to runtime"
        );
    }

    #[tokio::test]
    async fn submit_operation_route_rejects_legacy_thinking_budget_control() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-submit-legacy".to_string(), entry);

        let app = Router::new()
            .route("/api/v1/sessions/{id}/submit", post(submit_operation_route))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/sessions/sess-submit-legacy/submit")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "op": {
                                "type": "turn",
                                "parts": [
                                    { "type": "text", "text": "hello" }
                                ],
                                "context": {
                                    "thinking_budget_tokens": 2048
                                }
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = value["error"].as_str().unwrap_or_default();
        assert!(message.contains("thinking_budget_tokens"));
        assert!(message.contains("model_reasoning_effort"));
        assert!(message.contains("reasoning_effort"));
        match tokio::time::timeout(std::time::Duration::from_millis(100), submission_rx.recv())
            .await
        {
            Err(_) | Ok(None) => {}
            Ok(Some(_)) => {
                panic!("legacy request-control payload should not be forwarded to runtime")
            }
        }
    }

    #[tokio::test]
    async fn compact_session_submits_compact_with_options_without_focus() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-compact".to_string(), entry);

        let Json(resp) = compact_session(
            State(state),
            Path("sess-compact".to_string()),
            HeaderMap::new(),
            Bytes::new(),
        )
        .await
        .unwrap();
        assert!(resp.accepted);

        let submission = submission_rx.recv().await.unwrap();
        assert!(matches!(
            submission.op,
            Op::CompactWithOptions { focus: None }
        ));
    }

    #[tokio::test]
    async fn compact_session_with_focus_submits_compact_with_options_op() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-compact-focus".to_string(), entry);

        let Json(resp) = compact_session(
            State(state),
            Path("sess-compact-focus".to_string()),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "focus": " preserve todos and constraints "
                })
                .to_string(),
            ),
        )
        .await
        .unwrap();
        assert!(resp.accepted);

        let submission = submission_rx.recv().await.unwrap();
        match submission.op {
            Op::CompactWithOptions { focus } => {
                assert_eq!(focus.as_deref(), Some("preserve todos and constraints"));
            }
            _ => panic!("expected compact_with_options op"),
        }
    }

    #[tokio::test]
    async fn compact_session_accepts_empty_json_body_without_focus() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-compact-empty-json".to_string(), entry);

        let app = Router::new()
            .route("/api/v1/sessions/{id}/compact", post(compact_session))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/sessions/sess-compact-empty-json/compact")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let submission = submission_rx.recv().await.unwrap();
        assert!(matches!(
            submission.op,
            Op::CompactWithOptions { focus: None }
        ));
    }

    #[tokio::test]
    async fn fork_session_accepts_empty_json_body_for_legacy_clients() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _submission_rx) = session_entry(temp.path());
        let sessions_dir = temp.path().join(".alan").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let rollout_path = sessions_dir.join("rollout-20260316-sess-fork-source.jsonl");
        std::fs::write(
            &rollout_path,
            serde_json::to_string(&RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "sess-fork-source".to_string(),
                started_at: "2026-03-16T00:00:00Z".to_string(),
                cwd: temp.path().display().to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: None,
            }))
            .unwrap()
                + "\n",
        )
        .unwrap();
        entry.rollout_path = Some(rollout_path);
        state
            .sessions
            .write()
            .await
            .insert("sess-fork-source".to_string(), entry);

        let app = Router::new()
            .route("/api/v1/sessions/{id}/fork", post(fork_session))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/sessions/sess-fork-source/fork")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["reasoning_effort"], serde_json::json!("medium"));
    }

    #[tokio::test]
    async fn fork_session_explicit_reasoning_effort_overrides_source_metadata() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _submission_rx) = session_entry(temp.path());
        entry.reasoning_effort = Some(alan_protocol::ReasoningEffort::High);
        let sessions_dir = temp.path().join(".alan").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let rollout_path = sessions_dir.join("rollout-20260316-sess-fork-override.jsonl");
        std::fs::write(
            &rollout_path,
            serde_json::to_string(&RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "sess-fork-override".to_string(),
                started_at: "2026-03-16T00:00:00Z".to_string(),
                cwd: temp.path().display().to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some(alan_protocol::ReasoningEffort::High),
            }))
            .unwrap()
                + "\n",
        )
        .unwrap();
        entry.rollout_path = Some(rollout_path);
        state
            .sessions
            .write()
            .await
            .insert("sess-fork-override".to_string(), entry);

        let Json(resp) = fork_session(
            State(state),
            Path("sess-fork-override".to_string()),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "reasoning_effort": "low"
                })
                .to_string(),
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            resp.reasoning_effort,
            Some(alan_protocol::ReasoningEffort::Low)
        );
    }

    #[tokio::test]
    async fn fork_session_rejects_legacy_thinking_budget_control() {
        let state = test_state();

        let (status, body) = match fork_session(
            State(state),
            Path("sess-fork-legacy".to_string()),
            json_headers(),
            Bytes::from(
                serde_json::json!({
                    "thinking_budget_tokens": 2048
                })
                .to_string(),
            ),
        )
        .await
        {
            Ok(_) => panic!("legacy thinking budget control should be rejected"),
            Err(err) => err,
        };

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = body.0["error"].as_str().unwrap_or_default();
        assert!(message.contains("thinking_budget_tokens"));
        assert!(message.contains("model_reasoning_effort"));
        assert!(message.contains("reasoning_effort"));
    }

    #[tokio::test]
    async fn fork_session_without_reasoning_override_preserves_source_metadata() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, _submission_rx) = session_entry(temp.path());
        entry.reasoning_effort = Some(alan_protocol::ReasoningEffort::High);
        let sessions_dir = temp.path().join(".alan").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let rollout_path = sessions_dir.join("rollout-20260316-sess-fork-no-override.jsonl");
        std::fs::write(
            &rollout_path,
            serde_json::to_string(&RolloutItem::SessionMeta(alan_runtime::SessionMeta {
                session_id: "sess-fork-no-override".to_string(),
                started_at: "2026-03-16T00:00:00Z".to_string(),
                cwd: temp.path().display().to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some(alan_protocol::ReasoningEffort::High),
            }))
            .unwrap()
                + "\n",
        )
        .unwrap();
        entry.rollout_path = Some(rollout_path);
        state
            .sessions
            .write()
            .await
            .insert("sess-fork-no-override".to_string(), entry);

        let Json(resp) = fork_session(
            State(state),
            Path("sess-fork-no-override".to_string()),
            json_headers(),
            Bytes::from(serde_json::json!({}).to_string()),
        )
        .await
        .unwrap();

        assert_eq!(
            resp.reasoning_effort,
            Some(alan_protocol::ReasoningEffort::High)
        );
    }

    #[tokio::test]
    async fn rollback_session_validates_and_submits_rollback_op() {
        let state = test_state();
        let err = rollback_session(
            State(state.clone()),
            Path("missing".to_string()),
            Json(RollbackSessionRequest { turns: 0 }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err, StatusCode::BAD_REQUEST);

        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-rb".to_string(), entry);

        let Json(resp) = rollback_session(
            State(state),
            Path("sess-rb".to_string()),
            Json(RollbackSessionRequest { turns: 2 }),
        )
        .await
        .unwrap();
        assert!(resp.accepted);
        assert!(!resp.durability.durable);
        assert_eq!(resp.durability.scope, "in_memory");
        assert_eq!(resp.warning, alan_runtime::ROLLBACK_NON_DURABLE_WARNING);

        let submission = submission_rx.recv().await.unwrap();
        match submission.op {
            Op::Rollback { turns } => assert_eq!(turns, 2),
            other => panic!("Unexpected op: {:?}", other),
        }
    }

    #[tokio::test]
    async fn resume_and_fork_return_not_found_for_missing_session() {
        let state = test_state();
        let resume_err = resume_session(State(state.clone()), Path("missing".to_string()))
            .await
            .err()
            .unwrap();
        assert_eq!(resume_err, StatusCode::NOT_FOUND);

        let fork_err = fork_session(
            State(state),
            Path("missing".to_string()),
            HeaderMap::new(),
            Bytes::new(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(fork_err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stream_events_emits_ndjson_and_sets_headers() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _submission_rx) = session_entry(temp.path());
        let events_tx = entry.events_tx.clone();
        state
            .sessions
            .write()
            .await
            .insert("sess-stream".to_string(), entry);

        let resp = stream_events(State(state.clone()), Path("sess-stream".to_string()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/x-ndjson"
        );
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );

        events_tx
            .send(EventEnvelope {
                event_id: "evt_0000000000000001".to_string(),
                sequence: 1,
                session_id: "sess-stream".to_string(),
                submission_id: Some("sub-test".to_string()),
                turn_id: "turn_000001".to_string(),
                item_id: "item_000001_0001".to_string(),
                timestamp_ms: 1,
                event: Event::ThinkingDelta {
                    chunk: "ping".to_string(),
                    is_final: false,
                },
            })
            .unwrap();

        // Drop all senders so the NDJSON stream terminates and body can be collected.
        state.sessions.write().await.remove("sess-stream");
        drop(events_tx);

        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("\"type\":\"thinking_delta\""));
        assert!(text.contains("\"chunk\":\"ping\""));
        assert!(text.contains("\"event_id\":\"evt_0000000000000001\""));
        assert!(text.ends_with('\n'));
    }

    #[tokio::test]
    async fn stream_events_emits_error_event_when_resume_fails() {
        let state = test_state_with_runtime_limit(0);
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _submission_rx) = session_entry(temp.path());
        let events_tx = entry.events_tx.clone();
        state
            .sessions
            .write()
            .await
            .insert("sess-resume-fail".to_string(), entry);

        let resp = stream_events(State(state.clone()), Path("sess-resume-fail".to_string()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Drop all senders so the stream terminates after control/error events are emitted.
        state.sessions.write().await.remove("sess-resume-fail");
        drop(events_tx);

        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("\"type\":\"error\""));
        assert!(text.contains("Failed to resume session runtime before streaming events"));
    }

    #[tokio::test]
    async fn read_events_returns_buffered_envelopes_with_cursor_and_gap_flag() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _submission_rx) = session_entry(temp.path());
        {
            let mut log = entry.event_log.write().await;
            let e1 = log.append_runtime_event(
                "sess-events",
                runtime_event(Event::ThinkingDelta {
                    chunk: "start".to_string(),
                    is_final: false,
                }),
            );
            let _e2 = log.append_runtime_event(
                "sess-events",
                runtime_event(Event::TextDelta {
                    chunk: "hi".to_string(),
                    is_final: true,
                }),
            );
            let _ = entry.events_tx.send(e1);
        }
        state
            .sessions
            .write()
            .await
            .insert("sess-events".to_string(), entry);

        let Json(all) = read_events(
            State(state.clone()),
            Path("sess-events".to_string()),
            Query(ReadEventsQuery::default()),
        )
        .await
        .unwrap();
        assert_eq!(all.events.len(), 2);
        assert!(!all.gap);
        assert_eq!(all.events[0].event_id, "evt_0000000000000001");

        let Json(after_first) = read_events(
            State(state.clone()),
            Path("sess-events".to_string()),
            Query(ReadEventsQuery {
                after_event_id: Some("evt_0000000000000001".to_string()),
                limit: Some(10),
            }),
        )
        .await
        .unwrap();
        assert_eq!(after_first.events.len(), 1);
        assert_eq!(after_first.events[0].event_id, "evt_0000000000000002");
        assert!(!after_first.gap);

        let Json(gap_page) = read_events(
            State(state),
            Path("sess-events".to_string()),
            Query(ReadEventsQuery {
                after_event_id: Some("evt_not_a_number".to_string()),
                limit: Some(10),
            }),
        )
        .await
        .unwrap();
        assert!(gap_page.gap);
    }

    #[test]
    fn rollout_items_to_history_messages_keeps_final_messages_only() {
        let items = vec![
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some("hello".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:00Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "assistant".to_string(),
                content: Some("world".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:01Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "system".to_string(),
                content: Some("internal".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:02Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "assistant".to_string(),
                content: Some("   ".to_string()),
                tool_name: None,
                message: None,
                timestamp: "2026-02-23T00:00:03Z".to_string(),
            }),
        ];

        let messages = rollout_items_to_history_messages(items);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "hello");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "world");
    }

    #[test]
    fn rollout_items_to_history_messages_supports_rich_message_payload() {
        let items = vec![RolloutItem::Message(MessageRecord {
            role: "assistant".to_string(),
            content: None,
            tool_name: None,
            message: Some(alan_runtime::tape::Message::Assistant {
                parts: vec![
                    alan_runtime::tape::ContentPart::thinking("internal"),
                    alan_runtime::tape::ContentPart::text("visible"),
                ],
                tool_requests: vec![],
            }),
            timestamp: "2026-02-23T00:00:00Z".to_string(),
        })];

        let messages = rollout_items_to_history_messages(items);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "assistant");
        assert_eq!(messages[0].content, "visible");
    }

    #[test]
    fn latest_rollout_path_picks_latest_jsonl() {
        let dir = std::env::temp_dir().join(format!("agentd-routes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let older = dir.join("a.jsonl");
        let newer = dir.join("b.jsonl");
        let other = dir.join("ignore.txt");

        std::fs::write(&older, "{}\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&newer, "{}\n").unwrap();
        std::fs::write(&other, "x").unwrap();

        let found = latest_rollout_path(&dir).unwrap().unwrap();
        assert_eq!(found, newer);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_rollout_path_searches_nested_directories() {
        let dir = std::env::temp_dir().join(format!("agentd-routes-{}", uuid::Uuid::new_v4()));
        let nested = dir.join("2026").join("02").join("28");
        std::fs::create_dir_all(&nested).unwrap();
        let top = dir.join("a.jsonl");
        let nested_latest = nested.join("b.jsonl");

        std::fs::write(&top, "{}\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&nested_latest, "{}\n").unwrap();

        let found = latest_rollout_path(&dir).unwrap().unwrap();
        assert_eq!(found, nested_latest);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_rollout_path_in_dirs_uses_read_dir_priority() {
        let dir = std::env::temp_dir().join(format!("agentd-routes-{}", uuid::Uuid::new_v4()));
        let primary = dir.join("runtime").join("stable").join("sessions");
        let fallback = dir.join("sessions");
        std::fs::create_dir_all(&primary).unwrap();
        std::fs::create_dir_all(&fallback).unwrap();
        let primary_rollout = primary.join("primary.jsonl");
        let fallback_rollout = fallback.join("fallback.jsonl");

        std::fs::write(&primary_rollout, "{}\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&fallback_rollout, "{}\n").unwrap();

        let found = latest_rollout_path_in_dirs(&[primary, fallback])
            .unwrap()
            .unwrap();
        assert_eq!(found, primary_rollout);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_like_fork_parses_policy_overrides() {
        let parsed = JsonLikeFork::from_payload(Some(ForkSessionRequest {
            workspace_dir: Some(PathBuf::from("/tmp/ws")),
            agent_name: Some("coder".to_string()),
            profile_id: None,
            reasoning_effort: Some(alan_protocol::ReasoningEffort::High),
            governance: Some(alan_protocol::GovernanceConfig {
                profile: alan_protocol::GovernanceProfile::Autonomous,
                policy_path: Some(".alan/agents/default/policy.yaml".to_string()),
            }),
            streaming_mode: Some(alan_runtime::StreamingMode::On),
            partial_stream_recovery_mode: Some(alan_runtime::PartialStreamRecoveryMode::Off),
        }));

        assert_eq!(parsed.workspace_dir, Some(PathBuf::from("/tmp/ws")));
        assert_eq!(parsed.agent_name.as_deref(), Some("coder"));
        assert_eq!(
            parsed.reasoning_effort,
            Some(alan_protocol::ReasoningEffort::High)
        );
        assert_eq!(
            parsed.governance,
            Some(alan_protocol::GovernanceConfig {
                profile: alan_protocol::GovernanceProfile::Autonomous,
                policy_path: Some(".alan/agents/default/policy.yaml".to_string()),
            })
        );
        assert_eq!(parsed.streaming_mode, Some(alan_runtime::StreamingMode::On));
        assert_eq!(
            parsed.partial_stream_recovery_mode,
            Some(alan_runtime::PartialStreamRecoveryMode::Off)
        );
    }

    #[test]
    fn submit_request_parses_legacy_steer_alias_as_input_mode_steer() {
        let payload = serde_json::json!({
            "op": {
                "type": "steer",
                "parts": [
                    { "type": "text", "text": "legacy steer payload" }
                ]
            }
        });

        let parsed: SubmitRequest = serde_json::from_value(payload).unwrap();
        match parsed.op {
            Op::Input { parts, mode } => {
                assert_eq!(mode, alan_protocol::InputMode::Steer);
                assert_eq!(alan_protocol::parts_to_text(&parts), "legacy steer payload");
            }
            other => panic!("unexpected op: {other:?}"),
        }
    }
}
