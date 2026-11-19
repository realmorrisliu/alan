//! Vertex AI endpoint resolution and the authenticated HTTP transport.

use anyhow::{Context, Result};
use tracing::{debug, warn};

use super::auth::{AuthState, VertexAuth};
use super::wire::GoogleGeminiGenerateContentRequest;

const GLOBAL_HOST: &str = "https://aiplatform.googleapis.com";

/// Construction contract for a Vertex AI Gemini client.
#[derive(Debug, Clone)]
pub struct VertexClientConfig {
    /// GCP project; required for [`VertexAuth::Adc`], ignored in Express Mode.
    pub project_id: Option<String>,
    /// GCP location such as `global` or `us-central1`; ignored in Express Mode.
    pub location: String,
    /// Publisher model id, for example `gemini-2.5-pro`.
    pub model: String,
    pub auth: VertexAuth,
    /// Origin override (`scheme + host[:port]`) replacing the Vertex host; for tests.
    pub base_url: Option<String>,
}

/// Fully resolved model resource URL prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Endpoint {
    /// `{origin}/v1/{resource}` without the `:method` suffix.
    model_url: String,
}

/// GenerateContent RPC methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Method {
    Generate,
    StreamSse,
}

impl Endpoint {
    /// Resolve the model URL; ADC uses the project resource, Express Mode does not.
    pub(super) fn resolve(
        project_id: Option<&str>,
        location: &str,
        model: &str,
        api_key_mode: bool,
        base_url: Option<&str>,
    ) -> Result<Self> {
        let model = normalize_model(model);
        anyhow::ensure!(!model.is_empty(), "Vertex AI model must not be empty");
        anyhow::ensure!(
            !model.contains('/'),
            "Vertex AI model `{model}` must be a publisher model id"
        );
        let location = location.trim();
        let (default_origin, resource) = if api_key_mode {
            (
                GLOBAL_HOST.to_string(),
                format!("publishers/google/models/{model}"),
            )
        } else {
            let project = project_id
                .map(str::trim)
                .filter(|project| !project.is_empty())
                .context("Vertex AI ADC authentication requires project_id")?;
            anyhow::ensure!(
                !location.is_empty()
                    && location
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "invalid Vertex AI location `{location}`"
            );
            (
                origin_for_location(location),
                format!("projects/{project}/locations/{location}/publishers/google/models/{model}"),
            )
        };
        let origin = base_url
            .map(|url| url.trim_end_matches('/').to_string())
            .unwrap_or(default_origin);
        Ok(Self {
            model_url: format!("{origin}/v1/{resource}"),
        })
    }

    pub(super) fn url(&self, method: Method) -> String {
        match method {
            Method::Generate => format!("{}:generateContent", self.model_url),
            Method::StreamSse => format!("{}:streamGenerateContent?alt=sse", self.model_url),
        }
    }
}

/// `global` has no regional prefix; `global-aiplatform` does not exist.
fn origin_for_location(location: &str) -> String {
    if location.eq_ignore_ascii_case("global") {
        GLOBAL_HOST.to_string()
    } else {
        format!("https://{location}-aiplatform.googleapis.com")
    }
}

fn normalize_model(model: &str) -> &str {
    let model = model.trim();
    model
        .strip_prefix("publishers/google/models/")
        .or_else(|| model.strip_prefix("models/"))
        .unwrap_or(model)
}

/// Authenticated POST transport, cheap to clone into stream tasks.
#[derive(Clone)]
pub(super) struct Transport {
    pub(super) http: reqwest::Client,
    pub(super) endpoint: Endpoint,
    pub(super) auth: AuthState,
}

impl Transport {
    /// POST a request, retrying once with a refreshed ADC token on HTTP 401.
    pub(super) async fn post(
        &self,
        method: Method,
        request: &GoogleGeminiGenerateContentRequest,
    ) -> Result<reqwest::Response> {
        let url = self.endpoint.url(method);
        debug!(%url, "Sending Vertex AI request");
        match &self.auth {
            AuthState::ApiKey(key) => {
                let response = self
                    .http
                    .post(&url)
                    .header("x-goog-api-key", key.as_ref())
                    .json(request)
                    .send()
                    .await
                    .context("Failed to send request to Vertex AI")?;
                ensure_success(response).await
            }
            AuthState::Adc(cache) => {
                let token = cache.token().await?;
                let response = self.send_bearer(&url, &token, request).await?;
                if response.status() != reqwest::StatusCode::UNAUTHORIZED {
                    return ensure_success(response).await;
                }
                warn!("Vertex AI rejected the access token; refreshing once");
                let token = cache.refresh_rejected(&token).await?;
                let response = self.send_bearer(&url, &token, request).await?;
                ensure_success(response).await
            }
        }
    }

    async fn send_bearer(
        &self,
        url: &str,
        token: &str,
        request: &GoogleGeminiGenerateContentRequest,
    ) -> Result<reqwest::Response> {
        self.http
            .post(url)
            .bearer_auth(token)
            .json(request)
            .send()
            .await
            .context("Failed to send request to Vertex AI")
    }
}

/// Keep the `reqwest::Error` (status) in the chain for safe failure mapping and
/// add a bounded body excerpt for diagnostics.
async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response> {
    let Err(http_error) = response.error_for_status_ref() else {
        return Ok(response);
    };
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let excerpt: String = body.chars().take(1_000).collect();
    Err(anyhow::Error::from(http_error).context(format!(
        "Vertex AI returned HTTP {status}: {}",
        excerpt.trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_location_uses_unprefixed_host() {
        let endpoint =
            Endpoint::resolve(Some("proj"), "global", "gemini-3-pro-preview", false, None).unwrap();
        assert_eq!(
            endpoint.url(Method::Generate),
            "https://aiplatform.googleapis.com/v1/projects/proj/locations/global/publishers/google/models/gemini-3-pro-preview:generateContent"
        );
        assert!(!endpoint.url(Method::Generate).contains("global-aiplatform"));
    }

    #[test]
    fn regional_location_uses_regional_host() {
        let endpoint =
            Endpoint::resolve(Some("proj"), "us-central1", "gemini-2.5-pro", false, None).unwrap();
        assert_eq!(
            endpoint.url(Method::StreamSse),
            "https://us-central1-aiplatform.googleapis.com/v1/projects/proj/locations/us-central1/publishers/google/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn express_mode_has_no_project_resource() {
        let endpoint =
            Endpoint::resolve(None, "us-central1", "models/gemini-2.5-flash", true, None).unwrap();
        assert_eq!(
            endpoint.url(Method::Generate),
            "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:generateContent"
        );
    }

    #[test]
    fn adc_requires_project_and_base_url_overrides_origin() {
        assert!(Endpoint::resolve(None, "global", "gemini-2.5-pro", false, None).is_err());
        assert!(Endpoint::resolve(Some("p"), "us/evil", "gemini-2.5-pro", false, None).is_err());
        let endpoint = Endpoint::resolve(
            Some("p"),
            "europe-west4",
            "gemini-2.5-pro",
            false,
            Some("http://127.0.0.1:9/"),
        )
        .unwrap();
        assert_eq!(
            endpoint.url(Method::Generate),
            "http://127.0.0.1:9/v1/projects/p/locations/europe-west4/publishers/google/models/gemini-2.5-pro:generateContent"
        );
    }
}
