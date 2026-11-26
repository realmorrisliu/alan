#![cfg(unix)]

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use alan_agent_protocol::{
    UiModelBinding, UiModelControlSource, UiModelSnapshot, UiNoticeKind, UiNoticeSnapshot,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat,
};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_tui::{FileBackedRunConfig, IssueControlHandler, IssueSubmitResult, run_file_backed};
use anyhow::{Context, Result, bail};

const CHILD_ENV: &str = "ALAN_PTY_ISSUE_CHILD";
const MODE_ENV: &str = "ALAN_PTY_ISSUE_MODE";
const LOG_PATH_ENV: &str = "ALAN_PTY_ISSUE_LOG";

struct PtyState {
    parser: vt100::Parser,
    eof: bool,
}

#[derive(Clone)]
struct PtyScreen {
    shared: Arc<(Mutex<PtyState>, Condvar)>,
}

impl PtyScreen {
    fn new(rows: u16, cols: u16) -> Self {
        Self {
            shared: Arc::new((
                Mutex::new(PtyState {
                    parser: vt100::Parser::new(
                        NonZeroU16::new(rows).expect("nonzero rows"),
                        NonZeroU16::new(cols).expect("nonzero cols"),
                        0,
                    ),
                    eof: false,
                }),
                Condvar::new(),
            )),
        }
    }

    fn push_bytes(&self, bytes: &[u8]) {
        let mut guard = self.shared.0.lock().expect("pty lock");
        guard.parser.process(bytes);
        self.shared.1.notify_all();
    }

    fn mark_eof(&self) {
        let mut guard = self.shared.0.lock().expect("pty lock");
        guard.eof = true;
        self.shared.1.notify_all();
    }

    fn cursor_one_based(&self) -> (u16, u16) {
        let guard = self.shared.0.lock().expect("pty lock");
        let (row, col) = guard.parser.screen().cursor_position();
        (row.saturating_add(1), col.saturating_add(1))
    }

    fn wait_for(
        &self,
        label: &str,
        timeout: Duration,
        predicate: impl Fn(&str) -> bool,
    ) -> Result<String> {
        let deadline = Instant::now() + timeout;
        let mut guard = self.shared.0.lock().expect("pty lock");
        loop {
            let contents = guard.parser.screen().contents();
            if predicate(&contents) {
                return Ok(contents);
            }
            if guard.eof {
                bail!("PTY reached EOF before `{label}`.\nScreen:\n{contents}");
            }
            let now = Instant::now();
            if now >= deadline {
                bail!("timed out waiting for `{label}`.\nScreen:\n{contents}");
            }
            let (next_guard, _) = self
                .shared
                .1
                .wait_timeout(guard, deadline - now)
                .expect("condvar wait");
            guard = next_guard;
        }
    }
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn wait_with_timeout(&mut self, timeout: Duration) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        let child = self.0.as_mut().expect("child");
        loop {
            if let Some(status) = child.try_wait()? {
                self.0.take();
                return Ok(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                self.0.take();
                bail!("child did not exit within {timeout:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn open_pty(rows: u16, cols: u16) -> Result<(OwnedFd, OwnedFd)> {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: `master`, `slave`, and `ws` point to valid local storage.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &ws as *const libc::winsize as *mut libc::winsize,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error()).context("openpty");
    }
    // SAFETY: `openpty` succeeded and returned newly owned descriptors.
    Ok(unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) })
}

fn spawn_child(
    slave: &OwnedFd,
    master_to_close: RawFd,
    extra_env: &[(&str, &OsStr)],
) -> Result<Child> {
    let exe = std::env::current_exe().context("current exe")?;
    let slave_fd = std::os::fd::AsRawFd::as_raw_fd(slave);
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", "pty_e2e_issue_child_runner", "--nocapture"])
        .env(CHILD_ENV, "1")
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    // SAFETY: `pre_exec` uses only async-signal-safe POSIX descriptor and session syscalls.
    unsafe {
        cmd.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            for target in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
                if libc::dup2(slave_fd, target) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            if slave_fd > libc::STDERR_FILENO {
                libc::close(slave_fd);
            }
            if master_to_close > libc::STDERR_FILENO {
                libc::close(master_to_close);
            }
            Ok(())
        });
    }
    cmd.spawn().context("spawn issue pty child")
}

fn spawn_reader(
    master_read: OwnedFd,
    master_write: Arc<Mutex<std::fs::File>>,
    screen: PtyScreen,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = std::fs::File::from(master_read);
        let mut buf = [0u8; 4096];
        let mut tail = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    screen.mark_eof();
                    break;
                }
                Ok(n) => {
                    let chunk = &buf[..n];
                    screen.push_bytes(chunk);
                    tail.extend_from_slice(chunk);
                    if tail.windows(4).any(|w| w == b"\x1b[6n") {
                        let (row, col) = screen.cursor_one_based();
                        let reply = format!("\x1b[{row};{col}R");
                        if let Ok(mut w) = master_write.lock() {
                            let _ = w.write_all(reply.as_bytes());
                            let _ = w.flush();
                        }
                        tail.clear();
                    } else if tail.len() > 8 {
                        let drain = tail.len() - 4;
                        tail.drain(..drain);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    screen.mark_eof();
                    break;
                }
            }
        }
    })
}

struct PtyMockGithubFs {
    log_path: PathBuf,
    buffers: Mutex<std::collections::HashMap<Fid, Vec<u8>>>,
}

#[async_trait::async_trait]
impl FileServer for PtyMockGithubFs {
    async fn walk(&self, _fid: Fid, _newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match names.join("/").as_str() {
            ""
            | "repos"
            | "repos/realmorrisliu"
            | "repos/realmorrisliu/alan"
            | "repos/realmorrisliu/alan/issues" => Ok(Qid {
                kind: FileKind::Dir,
                path: 1,
                version: 0,
            }),
            "repos/realmorrisliu/alan/issues/new" => Ok(Qid {
                kind: FileKind::File,
                path: 2,
                version: 0,
            }),
            _ => Err(ErrorCode::NotFound),
        }
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            self.buffers.lock().unwrap().insert(fid, Vec::new());
        }
        Ok(Qid {
            kind: FileKind::File,
            path: 2,
            version: 0,
        })
    }

    async fn read(&self, _fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let receipt = br#"{"number":77,"url":"https://github.com/realmorrisliu/alan/issues/77"}"#;
        let start = (offset as usize).min(receipt.len());
        let end = (start + count as usize).min(receipt.len());
        Ok(receipt[start..end].to_vec())
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut guard = self.buffers.lock().unwrap();
        let buf = guard.entry(fid).or_default();
        let start = offset as usize;
        let end = start + data.len();
        if buf.len() < end {
            buf.resize(end, 0);
        }
        buf[start..end].copy_from_slice(data);
        Ok(data.len() as u32)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: "new".to_string(),
            qid: Qid {
                kind: FileKind::File,
                path: 2,
                version: 0,
            },
            length: 0,
            executable: false,
            writable: true,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if let Some(bytes) = self.buffers.lock().unwrap().remove(&fid)
            && !bytes.is_empty()
        {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_path)
                .map_err(|_| ErrorCode::Io)?;
            let _ = f.write_all(&bytes);
            let _ = f.write_all(b"\n");
        }
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn run_issue_pty_session() -> Result<()> {
    let mode = std::env::var(MODE_ENV).unwrap_or_else(|_| "mounted".to_string());
    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let mount_fs = Arc::new(MountFs::new(Namespace::new()));
    let live = mount_fs.live_namespace();
    live.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let shell = alan_shell::Shell::new(InProcessTransport::new(mount_fs.clone()));
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#)
        .await?;
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;
    live.mount(
        "/mnt/service-manager/units/root-agent",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "pid",
            format!("{pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    let owner = format!("/agent/{pid}");
    let model_snapshot = UiModelSnapshot {
        version: 1,
        process_path: format!("/proc/{pid}"),
        publication_version: 1,
        known: true,
        selected_next: Some(UiModelBinding {
            profile: "openai-main".into(),
            provider: "openai_responses".into(),
            model: "gpt-5.4".into(),
            reasoning: Default::default(),
            control_source: UiModelControlSource::ProviderDefault,
        }),
        active: None,
        catalog: None,
        admitted: Vec::new(),
    };
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&model_snapshot)?,
        )
        .await?;
    let tape_entry = serde_json::json!({
        "version": 1,
        "kind": "message",
        "role": "user",
        "content": "PRIVATE_TRANSCRIPT_PROMPT_SHOULD_NOT_LEAK",
        "submission_id": "sub-pty-1"
    });
    let mut tape_bytes = serde_json::to_vec(&tape_entry)?;
    tape_bytes.push(b'\n');
    shell
        .write(&format!("{owner}/machine/tape"), &tape_bytes)
        .await?;
    let notice = UiNoticeSnapshot {
        version: 1,
        kind: UiNoticeKind::Error,
        message: "Auth error Bearer ghp_ptysecret999 OPENAI_API_KEY=sk-proj-ptysecret".into(),
    };
    shell
        .write(
            &format!("{owner}/machine/ui/notice"),
            &serde_json::to_vec(&notice)?,
        )
        .await?;

    if mode == "mounted" {
        let log_path = PathBuf::from(std::env::var(LOG_PATH_ENV)?);
        let ghfs = Arc::new(PtyMockGithubFs {
            log_path,
            buffers: Mutex::new(std::collections::HashMap::new()),
        });
        live.mount(
            "/mnt/github",
            InProcessTransport::new(ghfs),
            Access::ReadWrite,
        );
    }

    let mut cfg = FileBackedRunConfig::new(InProcessTransport::new(mount_fs), "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    cfg.sandbox_backend = Some("seatbelt".into());
    cfg.trust_posture_label = Some("trust".into());
    if mode == "fallback" {
        let unavail: IssueControlHandler = Arc::new(|_| {
            Box::pin(async {
                Ok(IssueSubmitResult::GhUnavailable {
                    reason: "`gh` not logged in".to_string(),
                })
            })
        });
        cfg.issue_control = Some(unavail);
    }
    run_file_backed(cfg).await
}

#[test]
fn pty_e2e_issue_child_runner() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    if let Err(err) = run_issue_pty_session() {
        eprintln!("issue pty child failed: {err:#}");
        std::process::exit(2);
    }
}

#[test]
fn pty_e2e_issue_cancel_redaction_and_mounted_githubfs_submission() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let log_path = temp.path().join("githubfs-writes.jsonl");
    let (master, slave) = open_pty(28, 100)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
    let child = spawn_child(
        &slave,
        master_fd,
        &[
            (MODE_ENV, OsStr::new("mounted")),
            (LOG_PATH_ENV, log_path.as_os_str()),
            ("OPENAI_API_KEY", OsStr::new("sk-env-secret-must-not-leak")),
        ],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(28, 100);
    let reader = spawn_reader(master_read, master_write.clone(), screen.clone());

    screen.wait_for("startup ready", Duration::from_secs(6), |s| {
        s.contains("gpt-5.4") && s.contains("trust") && s.contains("Auth error")
    })?;

    // 1. Open /issue with title, inspect redacted diagnostics, then cancel via Esc.
    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"/issue Draft to cancel\r")?;
        w.flush()?;
    }
    let preview = screen.wait_for("issue draft preview", Duration::from_secs(5), |s| {
        s.contains("New Issue · realmorrisliu/alan")
            && s.contains("Draft to cancel")
            && s.contains("Redacted Diagnostics")
            && s.contains("gpt-5.4")
    })?;
    assert!(preview.contains("<redacted>"), "{preview}");
    assert!(!preview.contains("ghp_ptysecret999"), "{preview}");
    assert!(!preview.contains("sk-proj-ptysecret"), "{preview}");
    assert!(
        !preview.contains("sk-env-secret-must-not-leak"),
        "{preview}"
    );
    assert!(
        !preview.contains("PRIVATE_TRANSCRIPT_PROMPT_SHOULD_NOT_LEAK"),
        "{preview}"
    );

    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"\x1b")?;
        w.flush()?;
    }
    screen.wait_for("cancel notice", Duration::from_secs(5), |s| {
        s.contains("Issue draft cancelled.")
    })?;
    assert!(
        !log_path.exists(),
        "cancelling /issue must perform zero /mnt/github writes"
    );

    // 2. Open via /bug alias, edit body, confirm with Enter, verify Tier 1 write + receipt.
    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"/bug PTY crash\r")?;
        w.flush()?;
    }
    screen.wait_for("bug alias draft preview", Duration::from_secs(5), |s| {
        s.contains("New Issue · realmorrisliu/alan") && s.contains("PTY crash")
    })?;
    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"Repro details")?;
        w.write_all(b"\r")?;
        w.flush()?;
    }
    screen.wait_for("githubfs created receipt", Duration::from_secs(5), |s| {
        s.contains("Issue #77 created via /mnt/github")
            && s.contains("https://github.com/realmorrisliu/alan/issues/77")
    })?;

    let logged = std::fs::read_to_string(&log_path)?;
    let parsed: serde_json::Value = serde_json::from_str(logged.trim())?;
    assert_eq!(parsed["title"], "PTY crash");
    let body = parsed["body"].as_str().unwrap_or("");
    assert!(body.contains("Repro details"), "{body}");
    assert!(!body.contains("ghp_ptysecret999"), "{body}");

    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"\x11")?;
        w.flush()?;
    }
    let status = child.wait_with_timeout(Duration::from_secs(5))?;
    assert!(status.success(), "issue pty exited with {status:?}");
    drop(master_write);
    let _ = reader.join();
    Ok(())
}

#[test]
fn pty_e2e_issue_emits_bounded_web_fallback_when_unmounted_and_gh_unavailable() -> Result<()> {
    let (master, slave) = open_pty(28, 110)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
    let child = spawn_child(&slave, master_fd, &[(MODE_ENV, OsStr::new("fallback"))])?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(28, 110);
    let reader = spawn_reader(master_read, master_write.clone(), screen.clone());

    screen.wait_for("startup ready", Duration::from_secs(6), |s| {
        s.contains("gpt-5.4")
    })?;

    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"/bug Fallback URL check\r")?;
        w.flush()?;
    }
    screen.wait_for("fallback preview", Duration::from_secs(5), |s| {
        s.contains("Fallback URL check") && s.contains("New Issue")
    })?;
    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"\r")?;
        w.flush()?;
    }
    screen.wait_for("prefilled web url rendered", Duration::from_secs(5), |s| {
        s.contains("browser URL fallback")
            && s.contains(
                "https://github.com/realmorrisliu/alan/issues/new?title=Fallback%20URL%20check",
            )
    })?;

    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"\x11")?;
        w.flush()?;
    }
    let status = child.wait_with_timeout(Duration::from_secs(5))?;
    assert!(status.success(), "fallback pty exited with {status:?}");
    drop(master_write);
    let _ = reader.join();
    Ok(())
}
