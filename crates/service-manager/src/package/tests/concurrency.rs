use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
fn refresh_collects_materialized_revisions_that_never_reached_the_catalog() {
    let service = PackageService::ephemeral("test").unwrap();
    assert!(
        service
            .execute(PackageCommand::Install {
                request_id: "install".into(),
                package_id: "installed".into(),
                snapshot: native_snapshot("installed", "committed"),
            })
            .unwrap()
            .success
    );
    let catalog = service.catalog().unwrap();
    let unpublished = native_snapshot("unpublished", "uncommitted");
    let revision = fingerprint(&unpublished).unwrap();
    for id in ["installed", "orphan"] {
        {
            let _transaction = service.store.transaction().unwrap();
            PackageMaterializer::new(service.store.root())
                .materialize(id, &revision, &unpublished)
                .unwrap();
        }
        let orphan = service.store.revision_root(id, &revision);
        assert!(orphan.is_dir());
        assert_eq!(service.catalog().unwrap(), catalog);
        assert!(!orphan.exists());
    }
    assert!(service.acquire("installed").is_ok());
}

#[test]
fn refresh_recovers_an_interrupted_removal_without_reopening_the_service() {
    let service = PackageService::ephemeral("test").unwrap();
    assert!(
        service
            .execute(PackageCommand::Install {
                request_id: "install".into(),
                package_id: "interrupted".into(),
                snapshot: native_snapshot("interrupted", "body"),
            })
            .unwrap()
            .success
    );
    let before = service.catalog().unwrap();
    let staged = {
        let _transaction = service.store.transaction().unwrap();
        service
            .store
            .stage_package_revisions("interrupted")
            .unwrap()
            .unwrap()
    };
    assert!(!staged.active.exists());
    assert_eq!(service.catalog().unwrap(), before);
    assert!(staged.active.is_dir());
    assert!(!staged.staged.exists());
    assert!(service.acquire("interrupted").is_ok());
}

#[test]
fn catalog_refresh_does_not_read_content_but_acquisition_verifies_it() {
    let service = PackageService::ephemeral("test").unwrap();
    for id in ["intact", "tampered"] {
        assert!(
            service
                .execute(PackageCommand::Install {
                    request_id: id.into(),
                    package_id: id.into(),
                    snapshot: native_snapshot(id, "body"),
                })
                .unwrap()
                .success
        );
    }
    let record = service.resolve("tampered").unwrap();
    let root = service.store.revision_root("tampered", &record.revision);
    fs::write(root.join("manifest.json"), b"{}").unwrap();
    assert_eq!(service.catalog().unwrap().packages.len(), 2);
    assert!(service.acquire("intact").is_ok());
    assert!(service.acquire("tampered").is_err());
}

#[test]
fn independent_services_observe_mutations_and_retain_live_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let first = PackageService::open("dev", root.clone()).unwrap();
    let second = PackageService::open("dev", root.clone()).unwrap();
    assert!(
        first
            .execute(PackageCommand::Install {
                request_id: "install".into(),
                package_id: "shared".into(),
                snapshot: native_snapshot("shared", "first"),
            })
            .unwrap()
            .success
    );
    let lease = first.acquire("shared").unwrap();
    let old_content = lease.content_root().to_path_buf();
    assert_eq!(
        second.catalog().unwrap().packages["shared"].reference_count,
        1
    );
    assert!(
        second
            .execute(PackageCommand::Upgrade {
                request_id: "upgrade".into(),
                package_id: "shared".into(),
                snapshot: native_snapshot("shared", "second"),
            })
            .unwrap()
            .success
    );
    assert_eq!(
        first.resolve("shared").unwrap(),
        second.resolve("shared").unwrap()
    );
    assert!(
        fs::read_to_string(old_content.join("skills/shared/SKILL.md"))
            .unwrap()
            .contains("first")
    );
    let reopened = PackageService::open("dev", root.clone()).unwrap();
    assert_eq!(
        reopened.catalog().unwrap().packages["shared"].reference_count,
        1
    );
    assert!(
        second
            .execute(PackageCommand::Uninstall {
                request_id: "uninstall".into(),
                package_id: "shared".into(),
            })
            .unwrap()
            .success
    );
    assert_eq!(
        reopened.catalog().unwrap().packages["shared"].state,
        PackageState::Retiring
    );
    drop(first);
    assert!(
        old_content.is_dir(),
        "lease keeps its owner and revision alive"
    );
    drop(lease);
    assert!(second.catalog().unwrap().packages.is_empty());
    assert!(!root.join("revisions/shared").exists());
}

#[test]
fn concurrent_services_do_not_overwrite_each_others_catalog_entries() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let first = PackageService::open("dev", root.clone()).unwrap();
    let second = PackageService::open("dev", root).unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        for (service, id) in [(&first, "first"), (&second, "second")] {
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                assert!(
                    service
                        .execute(PackageCommand::Install {
                            request_id: id.into(),
                            package_id: id.into(),
                            snapshot: native_snapshot(id, id),
                        })
                        .unwrap()
                        .success
                );
            });
        }
    });
    assert_eq!(first.catalog().unwrap(), second.catalog().unwrap());
    assert_eq!(first.catalog().unwrap().packages.len(), 2);
}

#[test]
fn crashed_process_releases_revision_and_staging_is_recovered() {
    for surviving_reference in [false, true] {
        for retiring in [false, true] {
            verify_crash_recovery(surviving_reference, retiring);
        }
    }
}

fn verify_crash_recovery(surviving_reference: bool, retiring: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    assert!(
        service
            .execute(PackageCommand::Install {
                request_id: "install".into(),
                package_id: "crashed".into(),
                snapshot: native_snapshot("crashed", "body"),
            })
            .unwrap()
            .success
    );
    let survivor = surviving_reference.then(|| service.acquire("crashed").unwrap());
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "package::tests::concurrency::package_crash_worker",
            "--nocapture",
        ])
        .env("ALAN_PACKAGE_CRASH_TEST_ROOT", &root)
        .env(
            "ALAN_PACKAGE_CRASH_TEST_RETIRING",
            if retiring { "1" } else { "0" },
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    loop {
        let mut line = String::new();
        assert_ne!(
            output.read_line(&mut line).unwrap(),
            0,
            "child failed before acquiring lease"
        );
        if line.trim() == "LEASE_READY" {
            break;
        }
    }
    let catalog = service.catalog().unwrap();
    assert_eq!(
        catalog.packages["crashed"].reference_count,
        1 + u64::from(surviving_reference)
    );
    let reopened = PackageService::open("dev", root.clone()).unwrap();
    assert_eq!(
        reopened.catalog().unwrap().packages["crashed"].state,
        if retiring {
            PackageState::Retiring
        } else {
            PackageState::Installed
        }
    );
    let old_revision = service
        .store
        .revision_root("crashed", &catalog.packages["crashed"].revision);
    if !retiring {
        assert!(
            service
                .execute(PackageCommand::Upgrade {
                    request_id: "upgrade".into(),
                    package_id: "crashed".into(),
                    snapshot: native_snapshot("crashed", "upgraded"),
                })
                .unwrap()
                .success
        );
    }
    assert!(old_revision.is_dir());
    child.stdin.take().unwrap().write_all(b"x").unwrap();
    assert!(child.wait().unwrap().success());
    drop(survivor);
    let refreshed = service.catalog().unwrap();
    assert_eq!(refreshed.packages.is_empty(), retiring);
    if !retiring {
        assert_eq!(refreshed.packages["crashed"].reference_count, 0);
    }
    assert_eq!(reopened.catalog().unwrap(), refreshed);
    assert!(!old_revision.exists());
    fs::create_dir_all(root.join("staging/interrupted/source")).unwrap();
    fs::write(root.join("staging/interrupted/source/file"), b"partial").unwrap();
    fs::write(root.join("catalog-interrupted.tmp"), b"partial").unwrap();
    let recovered = PackageService::open("dev", root.clone()).unwrap();
    assert_eq!(recovered.catalog().unwrap(), refreshed);
    assert_eq!(service.catalog().unwrap(), refreshed);
    assert!(!old_revision.exists());
    assert_eq!(fs::read_dir(root.join("leases")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
    assert!(!root.join("catalog-interrupted.tmp").exists());
}

#[test]
fn package_crash_worker() {
    let Some(root) = std::env::var_os("ALAN_PACKAGE_CRASH_TEST_ROOT") else {
        return;
    };
    let service = PackageService::open("dev", root.into()).unwrap();
    let _lease = service.acquire("crashed").unwrap();
    if std::env::var("ALAN_PACKAGE_CRASH_TEST_RETIRING").unwrap() == "1" {
        assert!(
            service
                .execute(PackageCommand::Uninstall {
                    request_id: "remove".into(),
                    package_id: "crashed".into(),
                })
                .unwrap()
                .success
        );
    }
    println!("LEASE_READY");
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_exact(&mut [0]).unwrap();
    // Emulate process death: OS locks close, but Rust lease cleanup never runs.
    std::process::exit(0);
}

#[cfg(unix)]
#[test]
fn lease_scan_rejects_symlinks_without_removing_the_target() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    let victim = directory.path().join("victim");
    fs::write(&victim, b"keep").unwrap();
    std::os::unix::fs::symlink(&victim, root.join("leases/unsafe")).unwrap();
    assert!(PackageService::open("dev", root).is_err());
    assert_eq!(fs::read(&victim).unwrap(), b"keep");
    drop(service);
}

#[test]
fn preinstalled_retirement_preserves_live_leases_and_operator_packages() {
    let service = PackageService::ephemeral("test").unwrap();
    service
        .seed_preinstalled("obsolete", native_snapshot("old", "body"))
        .unwrap();
    service
        .seed_preinstalled("current", native_snapshot("current", "body"))
        .unwrap();
    let peer = PackageService::open("test", service.store.root().to_path_buf()).unwrap();
    let lease = peer.acquire("obsolete").unwrap();
    service.retire_preinstalled("obsolete").unwrap();
    assert!(peer.resolve("obsolete").is_err());
    assert!(peer.acquire("obsolete").is_err());
    assert!(lease.content_root().is_dir());
    assert_eq!(
        service.catalog().unwrap().packages["obsolete"].state,
        PackageState::Retiring
    );
    let generation = service.catalog().unwrap().generation;
    service.retire_preinstalled("obsolete").unwrap();
    assert_eq!(service.catalog().unwrap().generation, generation);
    drop(lease);
    assert!(!service.catalog().unwrap().packages.contains_key("obsolete"));
    assert!(!service.store.root().join("revisions/obsolete").exists());
    assert!(peer.resolve("current").is_ok());
    service.retire_preinstalled("current").unwrap();
    service.retire_preinstalled("current").unwrap();
    assert!(peer.resolve("current").is_err());
    let installed = service
        .execute(PackageCommand::Install {
            request_id: "operator".into(),
            package_id: "obsolete".into(),
            snapshot: native_snapshot("operator", "authored"),
        })
        .unwrap();
    assert!(installed.success);
    service.retire_preinstalled("obsolete").unwrap();
    assert_eq!(
        peer.resolve("obsolete").unwrap().kind,
        PackageKind::Installed
    );
}
