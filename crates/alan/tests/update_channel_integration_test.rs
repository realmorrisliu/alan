use std::collections::HashMap;
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use alan::cli::update::passive_check::{
    evaluate_cache_for_notice_with_target, passive_check_cache_path, refresh_passive_cache,
};
use alan::cli::update::{ReleaseChannel, UpdateArgs, run_with_options_and_configured_channel};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone)]
struct MockResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

struct MockHttpServer {
    addr: SocketAddr,
    archive_hits: Arc<AtomicUsize>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MockHttpServer {
    async fn start(routes: HashMap<String, MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock TcpListener");
        let addr = listener.local_addr().expect("local addr");
        let routes = Arc::new(routes);
        let archive_hits = Arc::new(AtomicUsize::new(0));
        let hits = Arc::clone(&archive_hits);
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { break };
                        let routes = Arc::clone(&routes);
                        let hits = Arc::clone(&hits);
                        tokio::spawn(async move {
                            let mut buf = Vec::with_capacity(2048);
                            let mut chunk = [0u8; 1024];
                            loop {
                                match socket.read(&mut chunk).await {
                                    Ok(0) | Err(_) => return,
                                    Ok(n) => {
                                        buf.extend_from_slice(&chunk[..n]);
                                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                            break;
                                        }
                                    }
                                }
                            }
                            let req = String::from_utf8_lossy(&buf);
                            let path = req
                                .lines()
                                .next()
                                .and_then(|line| line.split_whitespace().nth(1))
                                .unwrap_or("/")
                                .to_string();
                            if path.ends_with(".tar.gz") {
                                hits.fetch_add(1, Ordering::SeqCst);
                            }
                            let resp = routes.get(&path).cloned().unwrap_or(MockResponse {
                                status: 404,
                                content_type: "text/plain",
                                body: b"not found".to_vec(),
                            });
                            let reason = match resp.status {
                                200 => "OK",
                                404 => "Not Found",
                                _ => "Error",
                            };
                            let header = format!(
                                "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                resp.status,
                                resp.content_type,
                                resp.body.len()
                            );
                            let _ = socket.write_all(header.as_bytes()).await;
                            let _ = socket.write_all(&resp.body).await;
                            let _ = socket.flush().await;
                        });
                    }
                }
            }
        });

        Self {
            addr,
            archive_hits,
            shutdown: Some(tx),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    fn archive_requests(&self) -> usize {
        self.archive_hits.load(Ordering::SeqCst)
    }
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn current_target() -> &'static str {
    alan::cli::update::semver::target_triple_for(std::env::consts::OS, std::env::consts::ARCH)
        .expect("integration test host must be a supported target")
}

fn build_tar_gz(alan_bytes: &[u8]) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("tempdir");
    let stage = dir.path().join("stage");
    std::fs::create_dir_all(&stage).expect("create stage");
    let alan_path = stage.join("alan");
    std::fs::write(&alan_path, alan_bytes).expect("write alan");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&alan_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod alan");
    }
    std::fs::write(stage.join("manifest.json"), br#"{"product":"alan-cli"}"#)
        .expect("write manifest.json");
    let archive = dir.path().join("release.tar.gz");
    let out = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&stage)
        .arg(".")
        .output()
        .expect("run tar -czf");
    assert!(
        out.status.success(),
        "tar -czf failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read(&archive).expect("read release.tar.gz")
}

struct ManifestSpec<'a> {
    channel: &'a str,
    version: &'a str,
    tag: &'a str,
    commit: Option<&'a str>,
    target: &'a str,
    asset: &'a str,
    archive_url: &'a str,
    archive_sha256: &'a str,
    archive_size: usize,
}

fn manifest_bytes(spec: ManifestSpec<'_>) -> Vec<u8> {
    let mut payload = serde_json::json!({
        "schema_version": 1,
        "product": "alan",
        "channel": spec.channel,
        "version": spec.version,
        "tag": spec.tag,
        "published_at": "2026-11-23T00:00:00Z",
        "release_url": format!("https://github.com/realmorrisliu/alan/releases/tag/{}", spec.tag),
        "notes_url": format!("https://github.com/realmorrisliu/alan/releases/tag/{}", spec.tag),
        "installer_url": "https://alanworks.app/install.sh",
        "targets": {
            spec.target: {
                "asset": spec.asset,
                "url": spec.archive_url,
                "sha256": spec.archive_sha256,
                "size": spec.archive_size,
                "signed": true,
                "notarized": true,
            }
        }
    });
    if let Some(sha) = spec.commit {
        payload["commit"] = serde_json::json!(sha);
    }
    serde_json::to_vec(&payload).expect("serialize manifest")
}

async fn run_alan_update(home: &Path, args: &[String], extra_env: &[(&str, &str)]) -> Output {
    let home = home.to_path_buf();
    let args = args.to_vec();
    let extra_env: Vec<(String, String)> = extra_env
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_alan"));
        cmd.env("HOME", &home)
            .env("XDG_DATA_HOME", home.join("data"))
            .env_remove("ALAN_INSTALL_CHANNEL")
            .env_remove("ALAN_CHANNEL")
            .env_remove("ALAN_MANIFEST_URL")
            .env_remove("ALAN_RELEASE_MANIFEST_URL")
            .env_remove("ALAN_RELEASE_BASE_URL")
            .arg("update")
            .args(&args);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        cmd.output().expect("run alan update")
    })
    .await
    .expect("spawn_blocking")
}

#[test]
fn compiled_version_and_channel_flags_are_exposed() {
    let long = Command::new(env!("CARGO_BIN_EXE_alan"))
        .arg("--version")
        .output()
        .expect("run alan --version");
    assert!(long.status.success());
    assert_eq!(
        String::from_utf8_lossy(&long.stdout).trim(),
        format!("alan {}", env!("ALAN_LONG_VERSION"))
    );

    let short = Command::new(env!("CARGO_BIN_EXE_alan"))
        .arg("-V")
        .output()
        .expect("run alan -V");
    assert!(short.status.success());
    assert_eq!(
        String::from_utf8_lossy(&short.stdout).trim(),
        format!("alan {}", env!("ALAN_VERSION"))
    );

    let help = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["update", "--help"])
        .output()
        .expect("run alan update --help");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--channel"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_cli_switches_stable_to_beta_and_rejects_invalid_and_retired_channels() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let old_bin = b"#!/bin/sh\necho 'alan 0.1.5'\n";
    let old_sha = sha256_hex(old_bin);
    std::fs::write(install_dir.join("alan"), old_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{old_sha}\n"),
    )
    .unwrap();

    let beta_version = "9.9.9-beta.20261123.42";
    let beta_bin = b"#!/bin/sh\necho 'alan 9.9.9-beta.20261123.42 (beta, 6c1dfd7)'\n";
    let beta_bin_sha = sha256_hex(beta_bin);
    let target = current_target();
    let asset = format!("alan-{beta_version}-{target}.tar.gz");
    let archive_bytes = build_tar_gz(beta_bin);
    let archive_sha = sha256_hex(&archive_bytes);

    let mut archive_routes = HashMap::new();
    archive_routes.insert(
        format!("/releases/{asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: archive_bytes.clone(),
        },
    );
    let archive_server = MockHttpServer::start(archive_routes).await;

    let beta_manifest = manifest_bytes(ManifestSpec {
        channel: "beta",
        version: beta_version,
        tag: "beta",
        commit: Some("6c1dfd7"),
        target,
        asset: &asset,
        archive_url: &archive_server.url(&format!("/releases/{asset}")),
        archive_sha256: &archive_sha,
        archive_size: archive_bytes.len(),
    });
    let stable_asset = format!("alan-9.9.9-{target}.tar.gz");
    let stable_manifest = manifest_bytes(ManifestSpec {
        channel: "stable",
        version: "9.9.9",
        tag: "v9.9.9",
        commit: None,
        target,
        asset: &stable_asset,
        archive_url: &archive_server.url("/releases/unused.tar.gz"),
        archive_sha256: &archive_sha,
        archive_size: archive_bytes.len(),
    });
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/beta.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: beta_manifest,
        },
    );
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: stable_manifest,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;

    // 1. Retired ALAN_INSTALL_CHANNEL still fails immediately.
    let retired = run_alan_update(
        &home,
        &["--check".into()],
        &[("ALAN_INSTALL_CHANNEL", "beta")],
    )
    .await;
    assert!(!retired.status.success());
    assert!(
        String::from_utf8_lossy(&retired.stderr).contains("ALAN_INSTALL_CHANNEL is retired"),
        "{}",
        String::from_utf8_lossy(&retired.stderr)
    );

    // 2. Unknown ALAN_CHANNEL fails immediately even when --manifest-url is passed.
    let bad_env = run_alan_update(
        &home,
        &[
            "--check".into(),
            "--manifest-url".into(),
            manifest_server.url("/beta.json"),
        ],
        &[("ALAN_CHANNEL", "nightly")],
    )
    .await;
    assert!(!bad_env.status.success());
    assert!(
        String::from_utf8_lossy(&bad_env.stderr).contains("unknown release channel"),
        "{}",
        String::from_utf8_lossy(&bad_env.stderr)
    );

    // 3. Explicit --channel beta with a stable manifest fails channel validation.
    let mismatch = run_alan_update(
        &home,
        &[
            "--check".into(),
            "--channel".into(),
            "beta".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_server.url("/latest.json"),
        ],
        &[],
    )
    .await;
    assert!(!mismatch.status.success());
    assert!(
        String::from_utf8_lossy(&mismatch.stderr).contains("does not match expected channel"),
        "{}",
        String::from_utf8_lossy(&mismatch.stderr)
    );

    // 4. Switching stable -> beta installs beta and preserves exact .alan-cli-manifest format.
    let switch_out = run_alan_update(
        &home,
        &[
            "--channel".into(),
            "beta".into(),
            "--json".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_server.url("/beta.json"),
        ],
        &[],
    )
    .await;
    assert!(
        switch_out.status.success(),
        "stable->beta switch failed: {}",
        String::from_utf8_lossy(&switch_out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&switch_out.stdout).unwrap();
    assert_eq!(report["status"], "updated");
    assert_eq!(report["channel"], "beta");
    assert_eq!(report["previous_channel"], "stable");
    assert_eq!(report["target_version"], beta_version);
    assert_eq!(report["tag"], "beta");
    assert_eq!(report["updated"], true);
    assert_eq!(archive_server.archive_requests(), 1);
    assert_eq!(
        std::fs::read_to_string(install_dir.join(".alan-cli-manifest")).unwrap(),
        format!("alan|{beta_bin_sha}\n")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn beta_binary_avoids_loop_reinstall_and_allows_explicit_switch_back_to_stable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let current_beta = "0.1.6-beta.20261123.42";
    let older_beta = "0.1.6-beta.20261123.41";
    let newer_beta = "0.1.6-beta.20261123.43";
    let stable_ver = "0.1.5";
    let target = current_target();

    let current_bin = b"#!/bin/sh\necho 'alan 0.1.6-beta.20261123.42'\n";
    let current_sha = sha256_hex(current_bin);
    std::fs::write(install_dir.join("alan"), current_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{current_sha}\n"),
    )
    .unwrap();

    let newer_beta_bin = b"#!/bin/sh\necho 'alan 0.1.6-beta.20261123.43'\n";
    let newer_beta_archive = build_tar_gz(newer_beta_bin);
    let newer_beta_sha = sha256_hex(&newer_beta_archive);
    let newer_beta_asset = format!("alan-{newer_beta}-{target}.tar.gz");

    let stable_bin = b"#!/bin/sh\necho 'alan 0.1.5'\n";
    let stable_archive = build_tar_gz(stable_bin);
    let stable_archive_sha = sha256_hex(&stable_archive);
    let stable_bin_sha = sha256_hex(stable_bin);
    let stable_asset = format!("alan-{stable_ver}-{target}.tar.gz");

    let mut archive_routes = HashMap::new();
    archive_routes.insert(
        format!("/releases/{newer_beta_asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: newer_beta_archive.clone(),
        },
    );
    archive_routes.insert(
        format!("/releases/{stable_asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: stable_archive.clone(),
        },
    );
    let archive_server = MockHttpServer::start(archive_routes).await;

    let same_beta_asset = format!("alan-{current_beta}-{target}.tar.gz");
    let same_beta_manifest = manifest_bytes(ManifestSpec {
        channel: "beta",
        version: current_beta,
        tag: "beta",
        commit: Some("6c1dfd7"),
        target,
        asset: &same_beta_asset,
        archive_url: &archive_server.url("/releases/unused.tar.gz"),
        archive_sha256: &newer_beta_sha,
        archive_size: newer_beta_archive.len(),
    });
    let older_beta_asset = format!("alan-{older_beta}-{target}.tar.gz");
    let older_beta_manifest = manifest_bytes(ManifestSpec {
        channel: "beta",
        version: older_beta,
        tag: "beta",
        commit: Some("1111111"),
        target,
        asset: &older_beta_asset,
        archive_url: &archive_server.url("/releases/unused.tar.gz"),
        archive_sha256: &newer_beta_sha,
        archive_size: newer_beta_archive.len(),
    });
    let newer_beta_manifest = manifest_bytes(ManifestSpec {
        channel: "beta",
        version: newer_beta,
        tag: "beta",
        commit: Some("7d2efe8"),
        target,
        asset: &newer_beta_asset,
        archive_url: &archive_server.url(&format!("/releases/{newer_beta_asset}")),
        archive_sha256: &newer_beta_sha,
        archive_size: newer_beta_archive.len(),
    });
    let stable_manifest = manifest_bytes(ManifestSpec {
        channel: "stable",
        version: stable_ver,
        tag: "v0.1.5",
        commit: None,
        target,
        asset: &stable_asset,
        archive_url: &archive_server.url(&format!("/releases/{stable_asset}")),
        archive_sha256: &stable_archive_sha,
        archive_size: stable_archive.len(),
    });

    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/beta-same.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: same_beta_manifest,
        },
    );
    manifest_routes.insert(
        "/beta-older.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: older_beta_manifest,
        },
    );
    manifest_routes.insert(
        "/beta-newer.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: newer_beta_manifest,
        },
    );
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: stable_manifest,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;

    // 1. Beta binary matching beta.json version reports up_to_date without re-downloading.
    let same_report = run_with_options_and_configured_channel(
        UpdateArgs {
            command: None,
            check: false,
            json: true,
            version: None,
            channel: None,
            force: false,
            install_dir: Some(install_dir.clone()),
            manifest_url: Some(manifest_server.url("/beta-same.json")),
            quiet: true,
        },
        current_beta,
        ReleaseChannel::Beta,
        None,
        target,
        None,
        Some(&home),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(same_report.status, "up_to_date");
    assert_eq!(same_report.channel, "beta");
    assert!(!same_report.updated);
    assert_eq!(archive_server.archive_requests(), 0);

    // 2. Beta binary refuses same-channel downgrade without --version + --force.
    let older_err = run_with_options_and_configured_channel(
        UpdateArgs {
            command: None,
            check: false,
            json: true,
            version: None,
            channel: Some(ReleaseChannel::Beta),
            force: false,
            install_dir: Some(install_dir.clone()),
            manifest_url: Some(manifest_server.url("/beta-older.json")),
            quiet: true,
        },
        current_beta,
        ReleaseChannel::Beta,
        None,
        target,
        None,
        Some(&home),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert!(older_err.to_string().contains("downgrade"), "{older_err:#}");

    // 3. Switching back beta -> stable (0.1.6-beta.* -> 0.1.5) via --channel stable works
    //    without --version + --force!
    let check_switch = run_with_options_and_configured_channel(
        UpdateArgs {
            command: None,
            check: true,
            json: true,
            version: None,
            channel: Some(ReleaseChannel::Stable),
            force: false,
            install_dir: Some(install_dir.clone()),
            manifest_url: Some(manifest_server.url("/latest.json")),
            quiet: true,
        },
        current_beta,
        ReleaseChannel::Beta,
        None,
        target,
        None,
        Some(&home),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(check_switch.status, "channel_switch");
    assert_eq!(check_switch.channel, "stable");
    assert_eq!(check_switch.previous_channel.as_deref(), Some("beta"));
    assert!(!check_switch.update_available);
    assert_eq!(archive_server.archive_requests(), 0);

    let applied_switch = run_with_options_and_configured_channel(
        UpdateArgs {
            command: None,
            check: false,
            json: true,
            version: None,
            channel: Some(ReleaseChannel::Stable),
            force: false,
            install_dir: Some(install_dir.clone()),
            manifest_url: Some(manifest_server.url("/latest.json")),
            quiet: true,
        },
        current_beta,
        ReleaseChannel::Beta,
        None,
        target,
        None,
        Some(&home),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(applied_switch.status, "updated");
    assert_eq!(applied_switch.channel, "stable");
    assert_eq!(applied_switch.previous_channel.as_deref(), Some("beta"));
    assert!(applied_switch.updated);
    assert_eq!(
        applied_switch.binary_sha256.as_deref(),
        Some(stable_bin_sha.as_str())
    );
    assert_eq!(archive_server.archive_requests(), 1);
    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), stable_bin);

    // 4. Passive cache refresh & channel-mismatch invalidation for beta.
    let store = alan_os_host::SystemStorePaths::from_data_dir(&home).unwrap();
    let cache_path = passive_check_cache_path(&store).unwrap();
    let cached = refresh_passive_cache(
        &cache_path,
        &manifest_server.url("/beta-newer.json"),
        target,
    )
    .await
    .unwrap();
    assert_eq!(cached.channel, "beta");
    assert_eq!(cached.latest_version, newer_beta);

    let (beta_notice, beta_stale) = evaluate_cache_for_notice_with_target(
        &cache_path,
        current_beta,
        ReleaseChannel::Beta,
        ReleaseChannel::Beta,
        chrono::Utc::now(),
    );
    assert_eq!(
        beta_notice.as_deref(),
        Some("Beta update available: v0.1.6-beta.20261123.43 — run 'alan update'")
    );
    assert!(!beta_stale);

    let (stable_notice, stable_stale) = evaluate_cache_for_notice_with_target(
        &cache_path,
        "0.1.5",
        ReleaseChannel::Stable,
        ReleaseChannel::Stable,
        chrono::Utc::now(),
    );
    assert_eq!(stable_notice, None);
    assert!(stable_stale);

    // 5. Persisted configured channel (without --channel/--force) switches beta -> stable cleanly.
    std::fs::write(install_dir.join("alan"), current_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{current_sha}\n"),
    )
    .unwrap();
    let configured_switch = run_with_options_and_configured_channel(
        UpdateArgs {
            command: None,
            check: false,
            json: true,
            version: None,
            channel: None,
            force: false,
            install_dir: Some(install_dir.clone()),
            manifest_url: Some(manifest_server.url("/latest.json")),
            quiet: true,
        },
        current_beta,
        ReleaseChannel::Beta,
        Some(ReleaseChannel::Stable),
        target,
        None,
        Some(&home),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(configured_switch.status, "updated");
    assert_eq!(configured_switch.channel, "stable");
    assert_eq!(configured_switch.previous_channel.as_deref(), Some("beta"));
    assert!(configured_switch.updated);
    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), stable_bin);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_channel_subcommand_persists_preference_and_drives_update() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let old_bin = b"#!/bin/sh\necho 'alan 0.1.5'\n";
    let old_sha = sha256_hex(old_bin);
    std::fs::write(install_dir.join("alan"), old_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{old_sha}\n"),
    )
    .unwrap();

    // Default channel status reflects binary default before channel.json exists.
    let initial = run_alan_update(&home, &["channel".into(), "--json".into()], &[]).await;
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let initial_json: serde_json::Value = serde_json::from_slice(&initial.stdout).unwrap();
    assert_eq!(initial_json["channel"], env!("ALAN_RELEASE_CHANNEL"));
    assert_eq!(initial_json["source"], "binary_default");
    assert_eq!(initial_json["switch_pending"], false);

    // Select `beta` via `alan update channel beta --json`.
    let set_beta = run_alan_update(
        &home,
        &["channel".into(), "beta".into(), "--json".into()],
        &[],
    )
    .await;
    assert!(
        set_beta.status.success(),
        "{}",
        String::from_utf8_lossy(&set_beta.stderr)
    );
    let set_json: serde_json::Value = serde_json::from_slice(&set_beta.stdout).unwrap();
    assert_eq!(set_json["channel"], "beta");
    assert_eq!(set_json["source"], "configured");
    assert_eq!(
        set_json["switch_pending"],
        env!("ALAN_RELEASE_CHANNEL") != "beta"
    );

    // Showing channel again reports `configured`.
    let shown = run_alan_update(&home, &["channel".into(), "--json".into()], &[]).await;
    assert!(shown.status.success());
    let shown_json: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(shown_json["channel"], "beta");
    assert_eq!(shown_json["source"], "configured");

    // Plain `alan update --check --json` respects persisted `beta` preference against manifest URL.
    let beta_version = "9.9.9-beta.20261123.50";
    let target = current_target();
    let asset = format!("alan-{beta_version}-{target}.tar.gz");
    let archive_bytes = build_tar_gz(b"#!/bin/sh\necho 'alan beta'\n");
    let archive_sha = sha256_hex(&archive_bytes);
    let stable_manifest = manifest_bytes(ManifestSpec {
        channel: "stable",
        version: "9.9.9",
        tag: "v9.9.9",
        commit: None,
        target,
        asset: &format!("alan-9.9.9-{target}.tar.gz"),
        archive_url: "http://127.0.0.1:9/unused.tar.gz",
        archive_sha256: &archive_sha,
        archive_size: archive_bytes.len(),
    });
    let mut routes = HashMap::new();
    routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: stable_manifest,
        },
    );
    let beta_manifest = manifest_bytes(ManifestSpec {
        channel: "beta",
        version: beta_version,
        tag: "beta",
        commit: Some("abc1234"),
        target,
        asset: &asset,
        archive_url: "http://127.0.0.1:9/unused-beta.tar.gz",
        archive_sha256: &archive_sha,
        archive_size: archive_bytes.len(),
    });
    routes.insert(
        "/beta.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: beta_manifest,
        },
    );
    let server = MockHttpServer::start(routes).await;

    let mismatch = run_alan_update(
        &home,
        &[
            "--check".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            server.url("/latest.json"),
        ],
        &[],
    )
    .await;
    // Direct --manifest-url omits channel enforcement; check plain beta.json succeeds cleanly.
    assert!(mismatch.status.success());

    let check_beta = run_alan_update(
        &home,
        &[
            "--check".into(),
            "--json".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            server.url("/beta.json"),
        ],
        &[],
    )
    .await;
    assert!(
        check_beta.status.success(),
        "{}",
        String::from_utf8_lossy(&check_beta.stderr)
    );
    let check_json: serde_json::Value = serde_json::from_slice(&check_beta.stdout).unwrap();
    assert_eq!(check_json["channel"], "beta");
    assert_eq!(check_json["target_version"], beta_version);
}
