//! Self-update CLI (`alan update`) and non-blocking 24h TUI startup update check.

mod archive;
mod ownership;
pub mod passive_check;
mod progress;
pub mod semver;

#[cfg(test)]
mod tests;

use archive::{
    TempStagingDir, encode_lower_hex, inspect_and_extract_alan, is_valid_sha256_hex, sha256_file,
};
use ownership::{InstallLock, atomic_install_binary, preflight_ownership, resolve_install_dir};
use progress::UpdateProgress;
use semver::{SemVer, current_target_triple, display_tag, normalize_version_str, parse_semver};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use passive_check::prepare_passive_tui_update_notice;

/// Default canonical release metadata endpoint.
pub const DEFAULT_MANIFEST_URL: &str = "https://alanworks.app/latest.json";
/// Default tagged release download base URL when `--version` is requested without `--manifest-url`.
pub const DEFAULT_RELEASE_BASE_URL: &str =
    "https://github.com/realmorrisliu/alan/releases/download";
const MANIFEST_FETCH_TIMEOUT_SECS: u64 = 15;
const ARCHIVE_CONNECT_TIMEOUT_SECS: u64 = 30;
const ARCHIVE_CHUNK_IDLE_TIMEOUT_SECS: u64 = 30;
const ARCHIVE_MAX_TOTAL_TIMEOUT_SECS: u64 = 900;

/// Arguments for `alan update`.
#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    /// Check for an available update without downloading or installing anything
    #[arg(long)]
    pub check: bool,
    /// Emit structured JSON output
    #[arg(long)]
    pub json: bool,
    /// Install or check a specific release version or tag (e.g. `0.1.0` or `v0.1.0`)
    #[arg(long, value_name = "VER_OR_TAG")]
    pub version: Option<String>,
    /// Allow overwriting unowned/modified binaries, reinstalling, or downgrading with `--version`
    #[arg(long)]
    pub force: bool,
    /// Override the installation directory instead of inspecting the current executable
    #[arg(long, value_name = "DIR")]
    pub install_dir: Option<PathBuf>,
    /// Override the release manifest URL (`latest.json`)
    #[arg(long, value_name = "URL")]
    pub manifest_url: Option<String>,
    /// Suppress routine human-readable status output
    #[arg(short, long)]
    pub quiet: bool,
}

/// Release metadata schema published at `latest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    /// Manifest format version (must be `1`).
    pub schema_version: u64,
    /// Product identifier (must be `"alan"`).
    pub product: String,
    /// Semantic release version without `v` prefix.
    pub version: String,
    /// Git release tag (e.g. `v0.1.0`).
    pub tag: String,
    /// RFC-3339 release timestamp, if supplied.
    #[serde(default)]
    pub published_at: Option<String>,
    /// Web URL for the release page.
    #[serde(default)]
    pub release_url: Option<String>,
    /// Web URL for the release notes.
    #[serde(default)]
    pub notes_url: Option<String>,
    /// Canonical one-line installer script URL.
    #[serde(default)]
    pub installer_url: Option<String>,
    /// Per-target release artifacts keyed by Rust target triple.
    pub targets: BTreeMap<String, TargetArtifact>,
}

/// One target-specific release archive entry inside `latest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetArtifact {
    /// Archive filename (e.g. `alan-0.1.0-aarch64-apple-darwin.tar.gz`).
    #[serde(default)]
    pub asset: String,
    /// Direct download URL for the `.tar.gz` archive.
    pub url: String,
    /// Expected 64-hex SHA-256 checksum of the `.tar.gz` archive.
    pub sha256: String,
    /// Optional archive size in bytes (`0` when unspecified).
    #[serde(default)]
    pub size: u64,
    /// Whether the artifact binary is code-signed.
    #[serde(default)]
    pub signed: bool,
    /// Whether the artifact binary is notarized.
    #[serde(default)]
    pub notarized: bool,
}

/// Structured status output emitted for `alan update --json` and internal inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateReport {
    /// Outcome status: `update_available`, `up_to_date`, `updated`, or `downgrade_selected`.
    pub status: String,
    /// Running version before update.
    pub current_version: String,
    /// Version described by the resolved release manifest.
    pub target_version: String,
    /// Release tag associated with `target_version`.
    pub tag: String,
    /// Target triple selected for this host.
    pub target: String,
    /// Whether `target_version` is strictly newer than `current_version`.
    pub update_available: bool,
    /// Whether `<install_dir>/alan` was replaced during this run.
    pub updated: bool,
    /// Whether `--check` prevented downloading or installing.
    pub checked_only: bool,
    /// Resolved installation directory.
    pub install_dir: String,
    /// Resolved path of the installed `alan` executable.
    pub installed_binary: String,
    /// Manifest URL queried for this run.
    pub manifest_url: String,
    /// Release asset filename.
    pub asset: String,
    /// Release archive download URL.
    pub download_url: String,
    /// Expected archive SHA-256 digest (lowercase hex).
    pub archive_sha256: String,
    /// SHA-256 digest of the installed `alan` binary recorded in `.alan-cli-manifest`, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_sha256: Option<String>,
    /// Release web URL from `latest.json`, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
    /// Release notes URL from `latest.json`, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_url: Option<String>,
    /// Whether the target artifact is marked signed.
    pub signed: bool,
    /// Whether the target artifact is marked notarized.
    pub notarized: bool,
    /// Whether `--force` was supplied.
    pub forced: bool,
    /// Legacy files retired while replacing an owned installation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retired_legacy: Vec<String>,
}

/// Execute `alan update`.
pub async fn run(args: UpdateArgs) -> Result<()> {
    ensure!(
        std::env::var_os("ALAN_INSTALL_CHANNEL").is_none(),
        "ALAN_INSTALL_CHANNEL is retired; unset it and run `alan update`"
    );

    let current_exe = std::env::current_exe().ok();
    let home_dir = dirs::home_dir();
    let target_triple = current_target_triple()?;
    let json_output = args.json;
    let quiet = args.quiet;

    let report = run_with_options(
        args,
        env!("CARGO_PKG_VERSION"),
        target_triple,
        current_exe.as_deref(),
        home_dir.as_deref(),
    )
    .await?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !quiet {
        print_human_report(&report);
    }
    Ok(())
}

/// Resolve the default release manifest URL, honoring `ALAN_MANIFEST_URL` and
/// `ALAN_RELEASE_MANIFEST_URL`.
pub(crate) fn default_manifest_url() -> String {
    for env_key in ["ALAN_MANIFEST_URL", "ALAN_RELEASE_MANIFEST_URL"] {
        if let Ok(value) = std::env::var(env_key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    DEFAULT_MANIFEST_URL.to_string()
}

/// Resolve the manifest URL for an explicit invocation of `alan update`.
pub fn resolve_manifest_url(
    explicit_url: Option<&str>,
    requested_version: Option<&str>,
) -> Result<String> {
    let manifest_env = std::env::var("ALAN_MANIFEST_URL").ok();
    let release_manifest_env = std::env::var("ALAN_RELEASE_MANIFEST_URL").ok();
    let release_base_env = std::env::var("ALAN_RELEASE_BASE_URL").ok();
    resolve_manifest_url_with(
        explicit_url,
        requested_version,
        manifest_env.as_deref(),
        release_manifest_env.as_deref(),
        release_base_env.as_deref(),
    )
}

/// Pure manifest URL resolver taking explicit environment values for deterministic testing.
pub fn resolve_manifest_url_with(
    explicit_url: Option<&str>,
    requested_version: Option<&str>,
    manifest_env: Option<&str>,
    release_manifest_env: Option<&str>,
    release_base_env: Option<&str>,
) -> Result<String> {
    if let Some(url) = explicit_url {
        let trimmed = url.trim();
        ensure!(!trimmed.is_empty(), "`--manifest-url` cannot be empty");
        validate_http_url("--manifest-url", trimmed)?;
        return Ok(trimmed.to_string());
    }

    for (label, value) in [
        ("ALAN_MANIFEST_URL", manifest_env),
        ("ALAN_RELEASE_MANIFEST_URL", release_manifest_env),
    ] {
        if let Some(trimmed) = value.map(str::trim).filter(|v| !v.is_empty()) {
            validate_http_url(label, trimmed)?;
            return Ok(trimmed.to_string());
        }
    }

    if let Some(version_or_tag) = requested_version {
        let normalized = normalize_version_str(version_or_tag)?;
        let tag = display_tag(&normalized);
        let base = release_base_env
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(DEFAULT_RELEASE_BASE_URL);
        validate_http_url("ALAN_RELEASE_BASE_URL", base)?;
        return Ok(format!("{}/{tag}/latest.json", base.trim_end_matches('/')));
    }

    Ok(DEFAULT_MANIFEST_URL.to_string())
}

fn validate_http_url(label: &str, url: &str) -> Result<()> {
    ensure!(
        url.starts_with("https://") || url.starts_with("http://"),
        "{label} must use an `http://` or `https://` URL: `{url}`"
    );
    Ok(())
}

/// Core testable implementation of `alan update`.
pub async fn run_with_options(
    args: UpdateArgs,
    current_version: &str,
    target_triple: &str,
    current_exe: Option<&Path>,
    home_dir: Option<&Path>,
) -> Result<UpdateReport> {
    let current_ver = parse_semver(current_version)
        .with_context(|| format!("invalid current binary version `{current_version}`"))?;
    let requested_ver = args
        .version
        .as_deref()
        .map(parse_semver)
        .transpose()
        .context("invalid `--version` value")?;

    let install_dir = resolve_install_dir(args.install_dir.as_deref(), current_exe, home_dir)?;
    let installed_binary = install_dir.join("alan");
    let manifest_url = resolve_manifest_url(args.manifest_url.as_deref(), args.version.as_deref())?;
    let mut progress = UpdateProgress::new(!args.json && !args.quiet);

    let client = reqwest::Client::builder()
        .user_agent(format!("alan/{current_version}"))
        .build()
        .context("build release HTTP client")?;

    progress.stage("Checking for updates...");
    let manifest = fetch_and_validate_manifest(
        &client,
        &manifest_url,
        target_triple,
        requested_ver.as_ref(),
    )
    .await?;
    let target_artifact = manifest
        .targets
        .get(target_triple)
        .context("validated target artifact missing")?;
    let target_ver = parse_semver(&manifest.version)?;
    let is_newer = target_ver > current_ver;
    let is_same = target_ver == current_ver;
    let is_older = target_ver < current_ver;

    if is_older && !(args.version.is_some() && args.force) {
        bail!(
            "refusing to downgrade `alan` from `{current_version}` to `{}` (pass both `--version {}` and `--force` to allow downgrade)",
            manifest.version,
            manifest.version
        );
    }

    let archive_sha256 = target_artifact.sha256.to_ascii_lowercase();
    let asset = if target_artifact.asset.trim().is_empty() {
        format!("alan-{}-{target_triple}.tar.gz", manifest.version)
    } else {
        target_artifact.asset.clone()
    };

    if args.check {
        let status = if is_newer {
            "update_available"
        } else if is_same {
            "up_to_date"
        } else {
            "downgrade_selected"
        };
        let existing_sha = existing_regular_file_sha256(&installed_binary);
        return Ok(UpdateReport {
            status: status.to_string(),
            current_version: current_version.to_string(),
            target_version: manifest.version,
            tag: manifest.tag,
            target: target_triple.to_string(),
            update_available: is_newer,
            updated: false,
            checked_only: true,
            install_dir: install_dir.display().to_string(),
            installed_binary: installed_binary.display().to_string(),
            manifest_url,
            asset,
            download_url: target_artifact.url.clone(),
            archive_sha256,
            binary_sha256: existing_sha,
            release_url: manifest.release_url,
            notes_url: manifest.notes_url,
            signed: target_artifact.signed,
            notarized: target_artifact.notarized,
            forced: args.force,
            retired_legacy: Vec::new(),
        });
    }

    if is_same && !args.force {
        let existing_sha = existing_regular_file_sha256(&installed_binary);
        return Ok(UpdateReport {
            status: "up_to_date".to_string(),
            current_version: current_version.to_string(),
            target_version: manifest.version,
            tag: manifest.tag,
            target: target_triple.to_string(),
            update_available: false,
            updated: false,
            checked_only: false,
            install_dir: install_dir.display().to_string(),
            installed_binary: installed_binary.display().to_string(),
            manifest_url,
            asset,
            download_url: target_artifact.url.clone(),
            archive_sha256,
            binary_sha256: existing_sha,
            release_url: manifest.release_url,
            notes_url: manifest.notes_url,
            signed: target_artifact.signed,
            notarized: target_artifact.notarized,
            forced: false,
            retired_legacy: Vec::new(),
        });
    }

    let _lock = InstallLock::acquire(&install_dir)?;
    let preflight = preflight_ownership(&install_dir, args.force)?;

    let staging = TempStagingDir::new()?;
    let archive_path = staging.path().join(&asset);
    if let Err(err) = download_and_verify_archive(
        &client,
        &target_artifact.url,
        &asset,
        &archive_sha256,
        target_artifact.size,
        &archive_path,
        &mut progress,
    )
    .await
    {
        progress.finish_on_error();
        return Err(err);
    }

    progress.stage("Verifying archive and extracting...");
    let extracted_bin = inspect_and_extract_alan(&archive_path, staging.path())?;
    progress.stage(&format!("Installing into {}...", install_dir.display()));
    let (new_binary_sha256, retired_legacy) =
        atomic_install_binary(&install_dir, &extracted_bin, &preflight)?;

    Ok(UpdateReport {
        status: "updated".to_string(),
        current_version: current_version.to_string(),
        target_version: manifest.version,
        tag: manifest.tag,
        target: target_triple.to_string(),
        update_available: is_newer,
        updated: true,
        checked_only: false,
        install_dir: install_dir.display().to_string(),
        installed_binary: installed_binary.display().to_string(),
        manifest_url,
        asset,
        download_url: target_artifact.url.clone(),
        archive_sha256,
        binary_sha256: Some(new_binary_sha256),
        release_url: manifest.release_url,
        notes_url: manifest.notes_url,
        signed: target_artifact.signed,
        notarized: target_artifact.notarized,
        forced: args.force,
        retired_legacy,
    })
}

async fn fetch_and_validate_manifest(
    client: &reqwest::Client,
    manifest_url: &str,
    target_triple: &str,
    requested_ver: Option<&SemVer>,
) -> Result<ReleaseManifest> {
    let response = client
        .get(manifest_url)
        .timeout(Duration::from_secs(MANIFEST_FETCH_TIMEOUT_SECS))
        .send()
        .await
        .with_context(|| format!("fetch release manifest from `{manifest_url}`"))?
        .error_for_status()
        .with_context(|| format!("release manifest request failed for `{manifest_url}`"))?;
    let manifest: ReleaseManifest = response
        .json()
        .await
        .with_context(|| format!("parse release manifest JSON from `{manifest_url}`"))?;

    ensure!(
        manifest.schema_version == 1,
        "unsupported release manifest schema_version `{}` (expected 1)",
        manifest.schema_version
    );
    ensure!(
        manifest.product == "alan",
        "unexpected release manifest product `{}` (expected `alan`)",
        manifest.product
    );
    let manifest_ver = parse_semver(&manifest.version)
        .with_context(|| format!("invalid manifest version `{}`", manifest.version))?;
    ensure!(
        !manifest.tag.trim().is_empty(),
        "release manifest `tag` cannot be empty"
    );
    let tag_ver = parse_semver(&manifest.tag)
        .with_context(|| format!("invalid manifest tag `{}`", manifest.tag))?;
    ensure!(
        tag_ver == manifest_ver,
        "release manifest tag `{}` does not match version `{}`",
        manifest.tag,
        manifest.version
    );

    if let Some(req) = requested_ver {
        ensure!(
            &manifest_ver == req,
            "requested version `{req}` does not match release manifest version `{}`",
            manifest.version
        );
    }

    let artifact = manifest.targets.get(target_triple).with_context(|| {
        let available = manifest
            .targets
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "release manifest has no target `{target_triple}` (available: {})",
            if available.is_empty() {
                "<none>"
            } else {
                &available
            }
        )
    })?;

    validate_http_url("target download URL", &artifact.url)?;
    ensure!(
        is_valid_sha256_hex(&artifact.sha256),
        "release manifest target `{target_triple}` has invalid sha256 digest `{}`",
        artifact.sha256
    );

    Ok(manifest)
}

async fn download_and_verify_archive(
    client: &reqwest::Client,
    download_url: &str,
    asset: &str,
    expected_sha256: &str,
    expected_size: u64,
    archive_path: &Path,
    progress: &mut UpdateProgress,
) -> Result<()> {
    let mut response = tokio::time::timeout(
        Duration::from_secs(ARCHIVE_CONNECT_TIMEOUT_SECS),
        client.get(download_url).send(),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "timed out after {ARCHIVE_CONNECT_TIMEOUT_SECS}s connecting to `{download_url}`"
        )
    })?
    .with_context(|| format!("download release archive from `{download_url}`"))?
    .error_for_status()
    .with_context(|| format!("release archive download failed for `{download_url}`"))?;

    let total_bytes = if expected_size > 0 {
        Some(expected_size)
    } else {
        response.content_length()
    };
    progress.start_download(asset, total_bytes);

    let file = std::fs::File::create(archive_path)
        .with_context(|| format!("create staging archive `{}`", archive_path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(ARCHIVE_MAX_TOTAL_TIMEOUT_SECS);
    let idle_limit = Duration::from_secs(ARCHIVE_CHUNK_IDLE_TIMEOUT_SECS);

    loop {
        let now = tokio::time::Instant::now();
        let Some(remaining) = deadline.checked_duration_since(now) else {
            bail!(
                "download of `{download_url}` exceeded {ARCHIVE_MAX_TOTAL_TIMEOUT_SECS}s overall timeout"
            );
        };
        let chunk_timeout = remaining.min(idle_limit);
        let chunk = match tokio::time::timeout(chunk_timeout, response.chunk()).await {
            Ok(Ok(Some(chunk))) => chunk,
            Ok(Ok(None)) => break,
            Ok(Err(err)) => {
                return Err(err)
                    .with_context(|| format!("stream release archive body from `{download_url}`"));
            }
            Err(_) => {
                if remaining <= idle_limit {
                    bail!(
                        "download of `{download_url}` exceeded {ARCHIVE_MAX_TOTAL_TIMEOUT_SECS}s overall timeout"
                    );
                }
                bail!(
                    "download of `{download_url}` stalled (no data received for {ARCHIVE_CHUNK_IDLE_TIMEOUT_SECS}s)"
                );
            }
        };

        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if expected_size > 0 && downloaded > expected_size {
            bail!(
                "downloaded archive size mismatch for `{download_url}` (expected {expected_size} bytes, got more than {downloaded} bytes)"
            );
        }
        writer
            .write_all(&chunk)
            .with_context(|| format!("write staged archive `{}`", archive_path.display()))?;
        hasher.update(&chunk);
        progress.advance_download(asset, downloaded, total_bytes);
    }

    writer
        .flush()
        .with_context(|| format!("flush staged archive `{}`", archive_path.display()))?;
    drop(writer);
    progress.finish_download(asset, downloaded, total_bytes);

    if expected_size > 0 {
        ensure!(
            downloaded == expected_size,
            "downloaded archive size mismatch for `{download_url}` (expected {expected_size} bytes, got {downloaded} bytes)"
        );
    }

    let actual_sha256 = encode_lower_hex(&hasher.finalize());
    ensure!(
        actual_sha256 == expected_sha256,
        "SHA-256 checksum mismatch for `{download_url}` (expected {expected_sha256}, got {actual_sha256})"
    );

    Ok(())
}

fn existing_regular_file_sha256(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_file() && !meta.file_type().is_symlink() {
        sha256_file(path).ok()
    } else {
        None
    }
}

fn print_human_report(report: &UpdateReport) {
    if report.checked_only {
        if report.update_available {
            println!(
                "Update available: {} -> {} ({})",
                display_tag(&report.current_version),
                display_tag(&report.target_version),
                report.target
            );
            println!("Run `alan update` to install into {}.", report.install_dir);
        } else if report.status == "downgrade_selected" {
            println!(
                "Selected version {} is older than installed {}.",
                display_tag(&report.target_version),
                display_tag(&report.current_version)
            );
        } else {
            println!(
                "alan is up to date ({}).",
                display_tag(&report.current_version)
            );
        }
        return;
    }

    if !report.updated {
        println!(
            "alan is already up to date ({}).",
            display_tag(&report.current_version)
        );
        return;
    }

    println!(
        "Updated alan {} -> {} ({}) at {}.",
        display_tag(&report.current_version),
        display_tag(&report.target_version),
        report.target,
        report.installed_binary
    );
    if !report.retired_legacy.is_empty() {
        println!(
            "Retired legacy installation files: {}.",
            report.retired_legacy.join(", ")
        );
    }
}
