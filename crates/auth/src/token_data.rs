use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatgptIdTokenInfo {
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub user_id: Option<String>,
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    pub raw_jwt: String,
}

impl ChatgptIdTokenInfo {
    /// Account-level identifier when present, falling back to the OIDC subject `sub`.
    pub fn effective_account_id(&self) -> Option<&str> {
        self.account_id
            .as_deref()
            .or(self.subject.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatgptTokenData {
    #[serde(
        deserialize_with = "deserialize_id_token",
        serialize_with = "serialize_id_token"
    )]
    pub id_token: ChatgptIdTokenInfo,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Deserialize)]
struct IdClaims {
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "https://api.openai.com/profile", default)]
    profile: Option<ProfileClaims>,
    #[serde(rename = "https://api.openai.com/auth", default)]
    auth: Option<AuthClaims>,
}

#[derive(Deserialize)]
struct ProfileClaims {
    #[serde(default)]
    email: Option<String>,
}

#[derive(Deserialize)]
struct AuthClaims {
    #[serde(default)]
    chatgpt_plan_type: Option<String>,
    #[serde(default)]
    chatgpt_user_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    chatgpt_account_id: Option<String>,
}

#[derive(Deserialize)]
struct StandardJwtClaims {
    #[serde(default)]
    exp: Option<i64>,
}

#[derive(Debug, Error)]
pub enum TokenDataError {
    #[error("invalid JWT format")]
    InvalidFormat,
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn decode_jwt_payload<T: DeserializeOwned>(jwt: &str) -> Result<T, TokenDataError> {
    let mut parts = jwt.split('.');
    let (_header_b64, payload_b64, _sig_b64) = match (parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s)) if !h.is_empty() && !p.is_empty() && !s.is_empty() => (h, p, s),
        _ => return Err(TokenDataError::InvalidFormat),
    };

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64)?;
    Ok(serde_json::from_slice(&payload_bytes)?)
}

pub fn parse_jwt_expiration(jwt: &str) -> Result<Option<DateTime<Utc>>, TokenDataError> {
    let claims: StandardJwtClaims = decode_jwt_payload(jwt)?;
    Ok(claims
        .exp
        .and_then(|exp| DateTime::<Utc>::from_timestamp(exp, 0)))
}

pub fn parse_chatgpt_jwt_claims(jwt: &str) -> Result<ChatgptIdTokenInfo, TokenDataError> {
    let claims: IdClaims = decode_jwt_payload(jwt)?;
    let email = claims
        .email
        .or_else(|| claims.profile.and_then(|profile| profile.email));
    let subject = claims.sub.filter(|value| !value.trim().is_empty());
    let nonce = claims.nonce.filter(|value| !value.trim().is_empty());

    if let Some(auth) = claims.auth {
        let user_id = auth.chatgpt_user_id.or(auth.user_id).or(subject.clone());
        let account_id = auth
            .chatgpt_account_id
            .filter(|value| !value.trim().is_empty());
        Ok(ChatgptIdTokenInfo {
            email,
            plan_type: auth.chatgpt_plan_type,
            user_id,
            account_id,
            subject,
            nonce,
            raw_jwt: jwt.to_string(),
        })
    } else {
        Ok(ChatgptIdTokenInfo {
            email,
            plan_type: None,
            user_id: subject.clone(),
            account_id: None,
            subject,
            nonce,
            raw_jwt: jwt.to_string(),
        })
    }
}

fn deserialize_id_token<'de, D>(deserializer: D) -> Result<ChatgptIdTokenInfo, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    parse_chatgpt_jwt_claims(&raw).map_err(serde::de::Error::custom)
}

fn serialize_id_token<S>(token: &ChatgptIdTokenInfo, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&token.raw_jwt)
}

#[cfg(test)]
mod tests {
    use super::{parse_chatgpt_jwt_claims, parse_jwt_expiration};
    use base64::Engine;
    use serde_json::json;

    fn build_jwt(payload: serde_json::Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{payload}.sig")
    }

    #[test]
    fn parses_chatgpt_claims() {
        let jwt = build_jwt(json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "pro",
                "chatgpt_user_id": "user_123",
                "chatgpt_account_id": "acct_123"
            }
        }));

        let info = parse_chatgpt_jwt_claims(&jwt).expect("claims");
        assert_eq!(info.email.as_deref(), Some("user@example.com"));
        assert_eq!(info.plan_type.as_deref(), Some("pro"));
        assert_eq!(info.user_id.as_deref(), Some("user_123"));
        assert_eq!(info.account_id.as_deref(), Some("acct_123"));
        assert_eq!(info.raw_jwt, jwt);
    }

    #[test]
    fn parses_expiration_claim() {
        let jwt = build_jwt(json!({"exp": 1_800_000_000_i64}));
        let expiration = parse_jwt_expiration(&jwt).expect("expiration");
        assert!(expiration.is_some());
    }

    #[test]
    fn parses_standard_oidc_subject_and_nonce_claims() {
        let jwt = build_jwt(json!({
            "sub": "user-sub-123",
            "nonce": "nonce-abc",
            "email": "oidc@example.com"
        }));
        let info = parse_chatgpt_jwt_claims(&jwt).expect("claims");
        assert_eq!(info.account_id, None);
        assert_eq!(info.user_id.as_deref(), Some("user-sub-123"));
        assert_eq!(info.subject.as_deref(), Some("user-sub-123"));
        assert_eq!(info.effective_account_id(), Some("user-sub-123"));
        assert_eq!(info.nonce.as_deref(), Some("nonce-abc"));
        assert_eq!(info.email.as_deref(), Some("oidc@example.com"));
    }
}
