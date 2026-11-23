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
    DiffHunk, DiffLine, ReasoningEffort, ToolResultPresentation, UiActivitySnapshot,
    UiActivityState, UiEvent, UiInputStatus, UiModelBinding, UiModelCatalog, UiModelChoice,
    UiModelControlSource, UiModelSnapshot, UiQueueSnapshot, UserInputRecord,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::reference::MemFs;
use alan_ap::{Fid, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use alan_tui::{FileBackedRunConfig, run_file_backed, terminal};
use anyhow::{Context, Result, bail};

const CHILD_MODE_ENV: &str = "ALAN_PTY_APPROVAL_CHILD";
const CHILD_LOG_ENV: &str = "ALAN_PTY_APPROVAL_LOG_DIR";
const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";

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
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.parser.process(bytes);
        self.shared.1.notify_all();
    }

    fn mark_eof(&self) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.eof = true;
        self.shared.1.notify_all();
    }

    fn cursor_one_based(&self) -> (u16, u16) {
        let guard = self.shared.0.lock().expect("pty state lock");
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
        let mut guard = self.shared.0.lock().expect("pty state lock");
        loop {
            let contents = guard.parser.screen().contents();
            if predicate(&contents) {
                return Ok(contents);
            }
            if guard.eof {
                bail!("PTY reached EOF before `{label}` was satisfied.\nScreen:\n{contents}");
            }
            let now = Instant::now();
            if now >= deadline {
                bail!("timed out waiting for `{label}`.\nScreen:\n{contents}");
            }
            let (next_guard, wait_res) = self
                .shared
                .1
                .wait_timeout(guard, deadline - now)
                .expect("condvar wait");
            guard = next_guard;
            if wait_res.timed_out() {
                let contents = guard.parser.screen().contents();
                if predicate(&contents) {
                    return Ok(contents);
                }
                bail!("timed out waiting for `{label}`.\nScreen:\n{contents}");
            }
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
                bail!("child process did not exit within {timeout:?}");
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
    // SAFETY: `master`, `slave`, and `ws` point to local values; `name` and `termp` are null.
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
    // SAFETY: `openpty` returned newly owned file descriptors.
    Ok(unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) })
}

fn spawn_in_pty(
    slave: &OwnedFd,
    master_to_close: RawFd,
    mode: &str,
    extra_env: &[(&str, &OsStr)],
) -> Result<Child> {
    let exe = std::env::current_exe().context("locate test executable")?;
    let slave_fd: RawFd = std::os::fd::AsRawFd::as_raw_fd(slave);
    let mut cmd = Command::new(exe);
    cmd.args([
        "--exact",
        "pty_e2e_approval_queue_child_runner",
        "--nocapture",
    ])
    .env(CHILD_MODE_ENV, mode)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    // SAFETY: `pre_exec` uses async-signal-safe POSIX session and descriptor syscalls only.
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

    cmd.spawn().context("spawn pty child")
}

fn spawn_master_reader(
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

async fn allocate_clone_id(transport: &InProcessTransport, path: &str) -> Result<String> {
    let fid = Fid(92_000);
    let names = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    transport
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names,
        })
        .await?;
    transport
        .call(Request::Open {
            fid,
            mode: OpenMode::ReadWrite,
        })
        .await?;
    let bytes = match transport
        .call(Request::Read {
            fid,
            offset: 0,
            count: 128,
        })
        .await?
    {
        Response::Read { data } => data,
        other => bail!("unexpected clone read response: {other:?}"),
    };
    let _ = transport.call(Request::Clunk { fid }).await;
    Ok(String::from_utf8(bytes)?.trim().to_string())
}

async fn write_ui_event(shell: &Shell, owner: &str, event: &UiEvent) {
    if let Ok(mut bytes) = serde_json::to_vec(event) {
        bytes.push(b'\n');
        let _ = shell
            .write(&format!("{owner}/machine/ui/events"), &bytes)
            .await;
    }
}

async fn write_ui_state(
    shell: &Shell,
    owner: &str,
    activity: UiActivitySnapshot,
    queue: UiQueueSnapshot,
) {
    let _ = shell
        .write(
            &format!("{owner}/machine/ui/activity"),
            &serde_json::to_vec(&activity).unwrap_or_default(),
        )
        .await;
    write_ui_event(shell, owner, &UiEvent::Activity { snapshot: activity }).await;
    let _ = shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(&queue).unwrap_or_default(),
        )
        .await;
}

fn wait_for_json_file(path: &std::path::Path, timeout: Duration) -> Result<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let last_err = match std::fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(val) => return Ok(val),
                Err(err) => format!("json parse err ({err}) for raw={raw:?}"),
            },
            Err(err) => format!("read err: {err}"),
        };
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for JSON file {} ({last_err})",
                path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

async fn emit_confirmation_and_wait_response(
    shell: &Shell,
    transport: &InProcessTransport,
    owner: &str,
    options: serde_json::Value,
    prompt: &str,
    log_path: PathBuf,
) -> Result<()> {
    let req_id = allocate_clone_id(transport, &format!("{owner}/requests/clone")).await?;
    shell
        .write(&format!("{owner}/requests/{req_id}/kind"), b"confirmation")
        .await?;
    shell
        .write(
            &format!("{owner}/requests/{req_id}/prompt"),
            prompt.as_bytes(),
        )
        .await?;
    shell
        .write(
            &format!("{owner}/requests/{req_id}/options"),
            options.to_string().as_bytes(),
        )
        .await?;
    for _ in 0..800 {
        if let Ok(bytes) = shell
            .cat(&format!("{owner}/requests/{req_id}/response"))
            .await
            && !bytes.is_empty()
        {
            std::fs::write(&log_path, &bytes)?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    bail!("timed out waiting for request response {req_id}");
}

async fn run_child(log_dir: PathBuf) -> Result<()> {
    terminal::install_panic_hook();

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
    let transport = InProcessTransport::new(mount_fs);
    let shell = Shell::new(transport.clone());

    let pid = shell.spawn(EXEC_SPEC).await?;
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;
    live.mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "pid",
            format!("{pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    let owner = format!("/agent/{pid}");
    let binding = UiModelBinding {
        profile: "openai-main".into(),
        provider: "openai_responses".into(),
        model: "gpt-5.4".into(),
        reasoning: Default::default(),
        control_source: UiModelControlSource::AgentConfig,
    };
    let model = UiModelSnapshot {
        version: 1,
        process_path: format!("/proc/{pid}"),
        publication_version: 1,
        known: true,
        selected_next: Some(binding.clone()),
        active: Some(binding),
        catalog: Some(UiModelCatalog {
            profile: "openai-main".into(),
            models: vec![UiModelChoice {
                model: "gpt-5.4".into(),
                supported_reasoning_efforts: vec![],
                default_reasoning_effort: Some(ReasoningEffort::Medium),
            }],
            ..Default::default()
        }),
        admitted: Vec::new(),
    };
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&model)?,
        )
        .await?;
    write_ui_state(
        &shell,
        &owner,
        UiActivitySnapshot::idle(),
        UiQueueSnapshot {
            known: true,
            revision: 1,
            ..Default::default()
        },
    )
    .await;

    let io_shell = shell.clone();
    let io_transport = transport.clone();
    let io_owner = owner.clone();
    let io_task = tokio::spawn(async move {
        let Ok(mut tail) = io_shell.tail(&format!("{io_owner}/io/input")).await else {
            return;
        };
        let mut buf: Vec<u8> = Vec::new();
        let mut rev = 1u64;
        let mut active_sub: Option<String> = None;
        while let Ok(bytes) = tail.read(4096).await {
            if bytes.is_empty() {
                break;
            }
            buf.extend_from_slice(&bytes);
            while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
                let Ok(len_str) = std::str::from_utf8(&buf[..nl]) else {
                    buf.drain(..=nl);
                    continue;
                };
                let Ok(len) = len_str.trim().parse::<usize>() else {
                    buf.drain(..=nl);
                    continue;
                };
                if buf.len() < nl + 1 + len {
                    break;
                }
                let payload = buf[nl + 1..nl + 1 + len].to_vec();
                buf.drain(..nl + 1 + len);
                let Ok(Some(record)) = UserInputRecord::decode_payload(&payload) else {
                    continue;
                };
                let body = record.body.trim().to_string();
                let sub_id = record.submission_id.clone();

                match body.as_str() {
                    "bash-escalate" => {
                        rev += 1;
                        write_ui_state(
                            &io_shell,
                            &io_owner,
                            UiActivitySnapshot::running(10_000),
                            UiQueueSnapshot {
                                known: true,
                                revision: rev,
                                active_submission_ids: vec![sub_id.clone()],
                                ..Default::default()
                            },
                        )
                        .await;
                        let options = serde_json::json!({
                            "title": "Escalate tool call `bash`",
                            "options": ["approve", "allow_session", "reject"],
                            "default_option": "approve",
                            "details": {
                                "capability": "bash",
                                "reason": "Run workspace test command",
                                "presentation": ToolResultPresentation::Command {
                                    cmdline: "cargo test -p alan-terminal-ui".into(),
                                    exit_code: None,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    truncated: false,
                                }
                            }
                        });
                        let _ = emit_confirmation_and_wait_response(
                            &io_shell,
                            &io_transport,
                            &io_owner,
                            options,
                            "Run command outside sandbox?",
                            log_dir.join("bash.json"),
                        )
                        .await;
                        rev += 1;
                        write_ui_state(
                            &io_shell,
                            &io_owner,
                            UiActivitySnapshot::idle(),
                            UiQueueSnapshot {
                                known: true,
                                revision: rev,
                                ..Default::default()
                            },
                        )
                        .await;
                        write_ui_event(
                            &io_shell,
                            &io_owner,
                            &UiEvent::InputCompleted {
                                submission_ids: vec![sub_id],
                                status: UiInputStatus::Completed,
                                error: None,
                            },
                        )
                        .await;
                    }
                    "diff-escalate" => {
                        rev += 1;
                        write_ui_state(
                            &io_shell,
                            &io_owner,
                            UiActivitySnapshot::running(11_000),
                            UiQueueSnapshot {
                                known: true,
                                revision: rev,
                                active_submission_ids: vec![sub_id.clone()],
                                ..Default::default()
                            },
                        )
                        .await;
                        let options = serde_json::json!({
                            "title": "Escalate tool call `edit_file`",
                            "options": ["approve", "allow_session", "reject"],
                            "default_option": "approve",
                            "details": {
                                "presentation": ToolResultPresentation::Diff {
                                    path: "src/main.rs".into(),
                                    hunks: vec![DiffHunk {
                                        header: Some("@@ -1 +1 @@".into()),
                                        lines: vec![
                                            DiffLine::Removed { text: "old_fn()".into() },
                                            DiffLine::Added { text: "new_fn()".into() },
                                        ],
                                    }],
                                }
                            }
                        });
                        let _ = emit_confirmation_and_wait_response(
                            &io_shell,
                            &io_transport,
                            &io_owner,
                            options,
                            "Allow file mutation?",
                            log_dir.join("diff.json"),
                        )
                        .await;
                        if let Ok(act_id) =
                            allocate_clone_id(&io_transport, &format!("{io_owner}/actions/clone"))
                                .await
                        {
                            let _ = io_shell
                                .write(&format!("{io_owner}/actions/{act_id}/name"), b"edit_file")
                                .await;
                            let _ = io_shell
                                .write(&format!("{io_owner}/actions/{act_id}/status"), b"completed")
                                .await;
                        }
                        rev += 1;
                        write_ui_state(
                            &io_shell,
                            &io_owner,
                            UiActivitySnapshot::idle(),
                            UiQueueSnapshot {
                                known: true,
                                revision: rev,
                                ..Default::default()
                            },
                        )
                        .await;
                        write_ui_event(
                            &io_shell,
                            &io_owner,
                            &UiEvent::InputCompleted {
                                submission_ids: vec![sub_id],
                                status: UiInputStatus::Completed,
                                error: None,
                            },
                        )
                        .await;
                    }
                    "start-running" => {
                        active_sub = Some(sub_id.clone());
                        rev += 1;
                        write_ui_state(
                            &io_shell,
                            &io_owner,
                            UiActivitySnapshot::running(12_000),
                            UiQueueSnapshot {
                                known: true,
                                revision: rev,
                                active_submission_ids: vec![sub_id],
                                ..Default::default()
                            },
                        )
                        .await;
                    }
                    "follow up build check" => {
                        rev += 1;
                        let active = active_sub.clone().into_iter().collect();
                        write_ui_state(
                            &io_shell,
                            &io_owner,
                            UiActivitySnapshot {
                                version: 1,
                                state: UiActivityState::Running,
                                started_at_ms: Some(12_000),
                                waiting_submission_ids: Vec::new(),
                            },
                            UiQueueSnapshot {
                                known: true,
                                revision: rev,
                                active_submission_ids: active,
                                pending_submission_ids: vec![
                                    sub_id,
                                    "ext-1".into(),
                                    "ext-2".into(),
                                ],
                                ..Default::default()
                            },
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        }
    });

    let mut cfg = FileBackedRunConfig::new(transport, "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    run_file_backed(cfg).await?;
    io_task.abort();
    Ok(())
}

#[test]
fn pty_e2e_approval_queue_child_runner() {
    let Some(mode) = std::env::var_os(CHILD_MODE_ENV) else {
        return;
    };
    let mode = mode.to_string_lossy().into_owned();
    if mode == "styled" || mode == "dumb" {
        let log_dir = PathBuf::from(std::env::var_os(CHILD_LOG_ENV).expect("log dir"));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        if let Err(err) = rt.block_on(run_child(log_dir)) {
            eprintln!("pty approval child failed: {err:#}");
            std::process::exit(2);
        }
    }
}

#[test]
fn pty_e2e_confirmation_selector_reject_note_and_queued_preview() -> Result<()> {
    let log_dir = std::env::temp_dir().join(format!("alan-pty-approval-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&log_dir);
    std::fs::create_dir_all(&log_dir)?;

    let (master, slave) = open_pty(32, 100)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(
        &slave,
        master_fd,
        "styled",
        &[
            (CHILD_LOG_ENV, log_dir.as_os_str()),
            ("TERM", OsStr::new("xterm-256color")),
            ("COLORTERM", OsStr::new("truecolor")),
        ],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(32, 100);
    let _reader = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        screen.wait_for("initial prompt", Duration::from_secs(5), |s| {
            s.contains("gpt-5.4") && s.contains(": ")
        })?;

        // 1. Bash escalation: pre-execution Command preview + reject-with-note flow + zero leakage.
        write_master(&master_write, b"bash-escalate\r")?;
        let bash_prompt =
            screen.wait_for("bash confirmation selector", Duration::from_secs(5), |s| {
                s.contains("Approval required")
                    && s.contains("$ cargo test -p alan-terminal-ui")
                    && s.contains("[1] approve (default)")
                    && s.contains("[4] Reject with note")
                    && s.contains("choose · Enter confirm · Esc cancel")
            })?;
        assert!(!bash_prompt.contains("exit "), "{bash_prompt}");

        // Printable 'z' must be swallowed, not leaked into composer.
        write_master(&master_write, b"z")?;
        // Navigate Down x3 -> Reject with note…, Enter note editor, type draft, Esc back, then press 4.
        write_master(&master_write, b"\x1b[B\x1b[B\x1b[B\r")?;
        screen.wait_for("note input opened", Duration::from_secs(3), |s| {
            s.contains("Note:") && s.contains("Enter submit rejection note · Esc back to options")
        })?;
        write_master(&master_write, b"tmp\x1b")?;
        screen.wait_for("note exited back to options", Duration::from_secs(3), |s| {
            s.contains("choose · Enter confirm · Esc cancel")
                && !s.contains("Enter submit rejection note")
        })?;
        write_master(&master_write, b"4\x15use rg instead\r")?;
        let after_reject =
            screen.wait_for("settled reject receipt", Duration::from_secs(5), |s| {
                s.contains("✕ Rejected · bash: cargo test -p alan-terminal-ui (use rg instead)")
            })?;
        assert!(!after_reject.contains("[1] approve"), "{after_reject}");
        assert!(!after_reject.contains(": z"), "{after_reject}");

        let bash_resp = wait_for_json_file(&log_dir.join("bash.json"), Duration::from_secs(3))?;
        assert_eq!(bash_resp["choice"], "reject");
        assert_eq!(bash_resp["modifications"], "use rg instead");

        // 2. Edit diff escalation: pre-execution Diff preview + digit `2` + Ctrl+O idle footer.
        write_master(&master_write, b"diff-escalate\r")?;
        screen.wait_for("diff confirmation selector", Duration::from_secs(5), |s| {
            s.contains("src/main.rs") && s.contains("-old_fn()") && s.contains("+new_fn()")
        })?;
        write_master(&master_write, b"2")?;
        let after_diff = screen.wait_for(
            "settled allow receipt + details footer",
            Duration::from_secs(5),
            |s| {
                s.contains("✓ Allowed for session · edit_file src/main.rs")
                    && s.contains("Ctrl+O details (p: plans, t: tape)")
            },
        )?;
        assert!(!after_diff.contains("[1] approve"), "{after_diff}");
        assert!(!after_diff.contains(": 2"), "{after_diff}");

        let diff_resp = wait_for_json_file(&log_dir.join("diff.json"), Duration::from_secs(3))?;
        assert_eq!(diff_resp["choice"], "allow_session");

        // 3. Running turn footer + queued preview above composer + no `Input active` / `· input queued`.
        write_master(&master_write, b"start-running\r")?;
        screen.wait_for("running footer", Duration::from_secs(4), |s| {
            s.contains("Working ·")
                && s.contains("Enter queue next · Ctrl+S steer now · Esc interrupt")
        })?;
        write_master(&master_write, b"follow up build check\r")?;
        let queued_screen =
            screen.wait_for("queued preview rows", Duration::from_secs(5), |s| {
                s.contains("· 3 queued")
                    && s.contains("↳ Queued: follow up build check")
                    && s.contains("↳ Queued: queued input")
                    && s.contains("+1 more queued")
            })?;
        assert!(!queued_screen.contains("Input active"), "{queued_screen}");
        assert!(!queued_screen.contains("input queued"), "{queued_screen}");

        write_master(&master_write, b"\x11")?;
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(status.success(), "PTY child failed: {status:?}");
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&log_dir);
    res
}

#[test]
fn pty_e2e_dumb_terminal_confirmation_and_queue_preview() -> Result<()> {
    let log_dir = std::env::temp_dir().join(format!("alan-pty-dumb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&log_dir);
    std::fs::create_dir_all(&log_dir)?;

    let (master, slave) = open_pty(28, 90)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(
        &slave,
        master_fd,
        "dumb",
        &[
            (CHILD_LOG_ENV, log_dir.as_os_str()),
            ("TERM", OsStr::new("dumb")),
            ("NO_COLOR", OsStr::new("1")),
        ],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(28, 90);
    let _reader = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        screen.wait_for("dumb prompt", Duration::from_secs(5), |s| {
            s.contains("gpt-5.4") && s.contains(": ")
        })?;

        write_master(&master_write, b"bash-escalate\r")?;
        let dumb_conf = screen.wait_for("dumb confirmation", Duration::from_secs(5), |s| {
            s.contains("> [1] approve (default)") && s.contains("$ cargo test -p alan-terminal-ui")
        })?;
        assert!(!dumb_conf.contains("▶"), "{dumb_conf}");

        write_master(&master_write, b"\r")?;
        screen.wait_for("dumb settled approved", Duration::from_secs(5), |s| {
            s.contains("+ Approved · bash: cargo test -p alan-terminal-ui")
        })?;
        let dumb_bash = wait_for_json_file(&log_dir.join("bash.json"), Duration::from_secs(3))?;
        assert_eq!(dumb_bash["choice"], "approve");

        write_master(&master_write, b"start-running\r")?;
        screen.wait_for("dumb running", Duration::from_secs(4), |s| {
            s.contains("Enter queue next")
        })?;
        write_master(&master_write, b"follow up build check\r")?;
        let dumb_queued = screen.wait_for("dumb queued preview", Duration::from_secs(5), |s| {
            s.contains("> Queued: follow up build check")
        })?;
        assert!(!dumb_queued.contains("↳"), "{dumb_queued}");

        write_master(&master_write, b"\x11")?;
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(status.success(), "dumb PTY child failed: {status:?}");
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&log_dir);
    res
}
