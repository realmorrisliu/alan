use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn write_mock_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn alan_cmd(home: &Path, data_home: &Path, tmp: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_alan"));
    cmd.env("HOME", home)
        .env("XDG_DATA_HOME", data_home)
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("TMPDIR", tmp)
        .env_remove("ALAN_INSTALL_CHANNEL")
        .env_remove("ALAN_CONFIG_PATH")
        .env_remove("ALAN_INSTANCE_RUNTIME_DIR");
    cmd
}

fn init_fake_git_repo(dir: &Path, remote_url: &str) {
    let git_dir = dir.join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(
        git_dir.join("config"),
        format!("[remote \"origin\"]\n\turl = {remote_url}\n"),
    )
    .unwrap();
}

#[test]
fn cli_project_bind_status_workflow_inbox_and_ack_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let data = tmp.path().join("data");
    let primary = tmp.path().join("primary/alan");
    let foreign = tmp.path().join("foreign/alan");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&primary).unwrap();
    std::fs::create_dir_all(&foreign).unwrap();
    init_fake_git_repo(&primary, "git@github.com:example/alan.git");
    init_fake_git_repo(&foreign, "git@github.com:example/alan.git");

    // 1. Explicit bind in primary checkout.
    let bind = alan_cmd(&home, &data, tmp.path())
        .current_dir(&primary)
        .args(["project", "bind", "--json"])
        .output()
        .unwrap();
    assert!(
        bind.status.success(),
        "{}",
        String::from_utf8_lossy(&bind.stderr)
    );
    let bind_json: serde_json::Value = serde_json::from_slice(&bind.stdout).unwrap();
    let project_key = bind_json["project_key"].as_str().unwrap().to_string();

    // 2. Foreign checkout with matching Git remote & basename stays unbound.
    let foreign_status = alan_cmd(&home, &data, tmp.path())
        .current_dir(&foreign)
        .args(["project", "status"])
        .output()
        .unwrap();
    assert!(!foreign_status.status.success());

    // 3. Register project-bound workflow and run against mock binary with secrets.
    let init_wf = alan_cmd(&home, &data, tmp.path())
        .args([
            "workflow",
            "init",
            "daily-project-check",
            "--schedule",
            "every 1h",
            "--prompt",
            "Check project status",
            "--project-key",
            &project_key,
        ])
        .output()
        .unwrap();
    assert!(
        init_wf.status.success(),
        "{}",
        String::from_utf8_lossy(&init_wf.stderr)
    );

    let mock_bin = write_mock_script(
        tmp.path(),
        "mock-alan.sh",
        r#"#!/bin/sh
printf '{"type":"turn_finished","status":"completed","answer":"Checks ok; scrubbed sk-proj-topsecret99887766"}\n'
"#,
    );
    let run_wf = alan_cmd(&home, &data, tmp.path())
        .env("ALAN_WORKFLOW_ALAN_BIN", &mock_bin)
        .args(["workflow", "run", "daily-project-check", "--json"])
        .output()
        .unwrap();
    assert!(
        run_wf.status.success(),
        "{}",
        String::from_utf8_lossy(&run_wf.stderr)
    );
    let run_json: serde_json::Value = serde_json::from_slice(&run_wf.stdout).unwrap();
    let run_id = run_json["entry"]["run_id"].as_str().unwrap().to_string();

    // 4. `alan project status` reports unread inbox count = 1.
    let status = alan_cmd(&home, &data, tmp.path())
        .current_dir(&primary)
        .args(["project", "status", "--json"])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status_json: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status_json["project_key"], project_key);
    assert_eq!(status_json["unread_inbox_count"], 1);

    // 5. `alan project inbox` lists redacted digest and `--ack` sets digested_at.
    let inbox = alan_cmd(&home, &data, tmp.path())
        .current_dir(&primary)
        .args(["project", "inbox", "--json"])
        .output()
        .unwrap();
    assert!(inbox.status.success());
    let inbox_json: serde_json::Value = serde_json::from_slice(&inbox.stdout).unwrap();
    assert_eq!(inbox_json["unread_count"], 1);
    let summary = inbox_json["entries"][0]["summary"].as_str().unwrap();
    assert!(!summary.contains("sk-proj-topsecret99887766"));
    assert!(summary.contains("[REDACTED]"));

    let ack = alan_cmd(&home, &data, tmp.path())
        .current_dir(&primary)
        .args(["project", "inbox", "--ack", &run_id, "--json"])
        .output()
        .unwrap();
    assert!(
        ack.status.success(),
        "{}",
        String::from_utf8_lossy(&ack.stderr)
    );
    let ack_json: serde_json::Value = serde_json::from_slice(&ack.stdout).unwrap();
    assert_eq!(ack_json["unread_count"], 0);
    assert!(ack_json["entries"][0]["digested_at"].is_string());

    // 6. Status now reports 0 unread inbox items.
    let status_after = alan_cmd(&home, &data, tmp.path())
        .current_dir(&primary)
        .args(["project", "status", "--json"])
        .output()
        .unwrap();
    assert!(status_after.status.success());
    let status_after_json: serde_json::Value =
        serde_json::from_slice(&status_after.stdout).unwrap();
    assert_eq!(status_after_json["unread_inbox_count"], 0);
}
