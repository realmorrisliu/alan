#![cfg(unix)]

#[allow(
    dead_code,
    reason = "shared PTY helpers are consumed selectively across test binaries"
)]
mod pty_support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_agent_protocol::{
    ContentPart, InputIntent, InputMode, UiActivitySnapshot, UiActivityState, UiEvent,
    UiInputStatus, UiModelBinding, UiModelControlSource, UiModelSnapshot, UiQueueSnapshot,
    UserInputRecord,
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
    write_master,
};

const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";
const CHILD_IMG_ENV: &str = "ALAN_PTY_IMAGE_PATH";
const CHILD_RECORD_ENV: &str = "ALAN_PTY_RECORD_OUT";
const MINIMAL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15c4";

#[tokio::main(flavor = "current_thread")]
async fn run_child() -> Result<()> {
    let record_out = std::env::var(CHILD_RECORD_ENV).context("missing record output path")?;
    terminal::install_panic_hook();

    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let mount_fs = Arc::new(MountFs::new(Namespace::new()));
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

    let input_shell = shell.clone();
    let input_owner = owner.clone();
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
                if let Ok(Some(record)) = UserInputRecord::decode_payload(&payload)
                    && let Ok(json) = serde_json::to_vec(&record)
                {
                    let _ = std::fs::write(&record_out, &json);
                    let mut ev = serde_json::to_vec(&UiEvent::InputCompleted {
                        submission_ids: vec![record.submission_id],
                        status: UiInputStatus::Completed,
                        error: None,
                    })
                    .unwrap_or_default();
                    ev.push(b'\n');
                    let _ = input_shell
                        .write(&format!("{input_owner}/machine/ui/events"), &ev)
                        .await;
                    let tape = format!(
                        "{}\n",
                        serde_json::json!({
                            "version": 1,
                            "kind": "message",
                            "role": "assistant",
                            "content": "image received"
                        })
                    );
                    let _ = input_shell
                        .write(&format!("{input_owner}/machine/tape"), tape.as_bytes())
                        .await;
                }
            }
        }
    });

    let mut cfg = FileBackedRunConfig::new(root_transport, "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    run_file_backed(cfg).await?;
    input_task.abort();
    Ok(())
}

#[test]
fn pty_e2e_bracketed_image_paste_renders_preview_and_submits_without_slash_error() -> Result<()> {
    if std::env::var(CHILD_MODE_ENV).ok().as_deref() == Some("image-paste") {
        let _ = std::env::var(CHILD_IMG_ENV);
        return run_child();
    }

    let dir = tempfile::tempdir().context("tempdir")?;
    let png_path = dir.path().join("rex-clipboard-pty.png");
    let record_out = dir.path().join("submitted-record.json");
    std::fs::write(&png_path, MINIMAL_PNG).context("write png")?;

    let (master, slave) = open_pty(24, 80)?;
    let master_read = master.try_clone().context("clone pty master")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
    let master_raw = std::os::fd::AsRawFd::as_raw_fd(&*master_write.lock().unwrap());

    let child = spawn_in_pty(
        &slave,
        master_raw,
        "pty_e2e_bracketed_image_paste_renders_preview_and_submits_without_slash_error",
        "image-paste",
        &[
            (CHILD_IMG_ENV, png_path.as_os_str()),
            (CHILD_RECORD_ENV, record_out.as_os_str()),
        ],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(24, 80);
    let reader = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let result = (|| -> Result<()> {
        screen.wait_for("startup ready", Duration::from_secs(6), |s, _| {
            s.contains("gpt-5.4") && s.contains(": ")
        })?;

        // Paste PNG path via VT bracketed paste.
        let paste_seq = format!("\x1b[200~{}\x1b[201~", png_path.display());
        write_master(&master_write, paste_seq.as_bytes())?;

        let pasted_screen = screen.wait_for(
            "attachment preview strip",
            Duration::from_secs(4),
            |s, _| s.contains("+ 1 image:") && s.contains("rex-clipboard-pty.png"),
        )?;
        assert!(
            !pasted_screen.contains("unknown command"),
            "unexpected slash command warning:\n{pasted_screen}"
        );
        assert!(
            !pasted_screen.contains(&png_path.display().to_string()),
            "raw pasted path should not be inserted into composer:\n{pasted_screen}"
        );

        // Press Enter to submit image-only turn.
        write_master(&master_write, b"\r")?;

        screen.wait_for(
            "assistant response on screen",
            Duration::from_secs(5),
            |s, _| s.contains("image received"),
        )?;

        let raw_record = std::fs::read(&record_out).context("read submitted record")?;
        let record: UserInputRecord =
            serde_json::from_slice(&raw_record).context("decode submitted UserInputRecord")?;
        assert_eq!(record.intent, InputIntent::Agent);
        assert_eq!(record.mode, InputMode::FollowUp);
        assert!(record.body.is_empty(), "{record:?}");
        assert_eq!(record.parts.len(), 1, "{record:?}");
        match &record.parts[0] {
            ContentPart::Attachment {
                mime_type,
                metadata,
                ..
            } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(metadata["filename"], "rex-clipboard-pty.png");
                assert_eq!(metadata["byte_len"], MINIMAL_PNG.len());
            }
            other => bail!("expected attachment part, got {other:?}"),
        }

        // Quit cleanly via Ctrl+Q.
        write_master(&master_write, b"\x11")?;
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        if !status.success() {
            bail!("child exited unsuccessfully: {status}");
        }
        Ok(())
    })();

    drop(master_write);
    let _ = reader.join();
    result
}
