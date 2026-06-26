use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthProviderId {
    Chatgpt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthLoginMethod {
    Browser,
    DeviceCode,
    ExternalTokenHandoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatusKind {
    LoggedOut,
    LoggedIn,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthErrorCode {
    UnknownPendingLogin,
    ExpiredPendingLogin,
    ExternalTokenHandoffDisabled,
    NotLoggedIn,
    MissingAccountIdentity,
    TokenExpired,
    RefreshFailed,
    WorkspaceMismatch,
    UnauthorizedAfterRefresh,
    LoginFailed,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthErrorResponse {
    pub code: AuthErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthPendingLoginSummary {
    pub login_id: String,
    pub method: AuthLoginMethod,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthStatusSnapshot {
    pub provider: AuthProviderId,
    pub kind: AuthStatusKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_login: Option<AuthPendingLoginSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthEvent {
    StatusSnapshot {
        snapshot: AuthStatusSnapshot,
    },
    LoginStarted {
        login_id: String,
        method: AuthLoginMethod,
    },
    BrowserLoginReady {
        login_id: String,
        auth_url: String,
        redirect_uri: String,
    },
    DeviceCodeReady {
        login_id: String,
        verification_url: String,
        user_code: String,
        interval_secs: u64,
    },
    LoginSucceeded {
        login_id: String,
        account_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_type: Option<String>,
    },
    LoginFailed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        login_id: Option<String>,
        message: String,
        recoverable: bool,
    },
    LogoutCompleted {
        removed: bool,
    },
    TokenImported {
        account_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_type: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthEventEnvelope {
    pub event_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub provider: AuthProviderId,
    #[serde(flatten)]
    pub event: AuthEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_snapshot_round_trip() {
        let snapshot = AuthStatusSnapshot {
            provider: AuthProviderId::Chatgpt,
            kind: AuthStatusKind::LoggedIn,
            storage_path: Some("/tmp/auth.json".to_string()),
            account_id: Some("acct_123".to_string()),
            email: Some("user@example.com".to_string()),
            plan_type: Some("pro".to_string()),
            user_id: Some("user_123".to_string()),
            access_token_expires_at: None,
            last_refresh_at: None,
            pending_login: None,
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: AuthStatusSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, snapshot);
    }

    #[test]
    fn auth_event_envelope_serialization_includes_type() {
        let envelope = AuthEventEnvelope {
            event_id: "auth_evt_1".to_string(),
            sequence: 1,
            timestamp_ms: 1,
            provider: AuthProviderId::Chatgpt,
            event: AuthEvent::LogoutCompleted { removed: true },
        };

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"type\":\"logout_completed\""));
    }

    #[test]
    fn auth_error_response_round_trip() {
        let response = AuthErrorResponse {
            code: AuthErrorCode::NotLoggedIn,
            message: "not logged in".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: AuthErrorResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, response);
    }
}
