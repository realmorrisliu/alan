//! Persisted release channel selection (`<system_store>/services/updater/channel.json`)
//! and `alan update channel` subcommand.

use super::semver::display_tag;
use super::{ReleaseChannel, UpdateReport, current_binary_channel};
use alan_os_host::SystemStorePaths;
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CHANNEL_FILE_NAME: &str = "channel.json";
const MAX_CHANNEL_BYTES: u64 = 4096;

/// Subcommands under `alan update`.
#[derive(Debug, Clone, Subcommand)]
pub enum UpdateCommand {
    /// Show or select the persisted release channel (`stable` or `beta`)
    Channel {
        /// Release channel to select (`stable` or `beta`); omit to show current selection
        #[arg(value_enum)]
        channel: Option<ReleaseChannel>,
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,
    },
}

/// Source of the active effective release channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelSource {
    /// Explicitly saved in `<system_store>/services/updater/channel.json`.
    Configured,
    /// Inherited from the compiled channel of the running `alan` binary.
    BinaryDefault,
}

impl ChannelSource {
    /// Lowercase machine-readable identifier for JSON reports and display.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::BinaryDefault => "binary_default",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedChannel {
    channel: String,
}

/// Structured status output emitted for `alan update channel [--json]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelStatusReport {
    /// Effective release channel (`"stable"` or `"beta"`).
    pub channel: String,
    /// Where the effective channel came from (`"configured"` or `"binary_default"`).
    pub source: String,
    /// Release channel compiled into the running `alan` binary.
    pub binary_channel: String,
    /// Version compiled into the running `alan` binary.
    pub binary_version: String,
    /// Whether the effective channel differs from the running binary's compiled channel.
    pub switch_pending: bool,
}

/// Resolve `<system_store>/services/updater/channel.json`.
pub fn configured_channel_path(store: &SystemStorePaths) -> Result<PathBuf> {
    Ok(store.service("updater")?.join(CHANNEL_FILE_NAME))
}

/// Read the persisted release channel selection, ignoring missing or invalid files cleanly.
pub fn read_configured_channel(store: &SystemStorePaths) -> Option<ReleaseChannel> {
    let path = configured_channel_path(store).ok()?;
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_CHANNEL_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let parsed = serde_json::from_slice::<PersistedChannel>(&bytes).ok()?;
    ReleaseChannel::parse(&parsed.channel).ok()
}

/// Atomically persist the selected release channel to `<system_store>/services/updater/channel.json`
/// and invalidate any cached `last-check.json` passive update notice.
pub fn write_configured_channel(store: &SystemStorePaths, channel: ReleaseChannel) -> Result<()> {
    let channel_path = configured_channel_path(store)?;
    let parent = channel_path
        .parent()
        .context("configured channel path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create updater config directory `{}`", parent.display()))?;
    if let Ok(meta) = std::fs::symlink_metadata(&channel_path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "refusing to overwrite non-regular channel file `{}`",
            channel_path.display()
        );
    }

    let tmp_path = parent.join(format!(".channel.tmp.{}", uuid::Uuid::new_v4()));
    let encoded = serde_json::to_vec_pretty(&PersistedChannel {
        channel: channel.as_str().to_string(),
    })
    .context("serialize configured release channel")?;
    std::fs::write(&tmp_path, encoded)
        .with_context(|| format!("write temporary channel config `{}`", tmp_path.display()))?;
    if let Err(err) = std::fs::rename(&tmp_path, &channel_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "atomically publish channel config `{}`",
                channel_path.display()
            )
        });
    }

    let cache_path = parent.join("last-check.json");
    remove_regular_file_if_present(&cache_path);
    Ok(())
}

fn remove_regular_file_if_present(path: &Path) {
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.is_file()
        && !meta.file_type().is_symlink()
    {
        let _ = std::fs::remove_file(path);
    }
}

/// Return the effective release channel and whether it came from `channel.json` or the binary.
pub fn effective_channel(store: Option<&SystemStorePaths>) -> (ReleaseChannel, ChannelSource) {
    if let Some(store) = store
        && let Some(configured) = read_configured_channel(store)
    {
        return (configured, ChannelSource::Configured);
    }
    (current_binary_channel(), ChannelSource::BinaryDefault)
}

/// Persist `channel` in `store` and return whether it already matches [`current_binary_channel()`].
pub fn set_configured_channel(store: &SystemStorePaths, channel: ReleaseChannel) -> Result<bool> {
    write_configured_channel(store, channel)?;
    Ok(channel == current_binary_channel())
}

pub(super) fn run_channel_command(channel: Option<ReleaseChannel>, json: bool) -> Result<()> {
    let store = SystemStorePaths::detect()?;
    let report = run_channel_command_with_store(
        &store,
        channel,
        current_binary_channel(),
        env!("ALAN_VERSION"),
    )?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_channel_report(&report, channel.is_some());
    }
    Ok(())
}

pub(crate) fn run_channel_command_with_store(
    store: &SystemStorePaths,
    selected: Option<ReleaseChannel>,
    binary_channel: ReleaseChannel,
    binary_version: &str,
) -> Result<ChannelStatusReport> {
    let (channel, source) = match selected {
        Some(chosen) => {
            write_configured_channel(store, chosen)?;
            (chosen, ChannelSource::Configured)
        }
        None => match read_configured_channel(store) {
            Some(configured) => (configured, ChannelSource::Configured),
            None => (binary_channel, ChannelSource::BinaryDefault),
        },
    };
    Ok(ChannelStatusReport {
        channel: channel.as_str().to_string(),
        source: source.as_str().to_string(),
        binary_channel: binary_channel.as_str().to_string(),
        binary_version: binary_version.to_string(),
        switch_pending: channel != binary_channel,
    })
}

fn print_channel_report(report: &ChannelStatusReport, updated_selection: bool) {
    let display_ver = display_tag(&report.binary_version);
    if updated_selection {
        if report.switch_pending {
            println!(
                "Release channel set to {} (active binary is {} {}). Run 'alan update' to switch.",
                report.channel, report.binary_channel, display_ver
            );
        } else {
            println!(
                "Release channel set to {} (matches active binary {}).",
                report.channel, display_ver
            );
        }
        return;
    }

    let source_label = if report.source == ChannelSource::Configured.as_str() {
        "configured"
    } else {
        "binary default"
    };
    println!("Release channel: {} ({source_label})", report.channel);
    println!("Active binary: {} {}", report.binary_channel, display_ver);
    if report.switch_pending {
        println!("Run 'alan update' to switch to {}.", report.channel);
    }
}

pub(super) fn print_human_report(report: &UpdateReport) {
    if report.checked_only {
        if report.update_available {
            if let Some(prev) = &report.previous_channel {
                println!(
                    "Update available (channel switch {prev} -> {}): {} -> {} ({})",
                    report.channel,
                    display_tag(&report.current_version),
                    display_tag(&report.target_version),
                    report.target
                );
                println!(
                    "Run `alan update --channel {}` to install into {}.",
                    report.channel, report.install_dir
                );
            } else {
                println!(
                    "Update available ({} channel): {} -> {} ({})",
                    report.channel,
                    display_tag(&report.current_version),
                    display_tag(&report.target_version),
                    report.target
                );
                println!("Run `alan update` to install into {}.", report.install_dir);
            }
        } else if report.status == "channel_switch" {
            let prev = report.previous_channel.as_deref().unwrap_or("current");
            println!(
                "Channel switch available ({prev} -> {}): {} -> {} ({})",
                report.channel,
                display_tag(&report.current_version),
                display_tag(&report.target_version),
                report.target
            );
            println!(
                "Run `alan update --channel {}` to install into {}.",
                report.channel, report.install_dir
            );
        } else if report.status == "downgrade_selected" {
            println!(
                "Selected version {} is older than installed {}.",
                display_tag(&report.target_version),
                display_tag(&report.current_version)
            );
        } else {
            println!(
                "alan is up to date ({}, {} channel).",
                display_tag(&report.current_version),
                report.channel
            );
        }
        return;
    }

    if !report.updated {
        println!(
            "alan is already up to date ({}, {} channel).",
            display_tag(&report.current_version),
            report.channel
        );
        return;
    }

    if let Some(prev) = &report.previous_channel {
        println!(
            "Updated alan {} -> {} ({}, switched {prev} -> {} channel) at {}.",
            display_tag(&report.current_version),
            display_tag(&report.target_version),
            report.target,
            report.channel,
            report.installed_binary
        );
    } else {
        println!(
            "Updated alan {} -> {} ({}, {} channel) at {}.",
            display_tag(&report.current_version),
            display_tag(&report.target_version),
            report.target,
            report.channel,
            report.installed_binary
        );
    }
    if !report.retired_legacy.is_empty() {
        println!(
            "Retired legacy installation files: {}.",
            report.retired_legacy.join(", ")
        );
    }
}
