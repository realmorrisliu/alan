use alan_agent_protocol::AuthStatusSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Default)]
pub struct StartDeviceLoginRequest {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StartDeviceLoginResponse {
    pub login_id: String,
    pub verification_url: String,
    pub user_code: String,
    pub interval_secs: u64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteDeviceLoginRequest {
    pub login_id: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct StartBrowserLoginRequest {
    pub workspace_id: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StartBrowserLoginResponse {
    pub login_id: String,
    pub auth_url: String,
    pub redirect_uri: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionLoginSuccessResponse {
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    pub snapshot: AuthStatusSnapshot,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionLogoutResponse {
    pub removed: bool,
    pub snapshot: AuthStatusSnapshot,
}
