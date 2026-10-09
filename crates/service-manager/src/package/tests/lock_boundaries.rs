use super::lock_contention::contend;
use super::*;
use std::time::Duration;

#[test]
fn public_busy_catalog_and_mutation_preserve_store() {
    use alan_ap::InProcessTransport;
    use alan_shell::Shell;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open(root.clone()).unwrap();
    service
        .seed_preinstalled("kept", native_snapshot("kept", "body"))
        .unwrap();
    let before = service.catalog().unwrap();
    let disk_before = fs::read(root.join("catalog.json")).unwrap();
    let revision = service
        .store
        .revision_root("kept", &before.packages["kept"].revision);
    let worker = service.clone();
    let (bounded, result, elapsed, owned) =
        contend(&root, Duration::from_millis(1600), move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                let shell = Shell::new(InProcessTransport::new(worker.file_server()));
                let catalog = shell.cat("/catalog").await;
                let write = shell
                    .write(
                        "/ctl",
                        &serde_json::to_vec(&PackageCommand::Install {
                            request_id: "busy-install".into(),
                            package_id: "blocked".into(),
                            snapshot: native_snapshot("blocked", "body"),
                        })
                        .unwrap(),
                    )
                    .await;
                let projection = shell.cat("/result").await;
                (catalog, write, projection)
            })
        });
    assert!(bounded && owned);
    assert!(result.0.is_err());
    assert!(
        result.1.is_ok(),
        "valid command projects failure rather than rejecting commit"
    );
    let projection: BTreeMap<String, PackageCommandResult> =
        serde_json::from_slice(&result.2.unwrap()).unwrap();
    let failure = &projection["busy-install"];
    assert!(!failure.success && failure.message.contains("busy"));
    assert!(failure.catalog.is_none() && failure.package.is_none());
    assert!(elapsed >= Duration::from_millis(950) && elapsed < Duration::from_millis(1600));
    assert_eq!(fs::read(root.join("catalog.json")).unwrap(), disk_before);
    assert_eq!(service.catalog().unwrap(), before);
    assert!(revision.is_dir());
    assert!(!root.join("revisions/blocked").exists());
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
}

#[test]
fn busy_lease_drop_raii_then_refresh_reclaims_only_released_revision() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open(root.clone()).unwrap();
    for id in ["released", "live"] {
        service
            .seed_preinstalled(id, native_snapshot(id, "old"))
            .unwrap();
    }
    let released = service.acquire("released").unwrap();
    let live = service.acquire("live").unwrap();
    let released_path = released.content_root().to_path_buf();
    let live_path = live.content_root().to_path_buf();
    for id in ["released", "live"] {
        service
            .seed_preinstalled(id, native_snapshot(id, "new"))
            .unwrap();
    }
    let worker = service.clone();
    let (bounded, observation, elapsed, owned) =
        contend(&root, Duration::from_millis(900), move || {
            // Existing Drop logs failed release; field RAII still removes its OS lease.
            drop(released);
            (
                fs::read_dir(worker.store.root().join("leases"))
                    .unwrap()
                    .count(),
                worker.cached_catalog(),
            )
        });
    assert!(bounded && owned);
    assert!(elapsed >= Duration::from_millis(450) && elapsed < Duration::from_millis(900));
    assert_eq!(
        observation.0, 1,
        "underlying lease RAII removed the released lease"
    );
    assert_eq!(
        observation.1.packages["released"].reference_count, 1,
        "busy release did not commit bookkeeping"
    );
    assert!(released_path.is_dir() && live_path.is_dir());
    let refreshed = service.catalog().unwrap();
    assert_eq!(refreshed.packages["released"].reference_count, 0);
    assert_eq!(refreshed.packages["live"].reference_count, 1);
    assert!(!released_path.exists());
    assert!(live_path.is_dir());
    assert!(
        fs::read_to_string(live_path.join("skills/live/SKILL.md"))
            .unwrap()
            .contains("old")
    );
    drop(live);
    service.catalog().unwrap();
    assert!(!live_path.exists());
}
