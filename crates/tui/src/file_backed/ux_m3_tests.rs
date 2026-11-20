//! M3 TUI tests for trust posture badges and concurrent subagent progress projection.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;

use super::action_detail_io::read_detail;
use super::file_surface::ActionSnapshot;
use super::layout::{draw, live_region_lines_at};
use super::welcome::{from_app, render_banner_lines};
use super::*;
use crate::theme::Theme;

#[test]
fn trust_posture_badges_render_distinctly_in_context_line_and_welcome_banner() {
    let mut confined = FileBackedApp::new("/agent/root".to_string());
    confined.trust_posture_label = Some("confined/trusted".to_string());
    confined.set_effective_model(Some("gpt-5.4".to_string()));
    let confined_line = confined.context_line(80);
    let confined_text: String = confined_line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        confined_text.contains("confined/trusted"),
        "{confined_text}"
    );
    let confined_banner = render_banner_lines(&from_app(&confined), 80, Theme::midnight())
        .into_iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        confined_banner.contains("confined/trusted"),
        "{confined_banner}"
    );

    let mut unsafe_app = FileBackedApp::new("/agent/root".to_string());
    unsafe_app.trust_posture_label = Some("UNSANDBOXED host-account".to_string());
    unsafe_app.set_effective_model(Some("gpt-5.4".to_string()));
    // Even on narrower context lines, UNSANDBOXED posture stays visible and bold.
    let unsafe_line = unsafe_app.context_line(52);
    let unsafe_text: String = unsafe_line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        unsafe_text.contains("UNSANDBOXED host-account"),
        "{unsafe_text}"
    );
    let badge_span = unsafe_line
        .spans
        .iter()
        .find(|s| s.content.contains("UNSANDBOXED"))
        .expect("unsandboxed span present");
    assert!(badge_span.style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(badge_span.style.fg, Some(Theme::midnight().status_block));

    // Dumb mode keeps explicit textual warning marker without ANSI modifiers.
    unsafe_app.theme = Theme::dumb();
    let dumb_text: String = unsafe_app
        .context_line(80)
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        dumb_text.contains("! UNSANDBOXED host-account"),
        "{dumb_text}"
    );
}

#[test]
fn concurrent_running_subagents_surface_in_live_region_and_actions_modal() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    super::file_surface::sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "subagent:researcher".to_string(),
            status: "running".to_string(),
            output: String::new(),
            result: serde_json::json!({
                "title": "researcher · pid 41 · running",
                "subagent_role": "researcher",
                "child_pid": 41,
                "child_state": "running",
            })
            .to_string(),
        },
    );
    super::file_surface::sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a1".to_string(),
            name: "subagent:reviewer".to_string(),
            status: "running".to_string(),
            output: String::new(),
            result: serde_json::json!({
                "title": "reviewer · pid 42 · running",
                "subagent_role": "reviewer",
                "child_pid": 42,
                "child_state": "running",
            })
            .to_string(),
        },
    );

    let (live, _) = live_region_lines_at(&app, 100, 0);
    let live_text = live
        .into_iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        live_text.contains("researcher · pid 41 · running"),
        "{live_text}"
    );
    assert!(
        !live_text.contains("running · running"),
        "must not duplicate running suffix: {live_text}"
    );
    assert!(
        live_text.contains("reviewer · pid 42 · running"),
        "{live_text}"
    );

    // Open Ctrl+O Actions modal and confirm active concurrent subagents overview renders cleanly.
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.modal.active);
    let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer_text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(
        buffer_text.contains("researcher · pid 41 · running"),
        "{buffer_text}"
    );
    assert!(
        buffer_text.contains("reviewer · pid 42 · running"),
        "{buffer_text}"
    );
}

#[tokio::test]
async fn ctrl_o_action_detail_formats_subagent_role_pid_state_header() {
    use alan_ap::{Fid, FileServer, OpenMode};

    let (shell, root, _, pid) = crate::file_backed::stdio_tests::live_root_agent().await;
    let fid = Fid(950);
    root.walk(
        Fid::ROOT,
        fid,
        &[pid.clone(), "actions".into(), "clone".into()],
    )
    .await
    .unwrap();
    root.open(fid, OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(root.read(fid, 0, 64).await.unwrap()).unwrap();
    root.clunk(fid).await.unwrap();
    let owner = format!("/agent/{pid}");

    shell
        .write(&format!("{owner}/actions/{id}/name"), b"subagent:verifier")
        .await
        .unwrap();
    shell
        .write(&format!("{owner}/actions/{id}/status"), b"running")
        .await
        .unwrap();
    shell
        .write(
            &format!("{owner}/actions/{id}/result"),
            serde_json::json!({
                "subagent_role": "verifier",
                "child_pid": 88,
                "child_state": "running"
            })
            .to_string()
            .as_bytes(),
        )
        .await
        .unwrap();

    let rows = read_detail(&shell, &owner, &id)
        .await
        .into_iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rows.contains("Subagent: verifier · pid 88 · running"),
        "{rows}"
    );
}
