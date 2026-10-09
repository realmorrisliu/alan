use super::*;

fn fixture() -> (tempfile::TempDir, InstallationPaths) {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::from_data_dir(temp.path()).unwrap();
    (temp, paths)
}

fn journal(state: MigrationState) -> MigrationJournal {
    MigrationJournal {
        version: 1,
        id: uuid::Uuid::new_v4(),
        source: LegacyInstallation::Dev,
        state,
        source_fingerprint: "a".repeat(64),
        components: vec![MigrationComponent {
            payload: MigrationPayload::Services,
            fingerprint: "b".repeat(64),
        }],
    }
}

#[test]
fn active_readers_exclude_migration_and_migration_excludes_readers() {
    let (_temp, paths) = fixture();
    let first = paths.access().unwrap();
    let second = paths.access().unwrap();
    assert!(paths.migration_access().is_err());
    drop(first);
    assert!(paths.migration_access().is_err());
    drop(second);
    let migration = paths.migration_access().unwrap();
    assert!(paths.access().is_err());
    assert!(paths.migration_access().is_err());
    drop(migration);
    paths.access().unwrap();
}

#[test]
fn incomplete_journal_blocks_after_writer_exit_until_explicit_resolution() {
    let (_temp, paths) = fixture();
    for state in [
        MigrationState::Preparing,
        MigrationState::Publishing,
        MigrationState::RollingBack,
    ] {
        let migration = paths.migration_access().unwrap();
        let receipt = journal(state);
        migration.write_journal(&receipt).unwrap();
        assert_eq!(paths.read_migration_journal().unwrap(), Some(receipt));
        drop(migration);
        assert!(paths.access().is_err());
        let migration = paths.migration_access().unwrap();
        migration.remove_journal().unwrap();
    }
    let migration = paths.migration_access().unwrap();
    migration
        .write_journal(&journal(MigrationState::Committed))
        .unwrap();
    drop(migration);
    paths.access().unwrap();
    assert!(!paths.system_root().exists());
    assert!(!paths.host_root().exists());
}

#[test]
fn malformed_or_unknown_journal_fails_closed() {
    let (_temp, paths) = fixture();
    drop(paths.access().unwrap());
    for bytes in [b"{broken".as_slice(), b"{}", &vec![b' '; 16 * 1024 + 1]] {
        fs::write(paths.journal(), bytes).unwrap();
        assert!(paths.access().is_err());
        assert!(paths.migration_access().is_err());
    }
    let mut receipt = journal(MigrationState::Publishing);
    receipt.version = 2;
    fs::write(paths.journal(), serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(paths.access().is_err());
    receipt.version = 1;
    receipt.components.push(receipt.components[0].clone());
    fs::write(paths.journal(), serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(paths.access().is_err());
}

#[cfg(unix)]
#[test]
fn symlink_lock_or_product_ancestor_never_opens_other_storage() {
    let (temp, paths) = fixture();
    fs::create_dir_all(&paths.product).unwrap();
    let unrelated = temp.path().join("unrelated");
    fs::write(&unrelated, "keep").unwrap();
    std::os::unix::fs::symlink(&unrelated, paths.product.join("installation.lock")).unwrap();
    assert!(paths.access().is_err());
    assert_eq!(fs::read_to_string(&unrelated).unwrap(), "keep");
    let link = temp.path().join("link");
    fs::create_dir(&link).unwrap();
    let paths = InstallationPaths::from_data_dir(&link).unwrap();
    std::os::unix::fs::symlink(temp.path(), &paths.product).unwrap();
    assert!(paths.migration_access().is_err());
}
