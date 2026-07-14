use std::{
    path::{Path, PathBuf},
    process::Command,
};

use alan_agent_engine::{ConnectionsFile, InstallChannel};
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
fn host_import_installs_verified_skill_before_deleting_source() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("data");
    let source = temp.path().join("review-skill");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("SKILL.md"),
        "---\nname: reviewer\ndescription: Review code\n---\n",
    )
    .unwrap();

    let output = alan_command(&home, &xdg_data)
        .arg("host")
        .arg("legacy-state")
        .arg("import")
        .arg("skill")
        .arg(&source)
        .args(["--name", "reviewer", "--delete-source"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let data = detected_data_dir(&home, &xdg_data);
    let system = alan_os_host::SystemStorePaths::from_data_dir(
        &data,
        InstallChannel::Stable.descriptor().id,
    )
    .unwrap();
    let packages =
        alan_service_manager::PackageService::open(&system.channel_id, system.packages().unwrap())
            .unwrap();
    assert!(packages.resolve("reviewer").is_ok());
    assert!(!source.exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("source deleted after verification"));
}

#[cfg(unix)]
#[test]
fn host_import_never_follows_a_symlinked_source_for_deletion() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("data");
    let source = temp.path().join("real-skill");
    let source_link = temp.path().join("skill-link");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("SKILL.md"), "real authored content").unwrap();
    symlink(&source, &source_link).unwrap();

    let output = alan_command(&home, &xdg_data)
        .arg("host")
        .arg("legacy-state")
        .arg("import")
        .arg("skill")
        .arg(&source_link)
        .args(["--name", "linked-skill", "--delete-source"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must be a real directory"),
        "{output:?}"
    );
    assert!(source.join("SKILL.md").is_file());
    assert!(
        std::fs::symlink_metadata(&source_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let data = detected_data_dir(&home, &xdg_data);
    let system = alan_os_host::SystemStorePaths::from_data_dir(
        &data,
        InstallChannel::Stable.descriptor().id,
    )
    .unwrap();
    let packages =
        alan_service_manager::PackageService::open(&system.channel_id, system.packages().unwrap())
            .unwrap();
    assert!(!packages.catalog().packages.contains_key("linked-skill"));
}

#[cfg(unix)]
#[test]
fn host_import_never_follows_a_symlinked_source_ancestor_for_deletion() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("data");
    let real_parent = temp.path().join("real-parent");
    let source = real_parent.join("skill");
    let linked_parent = temp.path().join("linked-parent");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("SKILL.md"), "real authored content").unwrap();
    symlink(&real_parent, &linked_parent).unwrap();

    let output = alan_command(&home, &xdg_data)
        .arg("host")
        .arg("legacy-state")
        .arg("import")
        .arg("skill")
        .arg(linked_parent.join("skill"))
        .args(["--name", "ancestor-linked-skill", "--delete-source"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("symlinked path component"),
        "{output:?}"
    );
    assert!(source.join("SKILL.md").is_file());
    assert!(
        std::fs::symlink_metadata(&linked_parent)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let data = detected_data_dir(&home, &xdg_data);
    let system = alan_os_host::SystemStorePaths::from_data_dir(
        &data,
        InstallChannel::Stable.descriptor().id,
    )
    .unwrap();
    let packages =
        alan_service_manager::PackageService::open(&system.channel_id, system.packages().unwrap())
            .unwrap();
    assert!(
        !packages
            .catalog()
            .packages
            .contains_key("ancestor-linked-skill")
    );
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
