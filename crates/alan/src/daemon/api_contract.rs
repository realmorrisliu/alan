//! Canonical daemon API endpoint contract.

#![allow(dead_code)]

use axum::http::Method;
use serde::Serialize;

const API_PREFIX: &str = "/api/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiArea {
    Health,
    Connections,
    Sessions,
    Skills,
    Relay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionScope {
    Read,
    Write,
    Resume,
    Admin,
    HostAuthRead,
    HostAuthWrite,
}

impl SessionScope {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "session.read" => Some(Self::Read),
            "session.write" => Some(Self::Write),
            "session.resume" => Some(Self::Resume),
            "session.admin" => Some(Self::Admin),
            "host.auth.read" => Some(Self::HostAuthRead),
            "host.auth.write" => Some(Self::HostAuthWrite),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "session.read",
            Self::Write => "session.write",
            Self::Resume => "session.resume",
            Self::Admin => "session.admin",
            Self::HostAuthRead => "host.auth.read",
            Self::HostAuthWrite => "host.auth.write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Delete,
    Any,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Any => "ANY",
        }
    }

    pub fn matches(&self, method: &Method) -> bool {
        match self {
            Self::Get => method == Method::GET || method == Method::HEAD,
            Self::Post => method == Method::POST,
            Self::Patch => method == Method::PATCH,
            Self::Delete => method == Method::DELETE,
            Self::Any => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointId {
    Health,
    ConnectionsCatalog,
    ConnectionsList,
    ConnectionsCreate,
    ConnectionsCurrent,
    ConnectionsDefaultSet,
    ConnectionsDefaultClear,
    ConnectionsPin,
    ConnectionsUnpin,
    ConnectionsEvents,
    ConnectionsEventsRead,
    ConnectionGet,
    ConnectionUpdate,
    ConnectionDelete,
    ConnectionActivate,
    ConnectionCredentialStatus,
    ConnectionCredentialSecret,
    ConnectionBrowserLoginStart,
    ConnectionDeviceLoginStart,
    ConnectionDeviceLoginComplete,
    ConnectionCredentialLogout,
    ConnectionTest,
    SessionsList,
    SessionsCreate,
    SessionGet,
    SessionDelete,
    SessionChildRunsList,
    SessionChildRunGet,
    SessionChildRunTerminate,
    SessionRead,
    SessionReconnectSnapshot,
    SessionHistory,
    SessionEventsRead,
    SessionResume,
    SessionFork,
    SessionRollback,
    SessionCompact,
    SessionScheduleAt,
    SessionSleepUntil,
    SessionSubmit,
    SessionEventsStream,
    SessionWebSocket,
    SkillsCatalog,
    SkillsChanged,
    SkillsOverridesWrite,
    RelayNodesList,
    RelayTunnel,
    RelayProxy,
}

impl EndpointId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::ConnectionsCatalog => "connections_catalog",
            Self::ConnectionsList => "connections_list",
            Self::ConnectionsCreate => "connections_create",
            Self::ConnectionsCurrent => "connections_current",
            Self::ConnectionsDefaultSet => "connections_default_set",
            Self::ConnectionsDefaultClear => "connections_default_clear",
            Self::ConnectionsPin => "connections_pin",
            Self::ConnectionsUnpin => "connections_unpin",
            Self::ConnectionsEvents => "connections_events",
            Self::ConnectionsEventsRead => "connections_events_read",
            Self::ConnectionGet => "connection_get",
            Self::ConnectionUpdate => "connection_update",
            Self::ConnectionDelete => "connection_delete",
            Self::ConnectionActivate => "connection_activate",
            Self::ConnectionCredentialStatus => "connection_credential_status",
            Self::ConnectionCredentialSecret => "connection_credential_secret",
            Self::ConnectionBrowserLoginStart => "connection_browser_login_start",
            Self::ConnectionDeviceLoginStart => "connection_device_login_start",
            Self::ConnectionDeviceLoginComplete => "connection_device_login_complete",
            Self::ConnectionCredentialLogout => "connection_credential_logout",
            Self::ConnectionTest => "connection_test",
            Self::SessionsList => "sessions_list",
            Self::SessionsCreate => "sessions_create",
            Self::SessionGet => "session_get",
            Self::SessionDelete => "session_delete",
            Self::SessionChildRunsList => "session_child_runs_list",
            Self::SessionChildRunGet => "session_child_run_get",
            Self::SessionChildRunTerminate => "session_child_run_terminate",
            Self::SessionRead => "session_read",
            Self::SessionReconnectSnapshot => "session_reconnect_snapshot",
            Self::SessionHistory => "session_history",
            Self::SessionEventsRead => "session_events_read",
            Self::SessionResume => "session_resume",
            Self::SessionFork => "session_fork",
            Self::SessionRollback => "session_rollback",
            Self::SessionCompact => "session_compact",
            Self::SessionScheduleAt => "session_schedule_at",
            Self::SessionSleepUntil => "session_sleep_until",
            Self::SessionSubmit => "session_submit",
            Self::SessionEventsStream => "session_events_stream",
            Self::SessionWebSocket => "session_websocket",
            Self::SkillsCatalog => "skills_catalog",
            Self::SkillsChanged => "skills_changed",
            Self::SkillsOverridesWrite => "skills_overrides_write",
            Self::RelayNodesList => "relay_nodes_list",
            Self::RelayTunnel => "relay_tunnel",
            Self::RelayProxy => "relay_proxy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayPolicy {
    pub forwardable: bool,
    pub streaming: bool,
    pub websocket: bool,
    pub session_lifecycle: bool,
    pub session_binding: bool,
    pub response_url_fields: &'static [&'static str],
}

impl RelayPolicy {
    const fn new() -> Self {
        Self {
            forwardable: true,
            streaming: false,
            websocket: false,
            session_lifecycle: false,
            session_binding: false,
            response_url_fields: &[],
        }
    }

    const fn excluded() -> Self {
        Self {
            forwardable: false,
            streaming: false,
            websocket: false,
            session_lifecycle: false,
            session_binding: false,
            response_url_fields: &[],
        }
    }

    const fn streaming() -> Self {
        Self {
            forwardable: false,
            streaming: true,
            websocket: false,
            session_lifecycle: false,
            session_binding: true,
            response_url_fields: &[],
        }
    }

    const fn websocket() -> Self {
        Self {
            forwardable: false,
            streaming: false,
            websocket: true,
            session_lifecycle: false,
            session_binding: true,
            response_url_fields: &[],
        }
    }

    const fn lifecycle(response_url_fields: &'static [&'static str]) -> Self {
        Self {
            forwardable: true,
            streaming: false,
            websocket: false,
            session_lifecycle: true,
            session_binding: true,
            response_url_fields,
        }
    }

    const fn session() -> Self {
        Self {
            forwardable: true,
            streaming: false,
            websocket: false,
            session_lifecycle: false,
            session_binding: true,
            response_url_fields: &[],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EndpointDescriptor {
    pub id: EndpointId,
    pub method: HttpMethod,
    pub route_pattern: &'static str,
    pub path_params: &'static [&'static str],
    pub area: ApiArea,
    pub remote_scope: Option<SessionScope>,
    pub relay: RelayPolicy,
}

impl EndpointDescriptor {
    const fn new(
        id: EndpointId,
        method: HttpMethod,
        route_pattern: &'static str,
        path_params: &'static [&'static str],
        area: ApiArea,
        remote_scope: Option<SessionScope>,
        relay: RelayPolicy,
    ) -> Self {
        Self {
            id,
            method,
            route_pattern,
            path_params,
            area,
            remote_scope,
            relay,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RouteRegistration {
    pub route_pattern: &'static str,
    pub methods: &'static [HttpMethod],
}

impl RouteRegistration {
    const fn new(route_pattern: &'static str, methods: &'static [HttpMethod]) -> Self {
        Self {
            route_pattern,
            methods,
        }
    }
}

pub mod paths {
    use super::API_PREFIX;

    pub const HEALTH: &str = "/health";
    pub const CONNECTIONS_CATALOG: &str = "/api/v1/connections/catalog";
    pub const CONNECTIONS: &str = "/api/v1/connections";
    pub const CONNECTIONS_CURRENT: &str = "/api/v1/connections/current";
    pub const CONNECTIONS_DEFAULT_SET: &str = "/api/v1/connections/default/set";
    pub const CONNECTIONS_DEFAULT_CLEAR: &str = "/api/v1/connections/default/clear";
    pub const CONNECTIONS_PIN: &str = "/api/v1/connections/pin";
    pub const CONNECTIONS_UNPIN: &str = "/api/v1/connections/unpin";
    pub const CONNECTIONS_EVENTS: &str = "/api/v1/connections/events";
    pub const CONNECTIONS_EVENTS_READ: &str = "/api/v1/connections/events/read";
    pub const CONNECTION: &str = "/api/v1/connections/{profile_id}";
    pub const CONNECTION_ACTIVATE: &str = "/api/v1/connections/{profile_id}/activate";
    pub const CONNECTION_CREDENTIAL_STATUS: &str =
        "/api/v1/connections/{profile_id}/credential/status";
    pub const CONNECTION_CREDENTIAL_SECRET: &str =
        "/api/v1/connections/{profile_id}/credential/secret";
    pub const CONNECTION_BROWSER_LOGIN_START: &str =
        "/api/v1/connections/{profile_id}/credential/login/browser/start";
    pub const CONNECTION_DEVICE_LOGIN_START: &str =
        "/api/v1/connections/{profile_id}/credential/login/device/start";
    pub const CONNECTION_DEVICE_LOGIN_COMPLETE: &str =
        "/api/v1/connections/{profile_id}/credential/login/device/complete";
    pub const CONNECTION_CREDENTIAL_LOGOUT: &str =
        "/api/v1/connections/{profile_id}/credential/logout";
    pub const CONNECTION_TEST: &str = "/api/v1/connections/{profile_id}/test";

    pub const SESSIONS: &str = "/api/v1/sessions";
    pub const SESSION: &str = "/api/v1/sessions/{id}";
    pub const SESSION_CHILD_RUNS: &str = "/api/v1/sessions/{id}/child_runs";
    pub const SESSION_CHILD_RUN: &str = "/api/v1/sessions/{id}/child_runs/{child_run_id}";
    pub const SESSION_CHILD_RUN_TERMINATE: &str =
        "/api/v1/sessions/{id}/child_runs/{child_run_id}/terminate";
    pub const SESSION_READ: &str = "/api/v1/sessions/{id}/read";
    pub const SESSION_RECONNECT_SNAPSHOT: &str = "/api/v1/sessions/{id}/reconnect_snapshot";
    pub const SESSION_HISTORY: &str = "/api/v1/sessions/{id}/history";
    pub const SESSION_EVENTS_READ: &str = "/api/v1/sessions/{id}/events/read";
    pub const SESSION_RESUME: &str = "/api/v1/sessions/{id}/resume";
    pub const SESSION_FORK: &str = "/api/v1/sessions/{id}/fork";
    pub const SESSION_ROLLBACK: &str = "/api/v1/sessions/{id}/rollback";
    pub const SESSION_COMPACT: &str = "/api/v1/sessions/{id}/compact";
    pub const SESSION_SCHEDULE_AT: &str = "/api/v1/sessions/{id}/schedule_at";
    pub const SESSION_SLEEP_UNTIL: &str = "/api/v1/sessions/{id}/sleep_until";
    pub const SESSION_SUBMIT: &str = "/api/v1/sessions/{id}/submit";
    pub const SESSION_EVENTS: &str = "/api/v1/sessions/{id}/events";
    pub const SESSION_WS: &str = "/api/v1/sessions/{id}/ws";

    pub const SKILLS_CATALOG: &str = "/api/v1/skills/catalog";
    pub const SKILLS_CHANGED: &str = "/api/v1/skills/changed";
    pub const SKILLS_OVERRIDES: &str = "/api/v1/skills/overrides";

    pub const RELAY_NODES: &str = "/api/v1/relay/nodes";
    pub const RELAY_TUNNEL: &str = "/api/v1/relay/tunnel";
    pub const RELAY_PROXY: &str = "/api/v1/relay/nodes/{node_id}/{*path}";

    pub fn sessions() -> &'static str {
        SESSIONS
    }

    pub fn session(session_id: &str) -> String {
        format!("{SESSIONS}/{}", encode_segment(session_id))
    }

    pub fn session_child_runs(session_id: &str) -> String {
        format!("{}/child_runs", session(session_id))
    }

    pub fn session_child_run(session_id: &str, child_run_id: &str) -> String {
        format!(
            "{}/{}",
            session_child_runs(session_id),
            encode_segment(child_run_id)
        )
    }

    pub fn session_child_run_terminate(session_id: &str, child_run_id: &str) -> String {
        format!("{}/terminate", session_child_run(session_id, child_run_id))
    }

    pub fn session_read(session_id: &str) -> String {
        format!("{}/read", session(session_id))
    }

    pub fn session_reconnect_snapshot(session_id: &str) -> String {
        format!("{}/reconnect_snapshot", session(session_id))
    }

    pub fn session_history(session_id: &str) -> String {
        format!("{}/history", session(session_id))
    }

    pub fn session_events_read(session_id: &str) -> String {
        format!("{}/events/read", session(session_id))
    }

    pub fn session_resume(session_id: &str) -> String {
        format!("{}/resume", session(session_id))
    }

    pub fn session_fork(session_id: &str) -> String {
        format!("{}/fork", session(session_id))
    }

    pub fn session_rollback(session_id: &str) -> String {
        format!("{}/rollback", session(session_id))
    }

    pub fn session_compact(session_id: &str) -> String {
        format!("{}/compact", session(session_id))
    }

    pub fn session_schedule_at(session_id: &str) -> String {
        format!("{}/schedule_at", session(session_id))
    }

    pub fn session_sleep_until(session_id: &str) -> String {
        format!("{}/sleep_until", session(session_id))
    }

    pub fn session_submit(session_id: &str) -> String {
        format!("{}/submit", session(session_id))
    }

    pub fn session_events(session_id: &str) -> String {
        format!("{}/events", session(session_id))
    }

    pub fn session_ws(session_id: &str) -> String {
        format!("{}/ws", session(session_id))
    }

    pub fn connection(profile_id: &str) -> String {
        format!("{CONNECTIONS}/{}", encode_segment(profile_id))
    }

    pub fn connection_activate(profile_id: &str) -> String {
        format!("{}/activate", connection(profile_id))
    }

    pub fn connection_credential_status(profile_id: &str) -> String {
        format!("{}/credential/status", connection(profile_id))
    }

    pub fn connection_credential_secret(profile_id: &str) -> String {
        format!("{}/credential/secret", connection(profile_id))
    }

    pub fn connection_browser_login_start(profile_id: &str) -> String {
        format!("{}/credential/login/browser/start", connection(profile_id))
    }

    pub fn connection_device_login_start(profile_id: &str) -> String {
        format!("{}/credential/login/device/start", connection(profile_id))
    }

    pub fn connection_device_login_complete(profile_id: &str) -> String {
        format!(
            "{}/credential/login/device/complete",
            connection(profile_id)
        )
    }

    pub fn connection_credential_logout(profile_id: &str) -> String {
        format!("{}/credential/logout", connection(profile_id))
    }

    pub fn connection_test(profile_id: &str) -> String {
        format!("{}/test", connection(profile_id))
    }

    pub fn relay_node_prefix(node_id: &str) -> String {
        format!("{RELAY_NODES}/{}", encode_segment(node_id))
    }

    pub fn relay_node_proxy_path(node_id: &str, forwarded_path: &str) -> String {
        let forwarded_path = forwarded_path.trim_start_matches('/');
        format!("{}/{}", relay_node_prefix(node_id), forwarded_path)
    }

    pub fn api_prefix() -> &'static str {
        API_PREFIX
    }

    fn encode_segment(segment: &str) -> String {
        urlencoding::encode(segment).into_owned()
    }
}

const SESSION_RESPONSE_URL_FIELDS: &[&str] = &["websocket_url", "events_url", "submit_url"];

pub const ENDPOINTS: &[EndpointDescriptor] = &[
    EndpointDescriptor::new(
        EndpointId::Health,
        HttpMethod::Get,
        paths::HEALTH,
        &[],
        ApiArea::Health,
        None,
        RelayPolicy::excluded(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsCatalog,
        HttpMethod::Get,
        paths::CONNECTIONS_CATALOG,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthRead),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsList,
        HttpMethod::Get,
        paths::CONNECTIONS,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthRead),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsCreate,
        HttpMethod::Post,
        paths::CONNECTIONS,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsCurrent,
        HttpMethod::Get,
        paths::CONNECTIONS_CURRENT,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthRead),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsDefaultSet,
        HttpMethod::Post,
        paths::CONNECTIONS_DEFAULT_SET,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsDefaultClear,
        HttpMethod::Post,
        paths::CONNECTIONS_DEFAULT_CLEAR,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsPin,
        HttpMethod::Post,
        paths::CONNECTIONS_PIN,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsUnpin,
        HttpMethod::Post,
        paths::CONNECTIONS_UNPIN,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsEvents,
        HttpMethod::Get,
        paths::CONNECTIONS_EVENTS,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthRead),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionsEventsRead,
        HttpMethod::Get,
        paths::CONNECTIONS_EVENTS_READ,
        &[],
        ApiArea::Connections,
        Some(SessionScope::HostAuthRead),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionGet,
        HttpMethod::Get,
        paths::CONNECTION,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthRead),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionUpdate,
        HttpMethod::Patch,
        paths::CONNECTION,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionDelete,
        HttpMethod::Delete,
        paths::CONNECTION,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionActivate,
        HttpMethod::Post,
        paths::CONNECTION_ACTIVATE,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionCredentialStatus,
        HttpMethod::Get,
        paths::CONNECTION_CREDENTIAL_STATUS,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthRead),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionCredentialSecret,
        HttpMethod::Post,
        paths::CONNECTION_CREDENTIAL_SECRET,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionBrowserLoginStart,
        HttpMethod::Post,
        paths::CONNECTION_BROWSER_LOGIN_START,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionDeviceLoginStart,
        HttpMethod::Post,
        paths::CONNECTION_DEVICE_LOGIN_START,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionDeviceLoginComplete,
        HttpMethod::Post,
        paths::CONNECTION_DEVICE_LOGIN_COMPLETE,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionCredentialLogout,
        HttpMethod::Post,
        paths::CONNECTION_CREDENTIAL_LOGOUT,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::ConnectionTest,
        HttpMethod::Post,
        paths::CONNECTION_TEST,
        &["profile_id"],
        ApiArea::Connections,
        Some(SessionScope::HostAuthWrite),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionsList,
        HttpMethod::Get,
        paths::SESSIONS,
        &[],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionsCreate,
        HttpMethod::Post,
        paths::SESSIONS,
        &[],
        ApiArea::Sessions,
        Some(SessionScope::Write),
        RelayPolicy::lifecycle(SESSION_RESPONSE_URL_FIELDS),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionGet,
        HttpMethod::Get,
        paths::SESSION,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionDelete,
        HttpMethod::Delete,
        paths::SESSION,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Admin),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionChildRunsList,
        HttpMethod::Get,
        paths::SESSION_CHILD_RUNS,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionChildRunGet,
        HttpMethod::Get,
        paths::SESSION_CHILD_RUN,
        &["id", "child_run_id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionChildRunTerminate,
        HttpMethod::Post,
        paths::SESSION_CHILD_RUN_TERMINATE,
        &["id", "child_run_id"],
        ApiArea::Sessions,
        Some(SessionScope::Write),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionRead,
        HttpMethod::Get,
        paths::SESSION_READ,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionReconnectSnapshot,
        HttpMethod::Get,
        paths::SESSION_RECONNECT_SNAPSHOT,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionHistory,
        HttpMethod::Get,
        paths::SESSION_HISTORY,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionEventsRead,
        HttpMethod::Get,
        paths::SESSION_EVENTS_READ,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionResume,
        HttpMethod::Post,
        paths::SESSION_RESUME,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Resume),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionFork,
        HttpMethod::Post,
        paths::SESSION_FORK,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Admin),
        RelayPolicy::lifecycle(SESSION_RESPONSE_URL_FIELDS),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionRollback,
        HttpMethod::Post,
        paths::SESSION_ROLLBACK,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Admin),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionCompact,
        HttpMethod::Post,
        paths::SESSION_COMPACT,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Admin),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionScheduleAt,
        HttpMethod::Post,
        paths::SESSION_SCHEDULE_AT,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Admin),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionSleepUntil,
        HttpMethod::Post,
        paths::SESSION_SLEEP_UNTIL,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Admin),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionSubmit,
        HttpMethod::Post,
        paths::SESSION_SUBMIT,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Write),
        RelayPolicy::session(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionEventsStream,
        HttpMethod::Get,
        paths::SESSION_EVENTS,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Read),
        RelayPolicy::streaming(),
    ),
    EndpointDescriptor::new(
        EndpointId::SessionWebSocket,
        HttpMethod::Get,
        paths::SESSION_WS,
        &["id"],
        ApiArea::Sessions,
        Some(SessionScope::Write),
        RelayPolicy::websocket(),
    ),
    EndpointDescriptor::new(
        EndpointId::SkillsCatalog,
        HttpMethod::Get,
        paths::SKILLS_CATALOG,
        &[],
        ApiArea::Skills,
        Some(SessionScope::Read),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::SkillsChanged,
        HttpMethod::Get,
        paths::SKILLS_CHANGED,
        &[],
        ApiArea::Skills,
        Some(SessionScope::Read),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::SkillsOverridesWrite,
        HttpMethod::Post,
        paths::SKILLS_OVERRIDES,
        &[],
        ApiArea::Skills,
        Some(SessionScope::Admin),
        RelayPolicy::new(),
    ),
    EndpointDescriptor::new(
        EndpointId::RelayNodesList,
        HttpMethod::Get,
        paths::RELAY_NODES,
        &[],
        ApiArea::Relay,
        Some(SessionScope::Read),
        RelayPolicy::excluded(),
    ),
    EndpointDescriptor::new(
        EndpointId::RelayTunnel,
        HttpMethod::Get,
        paths::RELAY_TUNNEL,
        &[],
        ApiArea::Relay,
        None,
        RelayPolicy::excluded(),
    ),
    EndpointDescriptor::new(
        EndpointId::RelayProxy,
        HttpMethod::Any,
        paths::RELAY_PROXY,
        &["node_id", "path"],
        ApiArea::Relay,
        None,
        RelayPolicy::excluded(),
    ),
];

pub const SERVER_ROUTES: &[RouteRegistration] = &[
    RouteRegistration::new(paths::HEALTH, &[HttpMethod::Get]),
    RouteRegistration::new(paths::CONNECTIONS_CATALOG, &[HttpMethod::Get]),
    RouteRegistration::new(paths::CONNECTIONS, &[HttpMethod::Get, HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTIONS_CURRENT, &[HttpMethod::Get]),
    RouteRegistration::new(paths::CONNECTIONS_DEFAULT_SET, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTIONS_DEFAULT_CLEAR, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTIONS_PIN, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTIONS_UNPIN, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTIONS_EVENTS, &[HttpMethod::Get]),
    RouteRegistration::new(paths::CONNECTIONS_EVENTS_READ, &[HttpMethod::Get]),
    RouteRegistration::new(
        paths::CONNECTION,
        &[HttpMethod::Get, HttpMethod::Patch, HttpMethod::Delete],
    ),
    RouteRegistration::new(paths::CONNECTION_ACTIVATE, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTION_CREDENTIAL_STATUS, &[HttpMethod::Get]),
    RouteRegistration::new(paths::CONNECTION_CREDENTIAL_SECRET, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTION_BROWSER_LOGIN_START, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTION_DEVICE_LOGIN_START, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTION_DEVICE_LOGIN_COMPLETE, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTION_CREDENTIAL_LOGOUT, &[HttpMethod::Post]),
    RouteRegistration::new(paths::CONNECTION_TEST, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSIONS, &[HttpMethod::Get, HttpMethod::Post]),
    RouteRegistration::new(paths::SKILLS_CATALOG, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SKILLS_CHANGED, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SKILLS_OVERRIDES, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION, &[HttpMethod::Get, HttpMethod::Delete]),
    RouteRegistration::new(paths::SESSION_CHILD_RUNS, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SESSION_CHILD_RUN, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SESSION_CHILD_RUN_TERMINATE, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_READ, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SESSION_RECONNECT_SNAPSHOT, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SESSION_HISTORY, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SESSION_EVENTS_READ, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SESSION_RESUME, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_FORK, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_ROLLBACK, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_COMPACT, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_SCHEDULE_AT, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_SLEEP_UNTIL, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_SUBMIT, &[HttpMethod::Post]),
    RouteRegistration::new(paths::SESSION_EVENTS, &[HttpMethod::Get]),
    RouteRegistration::new(paths::SESSION_WS, &[HttpMethod::Get]),
    RouteRegistration::new(paths::RELAY_NODES, &[HttpMethod::Get]),
    RouteRegistration::new(paths::RELAY_TUNNEL, &[HttpMethod::Get]),
    RouteRegistration::new(paths::RELAY_PROXY, &[HttpMethod::Any]),
];

#[derive(Debug, Clone, Serialize)]
pub struct EndpointManifest {
    pub version: u32,
    pub endpoints: Vec<EndpointManifestEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointManifestEntry {
    pub id: &'static str,
    pub method: &'static str,
    pub route_pattern: &'static str,
    pub path_params: &'static [&'static str],
    pub area: ApiArea,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_scope: Option<&'static str>,
    pub relay: RelayManifestPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelayManifestPolicy {
    pub forwardable: bool,
    pub streaming: bool,
    pub websocket: bool,
    pub session_lifecycle: bool,
    pub session_binding: bool,
    pub response_url_fields: &'static [&'static str],
}

pub fn endpoint_manifest() -> EndpointManifest {
    EndpointManifest {
        version: 1,
        endpoints: ENDPOINTS
            .iter()
            .map(|endpoint| EndpointManifestEntry {
                id: endpoint.id.as_str(),
                method: endpoint.method.as_str(),
                route_pattern: endpoint.route_pattern,
                path_params: endpoint.path_params,
                area: endpoint.area,
                remote_scope: endpoint.remote_scope.map(|scope| scope.as_str()),
                relay: RelayManifestPolicy {
                    forwardable: endpoint.relay.forwardable,
                    streaming: endpoint.relay.streaming,
                    websocket: endpoint.relay.websocket,
                    session_lifecycle: endpoint.relay.session_lifecycle,
                    session_binding: endpoint.relay.session_binding,
                    response_url_fields: endpoint.relay.response_url_fields,
                },
            })
            .collect(),
    }
}

pub fn endpoint_for_id(id: EndpointId) -> &'static EndpointDescriptor {
    ENDPOINTS
        .iter()
        .find(|endpoint| endpoint.id == id)
        .expect("endpoint id has descriptor")
}

pub fn match_endpoint(method: &Method, path: &str) -> Option<&'static EndpointDescriptor> {
    let path = path_without_query(path);
    ENDPOINTS
        .iter()
        .find(|endpoint| endpoint.method.matches(method) && route_pattern_matches(endpoint, path))
}

pub fn match_forwarded_endpoint(
    method: &Method,
    relay_proxy_path: &str,
) -> Option<&'static EndpointDescriptor> {
    let forwarded_path = relay_proxy_forwarded_path(relay_proxy_path)?;
    match_endpoint(method, &forwarded_path)
}

pub fn relay_proxy_forwarded_path(path: &str) -> Option<String> {
    let path = path_without_query(path);
    let prefix = "/api/v1/relay/nodes/";
    let remainder = path.strip_prefix(prefix)?;
    let (_, forwarded_path) = remainder.split_once('/')?;
    let forwarded_path = forwarded_path.trim_start_matches('/');
    if forwarded_path.is_empty() {
        return None;
    }
    Some(format!("/{forwarded_path}"))
}

pub fn is_relay_proxy_path(path: &str) -> bool {
    path_without_query(path).starts_with("/api/v1/relay/nodes/")
        && relay_proxy_forwarded_path(path).is_some()
}

pub fn is_api_path(path: &str) -> bool {
    path_without_query(path).starts_with(paths::api_prefix())
}

pub fn path_without_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

pub fn extract_session_id_from_path(path: &str) -> Option<&str> {
    let path = path_without_query(path);
    let remainder = path.strip_prefix("/api/v1/sessions/")?;
    let session_id = remainder.split('/').next().unwrap_or("").trim();
    if session_id.is_empty() {
        return None;
    }
    Some(session_id)
}

pub fn relay_prefixed_session_url(node_id: &str, url: &str) -> Option<String> {
    if !path_without_query(url).starts_with(paths::SESSIONS) {
        return None;
    }
    Some(paths::relay_node_proxy_path(node_id, url))
}

fn route_pattern_matches(endpoint: &EndpointDescriptor, path: &str) -> bool {
    let mut pattern_segments = endpoint.route_pattern.trim_matches('/').split('/');
    let mut path_segments = path.trim_matches('/').split('/');

    loop {
        match (pattern_segments.next(), path_segments.next()) {
            (Some(pattern), Some(_)) if is_path_param(pattern) => continue,
            (Some(pattern), Some(path)) if is_wildcard_param(pattern) => {
                return !path.is_empty();
            }
            (Some(pattern), Some(path)) if pattern == path => continue,
            (None, None) => return true,
            _ => return false,
        }
    }
}

fn is_path_param(segment: &str) -> bool {
    segment.starts_with('{') && segment.ends_with('}') && !segment.starts_with("{*")
}

fn is_wildcard_param(segment: &str) -> bool {
    segment.starts_with("{*") && segment.ends_with('}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_server_route_has_endpoint_metadata() {
        let endpoints = ENDPOINTS
            .iter()
            .map(|endpoint| (endpoint.route_pattern, endpoint.method))
            .collect::<HashSet<_>>();

        for route in SERVER_ROUTES {
            for method in route.methods {
                assert!(
                    endpoints.contains(&(route.route_pattern, *method)),
                    "missing endpoint metadata for {} {}",
                    method.as_str(),
                    route.route_pattern
                );
            }
        }
    }

    #[test]
    fn match_endpoint_resolves_representative_paths() {
        assert_eq!(
            match_endpoint(&Method::GET, "/api/v1/sessions/s1/events/read")
                .map(|endpoint| endpoint.id),
            Some(EndpointId::SessionEventsRead)
        );
        assert_eq!(
            match_endpoint(&Method::HEAD, "/api/v1/sessions/s1/events/read")
                .map(|endpoint| endpoint.id),
            Some(EndpointId::SessionEventsRead)
        );
        assert_eq!(
            match_endpoint(&Method::POST, "/api/v1/sessions/s1/fork").map(|endpoint| endpoint.id),
            Some(EndpointId::SessionFork)
        );
        assert_eq!(
            match_endpoint(
                &Method::POST,
                "/api/v1/connections/chatgpt-main/credential/login/device/start"
            )
            .map(|endpoint| endpoint.id),
            Some(EndpointId::ConnectionDeviceLoginStart)
        );
        assert_eq!(
            match_endpoint(&Method::GET, "/api/v1/unknown").map(|endpoint| endpoint.id),
            None
        );
    }

    #[test]
    fn path_builders_encode_path_parameters() {
        assert_eq!(
            paths::session_submit("session/with slash"),
            "/api/v1/sessions/session%2Fwith%20slash/submit"
        );
        assert_eq!(
            paths::connection_test("profile/main"),
            "/api/v1/connections/profile%2Fmain/test"
        );
    }

    #[test]
    fn session_response_url_builders_preserve_public_paths() {
        assert_eq!(paths::session_ws("sess-1"), "/api/v1/sessions/sess-1/ws");
        assert_eq!(
            paths::session_events("sess-1"),
            "/api/v1/sessions/sess-1/events"
        );
        assert_eq!(
            paths::session_submit("sess-1"),
            "/api/v1/sessions/sess-1/submit"
        );
    }

    #[test]
    fn relay_forwarded_path_extracts_target_api_path() {
        assert_eq!(
            relay_proxy_forwarded_path("/api/v1/relay/nodes/node-a/api/v1/sessions/s1/submit")
                .as_deref(),
            Some("/api/v1/sessions/s1/submit")
        );
        assert_eq!(
            relay_proxy_forwarded_path("/api/v1/relay/nodes/node-a"),
            None
        );
    }
}
