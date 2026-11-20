use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

fn build_real_tar_gz_with_tar(alan_bytes: &[u8], extra_manifest: Option<&[u8]>) -> Vec<u8> {
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
    if let Some(manifest) = extra_manifest {
        std::fs::write(stage.join("manifest.json"), manifest).expect("write manifest.json");
    }
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

fn build_ustar_archive_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    for (name, content) in entries {
        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        assert!(name_bytes.len() <= 100);
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        header[100..108].copy_from_slice(b"0000755\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        let size_field = format!("{:011o}\0", content.len());
        header[124..136].copy_from_slice(size_field.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].copy_from_slice(b"        ");
        header[156] = b'0';
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
        .expect("stdin")
        .write_all(&tar_bytes)
        .expect("write tar");
    let out = gzip.wait_with_output().expect("wait gzip");
    assert!(out.status.success());
    out.stdout
}

fn manifest_json(
    version: &str,
    target: &str,
    asset: &str,
    archive_url: &str,
    archive_sha256: &str,
    archive_size: usize,
) -> Vec<u8> {
    let tag = format!("v{version}");
    serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "product": "alan",
        "version": version,
        "tag": tag,
        "published_at": "2026-11-20T00:00:00Z",
        "release_url": format!("https://github.com/realmorrisliu/alan/releases/tag/{tag}"),
        "notes_url": format!("https://github.com/realmorrisliu/alan/releases/tag/{tag}"),
        "installer_url": "https://alanworks.app/install.sh",
        "targets": {
            target: {
                "asset": asset,
                "url": archive_url,
                "sha256": archive_sha256,
                "size": archive_size,
                "signed": true,
                "notarized": true,
            }
        }
    }))
    .expect("serialize manifest")
}

async fn run_alan_update(home: &Path, args: &[String]) -> Output {
    let home = home.to_path_buf();
    let args = args.to_vec();
    tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_alan"))
            .env("HOME", &home)
            .env("XDG_DATA_HOME", home.join("data"))
            .env_remove("ALAN_INSTALL_CHANNEL")
            .env_remove("ALAN_MANIFEST_URL")
            .env_remove("ALAN_RELEASE_MANIFEST_URL")
            .env_remove("ALAN_RELEASE_BASE_URL")
            .arg("update")
            .args(&args)
            .output()
            .expect("run alan update")
    })
    .await
    .expect("spawn_blocking")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_check_json_and_live_upgrade_update_manifest_and_retire_legacy() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let old_bin = b"#!/bin/sh\necho 'alan 0.1.0'\n";
    let old_sha = sha256_hex(old_bin);
    std::fs::write(install_dir.join("alan"), old_bin).unwrap();
    std::fs::write(install_dir.join("alan-os-host"), b"legacy-host").unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest-stable"),
        format!(
            "alan|{old_sha}\nalan-os-host|{}\n",
            sha256_hex(b"legacy-host")
        ),
    )
    .unwrap();

    let new_bin = b"#!/bin/sh\necho 'alan 0.2.0'\n";
    let new_bin_sha = sha256_hex(new_bin);
    let target = current_target();
    let asset = format!("alan-0.2.0-{target}.tar.gz");
    let archive_bytes = build_real_tar_gz_with_tar(new_bin, Some(b"{\"product\":\"alan-cli\"}"));
    let archive_sha = sha256_hex(&archive_bytes);

    let archive_path = format!("/releases/{asset}");
    let server_url_hint = TcpListener::bind("127.0.0.1:0").await.unwrap();
    drop(server_url_hint);

    let mut routes = HashMap::new();
    routes.insert(
        archive_path.clone(),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: archive_bytes.clone(),
        },
    );
    let server = MockHttpServer::start(HashMap::new()).await;
    drop(server);

    // Build routes with a live listener bound to an active address via two-step start:
    // we serve relative? No, manifest needs full URL, so spin up archive server first.
    let archive_server = MockHttpServer::start(routes).await;
    let download_url = archive_server.url(&archive_path);
    let manifest_bytes = manifest_json(
        "0.2.0",
        target,
        &asset,
        &download_url,
        &archive_sha,
        archive_bytes.len(),
    );
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: manifest_bytes,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;
    let manifest_url = manifest_server.url("/latest.json");

    // 1. `--check --json` reports update_available without downloading archive or changing disk.
    let check_out = run_alan_update(
        &home,
        &[
            "--check".into(),
            "--json".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_url.clone(),
        ],
    )
    .await;
    assert!(
        check_out.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check_out.stderr)
    );
    let check_json: serde_json::Value = serde_json::from_slice(&check_out.stdout).unwrap();
    assert_eq!(check_json["status"], "update_available");
    assert_eq!(check_json["update_available"], true);
    assert_eq!(check_json["checked_only"], true);
    assert_eq!(check_json["updated"], false);
    assert_eq!(check_json["target_version"], "0.2.0");
    assert_eq!(archive_server.archive_requests(), 0);
    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), old_bin);

    // 2. Live upgrade replaces `alan`, updates `.alan-cli-manifest`, retires legacy files, cleans lock.
    let apply_out = run_alan_update(
        &home,
        &[
            "--json".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_url,
        ],
    )
    .await;
    assert!(
        apply_out.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_out.stderr)
    );
    let apply_json: serde_json::Value = serde_json::from_slice(&apply_out.stdout).unwrap();
    assert_eq!(apply_json["status"], "updated");
    assert_eq!(apply_json["updated"], true);
    assert_eq!(apply_json["binary_sha256"], new_bin_sha);
    assert_eq!(archive_server.archive_requests(), 1);

    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), new_bin);
    assert_eq!(
        std::fs::read_to_string(install_dir.join(".alan-cli-manifest")).unwrap(),
        format!("alan|{new_bin_sha}\n")
    );
    assert!(!install_dir.join("alan-os-host").exists());
    assert!(!install_dir.join(".alan-cli-manifest-stable").exists());
    assert!(!install_dir.join(".alan-cli-install.lock").exists());

    // Verify installed stub binary is executable.
    let run_installed = Command::new(install_dir.join("alan")).output().unwrap();
    assert!(run_installed.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run_installed.stdout).trim(),
        "alan 0.2.0"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_aborts_on_sha256_mismatch_without_modifying_existing_binary() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let old_bin = b"#!/bin/sh\necho old\n";
    let old_sha = sha256_hex(old_bin);
    std::fs::write(install_dir.join("alan"), old_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{old_sha}\n"),
    )
    .unwrap();

    let target = current_target();
    let asset = format!("alan-0.2.0-{target}.tar.gz");
    let archive_bytes = build_real_tar_gz_with_tar(b"tampered", None);
    let wrong_sha = "f".repeat(64);

    let mut archive_routes = HashMap::new();
    archive_routes.insert(
        format!("/{asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: archive_bytes.clone(),
        },
    );
    let archive_server = MockHttpServer::start(archive_routes).await;
    let manifest_bytes = manifest_json(
        "0.2.0",
        target,
        &asset,
        &archive_server.url(&format!("/{asset}")),
        &wrong_sha,
        archive_bytes.len(),
    );
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: manifest_bytes,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;

    let out = run_alan_update(
        &home,
        &[
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_server.url("/latest.json"),
        ],
    )
    .await;
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("SHA-256"), "{stderr}");
    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), old_bin);
    assert_eq!(
        std::fs::read_to_string(install_dir.join(".alan-cli-manifest")).unwrap(),
        format!("alan|{old_sha}\n")
    );
    assert!(!install_dir.join(".alan-cli-install.lock").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_refuses_unowned_binary_until_force_is_supplied() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let unowned_bin = b"#!/bin/sh\necho custom-unowned\n";
    std::fs::write(install_dir.join("alan"), unowned_bin).unwrap();

    let new_bin = b"#!/bin/sh\necho managed-new\n";
    let new_sha = sha256_hex(new_bin);
    let target = current_target();
    let asset = format!("alan-0.2.0-{target}.tar.gz");
    let archive_bytes = build_real_tar_gz_with_tar(new_bin, None);
    let archive_sha = sha256_hex(&archive_bytes);

    let mut archive_routes = HashMap::new();
    archive_routes.insert(
        format!("/{asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: archive_bytes.clone(),
        },
    );
    let archive_server = MockHttpServer::start(archive_routes).await;
    let manifest_bytes = manifest_json(
        "0.2.0",
        target,
        &asset,
        &archive_server.url(&format!("/{asset}")),
        &archive_sha,
        archive_bytes.len(),
    );
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: manifest_bytes,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;

    let refused = run_alan_update(
        &home,
        &[
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_server.url("/latest.json"),
        ],
    )
    .await;
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("unowned") && stderr.contains("--force"),
        "{stderr}"
    );
    assert_eq!(
        std::fs::read(install_dir.join("alan")).unwrap(),
        unowned_bin
    );
    assert_eq!(archive_server.archive_requests(), 0);
    assert!(!install_dir.join(".alan-cli-install.lock").exists());

    let forced = run_alan_update(
        &home,
        &[
            "--force".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_server.url("/latest.json"),
        ],
    )
    .await;
    assert!(
        forced.status.success(),
        "forced update failed: {}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), new_bin);
    assert_eq!(
        std::fs::read_to_string(install_dir.join(".alan-cli-manifest")).unwrap(),
        format!("alan|{new_sha}\n")
    );
    assert!(!install_dir.join(".alan-cli-install.lock").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_matching_current_version_is_up_to_date_noop() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let existing_bin = b"#!/bin/sh\necho current\n";
    let existing_sha = sha256_hex(existing_bin);
    std::fs::write(install_dir.join("alan"), existing_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{existing_sha}\n"),
    )
    .unwrap();

    let current = env!("CARGO_PKG_VERSION");
    let target = current_target();
    let asset = format!("alan-{current}-{target}.tar.gz");
    let archive_bytes = build_real_tar_gz_with_tar(b"replacement", None);
    let archive_sha = sha256_hex(&archive_bytes);

    let mut archive_routes = HashMap::new();
    archive_routes.insert(
        format!("/{asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: archive_bytes.clone(),
        },
    );
    let archive_server = MockHttpServer::start(archive_routes).await;
    let manifest_bytes = manifest_json(
        current,
        target,
        &asset,
        &archive_server.url(&format!("/{asset}")),
        &archive_sha,
        archive_bytes.len(),
    );
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: manifest_bytes,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;

    let out = run_alan_update(
        &home,
        &[
            "--json".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_server.url("/latest.json"),
        ],
    )
    .await;
    assert!(
        out.status.success(),
        "up-to-date check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["status"], "up_to_date");
    assert_eq!(parsed["updated"], false);
    assert_eq!(parsed["update_available"], false);
    assert_eq!(parsed["binary_sha256"], existing_sha);
    assert_eq!(archive_server.archive_requests(), 0);
    assert_eq!(
        std::fs::read(install_dir.join("alan")).unwrap(),
        existing_bin
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rejects_archive_with_path_traversal_before_extraction() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let old_bin = b"#!/bin/sh\necho old\n";
    let old_sha = sha256_hex(old_bin);
    std::fs::write(install_dir.join("alan"), old_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{old_sha}\n"),
    )
    .unwrap();

    let target = current_target();
    let asset = format!("alan-0.2.0-{target}.tar.gz");
    let traversal_archive = build_ustar_archive_gz(&[
        ("./alan", b"#!/bin/sh\necho new\n"),
        ("../evil-escape", b"pwned"),
    ]);
    let archive_sha = sha256_hex(&traversal_archive);

    let mut archive_routes = HashMap::new();
    archive_routes.insert(
        format!("/{asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: traversal_archive.clone(),
        },
    );
    let archive_server = MockHttpServer::start(archive_routes).await;
    let manifest_bytes = manifest_json(
        "0.2.0",
        target,
        &asset,
        &archive_server.url(&format!("/{asset}")),
        &archive_sha,
        traversal_archive.len(),
    );
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: manifest_bytes,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;

    let out = run_alan_update(
        &home,
        &[
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_server.url("/latest.json"),
        ],
    )
    .await;
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(".."), "{stderr}");
    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), old_bin);
    assert!(!temp.path().join("evil-escape").exists());
    assert!(!install_dir.join(".alan-cli-install.lock").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_refuses_downgrade_unless_explicit_version_and_force_are_passed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let current_bin = b"#!/bin/sh\necho 0.1.0\n";
    let current_sha = sha256_hex(current_bin);
    std::fs::write(install_dir.join("alan"), current_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest"),
        format!("alan|{current_sha}\n"),
    )
    .unwrap();

    let older_bin = b"#!/bin/sh\necho 0.0.9\n";
    let older_sha = sha256_hex(older_bin);
    let target = current_target();
    let asset = format!("alan-0.0.9-{target}.tar.gz");
    let archive_bytes = build_real_tar_gz_with_tar(older_bin, None);
    let archive_sha = sha256_hex(&archive_bytes);

    let mut archive_routes = HashMap::new();
    archive_routes.insert(
        format!("/{asset}"),
        MockResponse {
            status: 200,
            content_type: "application/gzip",
            body: archive_bytes.clone(),
        },
    );
    let archive_server = MockHttpServer::start(archive_routes).await;
    let manifest_bytes = manifest_json(
        "0.0.9",
        target,
        &asset,
        &archive_server.url(&format!("/{asset}")),
        &archive_sha,
        archive_bytes.len(),
    );
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: manifest_bytes,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;
    let manifest_url = manifest_server.url("/latest.json");

    // 1. Implicit downgrade refused.
    let refused_implicit = run_alan_update(
        &home,
        &[
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_url.clone(),
        ],
    )
    .await;
    assert!(!refused_implicit.status.success());
    assert!(
        String::from_utf8_lossy(&refused_implicit.stderr).contains("downgrade"),
        "{}",
        String::from_utf8_lossy(&refused_implicit.stderr)
    );

    // 2. `--version 0.0.9` without `--force` also refused.
    let refused_no_force = run_alan_update(
        &home,
        &[
            "--version".into(),
            "0.0.9".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_url.clone(),
        ],
    )
    .await;
    assert!(!refused_no_force.status.success());

    // 3. `--version v0.0.9 --force` succeeds.
    let allowed = run_alan_update(
        &home,
        &[
            "--version".into(),
            "v0.0.9".into(),
            "--force".into(),
            "--json".into(),
            "--install-dir".into(),
            install_dir.display().to_string(),
            "--manifest-url".into(),
            manifest_url,
        ],
    )
    .await;
    assert!(
        allowed.status.success(),
        "forced downgrade failed: {}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&allowed.stdout).unwrap();
    assert_eq!(parsed["status"], "updated");
    assert_eq!(parsed["updated"], true);
    assert_eq!(parsed["target_version"], "0.0.9");
    assert_eq!(parsed["binary_sha256"], older_sha);
    assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), older_bin);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn passive_cache_refresh_writes_last_check_json_in_system_store() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = alan_os_host::SystemStorePaths::from_data_dir(temp.path()).unwrap();
    let cache_path = alan::cli::update::passive_check::passive_check_cache_path(&store).unwrap();

    let target = current_target();
    let asset = format!("alan-0.2.5-{target}.tar.gz");
    let manifest_bytes = manifest_json(
        "0.2.5",
        target,
        &asset,
        "https://example.invalid/alan.tar.gz",
        &"a".repeat(64),
        1234,
    );
    let mut manifest_routes = HashMap::new();
    manifest_routes.insert(
        "/latest.json".to_string(),
        MockResponse {
            status: 200,
            content_type: "application/json",
            body: manifest_bytes,
        },
    );
    let manifest_server = MockHttpServer::start(manifest_routes).await;

    let cached = alan::cli::update::passive_check::refresh_passive_cache(
        &cache_path,
        &manifest_server.url("/latest.json"),
        target,
    )
    .await
    .expect("refresh passive cache");
    assert_eq!(cached.latest_version, "0.2.5");
    assert_eq!(cached.tag, "v0.2.5");
    assert!(cache_path.is_file());

    let (notice, needs_refresh) = alan::cli::update::passive_check::evaluate_cache_for_notice(
        &cache_path,
        "0.1.0",
        chrono::Utc::now(),
    );
    assert_eq!(
        notice.as_deref(),
        Some("Update available: v0.2.5 — run 'alan update'")
    );
    assert!(!needs_refresh);
}
