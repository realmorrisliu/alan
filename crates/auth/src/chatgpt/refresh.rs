//! Managed access-token refresh.
//!
//! Refresh is single-flight twice over: an in-process mutex serializes callers of one
//! manager, and a Host-store file lock serializes independent managers and Processes
//! sharing the same backing. Every waiter re-reads the store after acquiring both
//! locks, so a rotation performed by the first refresher satisfies all later ones.

use super::{
    ChatgptAuthError, ChatgptAuthManager, ChatgptRequestAuth, ensure_expected_account_matches,
    parse_chatgpt_id_token,
};
use crate::storage::{AuthStore, StoredChatgptAuth};
use crate::token_data::{ChatgptTokenData, parse_chatgpt_jwt_claims};
use chrono::Utc;
use reqwest::StatusCode;
use serde::Deserialize;
use std::io;
use std::time::Duration;

const REFRESH_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a caller wants credentials, which decides whether a refresh is required.
#[derive(Debug, Clone, Copy)]
pub(super) enum RefreshMode<'a> {
    /// Refresh only when the stored access token is expired or inside the safety window.
    IfNeeded,
    /// Refresh unless another refresher already rotated the observed credentials.
    Force,
    /// The backend rejected this access token; refresh only if it is still the stored one.
    Rejected(&'a str),
}

impl RefreshMode<'_> {
    fn requires_refresh(self, auth: &StoredChatgptAuth) -> bool {
        match self {
            Self::IfNeeded => auth.should_refresh(Utc::now()),
            Self::Force => true,
            Self::Rejected(token) => {
                auth.tokens.access_token == token || auth.should_refresh(Utc::now())
            }
        }
    }
}

impl ChatgptAuthManager {
    pub(super) async fn request_auth_with_mode(
        &self,
        expected_account_id: Option<&str>,
        mode: RefreshMode<'_>,
    ) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        let auth = self.refresh_if_needed(expected_account_id, mode).await?;
        ensure_expected_account_matches(expected_account_id, &auth)?;
        Ok(ChatgptRequestAuth {
            access_token: auth.tokens.access_token,
            account_id: auth.account_id,
        })
    }

    async fn refresh_if_needed(
        &self,
        expected_account_id: Option<&str>,
        mode: RefreshMode<'_>,
    ) -> Result<StoredChatgptAuth, ChatgptAuthError> {
        let observed = self
            .inner
            .storage
            .load()?
            .chatgpt
            .ok_or(ChatgptAuthError::NotLoggedIn)?;
        ensure_expected_account_matches(expected_account_id, &observed)?;
        if !mode.requires_refresh(&observed) {
            return Ok(observed);
        }

        let _guard = self.inner.refresh_lock.lock().await;
        let storage = self.inner.storage.clone();
        let _store_guard = tokio::task::spawn_blocking(move || storage.lock("auth.refresh.lock"))
            .await
            .map_err(io::Error::other)??;
        let store = self.inner.storage.load()?;
        let auth = store
            .chatgpt
            .as_ref()
            .ok_or(ChatgptAuthError::NotLoggedIn)?;
        ensure_expected_account_matches(expected_account_id, auth)?;
        // Another refresher (this Process or another one) rotated the credentials
        // while this caller waited; reuse its result instead of refreshing again.
        if *auth != observed || !mode.requires_refresh(auth) {
            return Ok(auth.clone());
        }

        if auth.tokens.refresh_token.trim().is_empty() {
            return Err(ChatgptAuthError::TokenExpired);
        }

        self.refresh_inner(store, expected_account_id).await
    }

    async fn refresh_inner(
        &self,
        expected: AuthStore,
        expected_account_id: Option<&str>,
    ) -> Result<StoredChatgptAuth, ChatgptAuthError> {
        #[derive(Deserialize)]
        struct RefreshResponse {
            id_token: Option<String>,
            access_token: String,
            refresh_token: Option<String>,
            #[serde(default)]
            expires_in: Option<i64>,
        }

        let existing = expected
            .chatgpt
            .as_ref()
            .ok_or(ChatgptAuthError::NotLoggedIn)?;
        let response = self
            .inner
            .client
            .post(format!(
                "{}/oauth/token",
                self.inner.config.issuer.trim_end_matches('/')
            ))
            .timeout(REFRESH_REQUEST_TIMEOUT)
            .json(&serde_json::json!({
                "client_id": self.inner.config.client_id,
                "grant_type": "refresh_token",
                "refresh_token": existing.tokens.refresh_token,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let error = if status == StatusCode::UNAUTHORIZED {
                ChatgptAuthError::RefreshFailed(body)
            } else {
                ChatgptAuthError::RefreshFailed(format!("{status}: {body}"))
            };
            return Err(error);
        }

        let refreshed: RefreshResponse = response.json().await?;
        let id_token = refreshed
            .id_token
            .unwrap_or_else(|| existing.tokens.id_token.raw_jwt.clone());
        let refresh_token = refreshed
            .refresh_token
            .unwrap_or_else(|| existing.tokens.refresh_token.clone());
        let mut persisted = StoredChatgptAuth::from_tokens(ChatgptTokenData {
            id_token: parse_chatgpt_id_token(&id_token)?,
            access_token: refreshed.access_token,
            refresh_token,
        })?;
        if persisted.access_token_expires_at.is_none()
            && let Some(expires_in) = refreshed.expires_in.filter(|secs| *secs >= 0)
        {
            persisted.access_token_expires_at =
                Some(Utc::now() + chrono::Duration::seconds(expires_in));
        }
        // A refresh can never move the stored login to another account/workspace;
        // check both the ID token and any account claim carried on the access token.
        if persisted.account_id != existing.account_id {
            return Err(ChatgptAuthError::WorkspaceMismatch {
                expected: existing.account_id.clone(),
                actual: Some(persisted.account_id),
            });
        }
        if let Ok(access_claims) = parse_chatgpt_jwt_claims(&persisted.tokens.access_token)
            && let Some(access_account_id) = access_claims.account_id
            && access_account_id != existing.account_id
        {
            return Err(ChatgptAuthError::WorkspaceMismatch {
                expected: existing.account_id.clone(),
                actual: Some(access_account_id),
            });
        }
        ensure_expected_account_matches(expected_account_id, &persisted)?;
        let mut store = expected.clone();
        store.version = 1;
        store.chatgpt = Some(persisted.clone());
        self.inner.storage.save_if_unchanged(&expected, &store)?;
        Ok(persisted)
    }
}
