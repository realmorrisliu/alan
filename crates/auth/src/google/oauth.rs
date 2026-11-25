//! Shared Google OAuth2 authorization code exchange and single-flight refresh helpers.

use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use tokio::sync::Mutex;
use url::Url;

use crate::pkce::PkceCodes;
use crate::storage::{AuthStorage, StoredGoogleAuth};

use super::GoogleAuthError;

#[derive(Debug, Deserialize)]
pub(super) struct GoogleTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

pub(super) fn build_authorize_url_with_scope(
    auth_url: &str,
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    pkce: &PkceCodes,
    state: &str,
) -> Result<String, GoogleAuthError> {
    let mut url = Url::parse(auth_url)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", scope)
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    Ok(url.to_string())
}

#[allow(
    clippy::too_many_arguments,
    reason = "internal OAuth helper forwarding explicit endpoint, PKCE, and client credentials"
)]
pub(super) async fn exchange_authorization_code(
    client: &reqwest::Client,
    storage: &AuthStorage,
    token_url: &str,
    callback_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    code_verifier: &str,
    code: &str,
) -> Result<(StoredGoogleAuth, Option<String>), GoogleAuthError> {
    let mut params = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", callback_url.to_string()),
        ("client_id", client_id.to_string()),
        ("code_verifier", code_verifier.to_string()),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret.to_string()));
    }

    let encoded_form = encode_form_params(&params);
    let response = client
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(encoded_form)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(GoogleAuthError::TokenExchangeFailed(format!(
            "HTTP {status}: {body}"
        )));
    }

    let parsed: GoogleTokenResponse = serde_json::from_str(&body).map_err(|error| {
        GoogleAuthError::TokenExchangeFailed(format!("invalid response JSON: {error}"))
    })?;
    let refresh_token = parsed
        .refresh_token
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            GoogleAuthError::TokenExchangeFailed(
                "Google token response omitted `refresh_token`".to_string(),
            )
        })?;
    let expires_at = parsed
        .expires_in
        .filter(|secs| *secs > 0)
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs));
    let email = parsed.id_token.as_deref().and_then(parse_jwt_email);

    let stored = StoredGoogleAuth {
        refresh_token,
        access_token: parsed.access_token,
        expires_at,
        client_id: Some(client_id.to_string()),
        client_secret: client_secret.map(ToOwned::to_owned),
        token_uri: Some(token_url.to_string()),
        email,
        last_refresh_at: Some(Utc::now()),
    };

    let saved = stored.clone();
    storage.update(move |store| {
        store.google = Some(saved);
    })?;

    Ok((stored, parsed.scope))
}

#[allow(
    clippy::too_many_arguments,
    reason = "internal single-flight refresh helper shared by Vertex and Drive OAuth managers"
)]
pub(super) async fn refresh_stored_google_auth(
    storage: &AuthStorage,
    client: &reqwest::Client,
    refresh_lock: &Mutex<()>,
    default_token_url: &str,
    default_client_id: Option<&str>,
    default_client_secret: Option<&str>,
    force: bool,
    missing_error: GoogleAuthError,
) -> Result<StoredGoogleAuth, GoogleAuthError> {
    let _memory_guard = refresh_lock.lock().await;
    let _file_guard = storage.lock("auth.refresh.lock")?;

    let Some(existing) = storage.load()?.google else {
        return Err(missing_error);
    };
    if !force && !existing.should_refresh(Utc::now()) && !existing.access_token.trim().is_empty() {
        return Ok(existing);
    }

    let token_uri = existing
        .token_uri
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(default_token_url);
    let client_id = existing
        .client_id
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .or(default_client_id)
        .ok_or(GoogleAuthError::OAuthClientNotConfigured)?;
    let client_secret = existing
        .client_secret
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .or(default_client_secret);

    let token_resp = exchange_refresh_token(
        client,
        token_uri,
        client_id,
        client_secret,
        &existing.refresh_token,
    )
    .await
    .map_err(GoogleAuthError::RefreshFailed)?;

    let expires_at = token_resp
        .expires_in
        .filter(|secs| *secs > 0)
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs));
    let updated = StoredGoogleAuth {
        refresh_token: token_resp
            .refresh_token
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(existing.refresh_token),
        access_token: token_resp.access_token,
        expires_at,
        client_id: Some(client_id.to_string()),
        client_secret: client_secret.map(ToOwned::to_owned),
        token_uri: Some(token_uri.to_string()),
        email: token_resp
            .id_token
            .as_deref()
            .and_then(parse_jwt_email)
            .or(existing.email),
        last_refresh_at: Some(Utc::now()),
    };

    let saved_entry = updated.clone();
    storage.update(move |store| {
        store.google = Some(saved_entry);
    })?;

    Ok(updated)
}

pub(super) async fn exchange_refresh_token(
    client: &reqwest::Client,
    token_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<GoogleTokenResponse, String> {
    let mut params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("client_id", client_id.to_string()),
        ("refresh_token", refresh_token.to_string()),
    ];
    if let Some(secret) = client_secret.map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("client_secret", secret.to_string()));
    }

    let encoded_form = encode_form_params(&params);
    let response = client
        .post(token_uri)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(encoded_form)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }
    serde_json::from_str(&body).map_err(|error| format!("invalid token JSON: {error}"))
}

fn encode_form_params(params: &[(&str, String)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())))
        .finish()
}

fn parse_jwt_email(jwt: &str) -> Option<String> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .get("email")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}
