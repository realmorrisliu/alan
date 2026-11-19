#![cfg(unix)]

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_tui::{FileBackedRunConfig, run_file_backed};
use anyhow::{Context, Result, bail};

const CHILD_ENV: &str = "ALAN_PTY_M3_CHILD";

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

fn spawn_child(slave: &OwnedFd, master_to_close: RawFd) -> Result<Child> {
    let exe = std::env::current_exe().context("current exe")?;
    let slave_fd = std::os::fd::AsRawFd::as_raw_fd(slave);
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", "pty_e2e_m3_child_runner", "--nocapture"])
        .env(CHILD_ENV, "1")
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
    cmd.spawn().context("spawn m3 pty child")
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

#[tokio::main(flavor = "current_thread")]
async fn run_m3_session() -> Result<()> {
    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let catalog_json = serde_json::json!({
        "active_project_cwd": "/mnt/project",
        "entries": [{
            "rollout_id": "rollout-m3-pty-1",
            "updated_at": "2026-11-19T09:00:00Z",
            "project_cwd": "/mnt/project",
            "project_label": "alan",
            "turn_count": 3,
            "last_status": "idle",
            "goal_snippet": "PTY continuity check",
            "has_pending_checkpoint": false,
            "has_unknown_effects": false,
            "corrupt_lines": 0
        }],
        "diagnostics": []
    })
    .to_string()
    .into_bytes();
    let summary_json = serde_json::json!({
        "exists": true,
        "deleted": false,
        "project_key": "alan-1234",
        "project_cwd": "/mnt/project",
        "summary": {
            "goal": "Ship M3 PTY verification",
            "progress": "Summary inspect ready",
            "verification": "PTY green",
            "source_rollouts": ["rollout-m3-pty-1"],
            "revision": 2,
            "updated_at": "2026-11-19T09:00:00Z",
            "origin": "generated",
            "deleted": false
        },
        "markdown_recall_active": true,
        "retention_note": "Derived project continuity summary only; retained rollouts separate."
    })
    .to_string()
    .into_bytes();
    let history_fs = Arc::new(MemFs::with_read_only_files([
        ("catalog".to_string(), catalog_json),
        ("summary".to_string(), summary_json),
    ]));
    agent_root.bind_history_server(history_fs).await;

    let mount_fs = Arc::new(MountFs::new(Namespace::new()));
    let live = mount_fs.live_namespace();
    live.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let pid = alan_shell::Shell::new(InProcessTransport::new(mount_fs.clone()))
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

    let mut cfg = FileBackedRunConfig::new(InProcessTransport::new(mount_fs), "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    cfg.sandbox_backend = Some("unsandboxed host-account".into());
    cfg.trust_posture_label = Some("UNSANDBOXED host-account".into());
    run_file_backed(cfg).await
}

#[test]
fn pty_e2e_m3_child_runner() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    if let Err(err) = run_m3_session() {
        eprintln!("m3 pty child failed: {err:#}");
        std::process::exit(2);
    }
}

#[test]
fn pty_e2e_m3_trust_badge_resume_picker_and_summary() -> Result<()> {
    let (master, slave) = open_pty(28, 96)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
    let child = spawn_child(&slave, master_fd)?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(28, 96);
    let reader = spawn_reader(master_read, master_write.clone(), screen.clone());

    // 1. Trust posture warning badge renders clearly on startup banner + context line.
    screen.wait_for("unsandboxed badge startup", Duration::from_secs(6), |s| {
        s.contains("UNSANDBOXED host-account") && s.contains("ALAN(1) · alan9")
    })?;

    // 2. `/resume` renders catalog entries + Start fresh option, and Esc cancels cleanly.
    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"/resume\r")?;
        w.flush()?;
    }
    screen.wait_for("resume picker screen", Duration::from_secs(5), |s| {
        s.contains("Resume Session")
            && s.contains("rollout-m3-pty-1")
            && s.contains("Start fresh session")
    })?;
    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"\x1b")?;
        w.flush()?;
    }
    screen.wait_for("resume cancelled notice", Duration::from_secs(5), |s| {
        s.contains("Resume cancelled.")
    })?;

    // 3. `/summary show` displays project continuity summary + provenance + retention note.
    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"/summary show\r")?;
        w.flush()?;
    }
    screen.wait_for("summary output rendered", Duration::from_secs(5), |s| {
        s.contains("Project Continuity Summary")
            && s.contains("Ship M3 PTY verification")
            && s.contains("Revision 2")
    })?;

    {
        let mut w = master_write.lock().expect("write lock");
        w.write_all(b"\x11")?;
        w.flush()?;
    }
    let status = child.wait_with_timeout(Duration::from_secs(5))?;
    assert!(status.success(), "m3 pty exited with {status:?}");
    drop(master_write);
    let _ = reader.join();
    Ok(())
}
