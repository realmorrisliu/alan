//! Vertex AI credentials: Application Default Credentials via `gcloud`, or an
//! Express Mode API key.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tracing::debug;

/// How the client authenticates to Vertex AI.
#[derive(Clone, PartialEq, Eq)]
pub enum VertexAuth {
    /// Application Default Credentials (`gcloud auth print-access-token`); requires a project.
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

/// Fetches tokens with `gcloud auth print-access-token`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GcloudAccessTokenSource;

#[async_trait::async_trait]
impl VertexAccessTokenSource for GcloudAccessTokenSource {
    async fn access_token(&self) -> Result<String> {
        debug!("Fetching Vertex AI access token via gcloud");
        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new("gcloud")
                .args(["auth", "print-access-token"])
                .output()
        })
        .await
        .context("gcloud token task failed")?
        .context("Failed to run gcloud command. Is gcloud CLI installed and authenticated?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gcloud auth failed: {}", stderr.trim());
        }
        let token = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in access token")?
            .trim()
            .to_string();
        anyhow::ensure!(!token.is_empty(), "gcloud returned an empty access token");
        Ok(token)
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
        match auth {
            VertexAuth::Adc => Self::Adc(Arc::new(AdcTokenCache::new(
                Arc::new(GcloudAccessTokenSource),
                ADC_TOKEN_TTL,
            ))),
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
}
