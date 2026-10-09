use super::*;

fn fixture() -> (tempfile::TempDir, InstallationPaths) {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::from_data_dir(temp.path()).unwrap();
    let source = paths
        .system_root()
        .join("dev/services/memory/stores/personal");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("note.md"), "durable memory").unwrap();
    (temp, paths)
}

#[tokio::test]
async fn dry_run_is_read_only_and_apply_preserves_source_and_supports_retry_and_rollback() {
    let (_temp, paths) = fixture();
    let dry = migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::DryRun,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(dry.state, "validated");
    assert!(!paths.journal().exists());
    assert!(!paths.product.join("installation.lock").exists());
    let report = migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(report.state, "committed");
    let relative = "services/memory/stores/personal/note.md";
    assert_eq!(
        fs::read(paths.system_root().join(relative)).unwrap(),
        fs::read(paths.system_root().join("dev").join(relative)).unwrap()
    );
    assert_eq!(
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Apply,
            |_, _| Ok(())
        )
        .await
        .unwrap()
        .state,
        "already-committed"
    );
    assert!(
        migrate(
            &paths,
            LegacyInstallation::Stable,
            MigrationMode::Apply,
            |_, _| Ok(())
        )
        .await
        .is_err()
    );
    assert_eq!(
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Rollback,
            |_, _| Ok(())
        )
        .await
        .unwrap()
        .state,
        "rolled-back"
    );
    assert!(!paths.system_root().join("services").exists());
    assert!(paths.system_root().join("dev").join(relative).exists());
    assert!(!paths.journal().exists());
}

#[tokio::test]
async fn changed_canonical_data_and_unverified_writer_state_prevent_destructive_actions() {
    let (_temp, paths) = fixture();
    assert!(
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Apply,
            |_, _| anyhow::bail!("writer active")
        )
        .await
        .is_err()
    );
    assert!(!paths.journal().exists());
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    let note = paths
        .system_root()
        .join("services/memory/stores/personal/note.md");
    fs::write(&note, "new work").unwrap();
    assert!(
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Rollback,
            |_, _| Ok(())
        )
        .await
        .is_err()
    );
    assert_eq!(fs::read_to_string(note).unwrap(), "new work");
    assert_eq!(
        paths.read_migration_journal().unwrap().unwrap().state,
        MigrationState::Committed
    );
}

#[tokio::test]
async fn every_publication_boundary_can_resume_without_exposing_partial_data() {
    for point in [
        "prepared",
        "staged",
        "publishing",
        "published",
        "before-commit",
        "committed",
    ] {
        for occurrence in 1..=3 {
            let (_temp, paths) = fixture();
            let host = paths.host_root().join("dev");
            fs::create_dir_all(host.join("credentials")).unwrap();
            fs::write(host.join("credentials/secrets.toml"), "revoked = ['key']\n").unwrap();
            fs::write(host.join("auth.json"), "{\"version\":1}").unwrap();
            let seen = std::cell::Cell::new(0);
            let result = migrate_with_checkpoints(
                &paths,
                LegacyInstallation::Dev,
                MigrationMode::Apply,
                |_, _| Ok(()),
                |name| {
                    if name == point {
                        seen.set(seen.get() + 1);
                        anyhow::ensure!(seen.get() != occurrence, "injected interruption");
                    }
                    Ok(())
                },
            )
            .await;
            if seen.get() < occurrence {
                result.unwrap();
                continue;
            }
            assert!(result.is_err(), "{point}/{occurrence}");
            if point == "committed" {
                paths.access().unwrap();
            } else {
                assert!(paths.access().is_err(), "{point}/{occurrence}");
            }
            let report = migrate(
                &paths,
                LegacyInstallation::Dev,
                MigrationMode::Apply,
                |_, _| Ok(()),
            )
            .await
            .unwrap();
            assert!(matches!(report.state, "committed" | "already-committed"));
            assert_eq!(
                fs::read(paths.host_root().join("auth.json")).unwrap(),
                fs::read(host.join("auth.json")).unwrap()
            );
            assert!(
                paths
                    .system_root()
                    .join("dev/services/memory/stores/personal/note.md")
                    .exists()
            );
            paths.access().unwrap();
        }
    }
}

#[tokio::test]
async fn interrupted_rollback_stays_blocked_and_resumes_explicitly() {
    let (_temp, paths) = fixture();
    let host = paths.host_root().join("dev");
    fs::create_dir_all(&host).unwrap();
    fs::write(host.join("auth.json"), "{\"version\":1}").unwrap();
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert!(
        migrate_with_checkpoints(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Rollback,
            |_, _| Ok(()),
            |point| {
                anyhow::ensure!(point != "removed", "injected interruption");
                Ok(())
            }
        )
        .await
        .is_err()
    );
    assert!(paths.access().is_err());
    assert!(
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Apply,
            |_, _| Ok(())
        )
        .await
        .is_err()
    );
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Rollback,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert!(!paths.journal().exists());
    assert!(!paths.host_root().join("auth.json").exists());
    assert!(host.join("auth.json").exists());
}

#[tokio::test]
async fn new_canonical_component_prevents_rollback_even_if_original_component_is_unchanged() {
    let (_temp, paths) = fixture();
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    fs::write(paths.host_root().join("auth.json"), "new auth").unwrap();
    assert!(
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Rollback,
            |_, _| Ok(())
        )
        .await
        .is_err()
    );
    assert!(paths.system_root().join("services").exists());
    assert_eq!(
        fs::read_to_string(paths.host_root().join("auth.json")).unwrap(),
        "new auth"
    );
}

#[tokio::test]
async fn changing_source_is_detected_before_publication_without_losing_the_new_bytes() {
    let (_temp, paths) = fixture();
    let note = paths
        .system_root()
        .join("dev/services/memory/stores/personal/note.md");
    let result = migrate_with_checkpoints(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
        |point| {
            if point == "publishing" {
                fs::write(&note, "new source bytes")?;
            }
            Ok(())
        },
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("source changed before publication")
    );
    assert_eq!(fs::read_to_string(note).unwrap(), "new source bytes");
    assert!(!paths.system_root().join("services").exists());
    assert!(paths.access().is_err());
}

#[tokio::test]
async fn rollback_uses_persisted_inventory_even_when_the_legacy_source_changes() {
    for point in ["prepared", "inventory", "staged", "published", "committed"] {
        let (_temp, paths) = fixture();
        let result = migrate_with_checkpoints(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Apply,
            |_, _| Ok(()),
            |name| {
                anyhow::ensure!(name != point, "injected interruption");
                Ok(())
            },
        )
        .await;
        assert!(result.is_err(), "{point}");
        let note = paths
            .system_root()
            .join("dev/services/memory/stores/personal/note.md");
        fs::write(&note, "new legacy work after interruption").unwrap();
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Rollback,
            |_, _| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read_to_string(&note).unwrap(),
            "new legacy work after interruption"
        );
        assert!(!paths.system_root().join("services").exists());
        assert!(!paths.journal().exists());
    }
}

#[tokio::test]
async fn committed_retry_and_rollback_do_not_need_the_legacy_source() {
    let (_temp, paths) = fixture();
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    fs::remove_dir_all(paths.system_root().join("dev")).unwrap();
    assert_eq!(
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Apply,
            |_, _| Ok(())
        )
        .await
        .unwrap()
        .state,
        "already-committed"
    );
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Rollback,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert!(!paths.system_root().join("services").exists());
}

#[tokio::test]
async fn interruption_after_inventory_removal_can_finish_rollback() {
    let (_temp, paths) = fixture();
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert!(
        migrate_with_checkpoints(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Rollback,
            |_, _| Ok(()),
            |point| {
                anyhow::ensure!(point != "inventory-removed", "injected interruption");
                Ok(())
            }
        )
        .await
        .is_err()
    );
    assert!(paths.access().is_err());
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Rollback,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert!(!paths.journal().exists());
}

#[tokio::test]
async fn missing_or_corrupt_inventory_never_authorizes_deleting_staged_or_canonical_data() {
    for corrupt in [false, true] {
        let (_temp, paths) = fixture();
        assert!(
            migrate_with_checkpoints(
                &paths,
                LegacyInstallation::Dev,
                MigrationMode::Apply,
                |_, _| Ok(()),
                |point| {
                    anyhow::ensure!(point != "published", "injected interruption");
                    Ok(())
                }
            )
            .await
            .is_err()
        );
        let id = paths.read_migration_journal().unwrap().unwrap().id;
        let inventory = paths
            .product
            .join(format!("installation-recovery-{id}.json"));
        if corrupt {
            fs::write(&inventory, "[]").unwrap();
        } else {
            fs::remove_file(&inventory).unwrap();
        }
        assert!(
            migrate(
                &paths,
                LegacyInstallation::Dev,
                MigrationMode::Rollback,
                |_, _| Ok(())
            )
            .await
            .is_err()
        );
        assert!(
            paths
                .system_root()
                .join("services/memory/stores/personal/note.md")
                .exists()
        );
        assert!(paths.access().is_err());
    }
}

#[tokio::test]
async fn selected_pair_never_merges_another_source_or_populated_canonical_store() {
    let (_temp, paths) = fixture();
    let other = paths.host_root().join("stable");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("auth.json"), "unselected bytes, not parsed").unwrap();
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert!(!paths.host_root().join("auth.json").exists());
    assert_eq!(
        fs::read_to_string(other.join("auth.json")).unwrap(),
        "unselected bytes, not parsed"
    );

    let (_temp, paths) = fixture();
    let existing = paths.system_root().join("services/memory/stores/personal");
    fs::create_dir_all(&existing).unwrap();
    fs::write(existing.join("note.md"), "canonical work").unwrap();
    for mode in [MigrationMode::Apply, MigrationMode::DryRun] {
        assert!(
            migrate(&paths, LegacyInstallation::Dev, mode, |_, _| Ok(()))
                .await
                .is_err()
        );
    }
    assert_eq!(
        fs::read_to_string(existing.join("note.md")).unwrap(),
        "canonical work"
    );
    assert!(!paths.journal().exists());
    assert!(!paths.product.join("installation.lock").exists());
}

#[tokio::test]
async fn dry_run_of_a_committed_receipt_never_recreates_a_removed_access_lock() {
    let (_temp, paths) = fixture();
    migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::Apply,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    fs::remove_file(paths.product.join("installation.lock")).unwrap();
    let report = migrate(
        &paths,
        LegacyInstallation::Dev,
        MigrationMode::DryRun,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(report.state, "already-committed");
    assert!(!paths.product.join("installation.lock").exists());
}

#[tokio::test]
async fn rollback_does_not_follow_or_require_a_valid_retained_source_root() {
    for symlink in [false, true] {
        let (_temp, paths) = fixture();
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Apply,
            |_, _| Ok(()),
        )
        .await
        .unwrap();
        let source = paths.system_root().join("dev");
        fs::remove_dir_all(&source).unwrap();
        let outside = _temp.path().join("outside");
        fs::write(&outside, "retained independently").unwrap();
        if symlink {
            std::os::unix::fs::symlink(&outside, &source).unwrap();
        } else {
            fs::write(&source, "replaced legacy root").unwrap();
        }
        drop(paths.access().unwrap());
        assert_eq!(
            migrate(
                &paths,
                LegacyInstallation::Dev,
                MigrationMode::Apply,
                |_, _| Ok(())
            )
            .await
            .unwrap()
            .state,
            "already-committed"
        );
        migrate(
            &paths,
            LegacyInstallation::Dev,
            MigrationMode::Rollback,
            |_, _| Ok(()),
        )
        .await
        .unwrap();
        assert!(!paths.system_root().join("services").exists());
        assert!(!paths.journal().exists());
        assert_eq!(
            fs::read_to_string(&outside).unwrap(),
            "retained independently"
        );
        assert_eq!(source.is_symlink(), symlink);
        if !symlink {
            assert_eq!(fs::read_to_string(&source).unwrap(), "replaced legacy root");
        }
    }
}
