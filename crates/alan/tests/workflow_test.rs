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

#[test]
fn cli_workflow_init_list_render_run_and_logs_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&data).unwrap();

    // 1. `alan workflow --help` lists all subcommands cleanly
    let help = alan_cmd(&home, &data, tmp.path())
        .args(["workflow", "--help"])
        .output()
        .unwrap();
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    let help_out = String::from_utf8_lossy(&help.stdout);
    for sub in ["init", "list", "run", "render-schedule", "logs"] {
        assert!(
            help_out.contains(sub),
            "missing `{sub}` in help:\n{help_out}"
        );
    }

    // 2. Reject raw host path mounts on `init` (no implicit host mounts allowed)
    let bad_init = alan_cmd(&home, &data, tmp.path())
        .args([
            "workflow",
            "init",
            "bad-mounts",
            "--schedule",
            "every 1h",
            "--prompt",
            "check",
            "--mount",
            "/Users/alice/Documents",
        ])
        .output()
        .unwrap();
    assert!(!bad_init.status.success());

    // 3. Valid `init` persists declarative TOML under System Store `services/workflows`
    let init = alan_cmd(&home, &data, tmp.path())
        .args([
            "workflow",
            "init",
            "morning-brief",
            "--schedule",
            "0 9 * * 1-5",
            "--prompt",
            "Summarize unread notes",
            "--mount",
            "notes",
        ])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    // Inspect System Store path directly to ensure default trust = "governed" & no extra grants
    #[cfg(target_os = "macos")]
    let manifest_path = home.join(
        "Library/Application Support/Alan/System Store/services/workflows/manifests/morning-brief.toml",
    );
    #[cfg(not(target_os = "macos"))]
    let manifest_path =
        data.join("Alan/System Store/services/workflows/manifests/morning-brief.toml");
    assert!(
        manifest_path.is_file(),
        "missing manifest at {}",
        manifest_path.display()
    );
    let manifest_toml = std::fs::read_to_string(&manifest_path).unwrap();
    assert!(
        manifest_toml.contains("trust = \"governed\""),
        "{manifest_toml}"
    );
    assert!(
        manifest_toml.contains("mounts = [\"notes\"]"),
        "{manifest_toml}"
    );

    // 4. `list --json` shows the registered workflow
    let list = alan_cmd(&home, &data, tmp.path())
        .args(["workflow", "list", "--json"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list_json["workflows"][0]["id"], "morning-brief");

    // 5. `render-schedule --target launchd` outputs plist without writing LaunchAgents unless --install
    let plist_dry = alan_cmd(&home, &data, tmp.path())
        .args([
            "workflow",
            "render-schedule",
            "morning-brief",
            "--target",
            "launchd",
        ])
        .output()
        .unwrap();
    assert!(plist_dry.status.success());
    let plist_text = String::from_utf8_lossy(&plist_dry.stdout);
    assert!(plist_text.contains("app.alanworks.workflow.morning-brief"));
    assert!(plist_text.contains("<key>StartCalendarInterval</key>"));
    assert!(
        !home
            .join("Library/LaunchAgents/app.alanworks.workflow.morning-brief.plist")
            .exists()
    );

    // `--install` writes user LaunchAgent into isolated HOME/Library/LaunchAgents
    let plist_install = alan_cmd(&home, &data, tmp.path())
        .args([
            "workflow",
            "render-schedule",
            "morning-brief",
            "--target",
            "launchd",
            "--install",
        ])
        .output()
        .unwrap();
    assert!(plist_install.status.success());
    assert!(
        home.join("Library/LaunchAgents/app.alanworks.workflow.morning-brief.plist")
            .is_file()
    );

    // `render-schedule --target cron` outputs valid crontab line
    let cron = alan_cmd(&home, &data, tmp.path())
        .args([
            "workflow",
            "render-schedule",
            "morning-brief",
            "--target",
            "cron",
        ])
        .output()
        .unwrap();
    assert!(cron.status.success());
    let cron_text = String::from_utf8_lossy(&cron.stdout);
    assert!(cron_text.contains("0 9 * * 1-5 "));
    assert!(cron_text.contains("workflow run morning-brief"));

    // 6. Hermetic `workflow run` against mock `alan` script records scrubbed NDJSON log & leaves root-rollout untouched
    let mock_bin = write_mock_script(
        tmp.path(),
        "mock-alan.sh",
        r#"#!/bin/sh
printf '{"type":"text_delta","text":"Drafting..."}\n'
printf '{"type":"turn_finished","status":"completed","answer":"Brief ready; token sk-proj-topsecret99887766 scrubbed"}\n'
"#,
    );
    let run = alan_cmd(&home, &data, tmp.path())
        .env("ALAN_WORKFLOW_ALAN_BIN", &mock_bin)
        .args(["workflow", "run", "morning-brief", "--json"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(run_json["entry"]["exit_code"], 0);
    assert_eq!(run_json["entry"]["ephemeral"], true);
    let summary = run_json["entry"]["summary"].as_str().unwrap();
    assert!(!summary.contains("sk-proj-topsecret99887766"));
    assert!(summary.contains("[REDACTED]"));

    // Ensure interactive root-rollout pointer was never created or mutated
    #[cfg(target_os = "macos")]
    let root_rollout = home.join(
        "Library/Application Support/Alan/System Store/services/agent-runtime/metadata/root-rollout",
    );
    #[cfg(not(target_os = "macos"))]
    let root_rollout = data.join("Alan/System Store/services/agent-runtime/metadata/root-rollout");
    assert!(
        !root_rollout.exists(),
        "scheduled workflow run must never mutate interactive root-rollout"
    );

    // 7. `logs` inspects bounded NDJSON execution history
    let logs = alan_cmd(&home, &data, tmp.path())
        .args([
            "workflow",
            "logs",
            "morning-brief",
            "--limit",
            "5",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(logs.status.success());
    let logs_json: serde_json::Value = serde_json::from_slice(&logs.stdout).unwrap();
    assert_eq!(logs_json["workflow_id"], "morning-brief");
    assert_eq!(logs_json["entries"].as_array().unwrap().len(), 1);
    assert!(
        !logs_json["entries"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("sk-proj-topsecret99887766")
    );
}
