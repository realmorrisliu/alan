#![cfg(unix)]

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use alan_agent_protocol::{
    UiActivitySnapshot, UiActivityState, UiEvent, UiInputStatus, UiModelBinding, UiModelCatalog,
    UiModelChoice, UiModelControlSource, UiModelSnapshot, UiQueueSnapshot,
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

const CHILD_MODE_ENV: &str = "ALAN_PTY_E2E_CHILD";
const CHILD_PROJECT_ENV: &str = "ALAN_PTY_E2E_PROJECT";
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

    fn contents(&self) -> String {
        let guard = self.shared.0.lock().expect("pty state lock");
        guard.parser.screen().contents()
    }

    fn wait_for<F>(&self, label: &str, timeout: Duration, mut predicate: F) -> Result<String>
    where
        F: FnMut(&str, bool) -> bool,
    {
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

    fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        Ok(self.0.as_mut().expect("child").try_wait()?)
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
    // SAFETY: `master`, `slave`, and `ws` are valid aligned pointers; `name` and `termp` are null.
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
    // SAFETY: `openpty` returned 0, so `master` and `slave` are newly created owned file descriptors.
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

    cmd.spawn().context("spawn pty child test")
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

fn stdin_restored_canon_and_echo() -> Result<bool> {
    // SAFETY: `libc::termios` is a plain C struct initialized immediately by `tcgetattr`.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: `STDIN_FILENO` is a valid fd and `termios` points to valid writable storage.
    if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut termios) } != 0 {
        return Err(std::io::Error::last_os_error()).context("tcgetattr stdin");
    }
    let has_canon = (termios.c_lflag & libc::ICANON) != 0;
    let has_echo = (termios.c_lflag & libc::ECHO) != 0;
    Ok(has_canon && has_echo)
}

async fn allocate_clone_id(transport: &InProcessTransport, path: &str) -> Result<String> {
    let fid = Fid(90_000);
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

async fn run_child_file_backed(project_dir: PathBuf) -> Result<()> {
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

    let process_path = format!("/proc/{pid}");
    let model_snapshot = UiModelSnapshot {
        version: 1,
        process_path: process_path.clone(),
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
                    supported_reasoning_efforts: Vec::new(),
                    default_reasoning_effort: None,
                },
                UiModelChoice {
                    model: "o3".into(),
                    supported_reasoning_efforts: Vec::new(),
                    default_reasoning_effort: None,
                },
            ],
        }),
        admitted: Vec::new(),
    };
    let owner = format!("/agent/{pid}");
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&model_snapshot)?,
        )
        .await
        .context("write ui/models")?;
    shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(&UiQueueSnapshot {
                known: false,
                revision: 0,
                ..Default::default()
            })?,
        )
        .await
        .context("write ui/queue")?;
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
        .await
        .context("write ui/activity")?;
    shell
        .write(
            &format!("{owner}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"**Bold lead** and regular prose that word wraps across narrow terminal widths cleanly.\"}\n",
        )
        .await
        .context("write tape")?;

    let action_id = allocate_clone_id(&root_transport, &format!("{owner}/actions/clone"))
        .await
        .context("allocate action id")?;
    shell
        .write(&format!("{owner}/actions/{action_id}/name"), b"bash")
        .await
        .context("write action name")?;
    shell
        .write(&format!("{owner}/actions/{action_id}/status"), b"completed")
        .await
        .context("write action status")?;
    shell
        .write(&format!("{owner}/actions/{action_id}/result"), b"ok")
        .await
        .context("write action result")?;
    shell
        .write(
            &format!("{owner}/actions/{action_id}/output"),
            b"pty-action-output",
        )
        .await
        .context("write action output")?;

    let mounted = Arc::new(AtomicBool::new(false));
    let mounted_flag = mounted.clone();
    let project_control: alan_tui::ProjectControlHandler = Arc::new(move |control| {
        let mounted_flag = mounted_flag.clone();
        Box::pin(async move {
            match control {
                ProjectControl::Mount {
                    host_path, access, ..
                } => {
                    mounted_flag.store(true, Ordering::SeqCst);
                    let label = host_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("project")
                        .to_string();
                    Ok(ProjectControlResult::Mounted {
                        receipt: ProjectMountReceipt {
                            grant_id: "g-pty".into(),
                            namespace_path: "/mnt/project".into(),
                            label,
                            access,
                        },
                        completion_root: host_path,
                    })
                }
                ProjectControl::Revoke { .. } => Ok(ProjectControlResult::Revoked),
            }
        })
    });

    let control_shell = shell.clone();
    let control_transport = root_transport.clone();
    let control_owner = owner.clone();
    let control_process_path = process_path.clone();
    let control_task = tokio::spawn(async move {
        let Ok(mut tail) = control_shell.tail(&format!("{control_owner}/events")).await else {
            return;
        };
        let mut model_pub_ver = 1u64;
        let mut buf = String::new();
        while let Ok(bytes) = tail.read(4096).await {
            if bytes.is_empty() {
                break;
            }
            buf.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                if let Some(rest) = line.strip_prefix("ctl:select-model ")
                    && let Some((id, model)) = rest.split_once(' ')
                {
                    model_pub_ver += 1;
                    let next = UiModelSnapshot {
                        version: 1,
                        process_path: control_process_path.clone(),
                        publication_version: model_pub_ver,
                        known: true,
                        selected_next: Some(UiModelBinding {
                            profile: "openai-main".into(),
                            provider: "openai_responses".into(),
                            model: model.to_string(),
                            reasoning: Default::default(),
                            control_source: UiModelControlSource::AgentMachineOverride,
                        }),
                        active: None,
                        catalog: Some(UiModelCatalog {
                            profile: "openai-main".into(),
                            models: vec![
                                UiModelChoice {
                                    model: "gpt-5.4".into(),
                                    supported_reasoning_efforts: Vec::new(),
                                    default_reasoning_effort: None,
                                },
                                UiModelChoice {
                                    model: "o3".into(),
                                    supported_reasoning_efforts: Vec::new(),
                                    default_reasoning_effort: None,
                                },
                            ],
                        }),
                        admitted: Vec::new(),
                    };
                    let _ = control_shell
                        .write(
                            &format!("{control_owner}/machine/ui/models"),
                            &serde_json::to_vec(&next).unwrap_or_default(),
                        )
                        .await;
                    let mut ev = serde_json::to_vec(&UiEvent::InputCompleted {
                        submission_ids: vec![id.to_string()],
                        status: UiInputStatus::Completed,
                        error: None,
                    })
                    .unwrap_or_default();
                    ev.push(b'\n');
                    let _ = control_shell
                        .write(&format!("{control_owner}/machine/ui/events"), &ev)
                        .await;
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
                        status: UiInputStatus::Completed,
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

    run_file_backed(cfg).await?;
    control_task.abort();

    if !stdin_restored_canon_and_echo()? {
        bail!("stdin raw mode or ECHO was not restored after clean exit");
    }
    let events = String::from_utf8(shell.cat("/agent/root/events").await?)?;
    if events.contains("interrupt") {
        bail!("idle Esc unexpectedly emitted interrupt event: {events:?}");
    }
    if !events.contains("select-model") {
        bail!("expected Enter on /model highlighted item to select model: {events:?}");
    }
    if !mounted.load(Ordering::SeqCst) {
        bail!("expected /project mount handler to have run");
    }
    Ok(())
}

#[test]
fn pty_e2e_child_runner() {
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
        if let Err(err) = rt.block_on(run_child_file_backed(project_dir)) {
            eprintln!("pty child failed: {err:#}");
            std::process::exit(2);
        }
    } else if mode == "panic" {
        terminal::install_panic_hook();
        let _session = terminal::TerminalSession::enter().expect("enter terminal");
        panic!("simulated pty panic");
    } else if mode == "secret" {
        let guard = terminal::StdinEchoGuard::disable_if_tty()
            .expect("disable_if_tty")
            .expect("slave tty must return echo guard");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).expect("read secret");
        assert_eq!(line.trim(), "super-secret-key-123");
        drop(guard);
        // SAFETY: `libc::termios` is a plain C struct initialized immediately by `tcgetattr`.
        let mut termios: libc::termios = unsafe { std::mem::zeroed() };
        assert_eq!(
            // SAFETY: `STDIN_FILENO` is a valid fd and `termios` points to valid writable storage.
            unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut termios) },
            0
        );
        assert_ne!(
            termios.c_lflag & libc::ECHO,
            0,
            "ECHO not restored after StdinEchoGuard drop"
        );
        let _ = std::io::stdout().write_all(b"ECHO_OK\n");
        let _ = std::io::stdout().flush();
    } else {
        panic!("unknown child mode {mode}");
    }
}

#[test]
fn pty_e2e_tui_startup_modals_ctrl_q_esc_ctrl_c_cjk_and_resize() -> Result<()> {
    let project_dir = std::env::temp_dir().join(format!("alan-pty-proj-{}", std::process::id()));
    std::fs::create_dir_all(&project_dir)?;

    let (master, slave) = open_pty(24, 80)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(
        &slave,
        master_fd,
        "pty_e2e_child_runner",
        "tui",
        &[(CHILD_PROJECT_ENV, project_dir.as_os_str())],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(24, 80);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        let startup = screen.wait_for("startup frame", Duration::from_secs(6), |s, _| {
            s.contains("Select project") && s.contains("gpt-5.4") && s.contains(": ")
        })?;
        assert!(
            !startup.contains("Queue unavailable"),
            "fresh boot flashed Queue unavailable:\n{startup}"
        );

        // Idle Esc must NOT interrupt or glitch; composer stays responsive.
        write_master(&master_write, b"\x1b")?;
        std::thread::sleep(Duration::from_millis(80));
        write_master(&master_write, b"x")?;
        let after_esc = screen.wait_for("idle Esc calm", Duration::from_secs(3), |s, _| {
            s.contains(": x")
        })?;
        assert!(!after_esc.contains("Queue unavailable"));
        write_master(&master_write, b"\x7f")?;
        screen.wait_for("backspace x", Duration::from_secs(3), |s, _| {
            !s.contains(": x")
        })?;

        // CJK rendering + Ctrl+C clearing draft.
        write_master(&master_write, "你好世界 draft".as_bytes())?;
        screen.wait_for("CJK draft visible", Duration::from_secs(3), |s, _| {
            s.contains("你好世界 draft")
        })?;
        write_master(&master_write, b"\x03")?;
        screen.wait_for("Ctrl+C clears draft", Duration::from_secs(3), |s, _| {
            !s.contains("你好世界")
        })?;

        // /model clears composer, opens chooser, Ctrl+Q does NOT exit inside chooser, Esc closes.
        write_master(&master_write, b"/model\r")?;
        let model_screen = screen.wait_for("model chooser", Duration::from_secs(3), |s, _| {
            s.contains("▶ ") && s.contains("o3")
        })?;
        assert!(
            !model_screen.contains(": /model"),
            "/model stayed in composer:\n{model_screen}"
        );
        write_master(&master_write, b"\x11")?;
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            child.try_wait()?.is_none(),
            "Ctrl+Q inside model chooser exited the process"
        );
        assert!(screen.contents().contains("o3"));
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("model chooser closes", Duration::from_secs(3), |s, _| {
            !s.contains("▶ ")
        })?;

        // Enter on highlighted first item selects it cleanly.
        write_master(&master_write, b"/model\r")?;
        screen.wait_for(
            "model chooser second open",
            Duration::from_secs(3),
            |s, _| s.contains("▶ ") && s.contains("o3"),
        )?;
        write_master(&master_write, b"\r")?;
        screen.wait_for(
            "model chooser closes on Enter",
            Duration::from_secs(3),
            |s, _| !s.contains("▶ "),
        )?;

        // ActionModal (Ctrl+O) stays open and responsive on Ctrl+Q, then closes on Esc.
        write_master(&master_write, b"\x0f")?;
        screen.wait_for("ActionModal open", Duration::from_secs(3), |s, _| {
            s.contains("pty-action-output")
        })?;
        write_master(&master_write, b"\x11")?;
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            child.try_wait()?.is_none(),
            "Ctrl+Q inside ActionModal exited"
        );
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("ActionModal close", Duration::from_secs(3), |s, _| {
            !s.contains("pty-action-output")
        })?;

        // /project opens picker, Ctrl+Q does NOT exit inside picker, Enter mounts project.
        write_master(&master_write, b"/project\r")?;
        screen.wait_for("project picker open", Duration::from_secs(3), |s, _| {
            s.contains("read-only") || s.contains("Mount")
        })?;
        write_master(&master_write, b"\x11")?;
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            child.try_wait()?.is_none(),
            "Ctrl+Q inside project picker exited"
        );
        write_master(&master_write, b"\r")?;
        let proj_label = project_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("alan-pty-proj");
        screen.wait_for("project mounted", Duration::from_secs(3), |s, _| {
            s.contains(proj_label) && !s.contains("Select project")
        })?;

        // Resize PTY and verify live reflow & continued input responsiveness.
        resize_pty(master_fd, 20, 50)?;
        screen.resize_parser(20, 50);
        std::thread::sleep(Duration::from_millis(120));
        write_master(&master_write, b"resize-ok")?;
        screen.wait_for(
            "input works after resize",
            Duration::from_secs(3),
            |s, _| s.contains("resize-ok"),
        )?;
        write_master(&master_write, b"\x03")?;
        screen.wait_for("clear resize-ok", Duration::from_secs(3), |s, _| {
            !s.contains("resize-ok")
        })?;

        // Clean Ctrl+Q exit from idle prompt restores terminal modes & bracketed paste.
        write_master(&master_write, b"\x11")?;
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(status.success(), "PTY child failed with {status:?}");
        drop(master_write);
        let raw = screen.wait_eof(Duration::from_secs(3))?;
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(
            raw_text.contains("\x1b[?2004l"),
            "missing bracketed paste disable sequence on exit"
        );
        Ok(())
    })();

    drop(child);
    let _ = reader_handle.join();
    let _ = std::fs::remove_dir_all(&project_dir);
    res
}

#[test]
fn pty_e2e_panic_hook_restores_bracketed_paste_and_cooked_tty() -> Result<()> {
    let (master, slave) = open_pty(24, 80)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(&slave, master_fd, "pty_e2e_child_runner", "panic", &[])?;
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(24, 80);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let status = child.wait_with_timeout(Duration::from_secs(5))?;
    assert!(!status.success(), "panic child unexpectedly succeeded");
    drop(master_write);

    // SAFETY: `libc::termios` is a plain C struct initialized immediately by `tcgetattr`.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    let slave_raw = std::os::fd::AsRawFd::as_raw_fd(&slave);
    // SAFETY: `slave_raw` is an open PTY slave fd and `termios` points to valid writable storage.
    let tc_rc = unsafe { libc::tcgetattr(slave_raw, &mut termios) };
    drop(slave);

    let raw = screen.wait_eof(Duration::from_secs(3))?;
    let _ = reader_handle.join();

    if tc_rc == 0 {
        assert_ne!(
            termios.c_lflag & libc::ICANON,
            0,
            "ICANON was not restored by panic hook"
        );
        assert_ne!(
            termios.c_lflag & libc::ECHO,
            0,
            "ECHO was not restored by panic hook"
        );
    }
    let raw_text = String::from_utf8_lossy(&raw);
    assert!(
        raw_text.contains("\x1b[?2004l"),
        "panic hook did not emit bracketed paste disable sequence"
    );
    Ok(())
}

#[test]
fn pty_e2e_stdin_echo_guard_hides_secret_and_restores_echo() -> Result<()> {
    let (master, slave) = open_pty(24, 80)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(&slave, master_fd, "pty_e2e_child_runner", "secret", &[])?;
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(24, 80);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    std::thread::sleep(Duration::from_millis(80));
    write_master(&master_write, b"super-secret-key-123\n")?;
    let status = child.wait_with_timeout(Duration::from_secs(5))?;
    assert!(status.success(), "secret child failed: {status:?}");
    drop(master_write);
    drop(slave);

    let raw = screen.wait_eof(Duration::from_secs(3))?;
    let _ = reader_handle.join();
    let raw_text = String::from_utf8_lossy(&raw);
    assert!(
        raw_text.contains("ECHO_OK"),
        "missing ECHO_OK confirmation: {raw_text:?}"
    );
    assert!(
        !raw_text.contains("super-secret-key-123"),
        "secret leaked through PTY echo: {raw_text:?}"
    );
    Ok(())
}
