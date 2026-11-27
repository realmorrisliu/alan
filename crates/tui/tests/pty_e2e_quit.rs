#![cfg(unix)]

mod pty_support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_agent_protocol::{
    UiActivitySnapshot, UiActivityState, UiModelBinding, UiModelControlSource, UiModelSnapshot,
    UiQueueSnapshot,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use alan_tui::{FileBackedRunConfig, run_file_backed, terminal};
use anyhow::{Context, Result, bail};
use pty_support::{
    CHILD_MODE_ENV, ChildGuard, PtyScreen, open_pty, spawn_in_pty, spawn_master_reader,
    stdin_restored_canon_and_echo, write_master,
};

const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";

async fn run_child_quit_session() -> Result<()> {
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
    let models = UiModelSnapshot {
        version: 1,
        process_path: format!("/proc/{pid}"),
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
        catalog: None,
        admitted: Vec::new(),
    };
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&models)?,
        )
        .await
        .context("write ui/models")?;
    shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(&UiQueueSnapshot {
                known: true,
                revision: 1,
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

    let mut cfg = FileBackedRunConfig::new(root_transport, "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    run_file_backed(cfg).await?;

    if !stdin_restored_canon_and_echo()? {
        bail!("stdin raw mode or ECHO was not restored after /quit exit");
    }
    Ok(())
}

#[test]
fn pty_e2e_quit_child_runner() {
    if std::env::var_os(CHILD_MODE_ENV).is_none() {
        return;
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(err) = rt.block_on(run_child_quit_session()) {
        eprintln!("pty quit child failed: {err:#}");
        std::process::exit(2);
    }
}

fn verify_quit_in_pty(
    send_quit: impl FnOnce(&Arc<Mutex<std::fs::File>>, &PtyScreen) -> Result<()>,
) -> Result<()> {
    let (master, slave) = open_pty(24, 80)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master fd")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));

    let child = spawn_in_pty(&slave, master_fd, "pty_e2e_quit_child_runner", "quit", &[])?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(24, 80);
    let reader_handle = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res: Result<()> = (|| {
        screen.wait_for("initial prompt rendered", Duration::from_secs(6), |s, _| {
            s.contains("gpt-5.4") && s.lines().any(|line| line.starts_with(": "))
        })?;

        send_quit(&master_write, &screen)?;

        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(
            status.success(),
            "/quit + single Enter child failed with {status:?}\nScreen:\n{}",
            screen.contents()
        );
        drop(master_write);
        let raw = screen.wait_eof(Duration::from_secs(3))?;
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(
            raw_text.contains("\x1b[?2004l"),
            "missing bracketed paste disable sequence on /quit exit"
        );
        Ok(())
    })();

    drop(child);
    let _ = reader_handle.join();
    res
}

#[test]
fn pty_e2e_slash_quit_single_enter_exits_immediately() -> Result<()> {
    verify_quit_in_pty(|master_write, screen| {
        write_master(master_write, b"/quit")?;
        screen.wait_for("/quit typed in composer", Duration::from_secs(3), |s, _| {
            s.contains(": /quit")
        })?;
        // Single Enter must exit immediately without requiring a second Enter.
        write_master(master_write, b"\r")?;
        Ok(())
    })
}

#[test]
fn pty_e2e_pasted_slash_quit_single_enter_exits_immediately() -> Result<()> {
    verify_quit_in_pty(|master_write, screen| {
        write_master(master_write, b"\x1b[200~/quit\x1b[201~")?;
        screen.wait_for(
            "/quit pasted in composer",
            Duration::from_secs(3),
            |s, _| s.contains(": /quit"),
        )?;
        // Single Enter must exit immediately without requiring a second Enter.
        write_master(master_write, b"\r")?;
        Ok(())
    })
}
