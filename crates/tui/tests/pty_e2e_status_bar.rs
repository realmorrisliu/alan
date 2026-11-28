#![cfg(unix)]

mod pty_support;

use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_agent_protocol::{
    ReasoningControls, ReasoningEffort, UI_SURFACE_VERSION, UiActivitySnapshot, UiActivityState,
    UiBudgetSnapshot, UiModelBinding, UiModelCatalog, UiModelChoice, UiModelControlSource,
    UiModelSnapshot, UiQueueSnapshot, UiUsageSnapshot,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use alan_tui::{FileBackedRunConfig, run_file_backed, terminal};
use anyhow::{Context, Result};
use pty_support::{
    CHILD_MODE_ENV, ChildGuard, PtyScreen, open_pty, resize_pty, spawn_in_pty, spawn_master_reader,
    stdin_restored_canon_and_echo, write_master,
};

const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";

fn has_nerd_font_glyph(text: &str) -> bool {
    text.chars().any(|ch| {
        let code = ch as u32;
        (0xE000..=0xF8FF).contains(&code)
            || (0xF0000..=0xFFFFD).contains(&code)
            || (0x100000..=0x10FFFD).contains(&code)
    })
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[tokio::main(flavor = "current_thread")]
async fn run_status_bar_child(dumb: bool) -> Result<()> {
    terminal::install_panic_hook();

    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let summary_json = serde_json::json!({
        "exists": true,
        "deleted": false,
        "project_key": "alan-pr2",
        "project_cwd": "/mnt/project",
        "summary": {
            "goal": "Verify PR 2 status bar and modal mouse",
            "progress": "In progress",
            "verification": "PTY",
            "source_rollouts": [],
            "revision": 5,
            "updated_at": "2026-11-26T00:00:00Z",
            "origin": "generated",
            "deleted": false
        },
        "markdown_recall_active": true,
        "retention_note": "derived"
    })
    .to_string()
    .into_bytes();
    agent_root
        .bind_history_server(Arc::new(MemFs::with_read_only_file(
            "summary",
            summary_json,
        )))
        .await;

    let mount_fs = Arc::new(MountFs::new(Namespace::new()));
    let live = mount_fs.live_namespace();
    live.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    live.mount(
        "/mnt/branch/branches",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "cow-1",
            br#"{"status":"active"}"#.to_vec(),
        ))),
        Access::ReadOnly,
    );
    let catalog_bytes = serde_json::json!({
        "packages": {
            "github-fileserver": {
                "state": "installed",
                "exports": [{"kind": "fileserver", "service_id": "github"}]
            }
        }
    })
    .to_string()
    .into_bytes();
    live.mount(
        "/mnt/package",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "catalog",
            catalog_bytes,
        ))),
        Access::ReadOnly,
    );
    live.mount(
        "/mnt/github",
        InProcessTransport::new(Arc::new(MemFs::new())),
        Access::ReadOnly,
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
    let models = UiModelSnapshot {
        version: UI_SURFACE_VERSION,
        process_path,
        publication_version: 1,
        known: true,
        selected_next: Some(UiModelBinding {
            profile: "openai-main".into(),
            provider: "openai".into(),
            model: "gpt-5.4".into(),
            reasoning: ReasoningControls {
                effort: Some(ReasoningEffort::High),
            },
            control_source: UiModelControlSource::AgentConfig,
        }),
        active: None,
        catalog: Some(UiModelCatalog {
            profile: "openai-main".into(),
            models: vec![UiModelChoice {
                model: "gpt-5.4".into(),
                supported_reasoning_efforts: vec![ReasoningEffort::Medium, ReasoningEffort::High],
                default_reasoning_effort: Some(ReasoningEffort::High),
            }],
            profiles: Vec::new(),
        }),
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
            &serde_json::to_vec(&UiQueueSnapshot::default())?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/ui/activity"),
            &serde_json::to_vec(&UiActivitySnapshot {
                version: UI_SURFACE_VERSION,
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
                version: UI_SURFACE_VERSION,
                input_tokens: 10_000,
                cached_input_tokens: Some(6_000),
                output_tokens: 500,
                context_tokens: 96_000,
                context_window: 128_000,
                session_input_tokens: 50_000,
                session_cached_input_tokens: Some(30_000),
                session_output_tokens: 2_000,
                session_cost_usd: Some(0.025),
                ..UiUsageSnapshot::empty()
            })?,
        )
        .await?;
    shell
        .write(
            &format!("{owner}/machine/ui/budget"),
            &serde_json::to_vec(&UiBudgetSnapshot {
                version: UI_SURFACE_VERSION,
                elapsed_secs: 1_080,
                max_secs: 1_800,
                tool_calls: 84,
                max_tool_calls: 256,
                model_calls: 43,
                max_model_calls: 64,
                auto_extensions_used: Some(1),
                max_auto_extensions: Some(3),
            })?,
        )
        .await?;

    // Seed an assistant message with a URL + relative path to test OSC 8 emission & widths.
    let tape_line = serde_json::json!({
        "version": 1,
        "kind": "message",
        "role": "assistant",
        "content": "Docs: https://example.com/pr2 and crates/tui/src/terminal.rs:42"
    })
    .to_string();
    shell
        .write_exclusive(
            &format!("{owner}/machine/tape"),
            format!("{tape_line}\n").as_bytes(),
        )
        .await?;

    let mut cfg = FileBackedRunConfig::new(root_transport, "/agent/root");
    cfg.effective_model = Some("gpt-5.4".into());
    cfg.sandbox_backend = Some("seatbelt".into());
    cfg.ephemeral = true;
    cfg.no_resume_update = true;
    if dumb {
        cfg.trust_posture_label = Some("unsandboxed".into());
    }
    run_file_backed(cfg).await
}

fn run_mouse_panic_child() -> Result<()> {
    terminal::install_panic_hook();
    let mut session = terminal::TerminalSession::enter()?;
    session.set_mouse_capture(true)?;
    panic!("intentional modal mouse panic test");
}

#[test]
fn pty_e2e_status_bar_wide_narrow_modal_mouse_and_osc8() -> Result<()> {
    if std::env::var_os(CHILD_MODE_ENV).as_deref() == Some(OsStr::new("status_bar")) {
        run_status_bar_child(false)?;
        assert!(
            stdin_restored_canon_and_echo()?,
            "expected cooked tty restoration after Ctrl+Q"
        );
        return Ok(());
    }

    let (master, slave) = open_pty(28, 108)?;
    let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
    let master_read = master.try_clone().context("clone master")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
    let child = spawn_in_pty(
        &slave,
        master_fd,
        "pty_e2e_status_bar_wide_narrow_modal_mouse_and_osc8",
        "status_bar",
        &[],
    )?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new(28, 108);
    let reader = spawn_master_reader(master_read, master_write.clone(), screen.clone());

    let res = (|| -> Result<()> {
        // 1. Wide startup: 2-row status bar below composer, live budget & usage, zero seatbelt/Nerd Font,
        //    and SGR mouse capture strictly OFF while inline.
        let startup =
            screen.wait_for("wide 2-row status bar", Duration::from_secs(6), |s, _| {
                s.contains("openai-main/gpt-5.4 · high")
                    && s.contains("ctx 75%")
                    && s.contains("18m/30m 84/256t 43/64m")
                    && s.contains("^T status")
                    && s.contains("Docs: https://example.com/pr2 and crates/tui/src/terminal.rs:42")
            })?;
        assert!(!startup.contains("seatbelt"), "{startup}");
        assert!(!has_nerd_font_glyph(&startup), "{startup}");

        let inline_raw = screen.raw_slice_from(0);
        assert!(
            !contains_bytes(&inline_raw, b"\x1b[?1000h")
                && !contains_bytes(&inline_raw, b"\x1b[?1006h"),
            "mouse capture must stay OFF during inline operation"
        );
        assert!(
            contains_bytes(&inline_raw, b"\x1b]8;;https://example.com/pr2\x1b\\"),
            "expected OSC 8 hyperlink for URL on non-dumb TTY"
        );
        assert!(
            contains_bytes(&inline_raw, b"\x1b]8;;file://"),
            "expected OSC 8 file:// hyperlink on non-dumb TTY"
        );

        // 2. Narrow resize (<80 cols): degrades cleanly without overflowing or leaking seatbelt.
        resize_pty(master_fd, 24, 62)?;
        screen.resize_parser(24, 62);
        std::thread::sleep(Duration::from_millis(120));
        write_master(&master_write, b"x")?;
        let narrow = screen.wait_for("narrow status bar", Duration::from_secs(4), |s, _| {
            s.contains(": x") && s.contains("gpt-5.4")
        })?;
        assert!(!narrow.contains("seatbelt"), "{narrow}");
        write_master(&master_write, b"\x7f")?;

        // Restore wide width and open Ctrl+T Status Panel (42 rows fits all 7 sections cleanly).
        resize_pty(master_fd, 42, 112)?;
        screen.resize_parser(42, 112);
        let before_modal_raw = screen.raw_len();
        write_master(&master_write, b"\x14")?; // Ctrl+T
        let panel = screen.wait_for("Ctrl+T status panel", Duration::from_secs(5), |s, _| {
            s.contains("Task Budget")
                && s.contains("18m00s / 30m00s")
                && s.contains("OS Sandbox & Isolation")
                && s.contains("seatbelt")
                && s.contains("System 1 (Fast Evaluator)")
                && s.contains("rev 5")
        })?;
        assert!(panel.contains("[s:Status]"), "{panel}");

        let modal_open_raw = screen.raw_slice_from(before_modal_raw);
        assert!(
            contains_bytes(&modal_open_raw, b"\x1b[?1000h")
                && contains_bytes(&modal_open_raw, b"\x1b[?1006h"),
            "expected EnableMouseCapture when Ctrl+T Status panel opens"
        );

        // 3. SGR mouse scroll down + SGR left-click on row 2 (`a:Actions` tab at col 2, row 2).
        write_master(&master_write, b"\x1b[<65;10;10M")?;
        write_master(&master_write, b"\x1b[<0;2;2M\x1b[<0;2;2m")?;
        screen.wait_for(
            "mouse click switches to Actions tab",
            Duration::from_secs(4),
            |s, _| s.contains("[a:Actions]"),
        )?;

        // 4. Esc closes modal and emits DisableMouseCapture.
        let before_close_raw = screen.raw_len();
        write_master(&master_write, b"\x1b")?;
        screen.wait_for("Esc closes modal", Duration::from_secs(4), |s, _| {
            !s.contains("[a:Actions]") && s.contains("^T status")
        })?;
        let close_raw = screen.raw_slice_from(before_close_raw);
        assert!(
            contains_bytes(&close_raw, b"\x1b[?1000l")
                && contains_bytes(&close_raw, b"\x1b[?1006l"),
            "expected DisableMouseCapture when modal closes"
        );

        write_master(&master_write, b"\x11")?; // Ctrl+Q
        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(status.success(), "child exited with {status:?}");
        Ok(())
    })();

    drop(master_write);
    drop(child);
    let _ = reader.join();
    res
}

#[test]
fn pty_e2e_dumb_ascii_status_bar_disables_osc8_and_mouse_and_panic_cleans_mouse() -> Result<()> {
    match std::env::var_os(CHILD_MODE_ENV).as_deref() {
        Some(v) if v == OsStr::new("dumb_status") => {
            run_status_bar_child(true)?;
            return Ok(());
        }
        Some(v) if v == OsStr::new("mouse_panic") => {
            run_mouse_panic_child()?;
            return Ok(());
        }
        _ => {}
    }

    // Part A: TERM=dumb -> pure ASCII status bar, `! unsandboxed`, NO OSC 8, NO mouse capture.
    {
        let (master, slave) = open_pty(28, 100)?;
        let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
        let master_read = master.try_clone().context("clone master")?;
        let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
        let child = spawn_in_pty(
            &slave,
            master_fd,
            "pty_e2e_dumb_ascii_status_bar_disables_osc8_and_mouse_and_panic_cleans_mouse",
            "dumb_status",
            &[("TERM", OsStr::new("dumb")), ("NO_COLOR", OsStr::new("1"))],
        )?;
        drop(slave);
        let mut child = ChildGuard::new(child);

        let screen = PtyScreen::new(28, 100);
        let reader = spawn_master_reader(master_read, master_write.clone(), screen.clone());

        let res = (|| -> Result<()> {
            let dumb_screen =
                screen.wait_for("dumb status bar", Duration::from_secs(6), |s, _| {
                    s.contains("! unsandboxed") && s.contains("ctx 75%") && s.contains("^T status")
                })?;
            for line in dumb_screen
                .lines()
                .filter(|l| l.contains("! unsandboxed") || l.contains("ctx 75%"))
            {
                assert!(line.is_ascii(), "expected pure ASCII status row: {line:?}");
            }

            // Open Ctrl+T in dumb mode -> mouse capture must stay OFF.
            write_master(&master_write, b"\x14")?;
            screen.wait_for("dumb Ctrl+T panel", Duration::from_secs(4), |s, _| {
                s.contains("Task Budget") && s.contains("[s:Status]")
            })?;
            write_master(&master_write, b"\x1b")?;
            screen.wait_for("Esc closes dumb modal", Duration::from_secs(4), |s, _| {
                !s.contains("[s:Status]") && s.contains("^T status")
            })?;
            write_master(&master_write, b"\x11")?;
            let status = child.wait_with_timeout(Duration::from_secs(5))?;
            assert!(status.success(), "dumb child exited with {status:?}");
            let all_raw = screen.wait_eof(Duration::from_secs(3))?;
            assert!(
                !contains_bytes(&all_raw, b"\x1b]8;;"),
                "TERM=dumb must not emit OSC 8 hyperlinks"
            );
            assert!(
                !contains_bytes(&all_raw, b"\x1b[?1000h")
                    && !contains_bytes(&all_raw, b"\x1b[?1006h"),
                "TERM=dumb must not enable mouse capture"
            );
            Ok(())
        })();

        drop(master_write);
        drop(child);
        let _ = reader.join();
        res?;
    }

    // Part B: Panic while modal SGR mouse capture is active -> panic hook emits DisableMouseCapture.
    {
        let (master, slave) = open_pty(24, 80)?;
        let master_fd = std::os::fd::AsRawFd::as_raw_fd(&master);
        let master_read = master.try_clone().context("clone master")?;
        let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
        let child = spawn_in_pty(
            &slave,
            master_fd,
            "pty_e2e_dumb_ascii_status_bar_disables_osc8_and_mouse_and_panic_cleans_mouse",
            "mouse_panic",
            &[],
        )?;
        drop(slave);
        let mut child = ChildGuard::new(child);

        let screen = PtyScreen::new(24, 80);
        let reader = spawn_master_reader(master_read, master_write.clone(), screen.clone());

        let status = child.wait_with_timeout(Duration::from_secs(5))?;
        assert!(!status.success(), "expected panic child to exit non-zero");
        let raw = screen.wait_eof(Duration::from_secs(3))?;
        assert!(
            contains_bytes(&raw, b"\x1b[?1000h") && contains_bytes(&raw, b"\x1b[?1006h"),
            "expected mouse capture to have been enabled before panic"
        );
        assert!(
            contains_bytes(&raw, b"\x1b[?1000l") && contains_bytes(&raw, b"\x1b[?1006l"),
            "expected panic hook to emit DisableMouseCapture"
        );
        drop(master_write);
        let _ = reader.join();
    }

    Ok(())
}
