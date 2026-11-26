#![cfg(unix)]

#[allow(
    dead_code,
    reason = "shared PTY helpers are consumed selectively across test binaries"
)]
mod pty_support;

use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alan_agent_protocol::{
    UiActivitySnapshot, UiActivityState, UiEvent, UiInputStatus, UiModelBinding,
    UiModelControlSource, UiModelSnapshot, UiQueueSnapshot, UserInputRecord,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::reference::MemFs;
use alan_ap::{Fid, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use alan_tui::graphics::{GraphicsCapability, KITTY_PROBE_QUERY, XTVERSION_QUERY};
use alan_tui::graphics_scrollback::ScrollbackItem;
use alan_tui::terminal::{TerminalSession, install_panic_hook};
use alan_tui::{FileBackedRunConfig, run_file_backed};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use crossterm::event::{Event, KeyCode, poll, read};
use pty_support::{
    CHILD_MODE_ENV, ChildGuard, PtyScreen, open_pty, spawn_in_pty, spawn_master_reader_with_probe,
    write_master,
};
use ratatui::widgets::Paragraph;

const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";
const CHILD_PNG_ENV: &str = "ALAN_PTY_GFX_PNG_PATH";

fn sample_png(width: u32, height: u32, pad_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24 + pad_bytes);
    bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.resize(24 + pad_bytes, 0x5a);
    bytes
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

fn count_subslice(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let (mut count, mut i) = (0, 0);
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            count += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    count
}

fn read_line_keys_until_enter() -> Result<String> {
    let mut out = String::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !poll(Duration::from_millis(50))? {
            continue;
        }
        if let Event::Key(key) = read()? {
            match key.code {
                KeyCode::Enter => return Ok(out),
                KeyCode::Char(ch) => out.push(ch),
                _ => {}
            }
        }
    }
    bail!("timed out waiting for Enter; collected `{out}`")
}

fn parse_top_marker(screen: &str, prefix: &str) -> Result<u16> {
    let start = screen
        .find(prefix)
        .with_context(|| format!("missing `{prefix}` marker in screen:\n{screen}"))?;
    let rest = &screen[start + prefix.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits
        .parse::<u16>()
        .with_context(|| format!("parse top row after `{prefix}` in `{rest}`"))
}

fn wait_for_cursor(screen: &PtyScreen, expected: (u16, u16), timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let actual = screen.cursor_position();
        if actual == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "cursor mismatch: expected {expected:?}, got {actual:?}\nScreen:\n{}",
                screen.contents()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn with_pty_harness<F>(
    test_name: &str,
    mode: &str,
    extra_env: &[(&str, &OsStr)],
    probe_reply: Option<(&'static [u8], &'static [u8])>,
    scrollback: usize,
    body: F,
) -> Result<()>
where
    F: FnOnce(&PtyScreen, &Arc<Mutex<std::fs::File>>, &mut ChildGuard) -> Result<()>,
{
    let (master, slave) = open_pty(24, 80)?;
    let master_read = master.try_clone().context("clone pty master")?;
    let master_write = Arc::new(Mutex::new(std::fs::File::from(master)));
    let master_raw = std::os::fd::AsRawFd::as_raw_fd(&*master_write.lock().unwrap());
    let child = spawn_in_pty(&slave, master_raw, test_name, mode, extra_env)?;
    drop(slave);
    let mut child = ChildGuard::new(child);

    let screen = PtyScreen::new_with_scrollback(24, 80, scrollback);
    let reader = spawn_master_reader_with_probe(
        master_read,
        master_write.clone(),
        screen.clone(),
        probe_reply,
    );
    let result = body(&screen, &master_write, &mut child);
    drop(master_write);
    let _ = reader.join();
    result
}

fn run_session_fallback_child(label: &str) -> Result<()> {
    install_panic_hook();
    let mut session = TerminalSession::enter()?;
    anyhow::ensure!(
        session.graphics_capability() == GraphicsCapability::None,
        "expected GraphicsCapability::None, got {:?}",
        session.graphics_capability()
    );
    let png = sample_png(320, 240, 64);
    session.draw_inline_frame_items(
        &[ScrollbackItem::Image {
            bytes: png,
            label: label.to_string(),
            dimensions: None,
        }],
        2,
        2,
        |frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new("FALLBACK-READY"), area);
            frame.set_cursor_position((area.x + 3, area.y));
        },
    )?;
    let _ = read_line_keys_until_enter()?;
    Ok(())
}

fn run_session_graphics_child(
    expected: GraphicsCapability,
    label: &str,
    pad_bytes: usize,
) -> Result<()> {
    install_panic_hook();
    let mut session = TerminalSession::enter()?;
    anyhow::ensure!(
        session.graphics_capability() == expected,
        "expected {expected:?}, got {:?}",
        session.graphics_capability()
    );
    anyhow::ensure!(
        !poll(Duration::from_millis(25))?,
        "unexpected leaked probe input bytes on stdin"
    );

    session.draw_inline_frame_items(&[], 3, 3, |frame| {
        let area = frame.area();
        frame.render_widget(Paragraph::new(format!("STAGE-1 top={}", area.y)), area);
        frame.set_cursor_position((area.x + 2, area.y + 1));
    })?;

    let typed = read_line_keys_until_enter()?;
    anyhow::ensure!(typed == "hello", "expected `hello`, got `{typed:?}`");

    let png = sample_png(640, 480, pad_bytes);
    session.draw_inline_frame_items(
        &[ScrollbackItem::Image {
            bytes: png,
            label: label.to_string(),
            dimensions: None,
        }],
        3,
        3,
        |frame| {
            let area = frame.area();
            frame.render_widget(
                Paragraph::new(format!("STAGE-2 top={} typed={typed}", area.y)),
                area,
            );
            frame.set_cursor_position((area.x + 7, area.y + 1));
        },
    )?;

    let next = read_line_keys_until_enter()?;
    anyhow::ensure!(next == "redraw", "expected `redraw`, got `{next:?}`");

    for step in 1..=2 {
        session.draw_inline_frame_items(&[], 3, 3, |frame| {
            let area = frame.area();
            frame.render_widget(
                Paragraph::new(format!("STAGE-3-{step} top={}", area.y)),
                area,
            );
            frame.set_cursor_position((area.x + 9, area.y + 2));
        })?;
    }

    let _ = read_line_keys_until_enter()?;
    Ok(())
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

#[tokio::main(flavor = "current_thread")]
async fn run_file_backed_graphics_child() -> Result<()> {
    install_panic_hook();

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

    let io_shell = shell.clone();
    let io_transport = root_transport.clone();
    let io_owner = owner.clone();
    let input_task = tokio::spawn(async move {
        let Ok(mut tail) = io_shell.tail(&format!("{io_owner}/io/input")).await else {
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
                let Ok(Some(record)) = UserInputRecord::decode_payload(&payload) else {
                    continue;
                };

                let user_tape = format!(
                    "{}\n",
                    serde_json::json!({
                        "version": 1,
                        "kind": "message",
                        "role": "user",
                        "content": record.body,
                        "input_intent": "agent",
                        "submission_id": record.submission_id,
                    })
                );
                let _ = io_shell
                    .write(&format!("{io_owner}/machine/tape"), user_tape.as_bytes())
                    .await;

                if let Ok(act_id) =
                    allocate_clone_id(&io_transport, &format!("{io_owner}/actions/clone")).await
                {
                    let tool_png = sample_png(400, 300, 128);
                    let tool_result = serde_json::json!({
                        "title": "read_file tool-diagram.png",
                        "content_parts": [{
                            "type": "image",
                            "mime_type": "image/png",
                            "filename": "tool-diagram.png",
                            "data": BASE64_STANDARD.encode(&tool_png),
                        }]
                    })
                    .to_string();
                    let _ = io_shell
                        .write(&format!("{io_owner}/actions/{act_id}/name"), b"read_file")
                        .await;
                    let _ = io_shell
                        .write(
                            &format!("{io_owner}/actions/{act_id}/result"),
                            tool_result.as_bytes(),
                        )
                        .await;
                    let _ = io_shell
                        .write(&format!("{io_owner}/actions/{act_id}/status"), b"completed")
                        .await;
                }

                tokio::time::sleep(Duration::from_millis(120)).await;

                let mut body = String::new();
                for line_idx in 1..=28 {
                    body.push_str(&format!("scrollback push line {line_idx:02}\n"));
                }
                body.push_str("FINAL_SCROLLBACK_MARKER");
                let assistant_tape = format!(
                    "{}\n",
                    serde_json::json!({
                        "version": 1,
                        "kind": "message",
                        "role": "assistant",
                        "content": body,
                    })
                );
                let _ = io_shell
                    .write(
                        &format!("{io_owner}/machine/tape"),
                        assistant_tape.as_bytes(),
                    )
                    .await;

                let mut ev = serde_json::to_vec(&UiEvent::InputCompleted {
                    submission_ids: vec![record.submission_id],
                    status: UiInputStatus::Completed,
                    error: None,
                })
                .unwrap_or_default();
                ev.push(b'\n');
                let _ = io_shell
                    .write(&format!("{io_owner}/machine/ui/events"), &ev)
                    .await;
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
fn pty_e2e_dumb_and_unsupported_terminal_fallback_to_badge_only() -> Result<()> {
    match std::env::var(CHILD_MODE_ENV).ok().as_deref() {
        Some("dumb") => return run_session_fallback_child("dumb-shot.png"),
        Some("unsupported") => return run_session_fallback_child("unsupported.png"),
        _ => {}
    }

    let test_name = "pty_e2e_dumb_and_unsupported_terminal_fallback_to_badge_only";
    // (1) TERM=dumb skips probing completely (even when master would reply to Kitty).
    with_pty_harness(
        test_name,
        "dumb",
        &[("TERM", OsStr::new("dumb")), ("COLORTERM", OsStr::new(""))],
        Some((KITTY_PROBE_QUERY, b"\x1b_Gi=31;OK\x1b\\")),
        100,
        |screen, master_write, child| {
            screen.wait_for("dumb fallback ready", Duration::from_secs(5), |s, _| {
                s.contains("FALLBACK-READY")
            })?;
            let full = screen.full_contents();
            assert!(
                full.contains("[image: dumb-shot.png · 320x240 · 88 B]"),
                "expected text badge in TERM=dumb output:\n{full}"
            );
            let raw = screen.raw_slice_from(0);
            assert!(!contains_bytes(&raw, KITTY_PROBE_QUERY));
            assert!(!contains_bytes(&raw, XTVERSION_QUERY));
            assert!(!contains_bytes(&raw, b"\x1b_G"));
            assert!(!contains_bytes(&raw, b"\x1b]1337;"));
            write_master(master_write, b"\r")?;
            anyhow::ensure!(child.wait_with_timeout(Duration::from_secs(5))?.success());
            Ok(())
        },
    )?;

    // (2) Non-dumb terminal with no probe reply falls back cleanly to badge-only.
    with_pty_harness(
        test_name,
        "unsupported",
        &[],
        None,
        100,
        |screen, master_write, child| {
            screen.wait_for(
                "unsupported fallback ready",
                Duration::from_secs(5),
                |s, _| s.contains("FALLBACK-READY"),
            )?;
            let full = screen.full_contents();
            assert!(
                full.contains("[image: unsupported.png · 320x240 · 88 B]"),
                "expected text badge for unsupported terminal:\n{full}"
            );
            let raw = screen.raw_slice_from(0);
            assert!(!contains_bytes(&raw, b"\x1b_Ga=T"));
            assert!(!contains_bytes(&raw, b"\x1b]1337;"));
            write_master(master_write, b"\r")?;
            anyhow::ensure!(child.wait_with_timeout(Duration::from_secs(5))?.success());
            Ok(())
        },
    )
}

#[test]
fn pty_e2e_kitty_probe_chunked_scrollback_and_cursor_restoration() -> Result<()> {
    if std::env::var(CHILD_MODE_ENV).ok().as_deref() == Some("kitty") {
        // 3600 bytes raw -> 4800 bytes base64 -> forces multi-chunk m=1 then m=0 Kitty APC.
        return run_session_graphics_child(GraphicsCapability::Kitty, "kitty-chart.png", 3600);
    }

    with_pty_harness(
        "pty_e2e_kitty_probe_chunked_scrollback_and_cursor_restoration",
        "kitty",
        &[],
        Some((KITTY_PROBE_QUERY, b"\x1b_Gi=31;OK\x1b\\")),
        100,
        |screen, master_write, child| {
            let stage1 = screen.wait_for("kitty stage 1", Duration::from_secs(5), |s, _| {
                s.contains("STAGE-1 top=")
            })?;
            let top1 = parse_top_marker(&stage1, "STAGE-1 top=")?;
            wait_for_cursor(screen, (top1 + 1, 2), Duration::from_secs(2))?;

            let before_commit = screen.raw_len();
            write_master(master_write, b"hello\r")?;

            let stage2 = screen.wait_for("kitty stage 2", Duration::from_secs(5), |s, _| {
                s.contains("STAGE-2 top=") && s.contains("typed=hello")
            })?;
            let top2 = parse_top_marker(&stage2, "STAGE-2 top=")?;
            assert!(
                top2 > top1,
                "expected scrollback push: top1={top1}, top2={top2}"
            );
            wait_for_cursor(screen, (top2 + 1, 7), Duration::from_secs(2))?;

            let commit_bytes = screen.raw_slice_from(before_commit);
            assert!(
                contains_bytes(&commit_bytes, b"\x1b_Ga=T,f=100,")
                    && contains_bytes(&commit_bytes, b",m=1,q=2;"),
                "expected first multi-chunk Kitty APC (a=T,f=100,...,m=1,q=2;)"
            );
            assert!(contains_bytes(&commit_bytes, b"\x1b_Gm=0;"));
            assert!(!contains_bytes(&commit_bytes, b"\x1b]1337;"));
            assert!(
                screen
                    .full_contents()
                    .contains("[image: kitty-chart.png · 640x480 · ")
            );

            let before_redraw = screen.raw_len();
            write_master(master_write, b"redraw\r")?;
            let stage3 = screen.wait_for("kitty stage 3", Duration::from_secs(5), |s, _| {
                s.contains("STAGE-3-2 top=")
            })?;
            let top3 = parse_top_marker(&stage3, "STAGE-3-2 top=")?;
            assert_eq!(top3, top2);
            wait_for_cursor(screen, (top3 + 2, 9), Duration::from_secs(2))?;

            let redraw_bytes = screen.raw_slice_from(before_redraw);
            assert!(!contains_bytes(&redraw_bytes, b"\x1b_G"));
            assert!(!contains_bytes(&redraw_bytes, b"\x1b]1337;"));

            write_master(master_write, b"\r")?;
            anyhow::ensure!(child.wait_with_timeout(Duration::from_secs(5))?.success());
            Ok(())
        },
    )
}

#[test]
fn pty_e2e_iterm2_probe_osc1337_scrollback_and_cursor_restoration() -> Result<()> {
    if std::env::var(CHILD_MODE_ENV).ok().as_deref() == Some("iterm2") {
        return run_session_graphics_child(GraphicsCapability::ITerm2, "iterm-photo.png", 256);
    }

    with_pty_harness(
        "pty_e2e_iterm2_probe_osc1337_scrollback_and_cursor_restoration",
        "iterm2",
        &[],
        Some((XTVERSION_QUERY, b"\x1bP>|iTerm2 3.5.0\x1b\\")),
        100,
        |screen, master_write, child| {
            let stage1 = screen.wait_for("iterm2 stage 1", Duration::from_secs(5), |s, _| {
                s.contains("STAGE-1 top=")
            })?;
            let top1 = parse_top_marker(&stage1, "STAGE-1 top=")?;
            wait_for_cursor(screen, (top1 + 1, 2), Duration::from_secs(2))?;

            let before_commit = screen.raw_len();
            write_master(master_write, b"hello\r")?;

            let stage2 = screen.wait_for("iterm2 stage 2", Duration::from_secs(5), |s, _| {
                s.contains("STAGE-2 top=") && s.contains("typed=hello")
            })?;
            let top2 = parse_top_marker(&stage2, "STAGE-2 top=")?;
            assert!(
                top2 > top1,
                "expected scrollback push: top1={top1}, top2={top2}"
            );
            wait_for_cursor(screen, (top2 + 1, 7), Duration::from_secs(2))?;

            let commit_bytes = screen.raw_slice_from(before_commit);
            assert!(contains_bytes(&commit_bytes, b"\x1b]1337;File=inline=1;"));
            assert!(!contains_bytes(&commit_bytes, b"\x1b_Ga=T"));
            assert!(
                screen
                    .full_contents()
                    .contains("[image: iterm-photo.png · 640x480 · ")
            );

            let before_redraw = screen.raw_len();
            write_master(master_write, b"redraw\r")?;
            let stage3 = screen.wait_for("iterm2 stage 3", Duration::from_secs(5), |s, _| {
                s.contains("STAGE-3-2 top=")
            })?;
            let top3 = parse_top_marker(&stage3, "STAGE-3-2 top=")?;
            assert_eq!(top3, top2);
            wait_for_cursor(screen, (top3 + 2, 9), Duration::from_secs(2))?;

            let redraw_bytes = screen.raw_slice_from(before_redraw);
            assert!(!contains_bytes(&redraw_bytes, b"\x1b]1337;"));

            write_master(master_write, b"\r")?;
            anyhow::ensure!(child.wait_with_timeout(Duration::from_secs(5))?.success());
            Ok(())
        },
    )
}

#[test]
fn pty_e2e_file_backed_app_commits_user_and_tool_images_to_scrollback() -> Result<()> {
    if std::env::var(CHILD_MODE_ENV).ok().as_deref() == Some("file-backed") {
        let _ = std::env::var(CHILD_PNG_ENV);
        return run_file_backed_graphics_child();
    }

    let dir = tempfile::tempdir().context("tempdir")?;
    let user_png_path = dir.path().join("user-upload.png");
    std::fs::write(&user_png_path, sample_png(320, 240, 3400)).context("write user png")?;

    with_pty_harness(
        "pty_e2e_file_backed_app_commits_user_and_tool_images_to_scrollback",
        "file-backed",
        &[(CHILD_PNG_ENV, user_png_path.as_os_str())],
        Some((KITTY_PROBE_QUERY, b"\x1b_Gi=31;OK\x1b\\")),
        200,
        |screen, master_write, child| {
            let startup =
                screen.wait_for("file-backed startup", Duration::from_secs(6), |s, _| {
                    s.contains("gpt-5.4") && s.contains(": ")
                })?;
            assert!(
                !startup.contains("i=31") && !startup.contains(";OK"),
                "probe reply leaked into composer:\n{startup}"
            );

            let after_startup = screen.raw_len();
            let paste_seq = format!("\x1b[200~{}\x1b[201~", user_png_path.display());
            write_master(master_write, paste_seq.as_bytes())?;
            screen.wait_for("attachment preview", Duration::from_secs(4), |s, _| {
                s.contains("+ 1 image:") && s.contains("user-upload.png")
            })?;

            write_master(master_write, b"inspect both images\r")?;
            screen.wait_for(
                "scrollback overflow after user + tool images",
                Duration::from_secs(8),
                |s, _| s.contains("FINAL_SCROLLBACK_MARKER"),
            )?;

            let commit_bytes = screen.raw_slice_from(after_startup);
            let kitty_transmissions = count_subslice(&commit_bytes, b"\x1b_Ga=T,f=100,");
            assert!(
                kitty_transmissions >= 2,
                "expected >= 2 Kitty APC transmissions (user + tool), got {kitty_transmissions}"
            );
            assert!(contains_bytes(&commit_bytes, b"\x1b_Gm=0;"));

            let full = screen.full_contents();
            assert!(
                full.contains("[image: user-upload.png · 320x240 · "),
                "expected user-attached image badge in scrollback:\n{full}"
            );
            assert!(
                full.contains("[image: tool-diagram.png · 400x300 · "),
                "expected tool-read image badge in scrollback:\n{full}"
            );

            let after_commit = screen.raw_len();
            write_master(master_write, b"followup")?;
            screen.wait_for("composer live redraw", Duration::from_secs(4), |s, _| {
                s.contains(": followup")
            })?;
            assert!(!contains_bytes(
                &screen.raw_slice_from(after_commit),
                b"\x1b_G"
            ));

            write_master(master_write, b"\x11")?;
            anyhow::ensure!(child.wait_with_timeout(Duration::from_secs(5))?.success());
            Ok(())
        },
    )
}
