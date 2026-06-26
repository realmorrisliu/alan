//! Remote-control request metadata and scope-based authn/authz for daemon APIs.
//!
//! This module implements Phase A direct-remote controls:
//! - additive routing metadata headers (`node_id`, `client_id`, `trace_id`, `transport_mode`)
//! - optional bearer-token scope enforcement for session APIs
//! - request-context injection for downstream audit/logging

use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};

use axum::{
    Json,
    extract::Request,
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::{debug, warn};

use super::api_contract::{self, EndpointId};

pub use super::api_contract::SessionScope;

const HEADER_NODE_ID: &str = "x-alan-node-id";
const HEADER_CLIENT_ID: &str = "x-alan-client-id";
const HEADER_TRACE_ID: &str = "x-alan-trace-id";
const HEADER_TRANSPORT_MODE: &str = "x-alan-transport-mode";

const ENV_REMOTE_AUTH_ENABLED: &str = "ALAN_REMOTE_AUTH_ENABLED";
const ENV_REMOTE_AUTH_TOKENS: &str = "ALAN_REMOTE_AUTH_TOKENS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    Direct,
    Relay,
}

impl TransportMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "direct" => Some(Self::Direct),
            "relay" => Some(Self::Relay),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Relay => "relay",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoteRequestContext {
    pub node_id: Option<String>,
    pub client_id: Option<String>,
    pub trace_id: Option<String>,
    pub transport_mode: Option<TransportMode>,
    pub required_scope: Option<SessionScope>,
    pub granted_scopes: Option<HashSet<SessionScope>>,
    pub auth_enabled: bool,
    pub authenticated: bool,
}

impl RemoteRequestContext {
    pub fn allows_scope(&self, required_scope: SessionScope) -> bool {
        if !self.auth_enabled {
            return true;
        }
        self.granted_scopes
            .as_ref()
            .is_some_and(|scopes| scopes.contains(&required_scope))
    }
}

struct ParsedRemoteHeaders {
    node_id: Option<String>,
    client_id: Option<String>,
    trace_id: Option<String>,
    transport_mode: Option<TransportMode>,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteAccessControl {
    enabled: bool,
    token_scopes: HashMap<String, HashSet<SessionScope>>,
}

impl RemoteAccessControl {
    pub fn from_env() -> anyhow::Result<Self> {
        let enabled = env_var_truthy(ENV_REMOTE_AUTH_ENABLED);
        let tokens_raw = env::var(ENV_REMOTE_AUTH_TOKENS).unwrap_or_default();
        Self::from_env_values(enabled, &tokens_raw)
    }

    fn from_env_values(enabled: bool, tokens_raw: &str) -> anyhow::Result<Self> {
        if !enabled {
            return Ok(Self {
                enabled: false,
                token_scopes: HashMap::new(),
            });
        }

        let token_scopes = parse_remote_auth_tokens(tokens_raw)?;
        if token_scopes.is_empty() {
            anyhow::bail!(
                "{} is enabled but {} is empty",
                ENV_REMOTE_AUTH_ENABLED,
                ENV_REMOTE_AUTH_TOKENS
            );
        }

        Ok(Self {
            enabled: true,
            token_scopes,
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn authorize_request(
        &self,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
    ) -> Result<RemoteRequestContext, AuthError> {
        let required_scope = required_scope_for_request(method, path);
        let parsed_headers = parse_remote_headers(headers)?;
        let mut context = RemoteRequestContext {
            node_id: parsed_headers.node_id,
            client_id: parsed_headers.client_id,
            trace_id: parsed_headers.trace_id,
            transport_mode: parsed_headers.transport_mode,
            required_scope,
            granted_scopes: None,
            auth_enabled: self.enabled,
            authenticated: false,
        };

        // Keep compatibility by default; strict scope auth is opt-in via env.
        if !self.enabled {
            return Ok(context);
        }

        let required_scope = match required_scope {
            Some(scope) => scope,
            None => return Ok(context),
        };
        let token = extract_bearer_token(headers)
            .ok_or(AuthError::unauthorized("missing or invalid bearer token"))?;
        let granted_scopes = self
            .token_scopes
            .get(token)
            .ok_or(AuthError::unauthorized("unknown bearer token"))?;
        if !request_scope_satisfied(method, path, required_scope, granted_scopes) {
            return Err(AuthError::forbidden_for_request(
                method,
                path,
                required_scope,
            ));
        }

        context.authenticated = true;
        context.granted_scopes = Some(granted_scopes.clone());
        Ok(context)
    }
}

#[derive(Debug)]
pub struct AuthError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl AuthError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "remote_auth_unauthorized",
            message: message.into(),
        }
    }

    fn forbidden(required: SessionScope) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "remote_auth_forbidden",
            message: format!("missing required scope: {}", required.as_str()),
        }
    }

    fn forbidden_any(required_scopes: &[SessionScope]) -> Self {
        let joined = required_scopes
            .iter()
            .map(SessionScope::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            status: StatusCode::FORBIDDEN,
            code: "remote_auth_forbidden",
            message: format!("missing required scope: one of {joined}"),
        }
    }

    fn forbidden_for_request(method: &Method, path: &str, required: SessionScope) -> Self {
        if is_submit_or_ws_route(method, path) && required == SessionScope::Write {
            return Self::forbidden_any(&[
                SessionScope::Write,
                SessionScope::Resume,
                SessionScope::Admin,
            ]);
        }
        Self::forbidden(required)
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "remote_auth_bad_request",
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct AuthErrorResponse<'a> {
    error: &'a str,
    code: &'a str,
}

pub async fn remote_access_middleware(
    control: Arc<RemoteAccessControl>,
    mut request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    match control.authorize_request(&method, &path, request.headers()) {
        Ok(context) => {
            debug!(
                method = %method,
                path = %path,
                auth_enabled = context.auth_enabled,
                authenticated = context.authenticated,
                required_scope = context.required_scope.map(|scope| scope.as_str()),
                node_id = context.node_id.as_deref(),
                client_id = context.client_id.as_deref(),
                trace_id = context.trace_id.as_deref(),
                transport_mode = context.transport_mode.map(|m| m.as_str()),
                "remote access check passed"
            );
            request.extensions_mut().insert(context);
            next.run(request).await
        }
        Err(err) => {
            warn!(
                method = %method,
                path = %path,
                status = %err.status,
                code = err.code,
                message = %err.message,
                "remote access check failed"
            );
            (
                err.status,
                Json(AuthErrorResponse {
                    error: &err.message,
                    code: err.code,
                }),
            )
                .into_response()
        }
    }
}

fn env_var_truthy(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn parse_remote_auth_tokens(raw: &str) -> anyhow::Result<HashMap<String, HashSet<SessionScope>>> {
    let mut bindings = HashMap::new();
    if raw.trim().is_empty() {
        return Ok(bindings);
    }

    for binding in raw.split(';') {
        let binding = binding.trim();
        if binding.is_empty() {
            continue;
        }
        let (token, scopes_raw) = binding.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("Invalid token binding `{binding}`; expected token=scopes")
        })?;
        let token = token.trim();
        let scopes_raw = scopes_raw.trim();
        if token.is_empty() || scopes_raw.is_empty() {
            anyhow::bail!("Invalid token binding `{binding}`; token and scopes must be non-empty");
        }

        let mut scopes = HashSet::new();
        for scope in scopes_raw.split(',') {
            let scope = scope.trim();
            if scope.is_empty() {
                continue;
            }
            if scope == "*" {
                scopes.insert(SessionScope::Read);
                scopes.insert(SessionScope::Write);
                scopes.insert(SessionScope::Resume);
                scopes.insert(SessionScope::Admin);
                scopes.insert(SessionScope::HostAuthRead);
                scopes.insert(SessionScope::HostAuthWrite);
                continue;
            }
            let Some(parsed) = SessionScope::parse(scope) else {
                anyhow::bail!("Unknown scope `{scope}` in binding `{binding}`");
            };
            scopes.insert(parsed);
        }
        if scopes.is_empty() {
            anyhow::bail!("Invalid token binding `{binding}`; no scopes parsed");
        }

        bindings.insert(token.to_string(), scopes);
    }

    Ok(bindings)
}

fn parse_remote_headers(headers: &HeaderMap) -> Result<ParsedRemoteHeaders, AuthError> {
    let node_id = parse_optional_header(headers, HEADER_NODE_ID)?;
    let client_id = parse_optional_header(headers, HEADER_CLIENT_ID)?;
    let trace_id = parse_optional_header(headers, HEADER_TRACE_ID)?;
    let transport_mode = parse_optional_header(headers, HEADER_TRANSPORT_MODE)?
        .map(|value| {
            TransportMode::parse(&value).ok_or_else(|| {
                AuthError::bad_request(format!(
                    "invalid {} header; expected `direct` or `relay`",
                    HEADER_TRANSPORT_MODE
                ))
            })
        })
        .transpose()?;
    Ok(ParsedRemoteHeaders {
        node_id,
        client_id,
        trace_id,
        transport_mode,
    })
}

fn parse_optional_header(headers: &HeaderMap, name: &str) -> Result<Option<String>, AuthError> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| AuthError::bad_request(format!("invalid `{name}` header encoding")))?;
    let value = value.trim();
    if value.is_empty() {
        return Err(AuthError::bad_request(format!("`{name}` cannot be empty")));
    }
    Ok(Some(value.to_string()))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let split_at = value.find(char::is_whitespace)?;
    let (scheme, token_part) = value.split_at(split_at);
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token_part.trim();
    if token.is_empty() {
        return None;
    }
    Some(token)
}

fn is_submit_or_ws_route(method: &Method, path: &str) -> bool {
    endpoint_for_request(method, path).is_some_and(|endpoint| {
        matches!(
            endpoint.id,
            EndpointId::SessionSubmit | EndpointId::SessionWebSocket
        )
    })
}

fn allows_resume_capable_scope(scopes: Option<&HashSet<SessionScope>>) -> bool {
    scopes.is_some_and(|scopes| {
        scopes.contains(&SessionScope::Write)
            || scopes.contains(&SessionScope::Resume)
            || scopes.contains(&SessionScope::Admin)
    })
}

fn request_scope_satisfied(
    method: &Method,
    path: &str,
    required_scope: SessionScope,
    granted_scopes: &HashSet<SessionScope>,
) -> bool {
    if required_scope == SessionScope::Write && is_submit_or_ws_route(method, path) {
        return allows_resume_capable_scope(Some(granted_scopes));
    }
    granted_scopes.contains(&required_scope)
}

fn required_scope_for_request(method: &Method, path: &str) -> Option<SessionScope> {
    endpoint_for_request(method, path).and_then(|endpoint| endpoint.remote_scope)
}

fn endpoint_for_request(
    method: &Method,
    path: &str,
) -> Option<&'static api_contract::EndpointDescriptor> {
    api_contract::match_forwarded_endpoint(method, path)
        .or_else(|| api_contract::match_endpoint(method, path))
}

pub fn required_scope_for_op(op: &alan_agent_protocol::Op) -> SessionScope {
    use alan_agent_protocol::Op;
    match op {
        Op::Resume { .. } => SessionScope::Resume,
        Op::CompactWithOptions { .. } | Op::Rollback { .. } => SessionScope::Admin,
        Op::Turn { .. }
        | Op::Input { .. }
        | Op::Interrupt
        | Op::RegisterDynamicTools { .. }
        | Op::SetClientCapabilities { .. } => SessionScope::Write,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderValue, Method};

    #[test]
    fn parse_remote_auth_tokens_parses_multi_token_scopes() {
        let parsed = parse_remote_auth_tokens(
            "reader=session.read,host.auth.read;writer=session.read,session.write,host.auth.write;admin=*",
        )
        .unwrap();

        assert_eq!(parsed.len(), 3);
        assert!(parsed["reader"].contains(&SessionScope::Read));
        assert!(parsed["reader"].contains(&SessionScope::HostAuthRead));
        assert!(!parsed["reader"].contains(&SessionScope::Write));
        assert!(parsed["writer"].contains(&SessionScope::Read));
        assert!(parsed["writer"].contains(&SessionScope::Write));
        assert!(parsed["writer"].contains(&SessionScope::HostAuthWrite));
        assert!(parsed["admin"].contains(&SessionScope::Read));
        assert!(parsed["admin"].contains(&SessionScope::Write));
        assert!(parsed["admin"].contains(&SessionScope::Resume));
        assert!(parsed["admin"].contains(&SessionScope::Admin));
        assert!(parsed["admin"].contains(&SessionScope::HostAuthRead));
        assert!(parsed["admin"].contains(&SessionScope::HostAuthWrite));
    }

    #[test]
    fn from_env_values_ignores_token_parsing_when_auth_disabled() {
        let control =
            RemoteAccessControl::from_env_values(false, "broken_binding_without_equals").unwrap();
        assert!(!control.enabled());
        assert!(control.token_scopes.is_empty());
    }

    #[test]
    fn from_env_values_requires_valid_tokens_when_auth_enabled() {
        assert!(RemoteAccessControl::from_env_values(true, "").is_err());
        assert!(RemoteAccessControl::from_env_values(true, "broken").is_err());

        let control = RemoteAccessControl::from_env_values(true, "writer=session.write").unwrap();
        assert!(control.enabled());
        assert!(control.token_scopes["writer"].contains(&SessionScope::Write));
    }

    #[test]
    fn required_scope_for_request_maps_session_routes() {
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/sessions"),
            Some(SessionScope::Read)
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/sessions"),
            Some(SessionScope::Write)
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/sessions/s1/resume"),
            Some(SessionScope::Resume)
        );
        assert_eq!(
            required_scope_for_request(
                &Method::POST,
                "/api/v1/sessions/s1/child_runs/c1/terminate"
            ),
            Some(SessionScope::Write)
        );
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/skills/catalog"),
            Some(SessionScope::Read)
        );
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/skills/changed"),
            Some(SessionScope::Read)
        );
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/connections/chatgpt-main"),
            Some(SessionScope::HostAuthRead)
        );
        assert_eq!(
            required_scope_for_request(&Method::HEAD, "/api/v1/connections/chatgpt-main"),
            Some(SessionScope::HostAuthRead)
        );
        assert_eq!(
            required_scope_for_request(
                &Method::POST,
                "/api/v1/connections/chatgpt-main/credential/login/device/start"
            ),
            Some(SessionScope::HostAuthWrite)
        );
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/connections/newer/route"),
            None
        );
        assert_eq!(
            required_scope_for_request(&Method::HEAD, "/api/v1/connections/newer/route"),
            None
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/connections/newer/route"),
            None
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/skills/overrides"),
            Some(SessionScope::Admin)
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/sessions/s1/rollback"),
            Some(SessionScope::Admin)
        );
        assert_eq!(
            required_scope_for_request(&Method::DELETE, "/api/v1/sessions/s1"),
            Some(SessionScope::Admin)
        );
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/sessions/s1/ws"),
            Some(SessionScope::Write)
        );
        assert_eq!(
            required_scope_for_request(&Method::HEAD, "/api/v1/sessions/s1/ws"),
            Some(SessionScope::Write)
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/relay/tunnel"),
            None
        );
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/relay/tunnel"),
            None
        );
        assert_eq!(
            required_scope_for_request(&Method::GET, "/api/v1/relay/nodes"),
            Some(SessionScope::Read)
        );
        assert_eq!(
            required_scope_for_request(
                &Method::POST,
                "/api/v1/relay/nodes/node-a/api/v1/sessions/s1/submit"
            ),
            Some(SessionScope::Write)
        );
        assert_eq!(
            required_scope_for_request(
                &Method::POST,
                "/api/v1/relay/nodes/node-a/api/v1/sessions/s1/resume"
            ),
            Some(SessionScope::Resume)
        );
        assert_eq!(
            required_scope_for_request(
                &Method::POST,
                "/api/v1/relay/nodes/node-a/api/v1/sessions/s1/child_runs/c1/terminate"
            ),
            Some(SessionScope::Write)
        );
        assert_eq!(
            required_scope_for_request(
                &Method::POST,
                "/api/v1/relay/nodes/node-a//api/v1/sessions/s1/submit"
            ),
            Some(SessionScope::Write)
        );
        assert_eq!(
            required_scope_for_request(
                &Method::GET,
                "/api/v1/relay/nodes/node-a/api/v1/sessions/s1/submit"
            ),
            None
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/relay/nodes/node-a/api/v1/unknown"),
            None
        );
        assert_eq!(
            required_scope_for_request(&Method::POST, "/api/v1/unknown"),
            None
        );
        assert_eq!(
            required_scope_for_request(
                &Method::POST,
                "/api/v1/relay/nodes/node-a/api/v1/connections/newer/route"
            ),
            None
        );
        assert_eq!(required_scope_for_request(&Method::GET, "/health"), None);
    }

    #[test]
    fn required_scope_for_op_maps_privileged_ops() {
        use alan_agent_protocol::Op;

        assert_eq!(required_scope_for_op(&Op::Interrupt), SessionScope::Write);
        assert_eq!(
            required_scope_for_op(&Op::Resume {
                request_id: "req-1".to_string(),
                content: vec![],
            }),
            SessionScope::Resume
        );
        assert_eq!(
            required_scope_for_op(&Op::CompactWithOptions { focus: None }),
            SessionScope::Admin
        );
        assert_eq!(
            required_scope_for_op(&Op::CompactWithOptions {
                focus: Some("preserve todos".to_string()),
            }),
            SessionScope::Admin
        );
        assert_eq!(
            required_scope_for_op(&Op::Rollback { turns: 1 }),
            SessionScope::Admin
        );
        assert_eq!(
            required_scope_for_op(&Op::SetClientCapabilities {
                capabilities: alan_agent_protocol::ClientCapabilities::default(),
            }),
            SessionScope::Write
        );
    }

    #[test]
    fn authorize_request_allows_when_auth_disabled() {
        let control = RemoteAccessControl::default();
        let headers = HeaderMap::new();
        let context = control
            .authorize_request(&Method::POST, "/api/v1/sessions/s1/submit", &headers)
            .unwrap();
        assert!(!context.auth_enabled);
        assert!(!context.authenticated);
        assert_eq!(context.required_scope, Some(SessionScope::Write));
    }

    #[test]
    fn authorize_request_requires_valid_bearer_when_enabled() {
        let mut token_scopes = HashMap::new();
        token_scopes.insert("token-r".to_string(), HashSet::from([SessionScope::Read]));
        let control = RemoteAccessControl {
            enabled: true,
            token_scopes,
        };

        let headers = HeaderMap::new();
        let err = control
            .authorize_request(&Method::GET, "/api/v1/sessions", &headers)
            .unwrap_err();
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-r"),
        );
        let context = control
            .authorize_request(&Method::GET, "/api/v1/sessions", &headers)
            .unwrap();
        assert!(context.auth_enabled);
        assert!(context.authenticated);
        assert_eq!(context.required_scope, Some(SessionScope::Read));

        let err = control
            .authorize_request(&Method::POST, "/api/v1/sessions/s1/submit", &headers)
            .unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn authorize_request_enforces_get_scopes_for_head_requests() {
        let mut token_scopes = HashMap::new();
        token_scopes.insert("token-r".to_string(), HashSet::from([SessionScope::Read]));
        token_scopes.insert(
            "token-host-r".to_string(),
            HashSet::from([SessionScope::HostAuthRead]),
        );
        let control = RemoteAccessControl {
            enabled: true,
            token_scopes,
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-r"),
        );
        let context = control
            .authorize_request(&Method::HEAD, "/api/v1/sessions", &headers)
            .unwrap();
        assert_eq!(context.required_scope, Some(SessionScope::Read));
        assert!(context.authenticated);

        let err = control
            .authorize_request(&Method::HEAD, "/api/v1/connections/chatgpt-main", &headers)
            .unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-host-r"),
        );
        let context = control
            .authorize_request(&Method::HEAD, "/api/v1/connections/chatgpt-main", &headers)
            .unwrap();
        assert_eq!(context.required_scope, Some(SessionScope::HostAuthRead));
        assert!(context.authenticated);
    }

    #[test]
    fn authorize_request_allows_resume_scope_for_submit_and_ws_routes() {
        let mut token_scopes = HashMap::new();
        token_scopes.insert(
            "token-resume".to_string(),
            HashSet::from([SessionScope::Resume]),
        );
        let control = RemoteAccessControl {
            enabled: true,
            token_scopes,
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-resume"),
        );

        let submit_context = control
            .authorize_request(&Method::POST, "/api/v1/sessions/s1/submit", &headers)
            .unwrap();
        assert_eq!(submit_context.required_scope, Some(SessionScope::Write));
        assert!(submit_context.authenticated);

        let ws_context = control
            .authorize_request(&Method::GET, "/api/v1/sessions/s1/ws", &headers)
            .unwrap();
        assert_eq!(ws_context.required_scope, Some(SessionScope::Write));
        assert!(ws_context.authenticated);
    }

    #[test]
    fn authorize_request_allows_write_scope_for_child_run_terminate() {
        let mut token_scopes = HashMap::new();
        token_scopes.insert(
            "token-read".to_string(),
            HashSet::from([SessionScope::Read]),
        );
        token_scopes.insert(
            "token-write".to_string(),
            HashSet::from([SessionScope::Write]),
        );
        let control = RemoteAccessControl {
            enabled: true,
            token_scopes,
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-write"),
        );
        let context = control
            .authorize_request(
                &Method::POST,
                "/api/v1/sessions/s1/child_runs/c1/terminate",
                &headers,
            )
            .unwrap();
        assert_eq!(context.required_scope, Some(SessionScope::Write));
        assert!(context.authenticated);

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-read"),
        );
        let err = control
            .authorize_request(
                &Method::POST,
                "/api/v1/sessions/s1/child_runs/c1/terminate",
                &headers,
            )
            .unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn authorize_request_rejects_invalid_transport_mode_header() {
        let control = RemoteAccessControl::default();
        let mut headers = HeaderMap::new();
        headers.insert(
            HEADER_TRANSPORT_MODE,
            HeaderValue::from_static("carrier-pigeon"),
        );
        let err = control
            .authorize_request(&Method::GET, "/api/v1/sessions", &headers)
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn authorize_request_requires_scope_on_relay_proxy_paths() {
        let mut token_scopes = HashMap::new();
        token_scopes.insert(
            "token-read".to_string(),
            HashSet::from([SessionScope::Read]),
        );
        let control = RemoteAccessControl {
            enabled: true,
            token_scopes,
        };

        let headers = HeaderMap::new();
        let err = control
            .authorize_request(
                &Method::POST,
                "/api/v1/relay/nodes/node-a/api/v1/sessions/s1/submit",
                &headers,
            )
            .unwrap_err();
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-read"),
        );
        let err = control
            .authorize_request(
                &Method::POST,
                "/api/v1/relay/nodes/node-a/api/v1/sessions/s1/submit",
                &headers,
            )
            .unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn extract_bearer_token_accepts_case_insensitive_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("bearer token-lower"),
        );
        assert_eq!(extract_bearer_token(&headers), Some("token-lower"));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("BeArEr token-mixed"),
        );
        assert_eq!(extract_bearer_token(&headers), Some("token-mixed"));
    }
}
