#![cfg(unix)]

mod pty_support;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_agent_protocol::{
    UiActivitySnapshot, UiEvent, UiInputStatus, UiModelBinding, UiModelControlSource,
    UiModelSnapshot, UiQueueSnapshot,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat, Stream,
    VersionTable,
};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use alan_tui::{
    FileBackedRunConfig, WorkstreamControl, WorkstreamControlHandler, WorkstreamControlResult,
    WorkstreamCountsDto, WorkstreamEntryDto, WorkstreamFocusTarget, WorkstreamInboxEntryDto,
    WorkstreamOverviewDto, WorkstreamPersistenceKind, WorkstreamStatusKind, run_file_backed,
    terminal,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use pty_support::{
    CHILD_MODE_ENV, ChildGuard, PtyScreen, open_pty, resize_pty, spawn_in_pty, spawn_master_reader,
    stdin_restored_canon_and_echo, write_master,
};

const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";

/// Minimal `/mnt/route` file server providing a writable `log` stream for PTY E2E tests.
struct RouteLogFs {
    log: Stream,
    versions: VersionTable,
    fids: tokio::sync::Mutex<HashMap<Fid, bool>>,
}

impl RouteLogFs {
    fn new() -> Self {
        Self {
            log: Stream::new(),
            versions: VersionTable::new(),
            fids: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    async fn append_log(&self, line: &str) {
        let mut bytes = line.as_bytes().to_vec();
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        self.log.append(&bytes).await;
    }
}

#[async_trait]
impl FileServer for RouteLogFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if newfid == Fid::ROOT || fid != Fid::ROOT {
            return Err(ErrorCode::BadRequest);
        }
        match names {
            [] => {
                self.fids.lock().await.insert(newfid, false);
                Ok(Qid {
                    kind: FileKind::Dir,
                    version: self.versions.get(1),
                    path: 1,
                })
            }
            [name] if name == "log" => {
                self.fids.lock().await.insert(newfid, true);
                Ok(Qid {
                    kind: FileKind::File,
                    version: self.versions.get(2),
                    path: 2,
                })
            }
            _ => Err(ErrorCode::NotFound),
        }
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            return Err(ErrorCode::NoAccess);
        }
        let is_log = *self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        let path = if is_log { 2 } else { 1 };
        let kind = if is_log {
            FileKind::File
        } else {
            FileKind::Dir
        };
        Ok(Qid {
            kind,
            version: self.versions.get(path),
            path,
        })
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let is_log = *self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        if !is_log {
            let listing = b"log";
            let start = (offset as usize).min(listing.len());
            let end = start.saturating_add(count as usize).min(listing.len());
            return Ok(listing[start..end].to_vec());
        }
        Ok(self.log.read(offset, count).await)
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let is_log = *self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        let (kind, path, length) = if is_log {
            (FileKind::File, 2, self.log.len().await)
        } else {
            (FileKind::Dir, 1, 3)
        };
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind,
                version: self.versions.get(path),
                path,
            },
            length,
            executable: false,
            writable: false,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids.lock().await.remove(&fid);
        Ok(())
    }
}

async fn init_agent_ui(shell: &Shell, owner: &str, process_path: &str, model: &str) -> Result<()> {
    let models = UiModelSnapshot {
        version: 1,
        process_path: process_path.to_string(),
        publication_version: 1,
        known: true,
        selected_next: Some(UiModelBinding {
            profile: "openai-main".into(),
            provider: "openai_responses".into(),
            model: model.into(),
            reasoning: Default::default(),
            control_source: UiModelControlSource::AgentConfig,
        }),
        active: None,
        catalog: None,
        admitted: Vec::new(),
    };
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&models)?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(&UiQueueSnapshot {
                known: false,
                revision: 0,
                ..Default::default()
            })?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/ui/activity"),
            &serde_json::to_vec(&UiActivitySnapshot::idle())?,
        )
        .await?;
    Ok(())
}

fn spawn_input_echo_responder(
    shell: Shell,
    owner: String,
    tag: String,
    route_fs: Option<Arc<RouteLogFs>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut tail) = shell.tail(&format!("{owner}/io/input")).await else {
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
                    if record.body.contains("trigger-reroute")
                        && let Some(route) = route_fs.as_ref()
                    {
                        let log_line = serde_json::json!({
                            "version": 1,
                            "port": "root",
                            "rule": "reroute_request",
                            "reason": "misdirected prompt",
                            "message": {
                                "version": 1,
                                "type": "reroute_request",
                                "kind": "reroute_request",
                                "source_stream": "worker",
                                "suggested_target": "docs-sync",
                                "hop_count": 1,
                                "correlation_id": "pty-corr-1",
                                "content": "switch to docs-sync"
                            }
                        })
                        .to_string();
                        route.append_log(&log_line).await;
                    }
                    let reply = format!("[{tag}] {}", record.body);
                    let user_line = serde_json::json!({
                        "version": 1,
                        "kind": "message",
                        "role": "user",
                        "content": record.body,
                        "submission_id": record.submission_id,
                    });
                    let assistant_line = serde_json::json!({
                        "version": 1,
                        "kind": "message",
                        "role": "assistant",
                        "content": reply,
                    });
                    let mut tape_bytes = serde_json::to_vec(&user_line).unwrap();
                    tape_bytes.push(b'\n');
                    tape_bytes.extend_from_slice(&serde_json::to_vec(&assistant_line).unwrap());
                    tape_bytes.push(b'\n');
                    let _ = shell
                        .write(&format!("{owner}/machine/tape"), &tape_bytes)
                        .await;
                    let mut ev = serde_json::to_vec(&UiEvent::InputCompleted {
                        submission_ids: vec![record.submission_id],
                        status: UiInputStatus::Completed,
                        error: None,
                    })
                    .unwrap();
                    ev.push(b'\n');
                    let _ = shell
                        .write(&format!("{owner}/machine/ui/events"), &ev)
                        .await;
                }
            }
        }
    })
}

async fn run_workstreams_child() -> Result<()> {
    terminal::install_panic_hook();

    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let route_fs = Arc::new(RouteLogFs::new());
    let ns = Namespace::new();
    let mount_fs = Arc::new(MountFs::new(ns));
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
    live.mount(
        "/mnt/route",
        InProcessTransport::new(route_fs.clone()),
        Access::ReadOnly,
    );

    let root_transport = InProcessTransport::new(mount_fs);
    let shell = Shell::new(root_transport.clone());

    let root_pid = shell.spawn(EXEC_SPEC).await?;
    agent_root
        .bind_process(root_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(root_pid.clone()).await;
    live.mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "pid",
            format!("{root_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    let child_pid = shell.spawn(EXEC_SPEC).await?;
    agent_root
        .bind_process(child_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    let child_pid_num: u64 = child_pid.parse()?;

    let root_owner = format!("/agent/{root_pid}");
    let child_owner = format!("/agent/{child_pid}");
    init_agent_ui(&shell, &root_owner, &format!("/proc/{root_pid}"), "gpt-5.4").await?;
    init_agent_ui(
        &shell,
        &child_owner,
        &format!("/proc/{child_pid}"),
        "o3-mini",
    )
    .await?;

    shell
        .write(
            &format!("{root_owner}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"root-seed-q\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"root-seed-a\"}\n",
        )
        .await?;
    shell
        .write(
            &format!("{child_owner}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"worker-seed-q\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"worker-seed-a\"}\n",
        )
        .await?;

    let root_task = spawn_input_echo_responder(
        shell.clone(),
        root_owner.clone(),
        "root-reply".into(),
        Some(route_fs.clone()),
    );
    let child_task = spawn_input_echo_responder(
        shell.clone(),
        child_owner.clone(),
        "worker-reply".into(),
        Some(route_fs.clone()),
    );

    let initial_overview = WorkstreamOverviewDto {
        project_key: Some("proj-pty".to_string()),
        counts: WorkstreamCountsDto {
            running: 1,
            idle: 1,
            paused_error: 0,
            archived: 2,
            unread_inbox: 2,
        },
        streams: vec![
            WorkstreamEntryDto {
                name: "worker".to_string(),
                scope: "Implement PTY tests".to_string(),
                persistence: WorkstreamPersistenceKind::Persistent,
                state: WorkstreamStatusKind::Running,
                child_pid: Some(child_pid_num),
                model_override: Some("o3-mini".to_string()),
                rollout_ref: None,
                updated_at: "2026-11-25T08:00:00Z".to_string(),
            },
            WorkstreamEntryDto {
                name: "docs-sync".to_string(),
                scope: "Sync OpenSpec deltas".to_string(),
                persistence: WorkstreamPersistenceKind::Persistent,
                state: WorkstreamStatusKind::Idle,
                child_pid: None,
                model_override: None,
                rollout_ref: None,
                updated_at: "2026-11-25T07:00:00Z".to_string(),
            },
        ],
        unread_inbox: vec![
            WorkstreamInboxEntryDto {
                run_id: "r1".to_string(),
                workflow_id: "nightly".to_string(),
                stream_name: Some("docs-sync".to_string()),
                status: "ok".to_string(),
                summary: "Nightly docs check clean".to_string(),
                created_at: "2026-11-25T05:00:00Z".to_string(),
            },
            WorkstreamInboxEntryDto {
                run_id: "r2".to_string(),
                workflow_id: "audit".to_string(),
                stream_name: None,
                status: "warning".to_string(),
                summary: "One pending review".to_string(),
                created_at: "2026-11-25T06:00:00Z".to_string(),
            },
        ],
    };

    let state = Arc::new(Mutex::new(initial_overview.clone()));
    let state_handler = state.clone();
    let child_agent_for_handler = child_owner.clone();
    let workstream_control: WorkstreamControlHandler = Arc::new(move |control| {
        let state = state_handler.clone();
        let child_agent = child_agent_for_handler.clone();
        Box::pin(async move {
            let mut guard = state.lock().expect("workstream test state");
            match control {
                WorkstreamControl::Inspect => Ok(WorkstreamControlResult::Overview(guard.clone())),
                WorkstreamControl::ResolveFocus { name } => {
                    if name == "worker" {
                        Ok(WorkstreamControlResult::FocusResolved(
                            WorkstreamFocusTarget {
                                stream_name: name,
                                agent_path: child_agent,
                                pid: Some(child_pid_num),
                            },
                        ))
                    } else {
                        Ok(WorkstreamControlResult::FocusUnavailable {
                            message: format!("stream '{name}' is not running"),
                            name,
                        })
                    }
                }
                WorkstreamControl::ClearInbox => {
                    guard.unread_inbox.clear();
                    guard.counts.unread_inbox = 0;
                    Ok(WorkstreamControlResult::ActionCompleted {
                        message: "Acknowledged 2 inbox digest(s).".to_string(),
                        overview: guard.clone(),
                    })
                }
                WorkstreamControl::Archive { name } => {
                    guard.streams.retain(|s| s.name != name);
                    guard.counts.idle = guard
                        .streams
                        .iter()
                        .filter(|s| s.state == WorkstreamStatusKind::Idle)
                        .count();
                    guard.counts.archived += 1;
                    Ok(WorkstreamControlResult::ActionCompleted {
                        message: format!("Archived '{name}'."),
                        overview: guard.clone(),
                    })
                }
                WorkstreamControl::Resume { name } => {
                    Ok(WorkstreamControlResult::ActionCompleted {
                        message: format!("Resumed '{name}'."),
                        overview: guard.clone(),
                    })
                }
            }
        })
    });

    let mut cfg = FileBackedRunConfig::new(root_transport, "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    cfg.initial_workstream_overview = Some(initial_overview);
    cfg.workstream_control = Some(workstream_control);

    let cleared_on_exit = Arc::new(AtomicBool::new(false));
    run_file_backed(cfg).await?;
    cleared_on_exit.store(true, Ordering::SeqCst);

    root_task.abort();
    child_task.abort();
    anyhow::ensure!(
        stdin_restored_canon_and_echo()?,
        "stdin raw mode or ECHO not restored"
    );
    anyhow::ensure!(cleared_on_exit.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn pty_e2e_workstreams_child_runner() {
    let Some(mode) = std::env::var_os(CHILD_MODE_ENV) else {
        return;
    };
    assert_eq!(mode.to_string_lossy(), "workstreams");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(err) = rt.block_on(run_workstreams_child()) {
        eprintln!("workstreams pty child failed: {err:#}");
        std::process::exit(2);
    }
}

#[test]
fn pty_e2e_workstreams_focus_routing_receipts_scrollback_and_narrow_resize() -> Result<()> {
    let (master, slave) = open_pty(36, 80)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(
        &slave,
        master_fd,
        "pty_e2e_workstreams_child_runner",
        "workstreams",
        &[],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new_with_scrollback(36, 80, 500);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        // 1. Startup shows compact footer status-bar count without a permanent sidebar.
        let startup = screen.wait_for("workstreams startup", Duration::from_secs(6), |s, _| {
            s.contains("root-seed-q")
                && s.contains("root-seed-a")
                && s.contains("streams: 1 active · 1 idle · 2 inbox")
                && s.contains(": ")
        })?;
        assert!(startup.contains("gpt-5.4"), "{startup}");

        // 2. /streams prints compact inline overview (active, idle, archived, inbox).
        write_master(&master_write, b"/streams\r")?;
        screen.wait_for(
            "/streams inline overview",
            Duration::from_secs(4),
            |s, _| {
                s.contains("Workstreams")
                    && s.contains("worker")
                    && s.contains("docs-sync")
                    && s.contains("Archived: 2 collapsed")
                    && s.contains("nightly")
            },
        )?;

        // 3. /focus worker switches mounted target to /agent/<child_pid> and shows child tape.
        write_master(&master_write, b"/focus worker\r")?;
        screen.wait_for("/focus worker", Duration::from_secs(4), |s, _| {
            s.contains("focus:worker")
                && s.contains("focused [worker]")
                && s.contains("worker-seed-a")
                && s.contains("o3-mini")
        })?;

        // 4. Sending prompt while focused goes to child /agent/<pid>/io/input + triggers reroute receipt.
        write_master(&master_write, b"trigger-reroute\r")?;
        screen.wait_for(
            "child reply and inline routing receipt",
            Duration::from_secs(4),
            |s, _| {
                s.contains("[worker-reply] trigger-reroute")
                    && s.contains("rerouted from [worker]")
                    && s.contains("[docs-sync]")
                    && s.lines().any(|line| line.trim_end() == ":")
            },
        )?;

        // 5. /root switches back to /agent/root and keeps coherent scrollback without duplication.
        write_master(&master_write, b"/root\r")?;
        screen.wait_for("/root switch", Duration::from_secs(4), |s, _| {
            s.contains("focused [root]") && s.contains("gpt-5.4") && !s.contains("focus:worker")
        })?;

        write_master(&master_write, b"hello-root\r")?;
        screen.wait_for("root reply", Duration::from_secs(4), |s, _| {
            s.contains("[root-reply] hello-root")
        })?;

        let full_scrollback = screen.full_contents();
        assert_eq!(
            full_scrollback.matches("root-seed-q").count(),
            1,
            "Root initial turn duplicated across switch:\n{full_scrollback}"
        );
        assert_eq!(
            full_scrollback.matches("worker-seed-q").count(),
            1,
            "Worker initial turn duplicated:\n{full_scrollback}"
        );

        // 6. /streams clear-inbox updates footer badge live to 0 inbox.
        write_master(&master_write, b"/streams clear-inbox\r")?;
        screen.wait_for("inbox cleared badge", Duration::from_secs(4), |s, _| {
            s.contains("streams: 1 active · 1 idle · 0 inbox")
        })?;

        // 7. Resize to narrow panes (38 cols, then 28 cols) and verify clean reflow + responsive input.
        resize_pty(master_fd, 18, 38)?;
        screen.resize_parser(18, 38);
        std::thread::sleep(Duration::from_millis(120));
        write_master(&master_write, b"narrow38")?;
        let narrow38 = screen.wait_for("narrow 38 input", Duration::from_secs(3), |s, _| {
            s.contains("narrow38")
        })?;
        assert!(
            narrow38.contains("streams:") || narrow38.contains("1a"),
            "missing compact streams badge in 38-col pane:\n{narrow38}"
        );
        write_master(&master_write, b"\x03")?;
        screen.wait_for("clear narrow38", Duration::from_secs(3), |s, _| {
            !s.contains("narrow38")
        })?;

        resize_pty(master_fd, 16, 28)?;
        screen.resize_parser(16, 28);
        std::thread::sleep(Duration::from_millis(120));
        write_master(&master_write, b"narrow28")?;
        screen.wait_for("narrow 28 input", Duration::from_secs(3), |s, _| {
            s.contains("narrow28")
        })?;
        write_master(&master_write, b"\x03")?;

        // Resize back and exit cleanly via Ctrl+Q.
        resize_pty(master_fd, 24, 80)?;
        screen.resize_parser(24, 80);
        std::thread::sleep(Duration::from_millis(100));
        write_master(&master_write, b"\x11")?;
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(status.success(), "PTY child exited with {status:?}");
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
