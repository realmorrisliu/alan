use super::*;
use crate::cli::update::archive::{sha256_bytes, validate_archive_listing};
use crate::cli::update::channel::{
    ChannelSource, configured_channel_path, effective_channel, read_configured_channel,
    run_channel_command_with_store, set_configured_channel, write_configured_channel,
};
use crate::cli::update::ownership::{
    InstallLock, atomic_install_binary, preflight_ownership, resolve_install_dir,
};
use crate::cli::update::passive_check::{
    LastCheckCache, evaluate_cache_for_notice, evaluate_cache_for_notice_for_channel,
    evaluate_cache_for_notice_with_target, passive_check_cache_path,
    should_skip_passive_check_with,
};
use crate::cli::update::semver::{is_newer_version, parse_semver, target_triple_for};
use alan_os_host::SystemStorePaths;
use chrono::{Duration, Utc};
use std::io::Write;
use std::process::{Command, Stdio};

fn build_ustar_archive_gz(entries: &[(&str, &[u8], u8, &str)]) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    for (name, content, typeflag, linkname) in entries {
        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        assert!(name_bytes.len() <= 100, "test entry name too long");
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        header[100..108].copy_from_slice(b"0000755\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        let size_field = format!("{:011o}\0", content.len());
        header[124..136].copy_from_slice(size_field.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].copy_from_slice(b"        ");
        header[156] = *typeflag;
        let link_bytes = linkname.as_bytes();
        assert!(link_bytes.len() <= 100);
        header[157..157 + link_bytes.len()].copy_from_slice(link_bytes);
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");

        let checksum: u32 = header.iter().map(|b| u32::from(*b)).sum();
        let chksum_field = format!("{checksum:06o}\0 ");
        header[148..156].copy_from_slice(chksum_field.as_bytes());

        tar_bytes.extend_from_slice(&header);
        if !content.is_empty() {
            tar_bytes.extend_from_slice(content);
            let rem = content.len() % 512;
            if rem != 0 {
                tar_bytes.extend(std::iter::repeat_n(0u8, 512 - rem));
            }
        }
    }
    tar_bytes.extend(std::iter::repeat_n(0u8, 1024));

    let mut gzip = Command::new("gzip")
        .arg("-c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn gzip");
    gzip.stdin
        .as_mut()
        .expect("gzip stdin")
        .write_all(&tar_bytes)
        .expect("write tar bytes");
    let out = gzip.wait_with_output().expect("wait gzip");
    assert!(out.status.success(), "gzip failed");
    out.stdout
}

#[test]
fn semver_parsing_and_comparison_follow_semver2() {
    assert!(is_newer_version("0.1.0", "0.1.1").unwrap());
    assert!(is_newer_version("v0.1.0", "v0.2.0").unwrap());
    assert!(is_newer_version("0.1.0-alpha.1", "0.1.0").unwrap());
    assert!(is_newer_version("0.1.0-alpha.1", "0.1.0-alpha.beta").unwrap());
    assert!(is_newer_version("0.1.0-1", "0.1.0-alpha").unwrap());
    assert!(!is_newer_version("0.1.0", "0.1.0").unwrap());
    assert!(!is_newer_version("0.1.0", "0.1.0+build.5").unwrap());
    assert!(!is_newer_version("0.2.0", "0.1.9").unwrap());

    assert_eq!(parse_semver("v1.2.3-rc.1+sha").unwrap().major, 1);
    assert!(parse_semver("").is_err());
    assert!(parse_semver("01.0.0").is_err());
    assert!(parse_semver("1.2").is_err());
    assert!(parse_semver("1.2.3-").is_err());
    assert!(parse_semver("1.2.3-01").is_err());
}

#[test]
fn target_triple_mapping_covers_supported_platforms_only() {
    assert_eq!(
        target_triple_for("macos", "aarch64").unwrap(),
        "aarch64-apple-darwin"
    );
    assert_eq!(
        target_triple_for("macos", "x86_64").unwrap(),
        "x86_64-apple-darwin"
    );
    assert_eq!(
        target_triple_for("linux", "x86_64").unwrap(),
        "x86_64-unknown-linux-gnu"
    );
    assert_eq!(
        target_triple_for("linux", "aarch64").unwrap(),
        "aarch64-unknown-linux-gnu"
    );
    assert!(target_triple_for("windows", "x86_64").is_err());
    assert!(target_triple_for("linux", "riscv64").is_err());
}

#[test]
fn manifest_url_resolution_precedence_works() {
    assert_eq!(
        resolve_manifest_url_with(
            Some("https://mirror.example/latest.json"),
            Some("0.2.0"),
            Some(ReleaseChannel::Beta),
            Some("https://env.example/latest.json"),
            Some("https://rel.example/latest.json"),
            None,
            Some("beta"),
            ReleaseChannel::Stable,
        )
        .unwrap(),
        "https://mirror.example/latest.json"
    );
    assert_eq!(
        resolve_manifest_url_with(
            None,
            None,
            Some(ReleaseChannel::Beta),
            Some("http://127.0.0.1:9000/a.json"),
            Some("http://127.0.0.1:9000/b.json"),
            None,
            Some("beta"),
            ReleaseChannel::Stable,
        )
        .unwrap(),
        "http://127.0.0.1:9000/a.json"
    );
    assert_eq!(
        resolve_manifest_url_with(
            None,
            Some("0.3.0"),
            Some(ReleaseChannel::Beta),
            None,
            None,
            Some("http://127.0.0.1:9000/releases/download/"),
            Some("beta"),
            ReleaseChannel::Stable,
        )
        .unwrap(),
        "http://127.0.0.1:9000/releases/download/v0.3.0/latest.json"
    );
    assert_eq!(
        resolve_manifest_url_with(
            None,
            None,
            Some(ReleaseChannel::Beta),
            None,
            None,
            None,
            Some("stable"),
            ReleaseChannel::Stable,
        )
        .unwrap(),
        DEFAULT_BETA_MANIFEST_URL
    );
    assert_eq!(
        resolve_manifest_url_with(
            None,
            None,
            None,
            None,
            None,
            None,
            Some("beta"),
            ReleaseChannel::Stable,
        )
        .unwrap(),
        DEFAULT_BETA_MANIFEST_URL
    );
    assert_eq!(
        resolve_manifest_url_with(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            ReleaseChannel::Beta,
        )
        .unwrap(),
        DEFAULT_BETA_MANIFEST_URL
    );
    assert_eq!(
        resolve_manifest_url_with(
            None,
            None,
            Some(ReleaseChannel::Stable),
            None,
            None,
            None,
            Some("beta"),
            ReleaseChannel::Beta,
        )
        .unwrap(),
        DEFAULT_MANIFEST_URL
    );
    assert_eq!(
        resolve_manifest_url_with(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(""),
            ReleaseChannel::Stable,
        )
        .unwrap(),
        DEFAULT_MANIFEST_URL
    );
    assert!(
        resolve_manifest_url_with(
            None,
            None,
            None,
            None,
            None,
            None,
            Some("nightly"),
            ReleaseChannel::Stable,
        )
        .is_err()
    );
}

#[test]
fn archive_listing_validation_rejects_traversal_absolute_and_missing_alan() {
    assert_eq!(
        validate_archive_listing("./\n./alan\n./manifest.json\n").unwrap(),
        "./alan"
    );
    assert_eq!(
        validate_archive_listing("alan\nmanifest.json\n").unwrap(),
        "alan"
    );
    assert!(validate_archive_listing("./alan\n../evil\n").is_err());
    assert!(validate_archive_listing("./alan\nsub/../../evil\n").is_err());
    assert!(validate_archive_listing("./alan\n/etc/passwd\n").is_err());
    assert!(validate_archive_listing("./alan\nC:\\evil\n").is_err());
    assert!(validate_archive_listing("./manifest.json\n").is_err());
    assert!(validate_archive_listing("").is_err());
}

#[test]
fn real_tar_extraction_rejects_symlink_and_path_traversal_entries() {
    let staging = TempStagingDir::new().unwrap();

    let traversal_gz = build_ustar_archive_gz(&[
        ("./alan", b"#!/bin/sh\necho ok\n", b'0', ""),
        ("../escape", b"bad", b'0', ""),
    ]);
    let traversal_path = staging.path().join("traversal.tar.gz");
    std::fs::write(&traversal_path, traversal_gz).unwrap();
    let err = inspect_and_extract_alan(&traversal_path, staging.path()).unwrap_err();
    assert!(err.to_string().contains(".."), "{err:#}");

    let symlink_gz = build_ustar_archive_gz(&[("./alan", b"", b'2', "/etc/passwd")]);
    let symlink_path = staging.path().join("symlink.tar.gz");
    std::fs::write(&symlink_path, symlink_gz).unwrap();
    let err = inspect_and_extract_alan(&symlink_path, staging.path()).unwrap_err();
    assert!(
        err.to_string().contains("regular file") || err.to_string().contains("inspect"),
        "{err:#}"
    );
}

#[cfg(unix)]
#[test]
fn resolve_install_dir_resolves_symlinks_and_falls_back_to_local_bin() {
    let temp = tempfile::tempdir().unwrap();
    let real_bin_dir = temp.path().join("real-bin");
    let link_dir = temp.path().join("link-bin");
    std::fs::create_dir_all(&real_bin_dir).unwrap();
    std::fs::create_dir_all(&link_dir).unwrap();

    let real_exe = real_bin_dir.join("alan");
    std::fs::write(&real_exe, b"bin").unwrap();
    let link_exe = link_dir.join("alan");
    std::os::unix::fs::symlink(&real_exe, &link_exe).unwrap();

    let resolved = resolve_install_dir(None, Some(&link_exe), Some(temp.path())).unwrap();
    assert_eq!(resolved, std::fs::canonicalize(&real_bin_dir).unwrap());

    let fallback = resolve_install_dir(None, None, Some(temp.path())).unwrap();
    assert_eq!(
        fallback,
        std::fs::canonicalize(temp.path())
            .unwrap()
            .join(".local/bin")
    );
}

#[test]
fn ownership_preflight_lock_and_legacy_retirement_work_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path();

    // Unowned existing binary refused unless `--force`.
    let old_bytes = b"#!/bin/sh\necho old\n";
    let alan_path = install_dir.join("alan");
    std::fs::write(&alan_path, old_bytes).unwrap();
    let err = preflight_ownership(install_dir, false).unwrap_err();
    assert!(err.to_string().contains("unowned"), "{err:#}");
    assert!(preflight_ownership(install_dir, true).is_ok());

    // Owned legacy stable + dev receipts schedule retirement of legacy binaries and manifests.
    let old_sha = sha256_bytes(old_bytes);
    let legacy_host_bytes = b"legacy-host";
    let legacy_dev_bytes = b"legacy-dev";
    std::fs::write(install_dir.join("alan-os-host"), legacy_host_bytes).unwrap();
    std::fs::write(install_dir.join("alan-dev"), legacy_dev_bytes).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest-stable"),
        format!(
            "alan|{old_sha}\nalan-os-host|{}\n",
            sha256_bytes(legacy_host_bytes)
        ),
    )
    .unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest-dev"),
        format!("alan-dev|{}\n", sha256_bytes(legacy_dev_bytes)),
    )
    .unwrap();

    let _lock = InstallLock::acquire(install_dir).unwrap();
    assert!(install_dir.join(".alan-cli-install.lock").is_dir());
    assert!(InstallLock::acquire(install_dir).is_err());

    let preflight = preflight_ownership(install_dir, false).unwrap();
    assert!(
        preflight
            .retired_legacy_binaries
            .contains(&"alan-os-host".to_string())
    );
    assert!(
        preflight
            .retired_legacy_binaries
            .contains(&"alan-dev".to_string())
    );
    assert!(
        preflight
            .retired_legacy_manifests
            .contains(&".alan-cli-manifest-stable".to_string())
    );
    assert!(
        preflight
            .retired_legacy_manifests
            .contains(&".alan-cli-manifest-dev".to_string())
    );

    let extracted_src = dir.path().join("new-alan");
    let new_bytes = b"#!/bin/sh\necho new\n";
    std::fs::write(&extracted_src, new_bytes).unwrap();
    let (new_sha, retired) =
        atomic_install_binary(install_dir, &extracted_src, &preflight).unwrap();
    drop(_lock);

    assert!(!install_dir.join(".alan-cli-install.lock").exists());
    assert_eq!(new_sha, sha256_bytes(new_bytes));
    assert_eq!(std::fs::read(&alan_path).unwrap(), new_bytes);
    assert_eq!(
        std::fs::read_to_string(install_dir.join(".alan-cli-manifest")).unwrap(),
        format!("alan|{new_sha}\n")
    );
    assert!(!install_dir.join("alan-os-host").exists());
    assert!(!install_dir.join("alan-dev").exists());
    assert!(!install_dir.join(".alan-cli-manifest-stable").exists());
    assert!(!install_dir.join(".alan-cli-manifest-dev").exists());
    assert_eq!(retired.len(), 4);
}

#[test]
fn passive_check_skip_conditions_and_24h_cache_evaluation_work() {
    assert!(!should_skip_passive_check_with(
        false, true, true, false, None, None
    ));
    assert!(should_skip_passive_check_with(
        true, true, true, false, None, None
    ));
    assert!(should_skip_passive_check_with(
        false, false, true, false, None, None
    ));
    assert!(should_skip_passive_check_with(
        false, true, false, false, None, None
    ));
    assert!(should_skip_passive_check_with(
        false, true, true, true, None, None
    ));
    assert!(should_skip_passive_check_with(
        false,
        true,
        true,
        false,
        Some("1"),
        None
    ));
    assert!(should_skip_passive_check_with(
        false,
        true,
        true,
        false,
        None,
        Some("true")
    ));
    assert!(should_skip_passive_check_with(
        false,
        true,
        true,
        false,
        None,
        Some("1")
    ));
    assert!(!should_skip_passive_check_with(
        false,
        true,
        true,
        false,
        Some("0"),
        Some("false")
    ));

    let temp = tempfile::tempdir().unwrap();
    let store = SystemStorePaths::from_data_dir(temp.path()).unwrap();
    let cache_path = passive_check_cache_path(&store).unwrap();
    assert_eq!(
        cache_path,
        temp.path()
            .join("Alan/System Store/services/updater/last-check.json")
    );

    let now = Utc::now();
    // Missing cache -> no notice, needs refresh.
    assert_eq!(
        evaluate_cache_for_notice(&cache_path, "0.1.0", now),
        (None, true)
    );

    // Fresh cache with newer version -> shows notice, no refresh.
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    let fresh = LastCheckCache {
        checked_at_rfc3339: (now - Duration::hours(2)).to_rfc3339(),
        channel: "stable".to_string(),
        latest_version: "0.1.1".to_string(),
        tag: "v0.1.1".to_string(),
        release_url: Some(
            "https://github.com/thinksmart-systems/alan/releases/tag/v0.1.1".to_string(),
        ),
    };
    std::fs::write(&cache_path, serde_json::to_vec(&fresh).unwrap()).unwrap();
    assert_eq!(
        evaluate_cache_for_notice(&cache_path, "0.1.0", now),
        (
            Some("Update available: v0.1.1 — run 'alan update'".to_string()),
            false
        )
    );
    // Switching binary channel invalidates cache and suppresses stale-channel notice.
    assert_eq!(
        evaluate_cache_for_notice_for_channel(&cache_path, "0.1.0", ReleaseChannel::Beta, now),
        (None, true)
    );
    // Already on 0.1.1 -> no notice, no refresh.
    assert_eq!(
        evaluate_cache_for_notice(&cache_path, "0.1.1", now),
        (None, false)
    );

    // >24h stale cache -> needs refresh.
    let stale = LastCheckCache {
        checked_at_rfc3339: (now - Duration::hours(25)).to_rfc3339(),
        channel: "stable".to_string(),
        latest_version: "0.1.2".to_string(),
        tag: "v0.1.2".to_string(),
        release_url: None,
    };
    std::fs::write(&cache_path, serde_json::to_vec(&stale).unwrap()).unwrap();
    assert_eq!(
        evaluate_cache_for_notice(&cache_path, "0.1.0", now),
        (
            Some("Update available: v0.1.2 — run 'alan update'".to_string()),
            true
        )
    );

    // Beta cache formats Beta update notice and rejects mismatched stable binary.
    let beta_fresh = LastCheckCache {
        checked_at_rfc3339: (now - Duration::hours(1)).to_rfc3339(),
        channel: "beta".to_string(),
        latest_version: "0.1.6-beta.20261123.42".to_string(),
        tag: "beta".to_string(),
        release_url: Some(
            "https://github.com/thinksmart-systems/alan/releases/tag/beta".to_string(),
        ),
    };
    std::fs::write(&cache_path, serde_json::to_vec(&beta_fresh).unwrap()).unwrap();
    assert_eq!(
        evaluate_cache_for_notice_for_channel(
            &cache_path,
            "0.1.6-beta.20261123.41",
            ReleaseChannel::Beta,
            now
        ),
        (
            Some("Beta update available: v0.1.6-beta.20261123.42 — run 'alan update'".to_string()),
            false
        )
    );
    assert_eq!(
        evaluate_cache_for_notice_for_channel(&cache_path, "0.1.5", ReleaseChannel::Stable, now),
        (None, true)
    );

    // Legacy cache JSON without `channel` defaults to `"stable"`.
    std::fs::write(
        &cache_path,
        serde_json::json!({
            "checked_at_rfc3339": (now - Duration::hours(1)).to_rfc3339(),
            "latest_version": "0.1.3",
            "tag": "v0.1.3"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        evaluate_cache_for_notice_for_channel(&cache_path, "0.1.0", ReleaseChannel::Stable, now),
        (
            Some("Update available: v0.1.3 — run 'alan update'".to_string()),
            false
        )
    );
    assert_eq!(
        evaluate_cache_for_notice_for_channel(&cache_path, "0.1.0", ReleaseChannel::Beta, now),
        (None, true)
    );

    // Corrupt cache -> no notice, needs refresh.
    std::fs::write(&cache_path, b"{not-json").unwrap();
    assert_eq!(
        evaluate_cache_for_notice(&cache_path, "0.1.0", now),
        (None, true)
    );
}

#[test]
fn beta_semver_ordering_and_manifest_validation_work() {
    assert!(is_newer_version("0.1.5", "0.1.6-beta.20261123.1").unwrap());
    assert!(is_newer_version("0.1.6-beta.20261123.1", "0.1.6-beta.20261123.2").unwrap());
    assert!(is_newer_version("0.1.6-beta.20261123.2", "0.1.6-beta.20261123.10").unwrap());
    assert!(is_newer_version("0.1.6-beta.20261123.10", "0.1.6").unwrap());

    let mut manifest = ReleaseManifest {
        schema_version: 1,
        product: "alan".to_string(),
        channel: "beta".to_string(),
        version: "0.1.6-beta.20261123.42".to_string(),
        tag: "beta".to_string(),
        commit: Some("6c1dfd7".to_string()),
        published_at: None,
        release_url: None,
        notes_url: None,
        installer_url: None,
        targets: BTreeMap::new(),
    };

    assert_eq!(
        validate_manifest_metadata(&manifest, None, Some(ReleaseChannel::Beta))
            .unwrap()
            .1,
        ReleaseChannel::Beta
    );
    assert!(validate_manifest_metadata(&manifest, None, Some(ReleaseChannel::Stable)).is_err());

    manifest.tag = "v0.1.6-beta.20261123.42".to_string();
    assert!(validate_manifest_metadata(&manifest, None, Some(ReleaseChannel::Beta)).is_ok());

    manifest.tag = "v0.1.5".to_string();
    assert!(validate_manifest_metadata(&manifest, None, None).is_err());

    manifest.tag = "beta".to_string();
    manifest.version = "0.1.6".to_string();
    assert!(validate_manifest_metadata(&manifest, None, None).is_err());

    manifest.channel = "nightly".to_string();
    assert!(validate_manifest_metadata(&manifest, None, None).is_err());
}

#[test]
fn persisted_channel_precedence_storage_and_cache_invalidation_work() {
    // 1. Precedence: --channel / ALAN_CHANNEL > configured_channel > current_channel.
    assert_eq!(
        resolve_manifest_url_with_configured(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(ReleaseChannel::Beta),
            ReleaseChannel::Stable,
        )
        .unwrap(),
        DEFAULT_BETA_MANIFEST_URL
    );
    assert_eq!(
        resolve_manifest_url_with_configured(
            None,
            None,
            None,
            None,
            None,
            None,
            Some("stable"),
            Some(ReleaseChannel::Beta),
            ReleaseChannel::Beta,
        )
        .unwrap(),
        DEFAULT_MANIFEST_URL
    );

    // 2. Store persistence + cache invalidation + symlink refusal.
    let temp = tempfile::tempdir().unwrap();
    let store = SystemStorePaths::from_data_dir(temp.path()).unwrap();
    let channel_path = configured_channel_path(&store).unwrap();
    let cache_path = passive_check_cache_path(&store).unwrap();

    assert_eq!(read_configured_channel(&store), None);
    assert_eq!(
        effective_channel(Some(&store)),
        (current_binary_channel(), ChannelSource::BinaryDefault)
    );

    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    let now = Utc::now();
    let stale_cache = LastCheckCache {
        checked_at_rfc3339: (now - Duration::hours(1)).to_rfc3339(),
        channel: "stable".to_string(),
        latest_version: "0.1.6".to_string(),
        tag: "v0.1.6".to_string(),
        release_url: None,
    };
    std::fs::write(&cache_path, serde_json::to_vec(&stale_cache).unwrap()).unwrap();
    assert!(cache_path.exists());

    write_configured_channel(&store, ReleaseChannel::Beta).unwrap();
    assert_eq!(read_configured_channel(&store), Some(ReleaseChannel::Beta));
    assert_eq!(
        effective_channel(Some(&store)),
        (ReleaseChannel::Beta, ChannelSource::Configured)
    );
    assert!(
        !cache_path.exists(),
        "changing channel must remove stale last-check.json"
    );

    // Corrupt channel.json degrades safely to None.
    std::fs::write(&channel_path, b"{bad-json").unwrap();
    assert_eq!(read_configured_channel(&store), None);

    // Writing over symlink is refused.
    #[cfg(unix)]
    {
        let victim = temp.path().join("victim.json");
        std::fs::write(&victim, b"keep").unwrap();
        std::fs::remove_file(&channel_path).unwrap();
        std::os::unix::fs::symlink(&victim, &channel_path).unwrap();
        assert!(write_configured_channel(&store, ReleaseChannel::Stable).is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        std::fs::remove_file(&channel_path).unwrap();
    }

    // 3. Channel helper & pending-switch passive notice.
    let matches_binary = set_configured_channel(&store, ReleaseChannel::Beta).unwrap();
    assert_eq!(
        matches_binary,
        current_binary_channel() == ReleaseChannel::Beta
    );
    let status =
        run_channel_command_with_store(&store, None, ReleaseChannel::Stable, "0.1.6").unwrap();
    assert_eq!(status.channel, "beta");
    assert_eq!(status.source, "configured");
    assert!(status.switch_pending);

    let beta_cache = LastCheckCache {
        checked_at_rfc3339: (now - Duration::minutes(10)).to_rfc3339(),
        channel: "beta".to_string(),
        latest_version: "0.1.7-beta.20261123.5".to_string(),
        tag: "beta".to_string(),
        release_url: None,
    };
    std::fs::write(&cache_path, serde_json::to_vec(&beta_cache).unwrap()).unwrap();
    let (notice, needs_refresh) = evaluate_cache_for_notice_with_target(
        &cache_path,
        "0.1.6",
        ReleaseChannel::Stable,
        ReleaseChannel::Beta,
        now,
    );
    assert_eq!(
        notice.as_deref(),
        Some("Channel switch pending (beta: v0.1.7-beta.20261123.5) — run 'alan update'")
    );
    assert!(!needs_refresh);
}
