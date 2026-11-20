//! Vertex AI credentials: Alan managed Google OAuth (`auth.json`), standard
//! ADC `authorized_user` JSON, `gcloud` CLI fallback, or Express Mode API key.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tracing::debug;

/// How the client authenticates to Vertex AI.
#[derive(Clone, PartialEq, Eq)]
pub enum VertexAuth {
    /// OAuth/ADC priority chain (managed Google OAuth -> standard ADC JSON -> `gcloud`); requires a project.
    Adc,
    /// Vertex AI Express Mode API key sent as `x-goog-api-key`.
    ApiKey(String),
}

impl std::fmt::Debug for VertexAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Adc => f.write_str("Adc"),
            Self::ApiKey(_) => f.write_str("ApiKey(<redacted>)"),
        }
    }
}

/// Source of OAuth access tokens for ADC authentication.
///
/// The default implementation shells out to `gcloud`; tests inject a fake.
#[async_trait::async_trait]
pub trait VertexAccessTokenSource: Send + Sync {
    /// Fetch a fresh access token.
    async fn access_token(&self) -> Result<String>;
}

/// Fetches tokens using the priority chain:
/// 1. Alan managed Google OAuth in `auth.json` (with single-flight refresh)
/// 2. Standard ADC `authorized_user` JSON file (`GOOGLE_APPLICATION_CREDENTIALS` or gcloud ADC)
/// 3. `gcloud auth application-default print-access-token` / `gcloud auth print-access-token`
#[derive(Debug, Default, Clone)]
pub struct VertexChainTokenSource {
    storage_path: Option<PathBuf>,
}

impl VertexChainTokenSource {
    /// Resolve tokens using a specific Host-managed `auth.json` before ADC/`gcloud` fallbacks.
    pub(super) fn with_storage_path(storage_path: PathBuf) -> Self {
        Self {
            storage_path: Some(storage_path),
        }
    }
}

#[async_trait::async_trait]
impl VertexAccessTokenSource for VertexChainTokenSource {
    async fn access_token(&self) -> Result<String> {
        debug!("Fetching Vertex AI access token via auth priority chain");
        let resolved = alan_auth::google::resolve_vertex_access_token(self.storage_path.as_deref())
            .await
            .map_err(anyhow::Error::from)?;
        debug!(source = resolved.source.label(), "Resolved Vertex AI token");
        Ok(resolved.access_token)
    }
}

/// Fetches tokens with `gcloud auth print-access-token`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GcloudAccessTokenSource;

#[async_trait::async_trait]
impl VertexAccessTokenSource for GcloudAccessTokenSource {
    async fn access_token(&self) -> Result<String> {
        debug!("Fetching Vertex AI access token via gcloud");
        alan_auth::google::gcloud_print_access_token()
            .await
            .map_err(anyhow::Error::from)
    }
}

/// gcloud tokens live for 60 minutes; refresh well before expiry.
pub(super) const ADC_TOKEN_TTL: Duration = Duration::from_secs(50 * 60);

struct CachedToken {
    token: String,
    fetched_at: Instant,
}

/// TTL cache in front of a token source, shared by a client and its stream tasks.
pub(super) struct AdcTokenCache {
    source: Arc<dyn VertexAccessTokenSource>,
    ttl: Duration,
    cached: tokio::sync::Mutex<Option<CachedToken>>,
}

impl AdcTokenCache {
    pub(super) fn new(source: Arc<dyn VertexAccessTokenSource>, ttl: Duration) -> Self {
        Self {
            source,
            ttl,
            cached: tokio::sync::Mutex::new(None),
        }
    }

    /// Return the cached token, fetching when absent or older than the TTL.
    pub(super) async fn token(&self) -> Result<String> {
        let mut cached = self.cached.lock().await;
        if let Some(entry) = cached.as_ref()
            && entry.fetched_at.elapsed() < self.ttl
        {
            return Ok(entry.token.clone());
        }
        self.fetch_into(&mut cached).await
    }

    /// Replace a token the server rejected. A concurrent caller that already
    /// refreshed wins, so one rejection never triggers two fetches.
    pub(super) async fn refresh_rejected(&self, rejected: &str) -> Result<String> {
        let mut cached = self.cached.lock().await;
        if let Some(entry) = cached.as_ref()
            && entry.token != rejected
            && entry.fetched_at.elapsed() < self.ttl
        {
            return Ok(entry.token.clone());
        }
        self.fetch_into(&mut cached).await
    }

    async fn fetch_into(&self, cached: &mut Option<CachedToken>) -> Result<String> {
        *cached = None;
        let token = self.source.access_token().await?;
        *cached = Some(CachedToken {
            token: token.clone(),
            fetched_at: Instant::now(),
        });
        Ok(token)
    }
}

/// Resolved credential state used by the transport.
#[derive(Clone)]
pub(super) enum AuthState {
    Adc(Arc<AdcTokenCache>),
    ApiKey(Arc<str>),
}

impl AuthState {
    pub(super) fn from_auth(auth: VertexAuth) -> Self {
        Self::from_auth_with_storage_path(auth, None)
    }

    pub(super) fn from_auth_with_storage_path(
        auth: VertexAuth,
        storage_path: Option<PathBuf>,
    ) -> Self {
        match auth {
            VertexAuth::Adc => {
                let source = match storage_path {
                    Some(path) => VertexChainTokenSource::with_storage_path(path),
                    None => VertexChainTokenSource::default(),
                };
                Self::Adc(Arc::new(AdcTokenCache::new(
                    Arc::new(source),
                    ADC_TOKEN_TTL,
                )))
            }
            VertexAuth::ApiKey(key) => Self::ApiKey(Arc::from(key)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(AtomicUsize);

    #[async_trait::async_trait]
    impl VertexAccessTokenSource for Counting {
        async fn access_token(&self) -> Result<String> {
            let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(format!("token-{n}"))
        }
    }

    #[tokio::test]
    async fn caches_until_ttl_and_refreshes_rejected_once() {
        let source = Arc::new(Counting(AtomicUsize::new(0)));
        let cache = AdcTokenCache::new(source.clone(), ADC_TOKEN_TTL);
        assert_eq!(cache.token().await.unwrap(), "token-1");
        assert_eq!(cache.token().await.unwrap(), "token-1");
        assert_eq!(cache.refresh_rejected("token-1").await.unwrap(), "token-2");
        // A stale rejection of an already-replaced token does not refetch.
        assert_eq!(cache.refresh_rejected("token-1").await.unwrap(), "token-2");
        assert_eq!(source.0.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn expired_token_is_refetched() {
        let source = Arc::new(Counting(AtomicUsize::new(0)));
        let cache = AdcTokenCache::new(source.clone(), Duration::ZERO);
        assert_eq!(cache.token().await.unwrap(), "token-1");
        assert_eq!(cache.token().await.unwrap(), "token-2");
    }

    #[test]
    fn api_key_is_redacted_in_debug() {
        let rendered = format!("{:?}", VertexAuth::ApiKey("secret".into()));
        assert!(!rendered.contains("secret"));
    }

    #[tokio::test]
    async fn chain_token_source_reads_managed_google_oauth_from_storage() {
        let dir = tempfile::TempDir::new().unwrap();
        let storage_path = dir.path().join("auth.json");
        std::fs::write(
            &storage_path,
            r#"{
                "version": 1,
                "google": {
                    "refresh_token": "refresh-1",
                    "access_token": "managed-vertex-token",
                    "expires_at": "2099-01-01T00:00:00Z",
                    "client_id": "cid",
                    "last_refresh_at": "2026-01-01T00:00:00Z"
                }
            }"#,
        )
        .unwrap();

        let source = VertexChainTokenSource::with_storage_path(storage_path);
        assert_eq!(source.access_token().await.unwrap(), "managed-vertex-token");
    }
}
