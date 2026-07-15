use std::{
    path::{Path, PathBuf},
    process::Command,
};

use alan_agent_engine::InstallChannel;
use alan_service_manager::ConnectionsFile;
use tempfile::TempDir;

fn detected_data_dir(home: &Path, xdg_data: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        xdg_data.to_path_buf()
    }
}

fn alan_command(home: &Path, xdg_data: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_alan"));
    command
        .env("HOME", home)
        .env("XDG_DATA_HOME", xdg_data)
        .env("ALAN_INSTALL_CHANNEL", "stable");
    command
}

#[test]
fn legacy_cleanup_migrates_metadata_and_host_secrets_once() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("data");
    let legacy = home.join(".alan");
    std::fs::create_dir_all(legacy.join("credentials")).unwrap();
    ConnectionsFile::default()
        .save_to_path(&legacy.join("connections.toml"))
        .unwrap();
    std::fs::write(
        legacy.join("credentials/secrets.toml"),
        "[secrets]\nmain = 'secret'\n",
    )
    .unwrap();
    std::fs::write(legacy.join("auth.json"), "{\"version\":1}").unwrap();

    let output = alan_command(&home, &xdg_data)
        .args(["host", "legacy-state", "cleanup", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let data = detected_data_dir(&home, &xdg_data);
    let system = alan_os_host::SystemStorePaths::from_data_dir(
        &data,
        InstallChannel::Stable.descriptor().id,
    )
    .unwrap();
    let host_store =
        alan_os_host::HostStorePaths::from_data_dir(&data, InstallChannel::Stable.descriptor().id)
            .unwrap();
    let connection_metadata = system.connections_metadata().unwrap();
    assert!(connection_metadata.is_file());
    assert!(
        !std::fs::read_to_string(&connection_metadata)
            .unwrap()
            .contains("secret")
    );
    assert!(host_store.credentials.join("secrets.toml").is_file());
    assert!(host_store.managed_auth.is_file());
    assert!(!legacy.join("connections.toml").exists());
    assert!(!legacy.join("credentials/secrets.toml").exists());
    assert!(!legacy.join("auth.json").exists());
}

#[test]
fn host_cleanup_deletes_generated_state_and_reports_authored_roots() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("data");
    let legacy = home.join(".alan");
    std::fs::create_dir_all(legacy.join("runtime/stable/rollouts")).unwrap();
    std::fs::write(
        legacy.join("runtime/stable/rollouts/one.jsonl"),
        "generated",
    )
    .unwrap();
    std::fs::create_dir_all(legacy.join("agents/default/persona")).unwrap();
    std::fs::write(legacy.join("agents/default/persona/SOUL.md"), "authored").unwrap();

    let output = alan_command(&home, &xdg_data)
        .args(["host", "legacy-state", "cleanup", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["removed_generated_paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path.as_str().unwrap().ends_with("runtime/stable/rollouts"))
    );
    assert!(
        report["authored_roots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|root| root["kind"] == "persona")
    );
    assert!(!legacy.join("runtime/stable/rollouts").exists());
    assert!(legacy.join("agents/default/persona/SOUL.md").is_file());
}

#[test]
fn removed_commands_and_boot_agent_selector_are_not_parseable() {
    for args in [
        vec!["init", "--help"],
        vec!["workspace", "--help"],
        vec!["--agent", "coding"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_alan"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Host Mount"), "{stderr}");
        assert!(stderr.contains("Alan Shell"), "{stderr}");
    }
}

#[test]
fn legacy_skill_import_is_not_parseable() {
    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args([
            "host",
            "legacy-state",
            "import",
            "skill",
            "/tmp/legacy-skill",
            "--name",
            "legacy-skill",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid value 'skill'"),
        "{output:?}"
    );
}
