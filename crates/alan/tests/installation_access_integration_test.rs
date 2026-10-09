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
        vec!["host", "legacy-state", "cleanup", "--from", "stable"],
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

#[test]
fn obsolete_selectors_fail_before_creating_product_data_but_help_stays_processless() {
    use std::os::unix::process::CommandExt;
    let fixture = Fixture::new();
    for selector in ["stable", "dev", "unknown", ""] {
        let output = fixture
            .command()
            .env("ALAN_INSTALL_CHANNEL", selector)
            .args(["connection", "list"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("ALAN_INSTALL_CHANNEL is retired")
        );
    }
    let output = fixture
        .command()
        .arg0("alan-dev")
        .args(["connection", "list"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("executable name is retired"));
    for flag in ["--help", "--version"] {
        let output = fixture
            .command()
            .arg0("alan-dev")
            .env("ALAN_INSTALL_CHANNEL", "dev")
            .arg(flag)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    assert!(!fixture.paths.product.exists());
}

#[test]
fn startup_never_selects_a_single_or_dual_legacy_source() {
    let fixture = Fixture::new();
    for source in ["stable", "dev"] {
        let old = fixture.paths.system_root().join(source);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("retained"), source).unwrap();
        let output = fixture
            .command()
            .args(["connection", "list"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("explicitly select a source"),
            "{output:?}"
        );
        assert!(!fixture.paths.system_root().join("services").exists());
        assert!(!fixture.paths.host_root().exists());
        assert_eq!(
            std::fs::read_to_string(old.join("retained")).unwrap(),
            source
        );
    }
}
