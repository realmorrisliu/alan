//! Fast, strictly read-only, secret-safe local diagnostics (`alan doctor` / `alan doctor --json`).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use alan_agent_engine::LlmProvider;
use alan_auth::AuthStorage;
use alan_os_host::{HostStorePaths, SecretStore, SystemStorePaths};
use alan_service_manager::{
    ConnectionProfile, ConnectionsFile, CredentialKind, sanitize_identifier,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Args;
use serde::Serialize;

const MANIFEST_FILE_NAME: &str = ".alan-cli-manifest";
const LEGACY_STABLE_MANIFEST: &str = ".alan-cli-manifest-stable";
const LEGACY_DEV_MANIFEST: &str = ".alan-cli-manifest-dev";
const MAX_ROLLOUT_SCAN_DEPTH: usize = 3;
const MAX_ROLLOUT_ENTRIES_SCANNED: usize = 4_096;

/// Arguments for `alan doctor`.
#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    /// Emit structured JSON diagnostics instead of human-readable text
    #[arg(long)]
    pub json: bool,
}

/// Complete read-only local diagnostic report emitted by `alan doctor`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DoctorReport {
    pub healthy: bool,
    pub version: VersionDiagnostics,
    pub environment: EnvironmentDiagnostics,
    pub connections: ConnectionsDiagnostics,
    pub rollouts: RolloutsDiagnostics,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VersionDiagnostics {
    pub version: String,
    pub executable_path: Option<String>,
    pub install_ownership: String,
    pub manifest_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StorePathStatus {
    pub path: String,
    pub exists: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EnvironmentDiagnostics {
    pub system_store: StorePathStatus,
    pub host_store: StorePathStatus,
    pub credentials_dir: StorePathStatus,
    pub managed_auth_path: String,
    pub managed_auth_exists: bool,
    pub sandbox_backend: String,
    pub config_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConnectionsDiagnostics {
    pub metadata_path: String,
    pub metadata_exists: bool,
    pub metadata_error: Option<String>,
    pub default_profile: Option<String>,
    pub missing_auth_profiles: Vec<String>,
    pub profiles: Vec<ProfileAuthDiagnostics>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProfileAuthDiagnostics {
    pub profile_id: String,
    pub provider: String,
    pub model: Option<String>,
    pub is_default: bool,
    pub auth_kind: String,
    pub auth_present: bool,
    pub auth_status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RolloutsDiagnostics {
    pub rollouts_dir: String,
    pub exists: bool,
    pub rollout_count: usize,
    pub truncated: bool,
    pub most_recent_path: Option<String>,
    pub most_recent_modified: Option<String>,
}

/// Execute `alan doctor` / `alan doctor --json`.
pub fn run(args: DoctorArgs) -> Result<()> {
    let report = collect_diagnostics()?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", format_human_report(&report));
    }
    Ok(())
}

/// Collect strictly read-only, offline local diagnostics for the current environment.
pub fn collect_diagnostics() -> Result<DoctorReport> {
    let exe_path = std::env::current_exe().ok();
    let system_store = SystemStorePaths::detect()?;
    let host_store = HostStorePaths::detect()?;
    let sandbox_backend = alan_agent_engine::tools::active_backend_name().to_string();
    let config_override = std::env::var("ALAN_CONFIG_PATH")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let adc_present = alan_auth::google::resolve_adc_credentials_path().is_some();

    Ok(collect_diagnostics_at(
        exe_path.as_deref(),
        &system_store,
        &host_store,
        sandbox_backend,
        config_override,
        adc_present,
    ))
}

/// Deterministic, strictly read-only diagnostic collector for runtime and tests.
pub(crate) fn collect_diagnostics_at(
    exe_path: Option<&Path>,
    system_store: &SystemStorePaths,
    host_store: &HostStorePaths,
    sandbox_backend: String,
    config_override: Option<String>,
    adc_present: bool,
) -> DoctorReport {
    let version = inspect_version(exe_path);
    let environment =
        inspect_environment(system_store, host_store, sandbox_backend, config_override);
    let connections = inspect_connections(system_store, host_store, adc_present);
    let rollouts = inspect_rollouts(system_store);

    let healthy = environment.system_store.writable
        && environment.host_store.writable
        && connections.metadata_error.is_none()
        && connections.missing_auth_profiles.is_empty();

    DoctorReport {
        healthy,
        version,
        environment,
        connections,
        rollouts,
    }
}

fn inspect_version(exe_path: Option<&Path>) -> VersionDiagnostics {
    let resolved_exe =
        exe_path.map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
    let (install_ownership, manifest_path) = match resolved_exe.as_deref().and_then(Path::parent) {
        Some(dir) => {
            let main_manifest = dir.join(MANIFEST_FILE_NAME);
            let legacy_stable = dir.join(LEGACY_STABLE_MANIFEST);
            let legacy_dev = dir.join(LEGACY_DEV_MANIFEST);
            if main_manifest.is_file() {
                (
                    "managed".to_string(),
                    Some(main_manifest.display().to_string()),
                )
            } else if legacy_stable.is_file() {
                (
                    "legacy_managed".to_string(),
                    Some(legacy_stable.display().to_string()),
                )
            } else if legacy_dev.is_file() {
                (
                    "legacy_managed".to_string(),
                    Some(legacy_dev.display().to_string()),
                )
            } else {
                ("dev_or_unmanaged".to_string(), None)
            }
        }
        None => ("dev_or_unmanaged".to_string(), None),
    };

    VersionDiagnostics {
        version: env!("CARGO_PKG_VERSION").to_string(),
        executable_path: resolved_exe.map(|p| p.display().to_string()),
        install_ownership,
        manifest_path,
    }
}

fn inspect_environment(
    system_store: &SystemStorePaths,
    host_store: &HostStorePaths,
    sandbox_backend: String,
    config_override: Option<String>,
) -> EnvironmentDiagnostics {
    let host_root = host_store
        .credentials
        .parent()
        .unwrap_or(host_store.credentials.as_path());
    EnvironmentDiagnostics {
        system_store: inspect_store_path(&system_store.root),
        host_store: inspect_store_path(host_root),
        credentials_dir: inspect_store_path(&host_store.credentials),
        managed_auth_path: host_store.managed_auth.display().to_string(),
        managed_auth_exists: host_store.managed_auth.is_file(),
        sandbox_backend,
        config_override,
    }
}

fn inspect_store_path(path: &Path) -> StorePathStatus {
    let exists = path.exists();
    let writable = is_path_writable_readonly(path);
    StorePathStatus {
        path: path.display().to_string(),
        exists,
        writable,
    }
}

/// Strictly read-only writability check: inspects `path` (or its nearest existing ancestor)
/// without creating or modifying any files.
fn is_path_writable_readonly(path: &Path) -> bool {
    let target = if path.exists() {
        Some(path)
    } else {
        path.ancestors().find(|ancestor| ancestor.exists())
    };
    let Some(target) = target else {
        return false;
    };

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = CString::new(target.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: `c_path` is a valid NUL-terminated path string and `access` performs
        // a pure read-only permission check without mutating the filesystem.
        unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
    }

    #[cfg(not(unix))]
    {
        std::fs::metadata(target)
            .map(|meta| !meta.permissions().readonly())
            .unwrap_or(false)
    }
}

fn inspect_connections(
    system_store: &SystemStorePaths,
    host_store: &HostStorePaths,
    adc_present: bool,
) -> ConnectionsDiagnostics {
    let metadata_path = system_store.connections_metadata().unwrap_or_else(|_| {
        system_store
            .root
            .join("services/connections/connections.toml")
    });
    let metadata_exists = metadata_path.is_file();

    if !metadata_exists {
        return ConnectionsDiagnostics {
            metadata_path: metadata_path.display().to_string(),
            metadata_exists: false,
            metadata_error: None,
            default_profile: None,
            missing_auth_profiles: Vec::new(),
            profiles: Vec::new(),
        };
    }

    let connections = match ConnectionsFile::load_from_path(&metadata_path) {
        Ok((file, _)) => file,
        Err(err) => {
            return ConnectionsDiagnostics {
                metadata_path: metadata_path.display().to_string(),
                metadata_exists: true,
                metadata_error: Some(format!("failed to read connections metadata: {err}")),
                default_profile: None,
                missing_auth_profiles: Vec::new(),
                profiles: Vec::new(),
            };
        }
    };

    let secret_store = SecretStore::from_directory(&host_store.credentials).ok();
    let managed_auth = AuthStorage::new(host_store.managed_auth.clone())
        .ok()
        .and_then(|storage| storage.load().ok());
    let chatgpt_auth_present = managed_auth
        .as_ref()
        .and_then(|store| store.chatgpt.as_ref())
        .is_some();
    let google_auth_present = managed_auth
        .as_ref()
        .and_then(|store| store.google.as_ref())
        .is_some();

    let default_profile = connections
        .default_profile
        .as_deref()
        .map(display_identifier);

    let mut profiles = Vec::with_capacity(connections.profiles.len());
    let mut missing_auth_profiles = Vec::new();

    for (profile_id, profile) in &connections.profiles {
        let safe_id = display_identifier(profile_id);
        let is_default = connections.default_profile.as_deref() == Some(profile_id.as_str());
        let diag = inspect_profile_auth(
            &safe_id,
            profile,
            is_default,
            secret_store.as_ref(),
            chatgpt_auth_present,
            google_auth_present,
            adc_present,
        );
        if !diag.auth_present {
            missing_auth_profiles.push(safe_id);
        }
        profiles.push(diag);
    }

    ConnectionsDiagnostics {
        metadata_path: metadata_path.display().to_string(),
        metadata_exists: true,
        metadata_error: None,
        default_profile,
        missing_auth_profiles,
        profiles,
    }
}

fn inspect_profile_auth(
    safe_id: &str,
    profile: &ConnectionProfile,
    is_default: bool,
    secret_store: Option<&SecretStore>,
    chatgpt_auth_present: bool,
    google_auth_present: bool,
    adc_present: bool,
) -> ProfileAuthDiagnostics {
    let descriptor = ConnectionsFile::profile_descriptor(profile.provider);
    let model = profile
        .settings
        .get("model")
        .or_else(|| profile.settings.get("deployment"))
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(ToString::to_string);

    let has_secret = |cred_id: Option<&str>| -> bool {
        let Some(cred_id) = cred_id else {
            return false;
        };
        let Some(store) = secret_store else {
            return false;
        };
        // Immediately reduce secret lookup to a boolean so secret bytes are never retained or emitted.
        store
            .load(cred_id)
            .ok()
            .flatten()
            .is_some_and(|secret| !secret.trim().is_empty())
    };

    let (auth_kind, auth_present, auth_status) = match profile.provider {
        LlmProvider::Chatgpt => (
            CredentialKind::ManagedOauth.as_str().to_string(),
            chatgpt_auth_present,
            if chatgpt_auth_present {
                "present (managed OAuth)".to_string()
            } else {
                format!("missing (run `alan connection login {safe_id}`)")
            },
        ),
        LlmProvider::GoogleGeminiGenerateContent => {
            if has_secret(profile.credential_id.as_deref()) {
                (
                    CredentialKind::SecretString.as_str().to_string(),
                    true,
                    "present (secret stored)".to_string(),
                )
            } else if google_auth_present {
                (
                    CredentialKind::ManagedOauth.as_str().to_string(),
                    true,
                    "present (managed Google OAuth)".to_string(),
                )
            } else if adc_present {
                (
                    CredentialKind::AmbientCloudAuth.as_str().to_string(),
                    true,
                    "present (ADC)".to_string(),
                )
            } else {
                (
                    CredentialKind::AmbientCloudAuth.as_str().to_string(),
                    false,
                    format!("missing (run `alan connection login {safe_id}` or configure ADC)"),
                )
            }
        }
        LlmProvider::Magpie | LlmProvider::OpenAiChatCompletionsCompatible => {
            if has_secret(profile.credential_id.as_deref()) {
                (
                    CredentialKind::SecretString.as_str().to_string(),
                    true,
                    "present (secret stored)".to_string(),
                )
            } else {
                (
                    "local_proxy".to_string(),
                    true,
                    "optional (local proxy / no secret)".to_string(),
                )
            }
        }
        LlmProvider::TypesafeEvaluation => ("none".to_string(), true, "not required".to_string()),
        _ => {
            let present = has_secret(profile.credential_id.as_deref());
            let kind = descriptor.credential_kind.as_str().to_string();
            let status = if present {
                "present (secret stored)".to_string()
            } else if descriptor.credential_optional {
                "optional (unset)".to_string()
            } else {
                format!("missing (run `alan connection set-secret {safe_id}`)")
            };
            (kind, present || descriptor.credential_optional, status)
        }
    };

    ProfileAuthDiagnostics {
        profile_id: safe_id.to_string(),
        provider: profile.provider.as_str().to_string(),
        model,
        is_default,
        auth_kind,
        auth_present,
        auth_status,
    }
}

fn inspect_rollouts(system_store: &SystemStorePaths) -> RolloutsDiagnostics {
    let rollouts_dir = system_store
        .agent_runtime()
        .map(|rt| rt.rollouts)
        .unwrap_or_else(|_| system_store.root.join("services/agent-runtime/rollouts"));
    if !rollouts_dir.is_dir() {
        return RolloutsDiagnostics {
            rollouts_dir: rollouts_dir.display().to_string(),
            exists: false,
            rollout_count: 0,
            truncated: false,
            most_recent_path: None,
            most_recent_modified: None,
        };
    }

    let mut count = 0usize;
    let mut scanned = 0usize;
    let mut truncated = false;
    let mut latest: Option<(SystemTime, PathBuf)> = None;
    scan_rollout_dir(
        &rollouts_dir,
        0,
        &mut count,
        &mut scanned,
        &mut truncated,
        &mut latest,
    );

    let (most_recent_path, most_recent_modified) = match latest {
        Some((modified, path)) => {
            let ts: DateTime<Utc> = modified.into();
            (Some(path.display().to_string()), Some(ts.to_rfc3339()))
        }
        None => (None, None),
    };

    RolloutsDiagnostics {
        rollouts_dir: rollouts_dir.display().to_string(),
        exists: true,
        rollout_count: count,
        truncated,
        most_recent_path,
        most_recent_modified,
    }
}

fn scan_rollout_dir(
    dir: &Path,
    depth: usize,
    count: &mut usize,
    scanned: &mut usize,
    truncated: &mut bool,
    latest: &mut Option<(SystemTime, PathBuf)>,
) {
    if depth > MAX_ROLLOUT_SCAN_DEPTH || *truncated {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        *scanned += 1;
        if *scanned > MAX_ROLLOUT_ENTRIES_SCANNED {
            *truncated = true;
            return;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            scan_rollout_dir(&path, depth + 1, count, scanned, truncated, latest);
        } else if file_type.is_file() {
            *count += 1;
            if let Ok(meta) = entry.metadata()
                && let Ok(modified) = meta.modified()
                && latest.as_ref().is_none_or(|(prev, _)| modified > *prev)
            {
                *latest = Some((modified, path));
            }
        }
    }
}

fn display_identifier(value: &str) -> String {
    sanitize_identifier(value).unwrap_or_else(|| "<redacted>".to_string())
}

/// Format a calm, structured human-readable diagnostic report.
pub fn format_human_report(report: &DoctorReport) -> String {
    let mut out = String::new();
    out.push_str("Alan Doctor\n\n");

    out.push_str("Version & Binary\n");
    out.push_str(&format!("  version:      {}\n", report.version.version));
    out.push_str(&format!(
        "  executable:   {}\n",
        report
            .version
            .executable_path
            .as_deref()
            .unwrap_or("unknown")
    ));
    match report.version.manifest_path.as_deref() {
        Some(manifest) => out.push_str(&format!(
            "  install:      {} ({manifest})\n",
            report.version.install_ownership
        )),
        None => out.push_str(&format!(
            "  install:      {}\n",
            report.version.install_ownership
        )),
    }

    out.push_str("\nStores & Environment\n");
    out.push_str(&format!(
        "  system store: {} ({})\n",
        report.environment.system_store.path,
        store_flags(&report.environment.system_store)
    ));
    out.push_str(&format!(
        "  host store:   {} ({})\n",
        report.environment.host_store.path,
        store_flags(&report.environment.host_store)
    ));
    out.push_str(&format!(
        "  sandbox:      {}\n",
        report.environment.sandbox_backend
    ));
    if let Some(config_path) = report.environment.config_override.as_deref() {
        out.push_str(&format!(
            "  config:       {config_path} (ALAN_CONFIG_PATH)\n"
        ));
    }

    out.push_str("\nConnections & Auth\n");
    if let Some(error) = report.connections.metadata_error.as_deref() {
        out.push_str(&format!("  status:       error ({error})\n"));
    } else if report.connections.profiles.is_empty() {
        out.push_str("  default:      none\n");
        out.push_str("  profiles:     none configured (run `alan login` to connect a provider)\n");
    } else {
        out.push_str(&format!(
            "  default:      {}\n",
            report
                .connections
                .default_profile
                .as_deref()
                .unwrap_or("none")
        ));
        out.push_str(&format!(
            "  profiles:     {} configured, {} missing auth\n",
            report.connections.profiles.len(),
            report.connections.missing_auth_profiles.len()
        ));
        for profile in &report.connections.profiles {
            let marker = if profile.is_default { "*" } else { " " };
            let model = profile.model.as_deref().unwrap_or("(default)");
            out.push_str(&format!(
                "  {marker} {:<16} {:<18} model={:<18} auth={}\n",
                profile.profile_id, profile.provider, model, profile.auth_status
            ));
        }
    }

    out.push_str("\nRecent Rollouts\n");
    out.push_str(&format!(
        "  directory:    {} ({})\n",
        report.rollouts.rollouts_dir,
        if report.rollouts.exists {
            "exists"
        } else {
            "not created yet"
        }
    ));
    let count_suffix = if report.rollouts.truncated { "+" } else { "" };
    out.push_str(&format!(
        "  rollouts:     {}{count_suffix}\n",
        report.rollouts.rollout_count
    ));
    if let Some(recent) = report.rollouts.most_recent_path.as_deref() {
        match report.rollouts.most_recent_modified.as_deref() {
            Some(ts) => out.push_str(&format!("  most recent:  {recent} ({ts})\n")),
            None => out.push_str(&format!("  most recent:  {recent}\n")),
        }
    }

    out
}

fn store_flags(status: &StorePathStatus) -> String {
    let presence = if status.exists {
        "exists"
    } else {
        "not created yet"
    };
    let access = if status.writable {
        "writable"
    } else {
        "not writable"
    };
    format!("{presence}, {access}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_service_manager::{ConnectionCredential, default_credential_backend};
    use std::collections::BTreeMap;

    #[test]
    fn doctor_reports_managed_install_connections_rollouts_and_never_leaks_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let fake_exe = bin_dir.join("alan");
        std::fs::write(&fake_exe, b"binary").unwrap();
        std::fs::write(bin_dir.join(MANIFEST_FILE_NAME), "alan|deadbeef\n").unwrap();

        let system = SystemStorePaths::from_root(&dir.path().join("System Store")).unwrap();
        let host = HostStorePaths::from_root(&dir.path().join("Host Store")).unwrap();
        std::fs::create_dir_all(&system.root).unwrap();
        std::fs::create_dir_all(&host.credentials).unwrap();

        // Store a secret and make sure its raw value NEVER appears in human or JSON output.
        let secret_sentinel = "sk-super-secret-doctor-sentinel-987654321";
        SecretStore::from_directory(&host.credentials)
            .unwrap()
            .save("openai-main", secret_sentinel)
            .unwrap();

        let mut connections = ConnectionsFile::default();
        let now = Utc::now();
        connections.default_profile = Some("openai-main".to_string());
        connections.credentials.insert(
            "openai-main".to_string(),
            ConnectionCredential {
                kind: CredentialKind::SecretString,
                provider_family: LlmProvider::OpenAiResponses,
                label: "OpenAI credential".to_string(),
                backend: default_credential_backend(CredentialKind::SecretString).to_string(),
            },
        );
        connections.profiles.insert(
            "openai-main".to_string(),
            ConnectionProfile {
                provider: LlmProvider::OpenAiResponses,
                label: Some("OpenAI".to_string()),
                credential_id: Some("openai-main".to_string()),
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings: BTreeMap::from([("model".to_string(), "gpt-5.4".to_string())]),
            },
        );
        connections.profiles.insert(
            "openrouter-missing".to_string(),
            ConnectionProfile {
                provider: LlmProvider::OpenRouter,
                label: None,
                credential_id: Some("openrouter-missing".to_string()),
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings: BTreeMap::new(),
            },
        );
        connections.profiles.insert(
            "magpie-local".to_string(),
            ConnectionProfile {
                provider: LlmProvider::Magpie,
                label: None,
                credential_id: None,
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings: BTreeMap::new(),
            },
        );

        let meta_path = system.connections_metadata().unwrap();
        std::fs::create_dir_all(meta_path.parent().unwrap()).unwrap();
        std::fs::write(&meta_path, toml::to_string_pretty(&connections).unwrap()).unwrap();

        let rollouts_dir = system.agent_runtime().unwrap().rollouts;
        std::fs::create_dir_all(&rollouts_dir).unwrap();
        let rollout_file = rollouts_dir.join("rollout-latest.jsonl");
        std::fs::write(&rollout_file, "{}\n").unwrap();

        let report = collect_diagnostics_at(
            Some(&fake_exe),
            &system,
            &host,
            "seatbelt".to_string(),
            None,
            false,
        );

        assert!(
            !report.healthy,
            "missing auth profile should mark unhealthy"
        );
        assert_eq!(report.version.install_ownership, "managed");
        assert!(report.version.manifest_path.is_some());
        assert_eq!(report.environment.sandbox_backend, "seatbelt");
        assert!(report.environment.system_store.exists);
        assert!(report.environment.system_store.writable);
        assert_eq!(
            report.connections.default_profile.as_deref(),
            Some("openai-main")
        );
        assert_eq!(
            report.connections.missing_auth_profiles,
            vec!["openrouter-missing".to_string()]
        );
        assert_eq!(report.rollouts.rollout_count, 1);
        assert!(
            report
                .rollouts
                .most_recent_path
                .as_deref()
                .is_some_and(|p| p.ends_with("rollout-latest.jsonl"))
        );

        let human = format_human_report(&report);
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(
            !human.contains(secret_sentinel),
            "human output leaked secret"
        );
        assert!(!json.contains(secret_sentinel), "json output leaked secret");
        assert!(human.contains("Alan Doctor"));
        assert!(human.contains("openai-main"));
        assert!(human.contains("openrouter-missing"));
        assert!(human.contains("magpie-local"));
    }

    #[test]
    fn doctor_is_strictly_readonly_when_stores_do_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let system = SystemStorePaths::from_root(&dir.path().join("System Store")).unwrap();
        let host = HostStorePaths::from_root(&dir.path().join("Host Store")).unwrap();

        let report =
            collect_diagnostics_at(None, &system, &host, "seatbelt".to_string(), None, false);

        assert!(report.healthy);
        assert!(!system.root.exists(), "doctor must not create System Store");
        assert!(
            !host.credentials.exists(),
            "doctor must not create Host Store"
        );
        assert!(!report.environment.system_store.exists);
        assert!(report.environment.system_store.writable);
        assert_eq!(report.version.install_ownership, "dev_or_unmanaged");
        assert_eq!(report.rollouts.rollout_count, 0);
    }
}
