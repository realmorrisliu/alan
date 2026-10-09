use super::*;

#[test]
fn offline_inspection_checks_revisions_without_recovering_or_writing_the_store() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("packages");
    let service = PackageService::open("test", root.clone()).unwrap();
    service
        .seed_preinstalled("alpha", native_snapshot("alpha", "Body"))
        .unwrap();
    let catalog_bytes = fs::read(root.join("catalog.json")).unwrap();
    let catalog = service.cached_catalog();
    drop(service);
    assert_eq!(
        validate_package_store_for_migration(&root).unwrap(),
        catalog
    );
    assert_eq!(fs::read(root.join("catalog.json")).unwrap(), catalog_bytes);
    let revision = &catalog.packages["alpha"].revision;
    fs::write(
        root.join("revisions/alpha")
            .join(revision)
            .join("source/SKILL.md"),
        "corrupt",
    )
    .unwrap();
    assert!(validate_package_store_for_migration(&root).is_err());
    assert_eq!(fs::read(root.join("catalog.json")).unwrap(), catalog_bytes);
}

#[test]
fn offline_inspection_preserves_pending_staging_and_unknown_schema() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("packages");
    let service = PackageService::open("test", root.clone()).unwrap();
    drop(service);
    fs::write(root.join("staging/source.patch"), "authored").unwrap();
    assert!(validate_package_store_for_migration(&root).is_err());
    assert_eq!(
        fs::read_to_string(root.join("staging/source.patch")).unwrap(),
        "authored"
    );
    fs::remove_file(root.join("staging/source.patch")).unwrap();
    fs::write(
        root.join("catalog.json"),
        r#"{"generation":0,"packages":{},"future_schema":true}"#,
    )
    .unwrap();
    assert!(validate_package_store_for_migration(&root).is_err());
}

#[test]
fn offline_inspection_rejects_an_active_store_writer() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("packages");
    let service = PackageService::open("test", root.clone()).unwrap();
    let transaction = service.store.transaction().unwrap();
    assert!(validate_package_store_for_migration(&root).is_err());
    drop(transaction);
    assert!(validate_package_store_for_migration(&root).is_ok());
}

#[test]
fn offline_inspection_rejects_live_references_without_removing_leases() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("packages");
    let service = PackageService::open("test", root.clone()).unwrap();
    service
        .seed_preinstalled("alpha", native_snapshot("alpha", "Body"))
        .unwrap();
    let lease = service.acquire("alpha").unwrap();
    assert!(validate_package_store_for_migration(&root).is_err());
    assert_eq!(fs::read_dir(root.join("leases")).unwrap().count(), 1);
    drop(lease);
    assert!(validate_package_store_for_migration(&root).is_ok());
}

#[cfg(unix)]
#[test]
fn offline_inspection_rejects_revision_parent_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("packages");
    let service = PackageService::open("test", root.clone()).unwrap();
    service
        .seed_preinstalled("alpha", native_snapshot("alpha", "Body"))
        .unwrap();
    drop(service);
    let outside = temp.path().join("retained-source");
    fs::rename(root.join("revisions/alpha"), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("revisions/alpha")).unwrap();
    assert!(validate_package_store_for_migration(&root).is_err());
    assert!(outside.is_dir());
}
