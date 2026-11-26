//! Vertex AI endpoint resolution and the authenticated HTTP transport.

use anyhow::{Context, Result};
use tracing::{debug, warn};

use super::auth::{AuthState, VertexAuth};
use super::wire::GoogleGeminiGenerateContentRequest;

const GLOBAL_HOST: &str = "https://aiplatform.googleapis.com";
const DEFAULT_CATALOG_HOST: &str = "https://us-central1-aiplatform.googleapis.com";

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

/// Fully resolved model resource URL prefix and catalog discovery endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Endpoint {
    /// `{origin}/v1/{resource}` without the `:method` suffix.
    model_url: String,
    /// `{origin}/v1beta/decisions` for native calibrated Vertex AI Decisions.
    decisions_url: String,
    /// `{catalog_origin}/v1beta1/publishers/google/models` for live discovery.
    catalog_url: String,
    /// Configured GCP project sent as `x-goog-user-project` on catalog GETs and Decisions POSTs.
    user_project: Option<String>,
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
        let trimmed_project = project_id
            .map(str::trim)
            .filter(|project| !project.is_empty())
            .map(ToString::to_string);
        let (default_origin, default_catalog_origin, resource) = if api_key_mode {
            (
                GLOBAL_HOST.to_string(),
                DEFAULT_CATALOG_HOST.to_string(),
                format!("publishers/google/models/{model}"),
            )
        } else {
            let project = trimmed_project
                .as_deref()
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
                catalog_origin_for_location(location),
                format!("projects/{project}/locations/{location}/publishers/google/models/{model}"),
            )
        };
        let origin_override = base_url.map(|url| url.trim_end_matches('/').to_string());
        let origin = origin_override.clone().unwrap_or(default_origin);
        let catalog_origin = origin_override.unwrap_or(default_catalog_origin);
        Ok(Self {
            model_url: format!("{origin}/v1/{resource}"),
            decisions_url: format!("{origin}/v1beta/decisions"),
            catalog_url: format!("{catalog_origin}/v1beta1/publishers/google/models"),
            user_project: trimmed_project,
        })
    }

    pub(super) fn url(&self, method: Method) -> String {
        match method {
            Method::Generate => format!("{}:generateContent", self.model_url),
            Method::StreamSse => format!("{}:streamGenerateContent?alt=sse", self.model_url),
        }
    }

    pub(super) fn decisions_url(&self) -> &str {
        &self.decisions_url
    }

    pub(super) fn catalog_url(&self) -> &str {
        &self.catalog_url
    }

    pub(super) fn user_project(&self) -> Option<&str> {
        self.user_project.as_deref()
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

/// Vertex Model Garden `publishers.models.list` requires a regional host; use
/// `us-central1-aiplatform.googleapis.com` when the client location is `global`.
fn catalog_origin_for_location(location: &str) -> String {
    if location.eq_ignore_ascii_case("global") {
        DEFAULT_CATALOG_HOST.to_string()
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
    pub(super) fn is_express_mode(&self) -> bool {
        matches!(self.auth, AuthState::ApiKey(_))
    }

    /// GET a Vertex Model Garden publisher models page using OAuth/ADC, retrying
    /// once with a refreshed token on HTTP 401. Never includes auth material or
    /// raw response bodies in returned errors.
    pub(super) async fn get_catalog_page(&self, url: &reqwest::Url) -> Result<reqwest::Response> {
        let AuthState::Adc(cache) = &self.auth else {
            anyhow::bail!("Vertex AI Express Mode does not support publisher model listing");
        };
        let token = cache.token().await.map_err(|_| {
            anyhow::anyhow!("Cannot resolve Vertex AI credentials for model catalog")
        })?;
        let response = self.send_catalog_get(url, &token).await?;
        let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            warn!(
                "Vertex AI rejected the access token during model catalog discovery; refreshing once"
            );
            let token = cache.refresh_rejected(&token).await.map_err(|_| {
                anyhow::anyhow!("Cannot refresh Vertex AI credentials for model catalog")
            })?;
            self.send_catalog_get(url, &token).await?
        } else {
            response
        };
        anyhow::ensure!(
            response.status().is_success(),
            "Vertex AI model catalog HTTP {}",
            response.status().as_u16()
        );
        Ok(response)
    }

    async fn send_catalog_get(&self, url: &reqwest::Url, token: &str) -> Result<reqwest::Response> {
        let mut request = self.http.get(url.clone()).bearer_auth(token);
        if let Some(project) = self.endpoint.user_project() {
            request = request.header("x-goog-user-project", project);
        }
        request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Cannot request Vertex AI model catalog"))
    }
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

    /// POST a native `/v1beta/decisions` request using Vertex AI Express Mode
    /// (`x-goog-api-key`) or OAuth/ADC (`Authorization: Bearer` + `x-goog-user-project`),
    /// retrying once with a refreshed ADC token on HTTP 401. Never includes auth
    /// material or raw response bodies in returned errors.
    pub(super) async fn post_decisions(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response> {
        let url = self.endpoint.decisions_url();
        debug!(%url, "Sending Vertex AI decisions request");
        let response = match &self.auth {
            AuthState::ApiKey(key) => self
                .http
                .post(url)
                .header("x-goog-api-key", key.as_ref())
                .json(body)
                .send()
                .await
                .map_err(|_| anyhow::anyhow!("Vertex AI decision transport failed"))?,
            AuthState::Adc(cache) => {
                let token = cache.token().await.map_err(|_| {
                    anyhow::anyhow!("Cannot resolve Vertex AI credentials for decisions")
                })?;
                let response = self.send_decisions_bearer(url, &token, body).await?;
                if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                    warn!("Vertex AI rejected the access token during decisions; refreshing once");
                    let token = cache.refresh_rejected(&token).await.map_err(|_| {
                        anyhow::anyhow!("Cannot refresh Vertex AI credentials for decisions")
                    })?;
                    self.send_decisions_bearer(url, &token, body).await?
                } else {
                    response
                }
            }
        };
        anyhow::ensure!(
            response.status().is_success(),
            "Vertex AI decision HTTP {}",
            response.status().as_u16()
        );
        Ok(response)
    }

    async fn send_decisions_bearer(
        &self,
        url: &str,
        token: &str,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response> {
        let mut request = self.http.post(url).bearer_auth(token);
        if let Some(project) = self.endpoint.user_project() {
            request = request.header("x-goog-user-project", project);
        }
        request
            .json(body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Vertex AI decision transport failed"))
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
    warn!(
        status = %status,
        body = %excerpt.trim(),
        "Vertex AI returned non-success HTTP status"
    );
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
        assert_eq!(
            endpoint.catalog_url(),
            "http://127.0.0.1:9/v1beta1/publishers/google/models"
        );
        assert_eq!(endpoint.user_project(), Some("p"));
    }

    #[test]
    fn catalog_url_uses_us_central1_for_global_and_regional_host_otherwise() {
        let global =
            Endpoint::resolve(Some("proj"), "global", "gemini-3-pro-preview", false, None).unwrap();
        assert_eq!(
            global.catalog_url(),
            "https://us-central1-aiplatform.googleapis.com/v1beta1/publishers/google/models"
        );
        assert_eq!(global.user_project(), Some("proj"));

        let regional =
            Endpoint::resolve(Some("proj"), "europe-west4", "gemini-2.5-pro", false, None).unwrap();
        assert_eq!(
            regional.catalog_url(),
            "https://europe-west4-aiplatform.googleapis.com/v1beta1/publishers/google/models"
        );
    }

    #[test]
    fn decisions_url_uses_v1beta_decisions_across_modes_and_overrides() {
        let global =
            Endpoint::resolve(Some("proj"), "global", "gemini-decision-1", false, None).unwrap();
        assert_eq!(
            global.decisions_url(),
            "https://aiplatform.googleapis.com/v1beta/decisions"
        );

        let regional = Endpoint::resolve(
            Some("proj"),
            "us-central1",
            "gemini-decision-1",
            false,
            None,
        )
        .unwrap();
        assert_eq!(
            regional.decisions_url(),
            "https://us-central1-aiplatform.googleapis.com/v1beta/decisions"
        );

        let express =
            Endpoint::resolve(None, "us-central1", "gemini-decision-1", true, None).unwrap();
        assert_eq!(
            express.decisions_url(),
            "https://aiplatform.googleapis.com/v1beta/decisions"
        );

        let overridden = Endpoint::resolve(
            Some("proj"),
            "europe-west4",
            "gemini-decision-1",
            false,
            Some("http://127.0.0.1:4321/"),
        )
        .unwrap();
        assert_eq!(
            overridden.decisions_url(),
            "http://127.0.0.1:4321/v1beta/decisions"
        );
    }
}
