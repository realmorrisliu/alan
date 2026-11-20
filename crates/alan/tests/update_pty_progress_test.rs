#![cfg(unix)]

use std::fmt::Write as _;
use std::io::Read;
use std::net::SocketAddr;
use std::num::NonZeroU16;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct SlowArchiveServer {
    addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SlowArchiveServer {
    async fn start(manifest_bytes: Vec<u8>, archive_path: String, archive_bytes: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock TcpListener");
        let addr = listener.local_addr().expect("local addr");
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { break };
                        let manifest = manifest_bytes.clone();
                        let release_path = archive_path.clone();
                        let archive = archive_bytes.clone();
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
                                .unwrap_or("/");
                            if path == "/latest.json" {
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    manifest.len()
                                );
                                let _ = socket.write_all(header.as_bytes()).await;
                                let _ = socket.write_all(&manifest).await;
                                let _ = socket.flush().await;
                                return;
                            }
                            if path == release_path {
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    archive.len()
                                );
                                let _ = socket.write_all(header.as_bytes()).await;
                                let _ = socket.flush().await;
                                let chunk_size = archive.len().div_ceil(4).max(1);
                                for (idx, part) in archive.chunks(chunk_size).enumerate() {
                                    if idx > 0 {
                                        tokio::time::sleep(Duration::from_millis(110)).await;
                                    }
                                    if socket.write_all(part).await.is_err() {
                                        return;
                                    }
                                    let _ = socket.flush().await;
                                }
                                return;
                            }
                            let _ = socket
                                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                                .await;
                        });
                    }
                }
            }
        });

        Self {
            addr,
            shutdown: Some(tx),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
}

impl Drop for SlowArchiveServer {
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

fn build_real_tar_gz_with_tar(alan_bytes: &[u8]) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("tempdir");
    let stage = dir.path().join("stage");
    std::fs::create_dir_all(&stage).expect("create stage");
    let alan_path = stage.join("alan");
    std::fs::write(&alan_path, alan_bytes).expect("write alan");
    std::fs::set_permissions(&alan_path, std::fs::Permissions::from_mode(0o755))
        .expect("chmod alan");
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

fn open_pty(rows: u16, cols: u16) -> (OwnedFd, OwnedFd) {
    let mut master = -1;
    let mut slave = -1;
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: `master`, `slave`, and `&ws` are valid pointers; `name` and `termp` are null.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &ws as *const libc::winsize as *mut libc::winsize,
        )
    };
    assert_eq!(rc, 0, "openpty failed: {}", std::io::Error::last_os_error());
    // SAFETY: `openpty` succeeded and returned newly opened master/slave file descriptors.
    unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) }
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("alan update PTY child timed out after {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pty_80_col_redraws_keep_progress_bar_on_single_row() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let install_dir = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&install_dir).unwrap();

    let old_bin = b"#!/bin/sh\necho 'alan 0.1.0'\n";
    let old_sha = sha256_hex(old_bin);
    std::fs::write(install_dir.join("alan"), old_bin).unwrap();
    std::fs::write(
        install_dir.join(".alan-cli-manifest-stable"),
        format!("alan|{old_sha}\n"),
    )
    .unwrap();

    let new_bin = b"#!/bin/sh\necho 'alan 9.9.9'\n";
    let target =
        alan::cli::update::semver::target_triple_for(std::env::consts::OS, std::env::consts::ARCH)
            .expect("host must be a supported target");
    let asset = format!("alan-9.9.9-{target}.tar.gz");
    let archive_path = format!("/releases/{asset}");
    let archive_bytes = build_real_tar_gz_with_tar(new_bin);
    let archive_sha = sha256_hex(&archive_bytes);

    let manifest_template = serde_json::json!({
        "schema_version": 1,
        "product": "alan",
        "version": "9.9.9",
        "tag": "v9.9.9",
        "published_at": "2026-11-20T00:00:00Z",
        "release_url": "https://github.com/realmorrisliu/alan/releases/tag/v9.9.9",
        "notes_url": "https://github.com/realmorrisliu/alan/releases/tag/v9.9.9",
        "installer_url": "https://alanworks.app/install.sh",
        "targets": {
            target: {
                "asset": asset,
                "url": "",
                "sha256": archive_sha,
                "size": archive_bytes.len(),
                "signed": true,
                "notarized": true,
            }
        }
    });

    // Serve `/latest.json` from a dynamic router once we know the archive server's bound port.
    let archive_server =
        SlowArchiveServer::start(Vec::new(), archive_path.clone(), archive_bytes.clone()).await;
    let mut manifest = manifest_template;
    manifest["targets"][target]["url"] = serde_json::json!(archive_server.url(&archive_path));
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let manifest_server =
        SlowArchiveServer::start(manifest_bytes, "/unused".to_string(), Vec::new()).await;
    let manifest_url = manifest_server.url("/latest.json");

    let (status, raw_bytes) = tokio::task::spawn_blocking(move || {
        let (master, slave) = open_pty(24, 80);
        let master_fd = master.as_raw_fd();
        let slave_fd = slave.as_raw_fd();

        let mut cmd = Command::new(env!("CARGO_BIN_EXE_alan"));
        cmd.arg("update")
            .arg("--manifest-url")
            .arg(&manifest_url)
            .arg("--install-dir")
            .arg(&install_dir)
            .arg("--force")
            .env("HOME", &home)
            .env("XDG_DATA_HOME", home.join("data"))
            .env("TERM", "xterm-256color")
            .env_remove("NO_COLOR")
            .env_remove("COLUMNS")
            .env_remove("ALAN_INSTALL_CHANNEL")
            .env_remove("ALAN_MANIFEST_URL")
            .env_remove("ALAN_RELEASE_MANIFEST_URL")
            .env_remove("ALAN_RELEASE_BASE_URL")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // SAFETY: `pre_exec` invokes only async-signal-safe POSIX syscalls (`setsid`, `ioctl`, `dup2`, `close`).
        unsafe {
            cmd.pre_exec(move || {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                for target_fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
                    if libc::dup2(slave_fd, target_fd) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                if slave_fd > libc::STDERR_FILENO {
                    libc::close(slave_fd);
                }
                if master_fd > libc::STDERR_FILENO {
                    libc::close(master_fd);
                }
                Ok(())
            });
        }

        let mut child = cmd.spawn().expect("spawn alan update in PTY");
        drop(slave);

        let reader = std::thread::spawn(move || {
            let mut file = std::fs::File::from(master);
            let mut out = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => out.extend_from_slice(&buf[..n]),
                    Err(err) if err.raw_os_error() == Some(libc::EIO) => break,
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(err) => panic!("read PTY master failed: {err}"),
                }
            }
            out
        });

        let status = wait_with_timeout(&mut child, Duration::from_secs(30));
        let raw_bytes = reader.join().expect("join PTY reader");
        assert_eq!(std::fs::read(install_dir.join("alan")).unwrap(), new_bin);
        (status, raw_bytes)
    })
    .await
    .expect("spawn_blocking");

    assert!(status.success(), "alan update failed in PTY: {status:?}");

    let raw_text = String::from_utf8_lossy(&raw_bytes);
    let redraw_count = raw_text.matches("\r\x1b[2K").count();
    assert!(
        redraw_count >= 2,
        "expected multiple inline redraws on slow download, got {redraw_count}: {raw_text:?}"
    );

    let mut parser = vt100::Parser::new(
        NonZeroU16::new(24).expect("nonzero rows"),
        NonZeroU16::new(80).expect("nonzero cols"),
        0,
    );
    parser.process(&raw_bytes);
    let screen = parser.screen().contents();

    let downloading_rows: Vec<&str> = screen
        .lines()
        .filter(|line| line.contains("Downloading"))
        .collect();
    assert_eq!(
        downloading_rows.len(),
        1,
        "progress bar wrapped or leaked across multiple rows:\n{screen}"
    );
    assert!(downloading_rows[0].contains("100%"), "{screen}");
    assert!(screen.contains("Checking for updates..."), "{screen}");
    assert!(
        screen.contains("Verifying archive and extracting..."),
        "{screen}"
    );
    assert!(screen.contains("Installing into"), "{screen}");
    assert!(screen.contains("Updated alan"), "{screen}");
}
