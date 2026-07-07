//! Snapshot-style checks for selected daemon payload fields consumed by dynamic clients.

use alan::daemon::connection_control::{ConnectionCredentialStatusKind, ConnectionProfileSummary};
use alan::daemon::connection_routes::{
    ConnectionCatalogResponse, ConnectionListResponse, ProviderDescriptorView,
};
use alan::daemon::routes::{
    ChildRunListResponse, ChildRunResponse, CreateSessionResponse, ForkSessionResponse,
    SessionDurabilityInfo, SessionListItem, SessionListResponse, SessionReadResponse,
};
use alan_agent_engine::runtime::ChildRunRecord;
use alan_agent_engine::{CredentialKind, LlmProvider, PartialStreamRecoveryMode, StreamingMode};
use alan_agent_protocol::GovernanceConfig;
use chrono::Utc;
use std::collections::BTreeMap;

#[test]
fn create_session_payload_preserves_selected_terminal_fields() {
    let payload = serde_json::to_value(sample_create_session_response()).unwrap();
    let object = payload
        .as_object()
        .expect("create-session payload should serialize as an object");

    assert_eq!(
        object.get("session_id").and_then(serde_json::Value::as_str),
        Some("sess-1")
    );
    assert_eq!(
        object.get("profile_id").and_then(serde_json::Value::as_str),
        Some("chatgpt-main")
    );
    assert_eq!(
        object
            .get("resolved_model")
            .and_then(serde_json::Value::as_str),
        Some("gpt-5.3-codex")
    );
    assert!(object.contains_key("provider"));
    assert!(object.contains_key("durability"));
}

#[test]
fn selected_dynamic_tui_payloads_remain_json_objects() {
    assert_payload_is_json_object("ForkSessionResponse", &sample_fork_session_response());
    assert_payload_is_json_object("SessionListItem", &sample_session_list_item());
    assert_payload_is_json_object(
        "SessionListResponse",
        &SessionListResponse {
            sessions: vec![sample_session_list_item()],
        },
    );
    assert_payload_is_json_object("SessionReadResponse", &sample_session_read_response());
    assert_payload_is_json_object("ChildRunRecord", &sample_child_run_record());
    assert_payload_is_json_object(
        "ChildRunListResponse",
        &ChildRunListResponse {
            child_runs: vec![sample_child_run_record()],
        },
    );
    assert_payload_is_json_object(
        "ChildRunResponse",
        &ChildRunResponse {
            child_run: sample_child_run_record(),
        },
    );
    assert_payload_is_json_object(
        "ConnectionCatalogResponse",
        &ConnectionCatalogResponse {
            providers: vec![sample_provider_descriptor()],
        },
    );
    assert_payload_is_json_object(
        "ConnectionProfileSummary",
        &sample_connection_profile_summary(),
    );
    assert_payload_is_json_object(
        "ConnectionListResponse",
        &ConnectionListResponse {
            default_profile: Some("chatgpt-main".to_string()),
            profiles: vec![sample_connection_profile_summary()],
        },
    );
}

fn assert_payload_is_json_object<T: serde::Serialize>(name: &str, value: &T) {
    let value = serde_json::to_value(value).expect("payload serializes to JSON");
    assert!(
        value.as_object().is_some_and(|object| !object.is_empty()),
        "{name} must serialize to a non-empty JSON object for dynamic Rust TUI readers"
    );
}

fn sample_create_session_response() -> CreateSessionResponse {
    CreateSessionResponse {
        session_id: "sess-1".to_string(),
        websocket_url: "/api/v1/sessions/sess-1/ws".to_string(),
        events_url: "/api/v1/sessions/sess-1/events".to_string(),
        submit_url: "/api/v1/sessions/sess-1/submit".to_string(),
        agent_name: Some("default".to_string()),
        governance: GovernanceConfig::default(),
        execution_backend: "workspace_path_guard".to_string(),
        streaming_mode: StreamingMode::Auto,
        partial_stream_recovery_mode: PartialStreamRecoveryMode::ContinueOnce,
        profile_id: Some("chatgpt-main".to_string()),
        provider: Some(LlmProvider::Chatgpt),
        resolved_model: "gpt-5.3-codex".to_string(),
        reasoning_effort: Some(alan_agent_protocol::ReasoningEffort::Medium),
        durability: sample_durability(),
    }
}

fn sample_fork_session_response() -> ForkSessionResponse {
    ForkSessionResponse {
        session_id: "sess-fork".to_string(),
        forked_from_session_id: "sess-1".to_string(),
        websocket_url: "/api/v1/sessions/sess-fork/ws".to_string(),
        events_url: "/api/v1/sessions/sess-fork/events".to_string(),
        submit_url: "/api/v1/sessions/sess-fork/submit".to_string(),
        agent_name: Some("default".to_string()),
        governance: GovernanceConfig::default(),
        streaming_mode: StreamingMode::Auto,
        partial_stream_recovery_mode: PartialStreamRecoveryMode::ContinueOnce,
        profile_id: Some("chatgpt-main".to_string()),
        provider: Some(LlmProvider::Chatgpt),
        resolved_model: "gpt-5.3-codex".to_string(),
        reasoning_effort: Some(alan_agent_protocol::ReasoningEffort::Low),
        durability: sample_durability(),
    }
}

fn sample_session_list_item() -> SessionListItem {
    SessionListItem {
        session_id: "sess-1".to_string(),
        workspace_id: "/tmp/workspace".to_string(),
        active: true,
        agent_name: Some("default".to_string()),
        governance: GovernanceConfig::default(),
        execution_backend: "workspace_path_guard".to_string(),
        streaming_mode: StreamingMode::Auto,
        partial_stream_recovery_mode: PartialStreamRecoveryMode::ContinueOnce,
        profile_id: Some("chatgpt-main".to_string()),
        provider: Some(LlmProvider::Chatgpt),
        resolved_model: "gpt-5.3-codex".to_string(),
        reasoning_effort: Some(alan_agent_protocol::ReasoningEffort::Medium),
        durability: sample_durability(),
    }
}

fn sample_session_read_response() -> SessionReadResponse {
    SessionReadResponse {
        session_id: "sess-1".to_string(),
        workspace_id: "/tmp/workspace".to_string(),
        active: true,
        agent_name: Some("default".to_string()),
        governance: GovernanceConfig::default(),
        execution_backend: "workspace_path_guard".to_string(),
        streaming_mode: StreamingMode::Auto,
        partial_stream_recovery_mode: PartialStreamRecoveryMode::ContinueOnce,
        profile_id: Some("chatgpt-main".to_string()),
        provider: Some(LlmProvider::Chatgpt),
        resolved_model: "gpt-5.3-codex".to_string(),
        reasoning_effort: Some(alan_agent_protocol::ReasoningEffort::Medium),
        durability: sample_durability(),
        rollout_path: Some("/tmp/workspace/.alan/runtime/stable/sessions/sess-1.jsonl".to_string()),
        latest_compaction_attempt: None,
        latest_memory_flush_attempt: None,
        latest_plan_snapshot: None,
        messages: vec![],
    }
}

fn sample_child_run_record() -> ChildRunRecord {
    let mut record = ChildRunRecord::new(
        "child-1".to_string(),
        "sess-1".to_string(),
        "sess-child".to_string(),
        Some("/tmp/workspace".to_string()),
        Some("/tmp/workspace/.alan/runtime/stable/sessions/child.jsonl".to_string()),
        Some("repo-coding".to_string()),
    );
    record.latest_event_kind = Some("text_delta".to_string());
    record.latest_status_summary = Some("working".to_string());
    record
}

fn sample_provider_descriptor() -> ProviderDescriptorView {
    ProviderDescriptorView {
        provider_id: LlmProvider::Chatgpt,
        display_name: "ChatGPT".to_string(),
        credential_kind: CredentialKind::ManagedOauth,
        supports_browser_login: true,
        supports_device_login: true,
        supports_secret_entry: false,
        supports_logout: true,
        supports_test: true,
        required_settings: vec!["model".to_string()],
        optional_settings: vec!["account_id".to_string()],
        default_settings: BTreeMap::from([("model".to_string(), "gpt-5.3-codex".to_string())]),
    }
}

fn sample_connection_profile_summary() -> ConnectionProfileSummary {
    ConnectionProfileSummary {
        profile_id: "chatgpt-main".to_string(),
        label: Some("ChatGPT".to_string()),
        provider: LlmProvider::Chatgpt,
        credential_id: Some("chatgpt".to_string()),
        settings: BTreeMap::from([("model".to_string(), "gpt-5.3-codex".to_string())]),
        credential_status: ConnectionCredentialStatusKind::Available,
        is_default: true,
        source: "managed".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn sample_durability() -> SessionDurabilityInfo {
    SessionDurabilityInfo {
        durable: false,
        required: false,
    }
}
