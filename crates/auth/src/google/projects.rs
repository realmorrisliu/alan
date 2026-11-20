//! Active Google Cloud Project discovery via Cloud Resource Manager API.

use super::{GOOGLE_PROJECTS_URL, GoogleAuthError};
use serde::{Deserialize, Serialize};
use url::Url;

/// Active Google Cloud Project returned by Cloud Resource Manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcpProject {
    #[serde(rename = "projectId")]
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "projectNumber", default)]
    pub project_number: Option<String>,
    #[serde(rename = "lifecycleState", default)]
    pub lifecycle_state: String,
}

#[derive(Debug, Deserialize)]
struct GcpProjectListResponse {
    #[serde(default)]
    projects: Vec<GcpProject>,
    #[serde(rename = "nextPageToken", default)]
    next_page_token: Option<String>,
}

/// Fetch active Google Cloud projects using Cloud Resource Manager API (`lifecycleState:ACTIVE`).
pub async fn list_active_gcp_projects(
    access_token: &str,
) -> Result<Vec<GcpProject>, GoogleAuthError> {
    list_active_gcp_projects_at(GOOGLE_PROJECTS_URL, access_token).await
}

/// Fetch active Google Cloud projects from `projects_url` (following `nextPageToken`).
pub async fn list_active_gcp_projects_at(
    projects_url: &str,
    access_token: &str,
) -> Result<Vec<GcpProject>, GoogleAuthError> {
    let client = reqwest::Client::new();
    let base_url = Url::parse(projects_url)?;
    let mut page_token: Option<String> = None;
    let mut projects = Vec::new();

    loop {
        let mut request_url = base_url.clone();
        if let Some(token) = &page_token {
            request_url
                .query_pairs_mut()
                .append_pair("pageToken", token);
        }

        let response = client
            .get(request_url)
            .bearer_auth(access_token)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(GoogleAuthError::ProjectDiscoveryFailed(format!(
                "HTTP {status}: {body}"
            )));
        }

        let page: GcpProjectListResponse = serde_json::from_str(&body).map_err(|error| {
            GoogleAuthError::ProjectDiscoveryFailed(format!("invalid response JSON: {error}"))
        })?;
        for project in page.projects {
            if project.lifecycle_state.is_empty()
                || project.lifecycle_state.eq_ignore_ascii_case("ACTIVE")
            {
                projects.push(project);
            }
        }

        page_token = page
            .next_page_token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        if page_token.is_none() {
            break;
        }
    }

    projects.sort_by(|a, b| a.project_id.cmp(&b.project_id));
    projects.dedup_by(|a, b| a.project_id == b.project_id);
    Ok(projects)
}
