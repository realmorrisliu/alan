#![cfg(unix)]

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry};
use alan_ap::{Fid, FileKind, InProcessTransport, OpenMode, Request};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_os_host::{AlanOsHost, HostBootConfig, HostEndpointPaths, LocalAttachment};
use alan_shell::Shell;
use alan_tui::{
    FileBackedRunConfig, WorkstreamControl, WorkstreamControlHandler, WorkstreamControlResult,
    WorkstreamCountsDto, WorkstreamEntryDto, WorkstreamOverviewDto, WorkstreamPersistenceKind,
    WorkstreamStatusKind, run_file_backed, terminal,
};
use anyhow::{Context, Result, bail};

const CHILD_ENV: &str = "ALAN_WORKSTREAM_PTY_CHILD";

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
                        500,
                    ),
                    eof: false,
                }),
                Condvar::new(),
            )),
        }
    }

    fn push_bytes(&self, bytes: &[u8]) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.parser.process(bytes);
        self.shared.1.notify_all();
    }

    fn mark_eof(&self) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.eof = true;
        self.shared.1.notify_all();
    }

    fn resize_parser(&self, rows: u16, cols: u16) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.parser.screen_mut().set_size(
            NonZeroU16::new(rows).expect("nonzero rows"),
            NonZeroU16::new(cols).expect("nonzero cols"),
        );
        self.shared.1.notify_all();
    }

    fn cursor_one_based(&self) -> (u16, u16) {
        let guard = self.shared.0.lock().expect("pty state lock");
        let (r, c) = guard.parser.screen().cursor_position();
        (r.saturating_add(1), c.saturating_add(1))
    }

    fn wait_for<F>(&self, label: &str, timeout: Duration, mut predicate: F) -> Result<String>
    where
        F: FnMut(&str) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut guard = self.shared.0.lock().expect("pty state lock");
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
    let exe = std::env::current_exe().context("locate test executable")?;
    let slave_fd = slave.as_raw_fd();
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", "workstream_pty_host_child_runner", "--nocapture"])
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
    cmd.spawn().context("spawn workstream PTY child")
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
    let mut guard = master.lock().expect("master write lock");
    guard.write_all(data)?;
    guard.flush()?;
    Ok(())
}

fn resize_pty(fd: RawFd, rows: u16, cols: u16) -> Result<()> {
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: `fd` is an open PTY master descriptor and `ws` is a valid `libc::winsize`.
    let rc = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ as _, &ws as *const libc::winsize) };
    if rc == -1 {
        return Err(std::io::Error::last_os_error()).context("TIOCSWINSZ");
    }
    Ok(())
}

async fn install_routefs_rule(
    transport: &InProcessTransport,
    name: &str,
    rule_json: &[u8],
) -> Result<()> {
    let dir_fid = Fid(81_001);
    let rule_fid = Fid(81_002);
    transport
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: dir_fid,
            names: vec!["mnt".into(), "route".into(), "rules".into()],
        })
        .await
        .map_err(|e| anyhow::anyhow!("walk /mnt/route/rules: {e:?}"))?;
    transport
        .call(Request::Create {
            fid: dir_fid,
            newfid: rule_fid,
            name: name.to_string(),
            kind: FileKind::File,
        })
        .await
        .map_err(|e| anyhow::anyhow!("create route rule: {e:?}"))?;
    transport
        .call(Request::Open {
            fid: rule_fid,
            mode: OpenMode::Write,
        })
        .await
        .map_err(|e| anyhow::anyhow!("open route rule: {e:?}"))?;
    transport
        .call(Request::Write {
            fid: rule_fid,
            offset: 0,
            data: rule_json.to_vec(),
        })
        .await
        .map_err(|e| anyhow::anyhow!("write route rule: {e:?}"))?;
    let _ = transport.call(Request::Clunk { fid: rule_fid }).await;
    let _ = transport.call(Request::Clunk { fid: dir_fid }).await;
    Ok(())
}

async fn run_host_pty_child() -> Result<()> {
    terminal::install_panic_hook();

    let runtime_dir = tempfile::tempdir()?;
    let paths = HostEndpointPaths::from_runtime_dir(runtime_dir.path())?;
    let mock = MockLlmProvider::new().with_responses(vec![GenerationResponse {
        content: "host-root-ok".to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }]);
    let mut proc_cfg = AgentProcessConfig::default();
    proc_cfg.agent_config.core_config.memory.enabled = false;
    let host = AlanOsHost::boot(
        HostBootConfig::ephemeral(proc_cfg, LlmClient::new(mock), ToolRegistry::new()),
        paths.clone(),
    )
    .await?;
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        host.serve_until(async move {
            let _ = stopped.await;
        })
        .await
    });

    let attachment = LocalAttachment::new(paths).connect().await?;
    let shell = Shell::new(attachment.root.clone());

    // Install a real RouteFs rule at /mnt/route/rules/reroute and emit a reroute_request via /mnt/route/send.
    let rule = serde_json::json!({
        "version": 1,
        "match_type": "reroute_request",
        "port": "root",
        "reason": "forward misdirected stream input to Root"
    });
    install_routefs_rule(&attachment.root, "reroute", &serde_json::to_vec(&rule)?).await?;

    let initial_overview = WorkstreamOverviewDto {
        project_key: Some("proj-host-pty".into()),
        counts: WorkstreamCountsDto {
            running: 1,
            idle: 1,
            paused_error: 0,
            archived: 0,
            unread_inbox: 1,
        },
        streams: vec![
            WorkstreamEntryDto {
                name: "alpha".into(),
                scope: "Alpha scope".into(),
                persistence: WorkstreamPersistenceKind::Persistent,
                state: WorkstreamStatusKind::Running,
                child_pid: None,
                model_override: None,
                rollout_ref: None,
                updated_at: "2026-11-25T08:00:00Z".into(),
            },
            WorkstreamEntryDto {
                name: "beta".into(),
                scope: "Beta scope".into(),
                persistence: WorkstreamPersistenceKind::Persistent,
                state: WorkstreamStatusKind::Idle,
                child_pid: None,
                model_override: None,
                rollout_ref: None,
                updated_at: "2026-11-25T07:00:00Z".into(),
            },
        ],
        unread_inbox: Vec::new(),
    };
    let overview_state = Arc::new(Mutex::new(initial_overview.clone()));
    let handler_state = overview_state.clone();
    let workstream_control: WorkstreamControlHandler = Arc::new(move |control| {
        let state = handler_state.clone();
        Box::pin(async move {
            let mut guard = state.lock().expect("overview lock");
            match control {
                WorkstreamControl::Inspect => Ok(WorkstreamControlResult::Overview(guard.clone())),
                WorkstreamControl::ClearInbox => {
                    guard.counts.unread_inbox = 0;
                    Ok(WorkstreamControlResult::ActionCompleted {
                        message: "Inbox cleared.".into(),
                        overview: guard.clone(),
                    })
                }
                WorkstreamControl::ResolveFocus { name } => {
                    Ok(WorkstreamControlResult::FocusUnavailable {
                        message: format!("stream '{name}' has no live child"),
                        name,
                    })
                }
                WorkstreamControl::Archive { name } | WorkstreamControl::Resume { name } => {
                    Ok(WorkstreamControlResult::ActionCompleted {
                        message: format!("Updated '{name}'."),
                        overview: guard.clone(),
                    })
                }
            }
        })
    });

    // Spawn background task that injects a real /mnt/route/send message once TUI is attached.
    let route_shell = shell.clone();
    let route_sender = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let handoff = serde_json::json!({
            "version": 1,
            "type": "reroute_request",
            "kind": "reroute_request",
            "source_stream": "alpha",
            "suggested_target": "beta",
            "hop_count": 1,
            "correlation_id": "host-pty-corr-1",
            "payload": "switch to beta"
        });
        let _ = route_shell
            .write("/mnt/route/send", &serde_json::to_vec(&handoff).unwrap())
            .await;
    });

    let mut cfg = FileBackedRunConfig::new(attachment.root, "/agent/root");
    cfg.initial_workstream_overview = Some(initial_overview);
    cfg.workstream_control = Some(workstream_control);

    run_file_backed(cfg).await?;
    route_sender.abort();
    let _ = shutdown.send(());
    server.await??;
    Ok(())
}

#[test]
fn workstream_pty_host_child_runner() {
    let Some(mode) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    assert_eq!(mode.to_string_lossy(), "1");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(err) = rt.block_on(run_host_pty_child()) {
        eprintln!("workstream host PTY child failed: {err:#}");
        std::process::exit(2);
    }
}

#[test]
fn alan_os_host_real_routefs_pty_renders_routing_receipt_status_counts_and_resize() -> Result<()> {
    let (master, slave) = open_pty(24, 120)?;
    let master_fd = master.as_raw_fd();
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let mut child = spawn_child(&slave, master_fd)?;
    drop(slave);

    let screen = PtyScreen::new(24, 120);
    let reader_handle = spawn_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        // Real RouteFs /mnt/route/send -> /mnt/route/log -> inline receipt + footer status counts.
        screen.wait_for(
            "host startup and real routefs receipt",
            Duration::from_secs(12),
            |s| {
                s.contains("1a·1i·1in")
                    && s.contains("rerouted from [alpha]")
                    && s.contains("[beta]")
            },
        )?;

        // Verify /streams inline output and /streams clear-inbox in real Host session.
        write_master(&master_write, b"/streams\r")?;
        screen.wait_for("/streams in host PTY", Duration::from_secs(4), |s| {
            s.contains("Workstreams") && s.contains("alpha") && s.contains("beta")
        })?;

        write_master(&master_write, b"/streams clear-inbox\r")?;
        screen.wait_for("clear-inbox count update", Duration::from_secs(4), |s| {
            s.contains("1a·1i·0in")
        })?;

        // Resize down to narrow 40-col pane and verify coherent rendering.
        resize_pty(master_fd, 18, 40)?;
        screen.resize_parser(18, 40);
        std::thread::sleep(Duration::from_millis(120));
        write_master(&master_write, b"narrow-ok")?;
        screen.wait_for("narrow pane responsive", Duration::from_secs(3), |s| {
            s.contains("narrow-ok")
        })?;
        write_master(&master_write, b"\x03")?;
        std::thread::sleep(Duration::from_millis(60));

        // Clean Ctrl+Q exit.
        write_master(&master_write, b"\x11")?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(status) = child.try_wait()? {
                assert!(status.success(), "child exited with {status:?}");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                bail!("child did not exit after Ctrl+Q");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    })();

    if res.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    drop(master_write);
    let _ = reader_handle.join();
    res
}
