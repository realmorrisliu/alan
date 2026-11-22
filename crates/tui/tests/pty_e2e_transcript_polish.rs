#![cfg(unix)]

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use alan_agent_protocol::{
    ReasoningControls, UiEvent, UiInputStatus, UiModelBinding, UiModelControlSource,
    UiModelSnapshot, UiThinkingSnapshot, UserInputRecord,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_tui::{FileBackedRunConfig, run_file_backed};
use anyhow::{Context, Result, bail};

const CHILD_ENV: &str = "ALAN_PTY_TRANSCRIPT_POLISH_CHILD";

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

    fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        let guard = self.shared.0.lock().expect("pty lock");
        f(guard.parser.screen())
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
    cmd.args([
        "--exact",
        "pty_e2e_transcript_polish_child_runner",
        "--nocapture",
    ])
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
    cmd.spawn().context("spawn transcript polish pty child")
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

fn write_master(master: &Arc<Mutex<std::fs::File>>, data: &[u8]) -> Result<()> {
    let mut w = master.lock().expect("write lock");
    w.write_all(data)?;
    w.flush()?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn run_polish_session() -> Result<()> {
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
    let binding = UiModelBinding {
        profile: "magpie-main".into(),
        provider: "magpie".into(),
        model: "auto/deepseek-v4.1-flash".into(),
        reasoning: ReasoningControls { effort: None },
        control_source: UiModelControlSource::AgentMachineOverride,
    };
    let model_snapshot = UiModelSnapshot {
        version: 1,
        known: true,
        process_path: format!("/proc/{pid}"),
        publication_version: 1,
        selected_next: Some(binding.clone()),
        active: Some(binding),
        ..Default::default()
    };
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&model_snapshot)?,
        )
        .await?;

    let responder_shell = shell.clone();
    let responder_owner = owner.clone();
    let responder = tokio::spawn(async move {
        let Ok(mut tail) = responder_shell
            .tail(&format!("{responder_owner}/io/input"))
            .await
        else {
            return;
        };
        let mut buf = Vec::new();
        while let Ok(bytes) = tail.read(4096).await {
            if bytes.is_empty() {
                break;
            }
            buf.extend_from_slice(&bytes);
            while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
                let Ok(len_str) = std::str::from_utf8(&buf[..nl]) else {
                    buf.clear();
                    break;
                };
                let Ok(len) = len_str.trim().parse::<usize>() else {
                    buf.clear();
                    break;
                };
                if buf.len() < nl + 1 + len {
                    break;
                }
                let payload: Vec<u8> = buf.drain(..nl + 1 + len).skip(nl + 1).collect();
                let Ok(Some(input)) = UserInputRecord::decode_payload(&payload) else {
                    continue;
                };
                let user_rec = serde_json::json!({
                    "version": 1,
                    "kind": "message",
                    "role": "user",
                    "content": input.body,
                    "submission_id": input.submission_id,
                });
                let _ = responder_shell
                    .write(
                        &format!("{responder_owner}/machine/tape"),
                        format!("{user_rec}\n").as_bytes(),
                    )
                    .await;
                if input.body == "hello" {
                    let thinking = UiEvent::Thinking {
                        snapshot: UiThinkingSnapshot::complete("internal reasoning step".into(), 0),
                    };
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    let _ = responder_shell
                        .write(
                            &format!("{responder_owner}/machine/ui/events"),
                            format!("{}\n", serde_json::to_string(&thinking).unwrap()).as_bytes(),
                        )
                        .await;
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    let reply = concat!(
                        "See `AGENTS.md` and [docs](https://example.com).\n",
                        "- alpha beta gamma delta epsilon zeta eta theta iota\n",
                        "10. one two three four five six seven eight nine ten"
                    );
                    let assistant_rec = serde_json::json!({
                        "version": 1,
                        "kind": "message",
                        "role": "assistant",
                        "content": reply,
                    });
                    let _ = responder_shell
                        .write(
                            &format!("{responder_owner}/machine/tape"),
                            format!("{assistant_rec}\n").as_bytes(),
                        )
                        .await;
                } else {
                    // Intentionally race io/output BEFORE UiEvent::Thinking::Complete to verify
                    // the renderer still places `· thought · 2s` above the streamed reply.
                    let _ = responder_shell
                        .write(&format!("{responder_owner}/io/output"), b"I am Alan.")
                        .await;
                    tokio::time::sleep(Duration::from_millis(60)).await;
                    let thinking = UiEvent::Thinking {
                        snapshot: UiThinkingSnapshot::complete("identity check".into(), 2),
                    };
                    let _ = responder_shell
                        .write(
                            &format!("{responder_owner}/machine/ui/events"),
                            format!("{}\n", serde_json::to_string(&thinking).unwrap()).as_bytes(),
                        )
                        .await;
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    let assistant_rec = serde_json::json!({
                        "version": 1,
                        "kind": "message",
                        "role": "assistant",
                        "content": "I am Alan.",
                    });
                    let _ = responder_shell
                        .write(
                            &format!("{responder_owner}/machine/tape"),
                            format!("{assistant_rec}\n").as_bytes(),
                        )
                        .await;
                }
                let completed = UiEvent::InputCompleted {
                    submission_ids: vec![input.submission_id],
                    status: UiInputStatus::Completed,
                    error: None,
                };
                let _ = responder_shell
                    .write(
                        &format!("{responder_owner}/machine/ui/events"),
                        format!("{}\n", serde_json::to_string(&completed).unwrap()).as_bytes(),
                    )
                    .await;
            }
        }
    });

    let mut cfg = FileBackedRunConfig::new(InProcessTransport::new(mount_fs), "/agent/root");
    cfg.sandbox_backend = Some("seatbelt".into());
    let res = run_file_backed(cfg).await;
    responder.abort();
    res
}

#[test]
fn pty_e2e_transcript_polish_child_runner() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    if let Err(err) = run_polish_session() {
        eprintln!("transcript polish pty child failed: {err:#}");
        std::process::exit(2);
    }
}

fn find_token_position(screen: &vt100::Screen, needle: &str) -> Option<(u16, u16)> {
    let (rows, cols) = screen.size();
    for r in 0..rows.get() {
        let mut line = String::new();
        for c in 0..cols.get() {
            let sym = screen.cell(r, c).map(|cell| cell.contents()).unwrap_or("");
            line.push_str(if sym.is_empty() { " " } else { sym });
        }
        if let Some(col) = line.find(needle) {
            return Some((r, col as u16));
        }
    }
    None
}

fn run_case(extra_env: &[(&str, &OsStr)], expect_styled: bool) -> Result<()> {
    // 32 rows x 46 cols keeps Welcome + 2 turns visible while wrapping lists.
    let (master, slave) = open_pty(32, 46)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
    let child = spawn_child(&slave, master_fd, extra_env)?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(32, 46);
    let reader = spawn_reader(master_read, master_write.clone(), screen.clone());

    let startup = screen.wait_for("startup with model", Duration::from_secs(6), |s| {
        s.contains("ALAN") && s.contains("auto/deepseek-v4.1-flash")
    })?;
    assert!(
        !startup.contains("effort unknown"),
        "status bar leaked effort unknown:\n{startup}"
    );

    write_master(&master_write, b"hello\r")?;
    screen.wait_for("turn 1 settled", Duration::from_secs(5), |s| {
        s.contains("· thought · <1s") && s.contains("AGENTS.md") && s.contains("10. one")
    })?;

    write_master(&master_write, b"who are you\r")?;
    let settled = screen.wait_for("turn 2 settled", Duration::from_secs(5), |s| {
        s.contains(": who are you") && s.contains("· thought · 2s") && s.contains("I am Alan.")
    })?;

    assert!(!settled.contains("effort unknown"), "{settled}");
    assert!(!settled.contains("thinking · 0s"), "{settled}");
    assert!(!settled.contains("ctrl+r to expand"), "{settled}");

    let lines: Vec<&str> = settled.lines().map(str::trim_end).collect();
    let turn1_idx = lines
        .iter()
        .position(|line| *line == ": hello")
        .context("missing `: hello` row")?;
    assert!(turn1_idx >= 2, "{lines:?}");
    assert_eq!(
        lines[turn1_idx - 1],
        "",
        "missing blank row between Welcome and `: hello`:\n{settled}"
    );
    assert_eq!(
        lines[turn1_idx + 1],
        "· thought · <1s",
        "thought summary should pair directly with turn 1:\n{settled}"
    );
    assert!(
        lines[turn1_idx + 2].contains("AGENTS.md"),
        "reply should pair cleanly with its turn without orphan blank row:\n{settled}"
    );

    let turn2_idx = lines
        .iter()
        .position(|line| *line == ": who are you")
        .context("missing `: who are you` row")?;
    assert_eq!(
        lines[turn2_idx - 1],
        "",
        "missing blank row between turn 1 reply and `: who are you`:\n{settled}"
    );
    assert_eq!(
        lines[turn2_idx + 1],
        "· thought · 2s",
        "late Thinking::Complete arriving after io/output must still render above reply:\n{settled}"
    );
    assert_eq!(lines[turn2_idx + 2], "I am Alan.", "{settled}");

    // Verify hanging indent on wrapped unordered and ordered list items.
    let bullet_prefix = if expect_styled { "• " } else { "- " };
    let bullet_idx = lines
        .iter()
        .position(|line| line.starts_with(bullet_prefix) && line.contains("alpha"))
        .context("missing bullet row")?;
    assert!(
        lines[bullet_idx + 1].starts_with("  "),
        "unordered list continuation missing hanging indent:\n{settled}"
    );
    let ordered_idx = lines
        .iter()
        .position(|line| line.starts_with("10. "))
        .context("missing ordered list row")?;
    assert!(
        lines[ordered_idx + 1].starts_with("    "),
        "ordered list continuation missing hanging indent:\n{settled}"
    );

    // Inspect live VT100 cell attributes: inline code must NOT be underlined; links ARE underlined when styled.
    screen.with_screen(|s| {
        let (code_row, code_col) =
            find_token_position(s, "AGENTS.md").expect("AGENTS.md on screen");
        for offset in 0.."AGENTS.md".len() as u16 {
            let cell = s.cell(code_row, code_col + offset).expect("code cell");
            assert!(
                !cell.underline(),
                "inline code cell must not be underlined at ({code_row}, {})",
                code_col + offset
            );
            if !expect_styled {
                assert_eq!(cell.fgcolor(), vt100::Color::Default);
                assert_eq!(cell.bgcolor(), vt100::Color::Default);
            }
        }
        let (link_row, link_col) = find_token_position(s, "docs").expect("docs link on screen");
        let link_cell = s.cell(link_row, link_col).expect("link cell");
        assert_eq!(
            link_cell.underline(),
            expect_styled,
            "link underline mismatch (expect_styled={expect_styled})"
        );
    });

    // Ctrl+R expands and re-collapses thinking live in the PTY.
    write_master(&master_write, b"\x12")?;
    screen.wait_for("ctrl+r expands thinking", Duration::from_secs(3), |s| {
        s.contains("internal reasoning step")
    })?;
    write_master(&master_write, b"\x12")?;
    screen.wait_for("ctrl+r collapses thinking", Duration::from_secs(3), |s| {
        s.contains("· thought · <1s") && !s.contains("internal reasoning step")
    })?;

    write_master(&master_write, b"\x11")?;
    let status = child.wait_with_timeout(Duration::from_secs(5))?;
    assert!(status.success(), "pty child exited with {status:?}");
    drop(master_write);
    let _ = reader.join();
    Ok(())
}

#[test]
fn pty_e2e_transcript_polish_styled_and_dumb() -> Result<()> {
    run_case(&[], true)?;
    run_case(
        &[
            ("TERM", OsStr::new("dumb")),
            ("NO_COLOR", OsStr::new("1")),
            ("COLORTERM", OsStr::new("")),
        ],
        false,
    )?;
    Ok(())
}
