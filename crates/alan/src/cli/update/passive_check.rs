//! Non-blocking 24-hour passive update check cached under Alan's System Store.

use super::semver::{current_target_triple, display_tag, is_newer_version, parse_semver};
use super::{
    ReleaseChannel, ReleaseManifest, current_binary_channel, default_channel,
    default_manifest_url_for_channel, validate_manifest_metadata,
};
use alan_os_host::SystemStorePaths;
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

/// Cache lifetime before interactive startup triggers a background refresh.
pub const PASSIVE_CHECK_TTL_HOURS: i64 = 24;
const PASSIVE_CHECK_TIMEOUT_SECS: u64 = 2;
const MAX_CACHE_BYTES: u64 = 64 * 1024;

/// Persisted passive update check entry at `<system_store>/services/updater/last-check.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastCheckCache {
    /// RFC-3339 UTC timestamp when the remote manifest was last fetched.
    pub checked_at_rfc3339: String,
    /// Release channel associated with this cached check (`"stable"` or `"beta"`).
    #[serde(default = "default_channel")]
    pub channel: String,
    /// Latest semantic version reported by the release manifest.
    pub latest_version: String,
    /// Release tag associated with `latest_version` (e.g. `v0.1.1` or `beta`).
    pub tag: String,
    /// Optional web URL for the release notes page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
}

/// Resolve `<system_store>/services/updater/last-check.json`.
pub fn passive_check_cache_path(store: &SystemStorePaths) -> Result<PathBuf> {
    Ok(store.service("updater")?.join("last-check.json"))
}

/// Return `true` when passive startup update checks must be skipped entirely.
pub fn should_skip_passive_check(
    ephemeral: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
    in_test: bool,
) -> bool {
    let no_update_check = std::env::var("ALAN_NO_UPDATE_CHECK").ok();
    let ci = std::env::var("CI").ok();
    should_skip_passive_check_with(
        ephemeral,
        stdin_is_tty,
        stdout_is_tty,
        in_test,
        no_update_check.as_deref(),
        ci.as_deref(),
    )
}

/// Pure helper evaluating passive update check skip rules for explicit environment values.
pub fn should_skip_passive_check_with(
    ephemeral: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
    in_test: bool,
    no_update_check_env: Option<&str>,
    ci_env: Option<&str>,
) -> bool {
    in_test
        || ephemeral
        || !stdin_is_tty
        || !stdout_is_tty
        || str_flag_is_truthy(no_update_check_env)
        || str_flag_is_truthy(ci_env)
}

fn str_flag_is_truthy(value: Option<&str>) -> bool {
    value.is_some_and(|raw| {
        let trimmed = raw.trim();
        !trimmed.is_empty() && trimmed != "0" && !trimmed.eq_ignore_ascii_case("false")
    })
}

/// Read the cached `last-check.json` synchronously (`< 1ms`) and decide whether to show a notice
/// and whether a background refresh is required (`missing`, `invalid`, channel mismatch, or `>= 24h` old).
pub fn evaluate_cache_for_notice(
    cache_path: &Path,
    current_version: &str,
    now: DateTime<Utc>,
) -> (Option<String>, bool) {
    evaluate_cache_for_notice_for_channel(
        cache_path,
        current_version,
        current_binary_channel(),
        now,
    )
}

/// Pure channel-parameterized cache evaluator for deterministic unit and integration testing.
pub fn evaluate_cache_for_notice_for_channel(
    cache_path: &Path,
    current_version: &str,
    current_channel: ReleaseChannel,
    now: DateTime<Utc>,
) -> (Option<String>, bool) {
    let Ok(metadata) = std::fs::symlink_metadata(cache_path) else {
        return (None, true);
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_CACHE_BYTES
    {
        return (None, true);
    }
    let Ok(bytes) = std::fs::read(cache_path) else {
        return (None, true);
    };
    let Ok(cache) = serde_json::from_slice::<LastCheckCache>(&bytes) else {
        return (None, true);
    };
    if cache.channel != current_channel.as_str() {
        return (None, true);
    }
    let Ok(checked_at) = DateTime::parse_from_rfc3339(&cache.checked_at_rfc3339) else {
        return (None, true);
    };
    let checked_at_utc = checked_at.with_timezone(&Utc);
    let age = now.signed_duration_since(checked_at_utc);
    let needs_refresh =
        age >= Duration::hours(PASSIVE_CHECK_TTL_HOURS) || age < Duration::minutes(-5);

    let notice = match (
        parse_semver(&cache.latest_version),
        is_newer_version(current_version, &cache.latest_version),
    ) {
        (Ok(parsed), Ok(true)) => {
            let display = display_tag(&parsed.to_string());
            Some(match current_channel {
                ReleaseChannel::Stable => {
                    format!("Update available: {display} — run 'alan update'")
                }
                ReleaseChannel::Beta => {
                    format!("Beta update available: {display} — run 'alan update'")
                }
            })
        }
        _ => None,
    };

    (notice, needs_refresh)
}

/// Read any cached interactive update notice synchronously and, if stale/missing, spawn a
/// non-blocking ~2s background refresh into System Store (`services/updater/last-check.json`).
pub fn prepare_passive_tui_update_notice(
    store: &SystemStorePaths,
    ephemeral: bool,
) -> Option<String> {
    if should_skip_passive_check(
        ephemeral,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        cfg!(test),
    ) {
        return None;
    }

    let cache_path = passive_check_cache_path(store).ok()?;
    let current_version = env!("ALAN_VERSION");
    let current_channel = current_binary_channel();
    let (notice, needs_refresh) =
        evaluate_cache_for_notice(&cache_path, current_version, Utc::now());

    if needs_refresh && let Ok(handle) = tokio::runtime::Handle::try_current() {
        let (manifest_url, overridden) = default_manifest_url_for_channel(current_channel);
        if let Ok(target_triple) = current_target_triple() {
            handle.spawn(async move {
                let _ = tokio::time::timeout(
                    StdDuration::from_secs(PASSIVE_CHECK_TIMEOUT_SECS),
                    async {
                        if overridden {
                            refresh_passive_cache(&cache_path, &manifest_url, target_triple).await
                        } else {
                            refresh_passive_cache_for_channel(
                                &cache_path,
                                &manifest_url,
                                target_triple,
                                Some(current_channel),
                            )
                            .await
                        }
                    },
                )
                .await;
            });
        }
    }

    notice
}

/// Fetch `manifest_url` with a short timeout and atomically persist `last-check.json`.
pub async fn refresh_passive_cache(
    cache_path: &Path,
    manifest_url: &str,
    target_triple: &str,
) -> Result<LastCheckCache> {
    refresh_passive_cache_for_channel(cache_path, manifest_url, target_triple, None).await
}

/// Fetch `manifest_url` with optional expected channel enforcement and atomically persist `last-check.json`.
pub async fn refresh_passive_cache_for_channel(
    cache_path: &Path,
    manifest_url: &str,
    target_triple: &str,
    expected_channel: Option<ReleaseChannel>,
) -> Result<LastCheckCache> {
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(PASSIVE_CHECK_TIMEOUT_SECS))
        .connect_timeout(StdDuration::from_secs(PASSIVE_CHECK_TIMEOUT_SECS))
        .user_agent(concat!("alan/", env!("ALAN_VERSION")))
        .build()
        .context("build passive update HTTP client")?;
    let response = client
        .get(manifest_url)
        .send()
        .await
        .context("fetch passive release manifest")?
        .error_for_status()
        .context("release manifest returned HTTP error")?;
    let manifest: ReleaseManifest = response
        .json()
        .await
        .context("parse passive release manifest JSON")?;
    let (_, manifest_channel) = validate_manifest_metadata(&manifest, None, expected_channel)?;
    anyhow::ensure!(
        manifest.targets.contains_key(target_triple),
        "passive release manifest does not include `{target_triple}`"
    );

    let cache = LastCheckCache {
        checked_at_rfc3339: Utc::now().to_rfc3339(),
        channel: manifest_channel.as_str().to_string(),
        latest_version: manifest.version,
        tag: manifest.tag,
        release_url: manifest.release_url,
    };
    write_cache_atomic(cache_path, &cache)?;
    Ok(cache)
}

fn write_cache_atomic(cache_path: &Path, cache: &LastCheckCache) -> Result<()> {
    let parent = cache_path
        .parent()
        .context("passive check cache path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create updater cache directory `{}`", parent.display()))?;
    if let Ok(meta) = std::fs::symlink_metadata(cache_path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "refusing to write updater cache over non-regular file `{}`",
            cache_path.display()
        );
    }

    let tmp_path = parent.join(format!(".last-check.tmp.{}", uuid::Uuid::new_v4()));
    let encoded = serde_json::to_vec_pretty(cache).context("serialize passive update cache")?;
    std::fs::write(&tmp_path, encoded)
        .with_context(|| format!("write temporary updater cache `{}`", tmp_path.display()))?;
    if let Err(err) = std::fs::rename(&tmp_path, cache_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "atomically publish updater cache `{}`",
                cache_path.display()
            )
        });
    }
    Ok(())
}
