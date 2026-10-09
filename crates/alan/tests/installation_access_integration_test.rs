use std::{path::PathBuf, process::Command};

use alan_os_host::installation::{
    InstallationPaths, LegacyInstallation, MigrationJournal, MigrationState,
};

struct Fixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    data: PathBuf,
    paths: InstallationPaths,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let data = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            temp.path().join("data")
        };
        let paths = InstallationPaths::from_data_dir(&data).unwrap();
        Self {
            _temp: temp,
            home,
            data,
            paths,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_alan"));
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("ALAN_") {
                command.env_remove(name);
            }
        }
        command
            .env("HOME", &self.home)
            .env("XDG_DATA_HOME", &self.data);
        command
    }

    fn write_pending_journal(&self) {
        self.paths
            .migration_access()
            .unwrap()
            .write_journal(&MigrationJournal {
                version: 1,
                id: uuid::Uuid::new_v4(),
                source: LegacyInstallation::Stable,
                state: MigrationState::Publishing,
                source_fingerprint: "a".repeat(64),
                components: Vec::new(),
            })
            .unwrap();
    }
}

#[test]
fn pending_publication_blocks_data_commands_before_any_store_is_opened() {
    let fixture = Fixture::new();
    fixture.write_pending_journal();
    for args in [
        vec![],
        vec!["connection", "list"],
        vec!["connection", "current"],
        vec!["connection", "logout", "main"],
        vec!["host", "legacy-state", "cleanup"],
    ] {
        let output = fixture.command().args(&args).output().unwrap();
        assert!(!output.status.success(), "{args:?}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("migration is incomplete"),
            "{args:?}: {output:?}"
        );
        assert!(!fixture.paths.system_root().exists());
        assert!(!fixture.paths.host_root().exists());
    }
}

#[test]
fn help_and_read_only_inspection_remain_available_for_recovery() {
    let fixture = Fixture::new();
    fixture.write_pending_journal();
    for args in [
        vec!["--help"],
        vec!["--version"],
        vec!["host", "legacy-state", "inspect", "--json"],
    ] {
        let output = fixture.command().args(&args).output().unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
    }
    assert!(!fixture.paths.system_root().exists());
    assert!(!fixture.paths.host_root().exists());
}

#[test]
fn exclusive_migration_lock_blocks_a_separate_cli_process() {
    let fixture = Fixture::new();
    let _migration = fixture.paths.migration_access().unwrap();
    let output = fixture
        .command()
        .args(["connection", "list"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("migration is running"),
        "{output:?}"
    );
    assert!(!fixture.paths.system_root().exists());
    assert!(!fixture.paths.host_root().exists());
}

#[test]
fn migration_requires_explicit_source_and_dry_run_does_not_initialize_absent_data() {
    let fixture = Fixture::new();
    let missing_source = fixture
        .command()
        .args(["legacy-state", "migrate-installation", "--dry-run"])
        .output()
        .unwrap();
    assert!(!missing_source.status.success());
    assert!(String::from_utf8_lossy(&missing_source.stderr).contains("--from"));
    let output = fixture
        .command()
        .args([
            "legacy-state",
            "migrate-installation",
            "--from",
            "dev",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("selected legacy installation is absent"),
        "{output:?}"
    );
    assert!(!fixture.paths.product.exists());
    let help = fixture
        .command()
        .args(["legacy-state", "migrate-installation", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--rollback"));
}
