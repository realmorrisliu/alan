use super::*;

#[test]
fn absent_inspection_does_not_create_stores() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::from_data_dir(temp.path()).unwrap();
    let report = paths.inspect().unwrap();
    assert!(!report.canonical.system_present && !report.canonical.host_present);
    assert!(
        report
            .sources
            .iter()
            .all(|source| !source.stores.system_present && !source.stores.host_present)
    );
    assert!(!paths.product.exists());
}

#[test]
fn inspection_reports_both_inputs_without_selecting_or_reading_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::from_data_dir(temp.path()).unwrap();
    for source in [LegacyInstallation::Stable, LegacyInstallation::Dev] {
        fs::create_dir_all(paths.system_root().join(source.id()).join("services")).unwrap();
        fs::create_dir_all(paths.host_root().join(source.id()).join("credentials")).unwrap();
        fs::write(
            paths.host_root().join(source.id()).join("auth.json"),
            "secret-do-not-read",
        )
        .unwrap();
        let report = paths.inspect().unwrap();
        assert!(!report.canonical.system_present && !report.canonical.host_present);
        let selected = report
            .sources
            .iter()
            .find(|entry| entry.source == source)
            .unwrap();
        assert!(selected.stores.system_present && selected.stores.host_present);
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("secret-do-not-read")
        );
    }
    fs::create_dir_all(paths.system_root().join("services")).unwrap();
    fs::create_dir_all(paths.host_root().join("credentials")).unwrap();
    let report = paths.inspect().unwrap();
    assert!(report.canonical.system_present && report.canonical.host_present);
    assert!(
        report
            .sources
            .iter()
            .all(|source| source.stores.system_present && source.stores.host_present)
    );
}

#[test]
fn incomplete_pair_and_malformed_layout_remain_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::from_data_dir(temp.path()).unwrap();
    fs::create_dir_all(paths.system_root().join("dev")).unwrap();
    let report = paths.inspect().unwrap();
    assert!(report.sources[1].stores.system_present);
    assert!(!report.sources[1].stores.host_present);
    fs::write(paths.system_root().join("stable"), "not a directory").unwrap();
    assert!(paths.inspect().is_err());
}

#[cfg(unix)]
#[test]
fn inspection_does_not_follow_symlinked_store_roots() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::from_data_dir(temp.path()).unwrap();
    fs::create_dir_all(&paths.product).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), paths.system_root()).unwrap();
    assert!(paths.inspect().is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
