#![cfg(unix)]

mod pty_support;

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    FileBackedRunConfig, LoginControl, LoginControlHandler, LoginMethod, LoginProviderChoice,
    LoginResult, ProjectControl, ProjectControlResult, ProjectMountReceipt, run_file_backed,
    terminal,
};
use anyhow::{Context, Result, bail};
use pty_support::{
    CHILD_MODE_ENV, ChildGuard, PtyScreen, open_pty, resize_pty, spawn_in_pty, spawn_master_reader,
    stdin_restored_canon_and_echo, write_master,
};

const CHILD_PROJECT_ENV: &str = "ALAN_PTY_E2E_PROJECT";
const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";

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

struct PtyAgentEnv {
    root_transport: InProcessTransport,
    shell: Shell,
    owner: String,
    process_path: String,
}

async fn boot_pty_agent_env(model_snapshot: Option<UiModelSnapshot>) -> Result<PtyAgentEnv> {
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
    let owner = format!("/agent/{pid}");
    let initial_models = model_snapshot.unwrap_or(UiModelSnapshot {
        version: 1,
        process_path: process_path.clone(),
        publication_version: 1,
        known: true,
        selected_next: None,
        active: None,
        catalog: None,
        admitted: Vec::new(),
    });
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&initial_models)?,
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

    Ok(PtyAgentEnv {
        root_transport,
        shell,
        owner,
        process_path,
    })
}

async fn run_child_file_backed(project_dir: PathBuf) -> Result<()> {
    let model_snapshot = UiModelSnapshot {
        version: 1,
        process_path: String::new(),
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
            ..Default::default()
        }),
        admitted: Vec::new(),
    };
    let env = boot_pty_agent_env(Some(model_snapshot)).await?;
    let root_transport = env.root_transport;
    let shell = env.shell;
    let owner = env.owner;
    let process_path = env.process_path;
    let updated_models = UiModelSnapshot {
        process_path: process_path.clone(),
        ..serde_json::from_slice(&shell.cat(&format!("{owner}/machine/ui/models")).await?)?
    };
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&updated_models)?,
        )
        .await
        .context("update ui/models process_path")?;
    shell
        .write_exclusive(
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

    let control_task = spawn_pty_control_task(
        shell.clone(),
        root_transport.clone(),
        owner.clone(),
        process_path.clone(),
        "openai-main".into(),
    );

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

fn spawn_pty_control_task(
    control_shell: Shell,
    control_transport: InProcessTransport,
    control_owner: String,
    control_process_path: String,
    catalog_profile: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
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
                            profile: catalog_profile.clone(),
                            provider: "openai_responses".into(),
                            model: model.to_string(),
                            reasoning: Default::default(),
                            control_source: UiModelControlSource::AgentMachineOverride,
                        }),
                        active: None,
                        catalog: Some(UiModelCatalog {
                            profile: catalog_profile.clone(),
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
                            ..Default::default()
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
                        .write_exclusive(&format!("{control_owner}/machine/ui/events"), &ev)
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
                        .write_exclusive(&format!("{control_owner}/machine/ui/events"), &ev)
                        .await;
                }
            }
        }
    })
}

async fn run_child_m6() -> Result<()> {
    let env = boot_pty_agent_env(None).await?;
    let control_task = spawn_pty_control_task(
        env.shell.clone(),
        env.root_transport.clone(),
        env.owner.clone(),
        env.process_path.clone(),
        "chatgpt-main".into(),
    );
    let input_shell = env.shell.clone();
    let input_owner = env.owner.clone();
    let input_task = tokio::spawn(async move {
        let Ok(mut tail) = input_shell.tail(&format!("{input_owner}/io/input")).await else {
            return;
        };
        let mut buf: Vec<u8> = Vec::new();
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
                if let Ok(Some(record)) =
                    alan_agent_protocol::UserInputRecord::decode_payload(&payload)
                {
                    let mut ev = serde_json::to_vec(&UiEvent::InputCompleted {
                        submission_ids: vec![record.submission_id],
                        status: UiInputStatus::Failed,
                        error: Some(
                            "No confirmed callable binding; generation input not admitted.".into(),
                        ),
                    })
                    .unwrap_or_default();
                    ev.push(b'\n');
                    let _ = input_shell
                        .write_exclusive(&format!("{input_owner}/machine/ui/events"), &ev)
                        .await;
                }
            }
        }
    });

    let login_control: LoginControlHandler = Arc::new(|control| {
        Box::pin(async move {
            match control {
                LoginControl::ListProviders => Ok(LoginResult::Providers(vec![
                    LoginProviderChoice {
                        id: "chatgpt".into(),
                        label: "ChatGPT (Plus / Pro OAuth)".into(),
                        detail: "browser OAuth".into(),
                        default_profile: "chatgpt-main".into(),
                        method: LoginMethod::BrowserOrAuto,
                    },
                    LoginProviderChoice {
                        id: "openai".into(),
                        label: "OpenAI API".into(),
                        detail: "API key".into(),
                        default_profile: "openai-main".into(),
                        method: LoginMethod::ApiKey {
                            allow_base_url: true,
                        },
                    },
                ])),
                LoginControl::Submit(sub) => Ok(LoginResult::Connected {
                    profile: format!("{}-main", sub.provider_id),
                    provider: sub.provider_id,
                    model: Some("gpt-5.4".into()),
                    message: None,
                }),
            }
        })
    });

    let mut cfg = FileBackedRunConfig::new(env.root_transport, "/agent/root");
    cfg.connection_configured = Some(false);
    cfg.effective_model = None;
    cfg.login_control = Some(login_control);

    run_file_backed(cfg).await?;
    control_task.abort();
    input_task.abort();

    if !stdin_restored_canon_and_echo()? {
        bail!("stdin raw mode or ECHO was not restored after clean exit");
    }
    let events = String::from_utf8(env.shell.cat("/agent/root/events").await?)?;
    if events.contains("interrupt") {
        bail!("idle Esc unexpectedly emitted interrupt event: {events:?}");
    }
    if !events.contains("ctl:trust-posture confined-smart") {
        bail!("expected Shift+Tab to emit trust-posture confined-smart: {events:?}");
    }
    if !events.contains("ctl:trust-posture confined-trusted") {
        bail!("expected Shift+Tab to emit trust-posture confined-trusted: {events:?}");
    }
    if !events.contains("ctl:trust-posture governed") {
        bail!("expected Shift+Tab to emit trust-posture governed: {events:?}");
    }
    if !events.contains("ctl:select-model ") || !events.contains("gpt-5.4") {
        bail!("expected /login completion to sync gpt-5.4 via select-model: {events:?}");
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
    } else if mode == "m6" {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        if let Err(err) = rt.block_on(run_child_m6()) {
            eprintln!("pty m6 child failed: {err:#}");
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
        // Signal the PTY parent only after `ECHO` is cleared so input never races child startup.
        let _ = std::io::stdout().write_all(b"ECHO_DISABLED\n");
        let _ = std::io::stdout().flush();
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
            s.contains("read-write") || s.contains("read-only") || s.contains("Mount")
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

        // Shift+Tab cycles rw·gov -> rw·smart -> rw·trust -> rw·gov on the mounted project badge.
        write_master(&master_write, b"\x1b[Z")?;
        screen.wait_for("rw·smart badge", Duration::from_secs(3), |s, _| {
            s.contains("rw·smart")
        })?;
        write_master(&master_write, b"\x1b[Z")?;
        screen.wait_for("rw·trust badge", Duration::from_secs(3), |s, _| {
            s.contains("rw·trust")
        })?;
        write_master(&master_write, b"\x1b[Z")?;
        screen.wait_for("rw·gov badge", Duration::from_secs(3), |s, _| {
            s.contains("rw·gov")
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
fn pty_e2e_m6_help_no_ghost_esc_dismiss_completion_shift_tab_and_login() -> Result<()> {
    let (master, slave) = open_pty(24, 80)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(&slave, master_fd, "pty_e2e_child_runner", "m6", &[])?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(24, 80);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        let startup =
            screen.wait_for("m6 unconfigured startup", Duration::from_secs(6), |s, _| {
                s.contains("=(o.o)=") && s.contains("Not connected") && s.contains("/login")
            })?;
        assert!(
            !startup.contains("gpt-5.4"),
            "unconfigured boot unexpectedly displayed gpt-5.4:\n{startup}"
        );

        // '/' shows prioritized commands near the top with truncation counter; Esc closes popup.
        write_master(&master_write, b"/")?;
        let slash_popup =
            screen.wait_for("prioritized slash popup", Duration::from_secs(3), |s, _| {
                s.contains("/login") && s.contains("/model") && s.contains("1-6/")
            })?;
        let login_idx = slash_popup.find("/login").expect("/login present");
        let model_idx = slash_popup.find("/model").expect("/model present");
        assert!(
            login_idx < model_idx,
            "expected /login ordered ahead of /model:\n{slash_popup}"
        );
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("close slash popup", Duration::from_secs(3), |s, _| {
            !s.contains("1-6/")
        })?;
        write_master(&master_write, b"\x7f")?;
        screen.wait_for("clear slash draft", Duration::from_secs(3), |s, _| {
            !s.contains(": /")
        })?;

        // /help at 80x24 leaves zero welcome-banner ghosting and wraps with hanging indent.
        write_master(&master_write, b"/help\r")?;
        let help_screen =
            screen.wait_for("/help rendered cleanly", Duration::from_secs(3), |s, _| {
                s.contains("Commands:")
                    && s.contains("Shortcuts & Modals:")
                    && s.contains("↑↓/jk, Esc)")
            })?;
        assert!(
            !help_screen.contains("=(o.o)="),
            "welcome banner ghosting detected inside /help:\n{help_screen}"
        );
        assert!(
            !help_screen.contains("ALAN(1)") && !help_screen.contains("Everything is a file."),
            "welcome banner text bled into /help:\n{help_screen}"
        );
        let continuation_line = help_screen
            .lines()
            .find(|line| line.contains("Esc)"))
            .expect("wrapped Ctrl+O continuation line");
        assert!(
            continuation_line.starts_with("   "),
            "expected /help right-column wrap to keep hanging indent: {continuation_line:?}\n{help_screen}"
        );

        // Idle Esc dismisses /help notice cleanly without interrupting.
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("Esc dismisses /help", Duration::from_secs(3), |s, _| {
            !s.contains("Shortcuts & Modals:") && !s.contains("Commands:")
        })?;

        // Shift+Tab cycles trust posture governed -> smart -> trust -> governed.
        write_master(&master_write, b"\x1b[Z")?;
        screen.wait_for("Shift+Tab smart", Duration::from_secs(3), |s, _| {
            s.contains("Permissions: smart")
        })?;
        write_master(&master_write, b"\x1b[Z")?;
        screen.wait_for("Shift+Tab trust", Duration::from_secs(3), |s, _| {
            s.contains("Permissions: trust")
        })?;
        write_master(&master_write, b"\x1b[Z")?;
        screen.wait_for(
            "Shift+Tab back to governed",
            Duration::from_secs(3),
            |s, _| s.contains("Permissions: governed"),
        )?;

        // Unconfigured prompt submission settles cleanly, shows guidance, and clears composer so /login runs right after.
        write_master(&master_write, b"hello\r")?;
        screen.wait_for(
            "unconfigured failure clears hello",
            Duration::from_secs(3),
            |s, _| {
                s.contains("No active model connection is configured")
                    && !s.lines().any(|line| line.starts_with(": hello"))
            },
        )?;

        // /login inside TUI selects provider and hot-refreshes connected model/status.
        write_master(&master_write, b"/login\r")?;
        screen.wait_for("/login provider picker", Duration::from_secs(3), |s, _| {
            s.contains("ChatGPT") && s.contains("OpenAI")
        })?;
        write_master(&master_write, b"\r")?;
        screen.wait_for("/login hot refresh", Duration::from_secs(3), |s, _| {
            s.contains("Connected chatgpt (chatgpt-main)")
                && s.contains("gpt-5.4")
                && !s.contains("Not connected")
        })?;

        // Clean Ctrl+Q exit restores cooked TTY modes & disables bracketed paste.
        write_master(&master_write, b"\x11")?;
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(status.success(), "m6 PTY child failed with {status:?}");
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

    screen.wait_for("ECHO_DISABLED", Duration::from_secs(5), |text, _| {
        text.contains("ECHO_DISABLED")
    })?;
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
