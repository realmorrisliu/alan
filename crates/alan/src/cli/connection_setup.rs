//! Shared Connection setup, probe, GCP project prioritization, and atomic upsert helpers.

use alan_agent_engine::LlmProvider;
use alan_auth::google::{GcpProject, ResolvedGoogleToken, resolve_vertex_access_token};
use alan_auth::magpie::{DEFAULT_MAGPIE_BASE_URL, MagpieProbeResult, probe_magpie_with_options};
use alan_os_host::{
    HostCommandPlane, HostEndpointPaths, HostStorePaths, SecretStore, SystemStorePaths,
};
use alan_service_manager::{
    ConnectionCredential, ConnectionProfile, ConnectionService, ConnectionStoreBindings,
    ConnectionsFile, CredentialKind, default_credential_backend, normalize_profile_settings,
    validate_profile_settings,
};
use alan_shell::Shell;
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const QUICK_MAGPIE_PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// Open an in-process `/mnt/connections` shell backed by `ConnectionService`.
pub(crate) fn open_connection_shell(bindings: &ConnectionStoreBindings) -> Result<Shell> {
    let service = ConnectionService::open(bindings)?;
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/mnt/connections",
        alan_ap::InProcessTransport::new(service.file_server()),
        alan_kernel::Access::ReadWrite,
    );
    Ok(Shell::new(alan_ap::InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(namespace),
    ))))
}

/// Read current Connection metadata via `/mnt/connections/metadata`.
pub(crate) async fn load_connections_readonly() -> Result<ConnectionsFile> {
    let system = SystemStorePaths::detect()?;
    let shell = open_connection_shell(&system.connection_bindings()?)?;
    let bytes = shell
        .cat("/mnt/connections/metadata")
        .await
        .map_err(|error| anyhow::anyhow!("read Connection Service metadata: {error}"))?;
    serde_json::from_slice(&bytes).context("decode Connection Service metadata")
}

/// Atomically validate profile metadata, persist any Host secret, and commit
/// Connection Service metadata via `/mnt/connections/ctl` using optimistic CAS.
///
/// Returns the resolved non-empty default `model` for `profile_id`, if any.
pub(crate) async fn atomic_upsert_profile(
    profile_id: &str,
    provider: LlmProvider,
    credential_id: Option<&str>,
    secret: Option<&str>,
    overrides: BTreeMap<String, String>,
    set_default: bool,
) -> Result<Option<String>> {
    let system = SystemStorePaths::detect()?;
    let host = HostStorePaths::detect()?;
    let shell = open_connection_shell(&system.connection_bindings()?)?;
    let bytes = shell
        .cat("/mnt/connections/metadata")
        .await
        .map_err(|error| anyhow::anyhow!("read Connection Service metadata: {error}"))?;
    let mut connections: ConnectionsFile =
        serde_json::from_slice(&bytes).context("decode Connection Service metadata")?;
    let expected = connections.fingerprint()?;

    let descriptor = ConnectionsFile::profile_descriptor(provider);
    if let Some(cred_id) = credential_id {
        let expected_kind = if descriptor.credential_kind == CredentialKind::AmbientCloudAuth
            && descriptor.supports_secret_entry
        {
            CredentialKind::SecretString
        } else {
            descriptor.credential_kind
        };
        if let Some(existing) = connections.credentials.get(cred_id) {
            anyhow::ensure!(
                existing.provider_family == provider,
                "Credential `{cred_id}` is already bound to provider `{}`",
                existing.provider_family.as_str()
            );
            anyhow::ensure!(
                existing.kind == expected_kind,
                "Credential `{cred_id}` uses kind `{}` but provider `{}` requires `{}`",
                existing.kind.as_str(),
                provider.as_str(),
                expected_kind.as_str()
            );
        } else {
            connections.credentials.insert(
                cred_id.to_string(),
                ConnectionCredential {
                    kind: expected_kind,
                    provider_family: provider,
                    label: format!("{} credential", descriptor.display_name),
                    backend: default_credential_backend(expected_kind).to_string(),
                },
            );
        }
    }

    let now = Utc::now();
    if let Some(existing) = connections.profiles.get_mut(profile_id) {
        anyhow::ensure!(
            existing.provider == provider,
            "Profile `{profile_id}` already exists for `{}`",
            existing.provider.as_str()
        );
        if let Some(cred_id) = credential_id {
            existing.credential_id = Some(cred_id.to_string());
        }
        let mut merged = existing.settings.clone();
        merged.extend(overrides);
        let normalized = normalize_profile_settings(provider, &merged);
        validate_profile_settings(provider, &normalized)?;
        existing.settings = normalized;
        existing.updated_at = now;
    } else {
        let settings = normalize_profile_settings(provider, &overrides);
        validate_profile_settings(provider, &settings)?;
        connections.profiles.insert(
            profile_id.to_string(),
            ConnectionProfile {
                provider,
                label: None,
                credential_id: credential_id.map(ToOwned::to_owned),
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings,
            },
        );
    }

    if set_default || (connections.default_profile.is_none() && provider.supports_generation()) {
        connections.default_profile = Some(profile_id.to_string());
    }

    let effective_model =
        normalize_profile_settings(provider, &connections.profiles[profile_id].settings)
            .get("model")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

    // Write secret only after metadata validation succeeded, before ctl commit triggers reload.
    if let (Some(cred_id), Some(secret_value)) = (
        credential_id,
        secret.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        SecretStore::from_directory(&host.credentials)?.save(cred_id, secret_value)?;
    }

    let command = serde_json::json!({
        "op": "replace_metadata",
        "expected": expected,
        "connections": connections,
    });
    shell
        .write(
            "/mnt/connections/ctl",
            &serde_json::to_vec(&command).context("encode Connection Service command")?,
        )
        .await
        .map_err(|error| anyhow::anyhow!("update Connection Service metadata: {error}"))?;

    Ok(effective_model)
}

/// Atomically persist Connection metadata/secrets and notify the running Host to reload callables.
#[allow(dead_code, reason = "called from foreground binary module")]
pub(crate) async fn atomic_upsert_profile_and_reload(
    paths: &HostEndpointPaths,
    profile_id: &str,
    provider: LlmProvider,
    credential_id: Option<&str>,
    secret: Option<&str>,
    overrides: BTreeMap<String, String>,
    set_default: bool,
) -> Result<Option<String>> {
    let model = atomic_upsert_profile(
        profile_id,
        provider,
        credential_id,
        secret,
        overrides,
        set_default,
    )
    .await?;
    HostCommandPlane::new(paths.clone())
        .reload_connections()
        .await?;
    Ok(model)
}

/// Quickly probe a local Magpie router at `DEFAULT_MAGPIE_BASE_URL` without delaying UI menus.
pub(crate) async fn probe_local_magpie_quick() -> Option<MagpieProbeResult> {
    probe_magpie_with_options(
        Some(DEFAULT_MAGPIE_BASE_URL),
        None,
        QUICK_MAGPIE_PROBE_TIMEOUT,
    )
    .await
    .ok()
}

/// Return a currently valid Vertex AI access token from managed OAuth or fresh ADC, if available.
pub(crate) async fn resolve_valid_vertex_token(managed_auth: &Path) -> Option<ResolvedGoogleToken> {
    resolve_vertex_access_token(Some(managed_auth)).await.ok()
}

/// Move any GCP project matching `preferred` to index 0 while preserving order of others.
pub(crate) fn prioritize_gcp_projects(
    mut projects: Vec<GcpProject>,
    preferred: Option<&str>,
) -> Vec<GcpProject> {
    let Some(preferred) = preferred.map(str::trim).filter(|id| !id.is_empty()) else {
        return projects;
    };
    if let Some(pos) = projects
        .iter()
        .position(|project| project.project_id == preferred)
        && pos > 0
    {
        let chosen = projects.remove(pos);
        projects.insert(0, chosen);
    }
    projects
}

/// Actionable user guidance when neither Google Desktop OAuth nor `gcloud` CLI is available.
pub(crate) fn vertex_auth_error_guidance() -> String {
    "No Google Desktop OAuth client (`ALAN_GOOGLE_OAUTH_CLIENT_ID`) or `gcloud` CLI found. \
     Install Google Cloud SDK and run `gcloud auth application-default login`, \
     set `ALAN_GOOGLE_OAUTH_CLIENT_ID`, or configure a Vertex AI Express API key via \
     `alan connection set-secret`."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prioritize_gcp_projects_moves_matching_project_first() {
        let projects = vec![
            GcpProject {
                project_id: "alpha-1".to_string(),
                name: "Alpha".to_string(),
                project_number: None,
                lifecycle_state: "ACTIVE".to_string(),
            },
            GcpProject {
                project_id: "workus-ai-473608".to_string(),
                name: "Workus".to_string(),
                project_number: None,
                lifecycle_state: "ACTIVE".to_string(),
            },
            GcpProject {
                project_id: "zeta-9".to_string(),
                name: "Zeta".to_string(),
                project_number: None,
                lifecycle_state: "ACTIVE".to_string(),
            },
        ];

        let prioritized = prioritize_gcp_projects(projects.clone(), Some(" workus-ai-473608 "));
        assert_eq!(
            prioritized
                .iter()
                .map(|p| p.project_id.as_str())
                .collect::<Vec<_>>(),
            vec!["workus-ai-473608", "alpha-1", "zeta-9"]
        );

        let unchanged = prioritize_gcp_projects(projects, Some("missing-proj"));
        assert_eq!(unchanged[0].project_id, "alpha-1");
    }
}
