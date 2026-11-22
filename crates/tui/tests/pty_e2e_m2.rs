#![cfg(unix)]

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use alan_agent_protocol::{
    DiffHunk, DiffLine, ReasoningEffort, ToolResultPresentation, UiActivitySnapshot,
    UiActivityState, UiEvent, UiModelBinding, UiModelCatalog, UiModelChoice, UiModelControlSource,
    UiModelSnapshot, UiQueueSnapshot, UiUsageSnapshot,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::reference::MemFs;
use alan_ap::{Fid, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use alan_tui::{
    FileBackedRunConfig, ProjectControl, ProjectControlResult, ProjectMountReceipt,
    run_file_backed, terminal,
};
use anyhow::{Context, Result, bail};

const CHILD_MODE_ENV: &str = "ALAN_PTY_M2_CHILD";
const CHILD_PROJECT_ENV: &str = "ALAN_PTY_M2_PROJECT";
const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";

struct PtyState {
    parser: vt100::Parser,
    raw: Vec<u8>,
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
                    raw: Vec::new(),
                    eof: false,
                }),
                Condvar::new(),
            )),
        }
    }

    fn push_bytes(&self, bytes: &[u8]) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.raw.extend_from_slice(bytes);
        guard.parser.process(bytes);
        self.shared.1.notify_all();
    }

    fn raw_len(&self) -> usize {
        self.shared.0.lock().expect("pty state lock").raw.len()
    }

    fn raw_slice_from(&self, start: usize) -> Vec<u8> {
        let guard = self.shared.0.lock().expect("pty state lock");
        guard.raw.get(start..).unwrap_or_default().to_vec()
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
        predicate: impl Fn(&str, bool) -> bool,
    ) -> Result<String> {
        let deadline = Instant::now() + timeout;
        let mut guard = self.shared.0.lock().expect("pty state lock");
        loop {
            let contents = guard.parser.screen().contents();
            if predicate(&contents, guard.eof) {
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
                if predicate(&contents, guard.eof) {
                    return Ok(contents);
                }
                bail!("timed out waiting for `{label}`.\nScreen:\n{contents}");
            }
        }
    }

    fn wait_eof(&self, timeout: Duration) -> Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut guard = self.shared.0.lock().expect("pty state lock");
        while !guard.eof {
            let now = Instant::now();
            if now >= deadline {
                bail!(
                    "timed out waiting for PTY EOF.\nScreen:\n{}",
                    guard.parser.screen().contents()
                );
            }
            let (next_guard, _) = self
                .shared
                .1
                .wait_timeout(guard, deadline - now)
                .expect("condvar wait");
            guard = next_guard;
        }
        Ok(guard.raw.clone())
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
                bail!("child process did not exit within {:?}", timeout);
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
    // SAFETY: `master`, `slave`, and `ws` are valid pointers; `name` and `termp` are null.
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

fn spawn_in_pty(
    slave: &OwnedFd,
    master_to_close: RawFd,
    test_name: &str,
    mode: &str,
    extra_env: &[(&str, &OsStr)],
) -> Result<Child> {
    let exe = std::env::current_exe().context("locate test executable")?;
    let slave_fd: RawFd = std::os::fd::AsRawFd::as_raw_fd(slave);
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", test_name, "--nocapture"])
        .env(CHILD_MODE_ENV, mode)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    // SAFETY: `pre_exec` invokes only async-signal-safe POSIX syscalls (`setsid`, `ioctl`, `dup2`, `close`).
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
    let fid = Fid(91_000);
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

async fn run_child_m2_tui(project_dir: PathBuf) -> Result<()> {
    terminal::install_panic_hook();

    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let namespace = Namespace::new();
    let mount_fs = Arc::new(MountFs::new(namespace));
    let live = mount_fs.live_namespace();
    live.mount(
        "/proc",
        InProcessTransport::new(procfs.clone()),
        Access::ReadWrite,
    );
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let root_transport = InProcessTransport::new(mount_fs);
    let shell = Shell::new(root_transport.clone());

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
    let process_path = format!("/proc/{pid}");
    let model_snapshot = UiModelSnapshot {
        version: 1,
        process_path,
        publication_version: 1,
        known: true,
        selected_next: Some(UiModelBinding {
            profile: "openai-main".into(),
            provider: "openai_responses".into(),
            model: "gpt-5.4".into(),
            reasoning: Default::default(),
            control_source: UiModelControlSource::AgentConfig,
        }),
        active: None,
        catalog: Some(UiModelCatalog {
            profile: "openai-main".into(),
            models: vec![
                UiModelChoice {
                    model: "gpt-5.4".into(),
                    supported_reasoning_efforts: vec![],
                    default_reasoning_effort: None,
                },
                UiModelChoice {
                    model: "claude-sonnet-4-5".into(),
                    supported_reasoning_efforts: vec![
                        ReasoningEffort::Low,
                        ReasoningEffort::Medium,
                        ReasoningEffort::High,
                    ],
                    default_reasoning_effort: Some(ReasoningEffort::Medium),
                },
            ],
            ..Default::default()
        }),
        admitted: Vec::new(),
    };
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&model_snapshot)?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(&UiQueueSnapshot {
                known: true,
                revision: 1,
                ..Default::default()
            })?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/ui/activity"),
            &serde_json::to_vec(&UiActivitySnapshot {
                version: 1,
                state: UiActivityState::Idle,
                started_at_ms: None,
                waiting_submission_ids: Vec::new(),
            })?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/ui/usage"),
            &serde_json::to_vec(&UiUsageSnapshot {
                input_tokens: 12_800,
                output_tokens: 400,
                context_window: 128_000,
                ..UiUsageSnapshot::empty()
            })?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/tape"),
            br#"{"version":1,"kind":"message","role":"assistant","content":"| Col | Val |\n| --- | --- |\n| one | two |\n"}"#,
        )
        .await?;

    let action_id = allocate_clone_id(&root_transport, &format!("{owner}/actions/clone")).await?;
    let action_result = serde_json::json!({
        "title": "edit_file src/lib.rs",
        "raw_secret_key": "raw-hidden-value",
        "presentation": ToolResultPresentation::Diff {
            path: "src/lib.rs".into(),
            hunks: vec![DiffHunk {
                header: Some("@@ -1 +1 @@".into()),
                lines: vec![
                    DiffLine::Removed { text: "let old = 1;".into() },
                    DiffLine::Added { text: "let new = 2;".into() },
                ],
            }],
        }
    });
    shell
        .write(&format!("{owner}/actions/{action_id}/name"), b"edit_file")
        .await?;
    shell
        .write(&format!("{owner}/actions/{action_id}/status"), b"completed")
        .await?;
    shell
        .write(
            &format!("{owner}/actions/{action_id}/result"),
            action_result.to_string().as_bytes(),
        )
        .await?;
    shell
        .write(
            &format!("{owner}/actions/{action_id}/output"),
            b"updated lib.rs",
        )
        .await?;

    let project_control: alan_tui::ProjectControlHandler = Arc::new(move |control| {
        Box::pin(async move {
            match control {
                ProjectControl::Mount {
                    host_path, access, ..
                } => Ok(ProjectControlResult::Mounted {
                    receipt: ProjectMountReceipt {
                        grant_id: "g-m2".into(),
                        namespace_path: "/mnt/project".into(),
                        label: "m2-repo".into(),
                        access,
                    },
                    completion_root: host_path,
                }),
                ProjectControl::Revoke { .. } => Ok(ProjectControlResult::Revoked),
            }
        })
    });

    let control_shell = shell.clone();
    let control_transport = root_transport.clone();
    let control_owner = owner.clone();
    let response_log = project_dir.join("yield_response.txt");
    let response_log_task = response_log.clone();
    let control_task = tokio::spawn(async move {
        let Ok(mut tail) = control_shell.tail(&format!("{control_owner}/events")).await else {
            return;
        };
        let mut buf = String::new();
        while let Ok(bytes) = tail.read(4096).await {
            if bytes.is_empty() {
                break;
            }
            buf.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                if line == "ctl:rollback"
                    && let Ok(live_id) = allocate_clone_id(
                        &control_transport,
                        &format!("{control_owner}/actions/clone"),
                    )
                    .await
                {
                    let live_result = serde_json::json!({
                        "title": "edit_file src/lib.rs",
                        "presentation": ToolResultPresentation::Diff {
                            path: "src/lib.rs".into(),
                            hunks: vec![DiffHunk {
                                header: Some("@@ -1 +1 @@".into()),
                                lines: vec![
                                    DiffLine::Removed { text: "let old = 1;".into() },
                                    DiffLine::Added { text: "let new = 2;".into() },
                                ],
                            }],
                        }
                    });
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/actions/{live_id}/name"),
                            b"edit_file",
                        )
                        .await;
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/actions/{live_id}/result"),
                            live_result.to_string().as_bytes(),
                        )
                        .await;
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/actions/{live_id}/status"),
                            b"completed",
                        )
                        .await;
                } else if line == "ctl:compact"
                    && let Ok(req_id) = allocate_clone_id(
                        &control_transport,
                        &format!("{control_owner}/requests/clone"),
                    )
                    .await
                {
                    let options = serde_json::json!({
                        "title": "Allow file mutation?",
                        "options": ["approve", "allow_session", "reject"],
                        "default_option": "approve",
                        "details": {
                            "presentation": ToolResultPresentation::Diff {
                                path: "src/app.rs".into(),
                                hunks: vec![DiffHunk {
                                    header: Some("@@ -1 +1 @@".into()),
                                    lines: vec![
                                        DiffLine::Removed { text: "unsafe_line()".into() },
                                        DiffLine::Added { text: "safe_line()".into() },
                                    ],
                                }],
                            }
                        }
                    });
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/requests/{req_id}/kind"),
                            b"confirmation",
                        )
                        .await;
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/requests/{req_id}/prompt"),
                            b"Allow write?",
                        )
                        .await;
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/requests/{req_id}/options"),
                            options.to_string().as_bytes(),
                        )
                        .await;
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/requests/{req_id}/status"),
                            b"pending",
                        )
                        .await;
                    for _ in 0..80 {
                        if let Ok(bytes) = control_shell
                            .cat(&format!("{control_owner}/requests/{req_id}/response"))
                            .await
                            && !bytes.is_empty()
                        {
                            let _ = std::fs::write(&response_log_task, &bytes);
                            let _ = control_shell
                                .write(
                                    &format!("{control_owner}/requests/{req_id}/status"),
                                    b"resolved",
                                )
                                .await;
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                } else if let Some(json) = line.strip_prefix("ctl:project-cwd-v1 ")
                    && let Ok(val) = serde_json::from_str::<serde_json::Value>(json)
                    && let (Some(id), Some(path)) = (val["id"].as_str(), val["path"].as_str())
                    && let Ok(act_id) = allocate_clone_id(
                        &control_transport,
                        &format!("{control_owner}/actions/clone"),
                    )
                    .await
                {
                    let _ = control_shell
                        .write(&format!("{control_owner}/actions/{act_id}/name"), b"cd")
                        .await;
                    let result_json = serde_json::json!({
                        "call_id": id,
                        "exit_code": 0,
                        "outcome": {"success": true, "cwd": path}
                    })
                    .to_string();
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/actions/{act_id}/result"),
                            result_json.as_bytes(),
                        )
                        .await;
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/actions/{act_id}/status"),
                            b"completed",
                        )
                        .await;
                    let mut ev = serde_json::to_vec(&UiEvent::InputCompleted {
                        submission_ids: vec![id.to_string()],
                        status: alan_agent_protocol::UiInputStatus::Completed,
                        error: None,
                    })
                    .unwrap_or_default();
                    ev.push(b'\n');
                    let _ = control_shell
                        .write(&format!("{control_owner}/machine/ui/events"), &ev)
                        .await;
                }
            }
        }
    });

    let mut cfg = FileBackedRunConfig::new(root_transport, "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    cfg.project_candidate = Some(project_dir);
    cfg.project_control = Some(project_control);
    cfg.sandbox_backend = Some("seatbelt".into());

    run_file_backed(cfg).await?;
    control_task.abort();
    Ok(())
}

#[test]
fn pty_e2e_m2_child_runner() {
    let Some(mode) = std::env::var_os(CHILD_MODE_ENV) else {
        return;
    };
    let mode = mode.to_string_lossy().into_owned();
    if mode == "tui" {
        let project_dir =
            PathBuf::from(std::env::var_os(CHILD_PROJECT_ENV).expect("child project dir"));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        if let Err(err) = rt.block_on(run_child_m2_tui(project_dir)) {
            eprintln!("pty m2 child failed: {err:#}");
            std::process::exit(2);
        }
    } else if mode == "panic" {
        terminal::install_panic_hook();
        let _session = terminal::TerminalSession::enter().expect("enter terminal");
        panic!("simulated kitty panic");
    }
}

#[test]
fn pty_e2e_m2_ergonomics_multiline_editor_modals_status_and_yield() -> Result<()> {
    let project_dir = std::env::temp_dir().join(format!("alan-pty-m2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(project_dir.join(".git"))?;
    std::fs::write(project_dir.join(".git/HEAD"), "ref: refs/heads/feat/m2\n")?;

    let editor_script = project_dir.join("mock-editor.sh");
    std::fs::write(
        &editor_script,
        "#!/bin/sh\nprintf 'edited-by-visual' > \"$1\"\n",
    )?;
    let mut perms = std::fs::metadata(&editor_script)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&editor_script, perms)?;

    let (master, slave) = open_pty(32, 100)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(
        &slave,
        master_fd,
        "pty_e2e_m2_child_runner",
        "tui",
        &[
            (CHILD_PROJECT_ENV, project_dir.as_os_str()),
            ("VISUAL", editor_script.as_os_str()),
        ],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(32, 100);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        // Wait for rendered markdown table + ctx usage without raw sandbox backend jargon.
        let initial = screen.wait_for("initial M2 UI frame", Duration::from_secs(6), |s, _| {
            s.contains("ctx 10%") && s.contains("Col") && s.contains("two")
        })?;
        assert!(!initial.contains("seatbelt"), "{initial}");

        // Mount /project so git branch + read-write posture appear cleanly in status bar.
        write_master(&master_write, b"/project\r")?;
        screen.wait_for("project picker open", Duration::from_secs(3), |s, _| {
            s.contains("project path")
        })?;
        write_master(&master_write, b"\r")?;
        screen.wait_for("git branch mounted", Duration::from_secs(4), |s, _| {
            s.contains("m2-repo")
                && s.contains("git:feat/m2")
                && s.contains("read-write")
                && !s.contains("seatbelt")
        })?;

        // ActionModal hides raw JSON by default, switches items with `[`, and toggles with `d`.
        write_master(&master_write, b"\x0f")?;
        screen.wait_for("ActionModal open", Duration::from_secs(3), |s, _| {
            s.contains("2/2") && s.contains("press d to expand")
        })?;
        write_master(&master_write, b"[")?;
        let modal = screen.wait_for(
            "ActionModal switched to edit_file",
            Duration::from_secs(3),
            |s, _| s.contains("1/2") && s.contains("edit_file"),
        )?;
        assert!(!modal.contains("raw_secret_key"), "{modal}");
        write_master(&master_write, b"d")?;
        screen.wait_for(
            "ActionModal raw JSON expanded",
            Duration::from_secs(3),
            |s, _| s.contains("raw_secret_key"),
        )?;
        write_master(&master_write, b"t")?;
        screen.wait_for(
            "Turing Tape modal via t key",
            Duration::from_secs(3),
            |s, _| s.contains("Turing Tape") && s.contains("[out]"),
        )?;
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("ActionModal closed", Duration::from_secs(3), |s, _| {
            !s.contains("raw_secret_key") && !s.contains("Turing Tape")
        })?;

        // Trigger a live edit_file action so compact inline diff renders in transcript.
        write_master(&master_write, b"/rollback\r")?;
        screen.wait_for(
            "inline edit diff rendered",
            Duration::from_secs(4),
            |s, _| s.contains("let old = 1;") && s.contains("let new = 2;"),
        )?;

        // Universal multiline fallbacks (Ctrl+J, Alt+Enter, trailing \+Enter) + Up nav.
        write_master(&master_write, b"row1\nrow2\x1b\rrow3\\\rrow4")?;
        screen.wait_for("4-line composer", Duration::from_secs(3), |s, _| {
            s.contains("row1") && s.contains("row2") && s.contains("row3") && s.contains("row4")
        })?;
        write_master(&master_write, b"\x1b[A\x1b[A!")?;
        screen.wait_for(
            "Up navigated onto earlier line",
            Duration::from_secs(3),
            |s, _| s.contains("row2!"),
        )?;
        write_master(&master_write, b"\x03")?;
        screen.wait_for("cleared multiline draft", Duration::from_secs(3), |s, _| {
            !s.contains("row1")
        })?;

        // External $EDITOR (Ctrl+G) edits draft cleanly.
        write_master(&master_write, b"\x07")?;
        screen.wait_for("external editor applied", Duration::from_secs(5), |s, _| {
            s.contains("edited-by-visual")
        })?;
        write_master(&master_write, b"\x03")?;

        // /help multiline output, /man alan9 reader, and /model fuzzy search.
        write_master(&master_write, b"/help\r")?;
        screen.wait_for("multiline help", Duration::from_secs(3), |s, _| {
            s.contains("Commands:") && s.contains("Ctrl+S") && s.contains("Ctrl+G")
        })?;
        write_master(&master_write, b"/man alan9\r")?;
        screen.wait_for("/man alan9 modal", Duration::from_secs(3), |s, _| {
            s.contains("Manual · /man/7/alan9") && s.contains("STANDARD NAMESPACE")
        })?;
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("/man closed", Duration::from_secs(3), |s, _| {
            !s.contains("Manual · /man/7/alan9")
        })?;
        write_master(&master_write, b"/model\r")?;
        screen.wait_for("model chooser open", Duration::from_secs(3), |s, _| {
            s.contains("claude-sonnet-4-5")
        })?;
        write_master(&master_write, b"snnt")?;
        screen.wait_for("model fuzzy match", Duration::from_secs(3), |s, _| {
            s.contains("1 match") && s.contains("claude-sonnet-4-5")
        })?;
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("model chooser closed", Duration::from_secs(3), |s, _| {
            !s.contains("Filter:")
        })?;

        // Confirmation yield shows [1]/[2]/(default), diff preview, allow_session, and rings BEL.
        let before_yield_raw = screen.raw_len();
        write_master(&master_write, b"/compact\r")?;
        screen.wait_for(
            "confirmation yield rendered",
            Duration::from_secs(4),
            |s, _| {
                s.contains("[1] approve")
                    && s.contains("(default)")
                    && s.contains("[2] allow_session")
                    && s.contains("safe_line")
            },
        )?;
        let yield_raw = screen.raw_slice_from(before_yield_raw);
        assert!(
            yield_raw.contains(&0x07),
            "expected BEL (0x07) when confirmation yield arrived"
        );
        write_master(&master_write, b"2")?;
        screen.wait_for("confirmation answered", Duration::from_secs(4), |s, _| {
            s.contains("response sent") && s.contains("Ctrl+O details") && s.contains("p: plans")
        })?;

        // /new clears earlier tape messages and usage cleanly.
        write_master(&master_write, b"/new\r")?;
        screen.wait_for("session reset", Duration::from_secs(3), |s, _| {
            s.contains("Started a fresh session.")
                && !s.contains("Allow file mutation?")
                && !s.contains("ctx 10%")
        })?;

        // Clean Ctrl+Q exit emits Kitty pop + bracketed paste disable + OSC 0 title earlier.
        write_master(&master_write, b"\x11")?;
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(status.success(), "PTY M2 child failed: {status:?}");
        drop(master_write);
        let raw = screen.wait_eof(Duration::from_secs(3))?;
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(raw_text.contains("\x1b[>1u"), "missing Kitty push");
        assert!(raw_text.contains("\x1b[<1u"), "missing Kitty pop");
        assert!(
            raw_text.contains("\x1b[?2004l"),
            "missing bracketed paste disable"
        );
        assert!(
            raw_text.contains("\x1b]0;alan"),
            "missing OSC 0 title update"
        );
        let saved = std::fs::read_to_string(project_dir.join("yield_response.txt"))?;
        assert!(
            saved.contains("allow_session"),
            "unexpected response: {saved:?}"
        );
        Ok(())
    })();

    drop(child);
    let _ = reader_handle.join();
    let _ = std::fs::remove_dir_all(&project_dir);
    res
}

#[test]
fn pty_e2e_m2_panic_pops_kitty_keyboard_protocol() -> Result<()> {
    let (master, slave) = open_pty(24, 80)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(&slave, master_fd, "pty_e2e_m2_child_runner", "panic", &[])?;
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(24, 80);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let status = child.wait_with_timeout(Duration::from_secs(5))?;
    assert!(!status.success(), "panic child unexpectedly succeeded");
    drop(master_write);

    // SAFETY: `libc::termios` is initialized immediately by `tcgetattr`.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    let slave_raw = std::os::fd::AsRawFd::as_raw_fd(&slave);
    // SAFETY: `slave_raw` is an open PTY slave fd and `termios` points to writable memory.
    let tc_rc = unsafe { libc::tcgetattr(slave_raw, &mut termios) };
    drop(slave);

    let raw = screen.wait_eof(Duration::from_secs(3))?;
    let _ = reader_handle.join();

    if tc_rc == 0 {
        assert_ne!(termios.c_lflag & libc::ICANON, 0, "ICANON not restored");
        assert_ne!(termios.c_lflag & libc::ECHO, 0, "ECHO not restored");
    }
    let raw_text = String::from_utf8_lossy(&raw);
    assert!(
        raw_text.contains("\x1b[<1u"),
        "panic hook missing Kitty pop"
    );
    assert!(
        raw_text.contains("\x1b[?2004l"),
        "panic hook missing bracketed paste disable"
    );
    Ok(())
}
