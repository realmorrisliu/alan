use super::materializer::{MAX_SOURCE_FILE_BYTES, MaterializationManifest};
use super::*;
use alan_agent_engine::skills::{SkillScope, parse_skill_metadata};

mod file_surface;

fn native_snapshot(name: &str, body: &str) -> PackageSnapshot {
    PackageSnapshot {
        source_name: format!("{name}-distribution"),
        entries: vec![PackageSnapshotEntry {
            path: format!("{name}/SKILL.md"),
            bytes: format!("---\nname: {name}\ndescription: Test Skill.\n---\n\n{body}\n")
                .into_bytes(),
            executable: false,
        }],
    }
}

#[test]
fn fingerprint_is_order_independent_and_materializer_scoped() {
    let mut first = native_snapshot("alpha", "A");
    first.entries.push(PackageSnapshotEntry {
        path: "assets/value.txt".to_string(),
        bytes: b"value".to_vec(),
        executable: false,
    });
    let mut second = first.clone();
    second.entries.reverse();
    assert_eq!(fingerprint(&first).unwrap(), fingerprint(&second).unwrap());
}

#[test]
fn valid_source_limits_do_not_reject_duplicated_materialized_output() {
    let service = PackageService::ephemeral("test").unwrap();
    let asset = vec![b'x'; 3 * 1024 * 1024 + 1];
    let result = service
        .execute(PackageCommand::Install {
            request_id: "large-materialized-install".to_string(),
            package_id: "large-materialized".to_string(),
            snapshot: PackageSnapshot {
                source_name: "large-materialized".to_string(),
                entries: vec![
                    PackageSnapshotEntry {
                        path: "SKILL.md".to_string(),
                        bytes:
                            b"---\nname: Large Materialized\ndescription: Large valid Skill.\n---\n"
                                .to_vec(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "assets/first.bin".to_string(),
                        bytes: asset.clone(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "assets/second.bin".to_string(),
                        bytes: asset,
                        executable: false,
                    },
                ],
            },
        })
        .unwrap();

    assert!(result.success, "{}", result.message);
    assert!(service.acquire("large-materialized").is_ok());
}

#[test]
fn native_required_tools_are_recorded_as_package_dependencies() {
    let service = PackageService::ephemeral("test").unwrap();
    let result = service
        .execute(PackageCommand::Install {
            request_id: "required-tools-install".to_string(),
            package_id: "required-tools-pack".to_string(),
            snapshot: PackageSnapshot {
                source_name: "required-tools-distribution".to_string(),
                entries: vec![PackageSnapshotEntry {
                    path: "research/SKILL.md".to_string(),
                    bytes: b"---\nname: research\ndescription: Test Skill.\ncapabilities:\n  required_tools:\n    - rg\ncompatibility:\n  dependencies:\n    - type: env_var\n      name: RESEARCH_TOKEN\n    - type: tool\n      name: rg\n---\n"
                        .to_vec(),
                    executable: false,
                }],
            },
        })
        .unwrap();

    assert!(result.success, "{}", result.message);
    assert_eq!(
        result.package.unwrap().exports[0]
            .dependencies
            .iter()
            .map(SkillTypedDependency::identity_key)
            .collect::<Vec<_>>(),
        vec!["env_var:RESEARCH_TOKEN", "tool:rg"]
    );
}

#[test]
fn install_rejects_a_stale_file_at_the_revision_path() {
    let service = PackageService::ephemeral("test").unwrap();
    let snapshot = native_snapshot("stale-file", "body");
    let revision = fingerprint(&snapshot).unwrap();
    let target = revision_root(&service.store_root, "stale-file-pack", &revision);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"stale").unwrap();

    let result = service.execute(PackageCommand::Install {
        request_id: "stale-file-install".to_string(),
        package_id: "stale-file-pack".to_string(),
        snapshot,
    });

    assert!(!result.unwrap().success);
    assert!(target.is_file());
    assert!(!service.catalog().packages.contains_key("stale-file-pack"));
}

#[cfg(unix)]
#[test]
fn install_rejects_a_stale_symlink_at_the_revision_path() {
    use std::os::unix::fs::symlink;

    let service = PackageService::ephemeral("test").unwrap();
    let snapshot = native_snapshot("stale-link", "body");
    let revision = fingerprint(&snapshot).unwrap();
    let target = revision_root(&service.store_root, "stale-link-pack", &revision);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), &target).unwrap();

    let result = service.execute(PackageCommand::Install {
        request_id: "stale-link-install".to_string(),
        package_id: "stale-link-pack".to_string(),
        snapshot,
    });

    assert!(!result.unwrap().success);
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!service.catalog().packages.contains_key("stale-link-pack"));
}

#[cfg(unix)]
#[test]
fn install_rejects_a_symlinked_package_revision_parent() {
    use std::os::unix::fs::symlink;

    let service = PackageService::ephemeral("test").unwrap();
    let outside = tempfile::tempdir().unwrap();
    let parent = service.store_root.join("revisions/symlinked-parent-pack");
    symlink(outside.path(), &parent).unwrap();

    let result = service.execute(PackageCommand::Install {
        request_id: "symlinked-parent-install".to_string(),
        package_id: "symlinked-parent-pack".to_string(),
        snapshot: native_snapshot("symlinked-parent", "body"),
    });

    assert!(!result.unwrap().success);
    assert!(outside.path().read_dir().unwrap().next().is_none());
    assert!(
        !service
            .catalog()
            .packages
            .contains_key("symlinked-parent-pack")
    );
}

#[test]
fn snapshot_rejects_traversal_and_vcs_metadata() {
    let traversal = PackageSnapshot {
        source_name: "traversal".to_string(),
        entries: vec![PackageSnapshotEntry {
            path: "../SKILL.md".to_string(),
            bytes: Vec::new(),
            executable: false,
        }],
    };
    assert!(validate_snapshot(&traversal).is_err());
    let vcs = PackageSnapshot {
        source_name: "vcs".to_string(),
        entries: vec![PackageSnapshotEntry {
            path: ".git/config".to_string(),
            bytes: Vec::new(),
            executable: false,
        }],
    };
    assert!(validate_snapshot(&vcs).is_err());
    let noncanonical = PackageSnapshot {
        source_name: "noncanonical".to_string(),
        entries: vec![PackageSnapshotEntry {
            path: "dir//file".to_string(),
            bytes: Vec::new(),
            executable: false,
        }],
    };
    assert!(validate_snapshot(&noncanonical).is_err());
    let duplicate = PackageSnapshot {
        source_name: "duplicate".to_string(),
        entries: vec![
            PackageSnapshotEntry {
                path: "dir/file".to_string(),
                bytes: b"first".to_vec(),
                executable: false,
            },
            PackageSnapshotEntry {
                path: "dir/file".to_string(),
                bytes: b"second".to_vec(),
                executable: false,
            },
        ],
    };
    assert!(validate_snapshot(&duplicate).is_err());
}

#[test]
fn install_rejects_case_colliding_snapshot_paths_before_materialization() {
    let service = PackageService::ephemeral("test").unwrap();
    let error = service
        .execute(PackageCommand::Install {
            request_id: "case-collision-install".to_string(),
            package_id: "case-collision-pack".to_string(),
            snapshot: PackageSnapshot {
                source_name: "case-collision-source".to_string(),
                entries: vec![
                    PackageSnapshotEntry {
                        path: "research/SKILL.md".to_string(),
                        bytes: b"---\nname: research\ndescription: Test Skill.\n---\n".to_vec(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "assets/Icon.png".to_string(),
                        bytes: b"upper".to_vec(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "assets/icon.png".to_string(),
                        bytes: b"lower".to_vec(),
                        executable: false,
                    },
                ],
            },
        })
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("snapshot paths collide on a case-insensitive Package Store")
    );
    assert!(
        !service
            .catalog()
            .packages
            .contains_key("case-collision-pack")
    );
    assert!(
        !service
            .store_root
            .join("revisions/case-collision-pack")
            .exists()
    );
}

#[test]
fn directory_snapshot_rejects_oversized_file_before_reading_it() {
    let source = tempfile::tempdir().unwrap();
    File::create(source.path().join("oversized.bin"))
        .unwrap()
        .set_len(MAX_SOURCE_FILE_BYTES as u64 + 1)
        .unwrap();

    let error = PackageSnapshot::from_directory(source.path()).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("package source file is too large")
    );
}

#[test]
fn package_ids_use_the_exact_bounded_ascii_contract() {
    assert!(validate_package_id(&"a".repeat(64)).is_ok());
    assert!(validate_package_id(&"a".repeat(65)).is_err());
    assert!(validate_package_id("alpha-2").is_ok());
    for invalid in ["Alpha", "alpha_2", "alpha--2", "-alpha", "alpha-"] {
        assert!(validate_package_id(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn portable_root_uses_source_leaf_and_suppresses_nested_discovery() {
    let service = PackageService::ephemeral("test").unwrap();
    let result = service
        .execute(PackageCommand::Install {
            request_id: "portable-root".to_string(),
            package_id: "portable-distribution".to_string(),
            snapshot: PackageSnapshot {
                source_name: "Portable Root".to_string(),
                entries: vec![
                    PackageSnapshotEntry {
                        path: "SKILL.md".to_string(),
                        bytes: b"---\nname: Declared Name\ndescription: Root Skill.\n---\n"
                            .to_vec(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "nested/ignored/SKILL.md".to_string(),
                        bytes: b"---\nname: Nested\ndescription: Nested Skill.\n---\n".to_vec(),
                        executable: false,
                    },
                ],
            },
        })
        .unwrap();
    assert!(result.success, "{}", result.message);
    assert_eq!(
        result
            .package
            .unwrap()
            .exports
            .iter()
            .map(|export| export.skill_id.as_str())
            .collect::<Vec<_>>(),
        vec!["portable-root"]
    );
}

#[test]
fn one_distribution_rejects_duplicate_runtime_skill_ids() {
    let service = PackageService::ephemeral("test").unwrap();
    let document = b"---\nname: Skill\ndescription: Test Skill.\n---\n".to_vec();
    let result = service
        .execute(PackageCommand::Install {
            request_id: "duplicate-skills".to_string(),
            package_id: "duplicate-skills".to_string(),
            snapshot: PackageSnapshot {
                source_name: "duplicate-skills".to_string(),
                entries: vec![
                    PackageSnapshotEntry {
                        path: "Foo Bar/SKILL.md".to_string(),
                        bytes: document.clone(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "foo_bar/SKILL.md".to_string(),
                        bytes: document,
                        executable: false,
                    },
                ],
            },
        })
        .unwrap();
    assert!(!result.success);
    assert!(result.message.contains("duplicate Skill id"));
    assert!(service.catalog().packages.is_empty());
}

#[test]
fn install_upgrade_and_uninstall_are_exact() {
    let service = PackageService::ephemeral("test").unwrap();
    let installed = service
        .execute(PackageCommand::Install {
            request_id: "install-1".to_string(),
            package_id: "research-pack".to_string(),
            snapshot: native_snapshot("research", "first"),
        })
        .unwrap();
    assert_eq!(installed.message, "installed");
    let repeated = service
        .execute(PackageCommand::Install {
            request_id: "install-2".to_string(),
            package_id: "research-pack".to_string(),
            snapshot: native_snapshot("research", "first"),
        })
        .unwrap();
    assert_eq!(repeated.message, "already installed");
    let upgraded = service
        .execute(PackageCommand::Upgrade {
            request_id: "upgrade-1".to_string(),
            package_id: "research-pack".to_string(),
            snapshot: native_snapshot("research", "second"),
        })
        .unwrap();
    assert_eq!(upgraded.message, "upgraded");
    assert_ne!(
        installed.package.unwrap().revision,
        upgraded.package.unwrap().revision
    );
    let removed = service
        .execute(PackageCommand::Uninstall {
            request_id: "remove-1".to_string(),
            package_id: "research-pack".to_string(),
        })
        .unwrap();
    assert_eq!(removed.message, "uninstalled");
    assert!(service.resolve("research-pack").is_err());
}

#[test]
fn failed_upgrade_keeps_the_current_catalog_and_revision() {
    let service = PackageService::ephemeral("test").unwrap();
    service
        .execute(PackageCommand::Install {
            request_id: "atomic-install".to_string(),
            package_id: "atomic-pack".to_string(),
            snapshot: native_snapshot("research", "current"),
        })
        .unwrap();
    let before = service.catalog();
    let failed = service
        .execute(PackageCommand::Upgrade {
            request_id: "atomic-upgrade".to_string(),
            package_id: "atomic-pack".to_string(),
            snapshot: PackageSnapshot {
                source_name: "atomic-pack".to_string(),
                entries: vec![PackageSnapshotEntry {
                    path: "research/SKILL.md".to_string(),
                    bytes: b"not valid Skill metadata".to_vec(),
                    executable: false,
                }],
            },
        })
        .unwrap();

    assert!(!failed.success);
    assert_eq!(service.catalog(), before);
    assert_eq!(
        fs::read_dir(service.store_root.join("revisions/atomic-pack"))
            .unwrap()
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn upgrade_reports_success_when_post_commit_revision_cleanup_fails() {
    use std::os::unix::fs::PermissionsExt;

    let service = PackageService::ephemeral("test").unwrap();
    let installed = service
        .execute(PackageCommand::Install {
            request_id: "cleanup-install".to_string(),
            package_id: "cleanup-pack".to_string(),
            snapshot: native_snapshot("research", "current"),
        })
        .unwrap();
    assert!(installed.success);
    let locked_revision = service
        .store_root
        .join("revisions/cleanup-pack")
        .join("stale-locked-revision");
    fs::create_dir(&locked_revision).unwrap();
    fs::write(locked_revision.join("retained"), b"stale").unwrap();
    fs::set_permissions(&locked_revision, fs::Permissions::from_mode(0o000)).unwrap();

    let upgraded = service
        .execute(PackageCommand::Upgrade {
            request_id: "cleanup-upgrade".to_string(),
            package_id: "cleanup-pack".to_string(),
            snapshot: native_snapshot("research", "upgraded"),
        })
        .unwrap();

    assert!(upgraded.success, "{}", upgraded.message);
    let upgraded_revision = upgraded.package.unwrap().revision;
    assert_eq!(
        service.resolve("cleanup-pack").unwrap().revision,
        upgraded_revision
    );
    fs::set_permissions(&locked_revision, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(locked_revision.exists());
}

#[test]
fn failed_uninstall_keeps_the_current_catalog_and_revision() {
    let service = PackageService::ephemeral("test").unwrap();
    service
        .execute(PackageCommand::Install {
            request_id: "atomic-uninstall-install".to_string(),
            package_id: "atomic-uninstall-pack".to_string(),
            snapshot: native_snapshot("atomic-uninstall", "current"),
        })
        .unwrap();
    let before = service.catalog();
    let staging = service.store_root.join("staging");
    fs::remove_dir(&staging).unwrap();
    fs::write(&staging, b"block revision staging").unwrap();

    let failed = service
        .execute(PackageCommand::Uninstall {
            request_id: "atomic-uninstall".to_string(),
            package_id: "atomic-uninstall-pack".to_string(),
        })
        .unwrap();

    assert!(!failed.success);
    assert_eq!(service.catalog(), before);
    assert!(service.resolve("atomic-uninstall-pack").is_ok());
    assert!(
        service
            .store_root
            .join("revisions/atomic-uninstall-pack")
            .is_dir()
    );
}

#[test]
fn restart_rolls_back_revision_removal_when_catalog_is_still_old() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    service
        .execute(PackageCommand::Install {
            request_id: "crash-window-install".to_string(),
            package_id: "crash-window-pack".to_string(),
            snapshot: native_snapshot("crash-window", "body"),
        })
        .unwrap();
    let record = service.resolve("crash-window-pack").unwrap();
    let staged = stage_package_revisions(&root, "crash-window-pack")
        .unwrap()
        .unwrap();
    assert!(staged.staged.is_dir());
    assert!(!staged.active.exists());
    drop(service);

    let reopened = PackageService::open("dev", root.clone()).unwrap();

    assert_eq!(reopened.resolve("crash-window-pack").unwrap(), record);
    assert!(revision_root(&root, "crash-window-pack", &record.revision).is_dir());
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
}

#[test]
fn live_reference_retains_old_revision_until_retiring_package_is_released() {
    let service = PackageService::ephemeral("test").unwrap();
    let installed = service
        .execute(PackageCommand::Install {
            request_id: "lease-install".to_string(),
            package_id: "leased-pack".to_string(),
            snapshot: native_snapshot("research", "first"),
        })
        .unwrap()
        .package
        .unwrap();
    let lease = service.acquire("leased-pack").unwrap();
    assert_eq!(lease.record().revision, installed.revision);
    let upgraded = service
        .execute(PackageCommand::Upgrade {
            request_id: "lease-upgrade".to_string(),
            package_id: "leased-pack".to_string(),
            snapshot: native_snapshot("research", "second"),
        })
        .unwrap()
        .package
        .unwrap();
    assert_ne!(upgraded.revision, lease.record().revision);
    assert!(
        lease
            .content_root()
            .join("skills/research/SKILL.md")
            .is_file()
    );
    let retiring = service
        .execute(PackageCommand::Uninstall {
            request_id: "lease-uninstall".to_string(),
            package_id: "leased-pack".to_string(),
        })
        .unwrap();
    assert_eq!(retiring.message, "retiring");
    assert!(service.resolve("leased-pack").is_err());
    assert!(lease.content_root().is_dir());
    drop(lease);
    assert!(!service.catalog().packages.contains_key("leased-pack"));
    assert!(!service.store_root.join("revisions/leased-pack").exists());
}

#[test]
fn preinstalled_packages_update_only_through_seeding_and_cannot_be_removed() {
    let service = PackageService::ephemeral("test").unwrap();
    service
        .seed_preinstalled("alan-memory", native_snapshot("memory", "first"))
        .unwrap();
    let first = service.resolve("alan-memory").unwrap();
    let uninstall = service
        .execute(PackageCommand::Uninstall {
            request_id: "remove-preinstalled".to_string(),
            package_id: "alan-memory".to_string(),
        })
        .unwrap();
    assert!(!uninstall.success);
    let collision = service
        .execute(PackageCommand::Install {
            request_id: "replace-preinstalled".to_string(),
            package_id: "alan-memory".to_string(),
            snapshot: native_snapshot("memory", "operator"),
        })
        .unwrap();
    assert!(!collision.success);
    service
        .seed_preinstalled("alan-memory", native_snapshot("memory", "second"))
        .unwrap();
    let second = service.resolve("alan-memory").unwrap();
    assert_ne!(first.revision, second.revision);
    assert_eq!(second.kind, PackageKind::Preinstalled);
}

#[test]
fn restart_cleans_staging_and_finalizes_ephemeral_retiring_references() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    service
        .execute(PackageCommand::Install {
            request_id: "restart-install".to_string(),
            package_id: "restart-pack".to_string(),
            snapshot: native_snapshot("restart", "body"),
        })
        .unwrap();
    let lease = service.acquire("restart-pack").unwrap();
    service
        .execute(PackageCommand::Uninstall {
            request_id: "restart-uninstall".to_string(),
            package_id: "restart-pack".to_string(),
        })
        .unwrap();
    std::mem::forget(lease);
    drop(service);
    fs::create_dir_all(root.join("staging/interrupted/source")).unwrap();
    fs::write(root.join("staging/interrupted/source/file"), b"partial").unwrap();
    fs::write(root.join("catalog-interrupted.tmp"), b"partial").unwrap();

    let reopened = PackageService::open("dev", root.clone()).unwrap();
    assert!(!reopened.catalog().packages.contains_key("restart-pack"));
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
    assert!(!root.join("catalog-interrupted.tmp").exists());
    assert!(!root.join("revisions/restart-pack").exists());
}

#[test]
fn restart_fails_closed_when_revision_content_is_tampered() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    let record = service
        .execute(PackageCommand::Install {
            request_id: "tamper-install".to_string(),
            package_id: "tamper-pack".to_string(),
            snapshot: native_snapshot("tamper", "original"),
        })
        .unwrap()
        .package
        .unwrap();
    drop(service);
    let manifest = revision_root(&root, "tamper-pack", &record.revision).join("manifest.json");
    fs::write(manifest, b"{}").unwrap();
    assert!(PackageService::open("dev", root).is_err());
}

#[cfg(unix)]
#[test]
fn restart_rejects_symlinked_materialized_root() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    let record = service
        .execute(PackageCommand::Install {
            request_id: "symlinked-content-install".to_string(),
            package_id: "symlinked-content-pack".to_string(),
            snapshot: native_snapshot("symlinked-content", "original"),
        })
        .unwrap()
        .package
        .unwrap();
    drop(service);

    let content = revision_root(&root, "symlinked-content-pack", &record.revision).join("content");
    fs::remove_dir_all(&content).unwrap();
    let victim = directory.path().join("victim");
    fs::create_dir(&victim).unwrap();
    fs::write(victim.join("SKILL.md"), b"mutable external content").unwrap();
    symlink(&victim, content).unwrap();

    let error = PackageService::open("dev", root).unwrap_err();
    assert!(error.to_string().contains("materialized package root"));
}

#[test]
fn restart_rejects_invalid_retiring_package_id_before_removing_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    service
        .execute(PackageCommand::Install {
            request_id: "unsafe-recovery-install".to_string(),
            package_id: "unsafe-recovery-pack".to_string(),
            snapshot: native_snapshot("unsafe-recovery", "body"),
        })
        .unwrap();
    let mut catalog = service.catalog();
    drop(service);

    let victim = directory.path().join("victim");
    fs::create_dir(&victim).unwrap();
    fs::write(victim.join("sentinel"), b"keep").unwrap();
    let record = catalog.packages.get_mut("unsafe-recovery-pack").unwrap();
    record.id = victim.to_string_lossy().into_owned();
    record.state = PackageState::Retiring;
    persist_catalog(&root, &catalog).unwrap();

    assert!(PackageService::open("dev", root).is_err());
    assert_eq!(fs::read(victim.join("sentinel")).unwrap(), b"keep");
}

#[cfg(unix)]
#[test]
fn restart_rejects_symlinked_staging_without_deleting_its_target() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();
    drop(service);

    fs::remove_dir(root.join("staging")).unwrap();
    let victim = directory.path().join("victim");
    fs::create_dir(&victim).unwrap();
    fs::write(victim.join("sentinel"), b"keep").unwrap();
    symlink(&victim, root.join("staging")).unwrap();

    let error = PackageService::open("dev", root).unwrap_err();
    assert!(error.to_string().contains("staging path"));
    assert_eq!(fs::read(victim.join("sentinel")).unwrap(), b"keep");
}

#[test]
fn package_store_has_only_one_live_service_owner() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open("dev", root.clone()).unwrap();

    let error = PackageService::open("dev", root.clone()).unwrap_err();
    assert!(error.to_string().contains("already owned"));

    drop(service);
    assert!(PackageService::open("dev", root).is_ok());
}

#[test]
fn stable_and_dev_package_catalogs_are_isolated() {
    let directory = tempfile::tempdir().unwrap();
    let stable_root = directory.path().join("stable/services/packages");
    let dev_root = directory.path().join("dev/services/packages");
    let stable = PackageService::open("stable", stable_root.clone()).unwrap();
    let dev = PackageService::open("dev", dev_root).unwrap();
    stable
        .execute(PackageCommand::Install {
            request_id: "stable-install".to_string(),
            package_id: "stable-only".to_string(),
            snapshot: native_snapshot("stable-skill", "body"),
        })
        .unwrap();
    assert!(stable.resolve("stable-only").is_ok());
    assert!(dev.resolve("stable-only").is_err());
    assert!(dev.catalog().packages.is_empty());
    let catalog = fs::read_to_string(stable_root.join("catalog.json")).unwrap();
    let host_root = directory.path().to_string_lossy();
    assert!(!catalog.contains(host_root.as_ref()));
}

#[cfg(unix)]
#[test]
fn channel_rejects_a_symlinked_package_store_root() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let stable_root = directory.path().join("stable/services/packages");
    let dev_parent = directory.path().join("dev/services");
    fs::create_dir_all(&stable_root).unwrap();
    fs::create_dir_all(&dev_parent).unwrap();
    fs::write(stable_root.join("sentinel"), b"stable").unwrap();
    symlink(&stable_root, dev_parent.join("packages")).unwrap();

    let error = PackageService::open("dev", dev_parent.join("packages")).unwrap_err();

    assert!(error.to_string().contains("Package Store root"));
    assert_eq!(fs::read(stable_root.join("sentinel")).unwrap(), b"stable");
    assert!(!stable_root.join("revisions").exists());
    assert!(!stable_root.join("staging").exists());
}

#[cfg(unix)]
#[test]
fn channel_rejects_a_symlinked_package_store_ancestor() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let stable_services = directory.path().join("stable/services");
    let stable_root = stable_services.join("packages");
    let dev_channel = directory.path().join("dev");
    fs::create_dir_all(&stable_root).unwrap();
    fs::create_dir_all(&dev_channel).unwrap();
    fs::write(stable_root.join("sentinel"), b"stable").unwrap();
    symlink(&stable_services, dev_channel.join("services")).unwrap();

    let error = PackageService::open("dev", dev_channel.join("services/packages")).unwrap_err();

    assert!(error.to_string().contains("unsupported ancestor"));
    assert_eq!(fs::read(stable_root.join("sentinel")).unwrap(), b"stable");
    assert!(!stable_root.join("revisions").exists());
    assert!(!stable_root.join("staging").exists());
}

#[test]
fn command_materialization_keeps_unsupported_capability_visible() {
    let service = PackageService::ephemeral("test").unwrap();
    let result = service
        .execute(PackageCommand::Install {
            request_id: "command-1".to_string(),
            package_id: "foreign-pack".to_string(),
            snapshot: PackageSnapshot {
                source_name: "foreign-pack".to_string(),
                entries: vec![PackageSnapshotEntry {
                    path: "skills/research.md".to_string(),
                    bytes: b"Use WebSearch and TeamCreate for this work.".to_vec(),
                    executable: false,
                }],
            },
        })
        .unwrap();
    let record = result.package.unwrap();
    assert_eq!(
        record.exports[0]
            .dependencies
            .iter()
            .map(SkillTypedDependency::identity_key)
            .collect::<Vec<_>>(),
        vec![
            "runtime_capability:team-orchestration",
            "runtime_capability:web-search"
        ]
    );
    let lease = service.acquire("foreign-pack").unwrap();
    let content =
        fs::read_to_string(lease.content_root().join("skills/research/SKILL.md")).unwrap();
    assert!(content.contains("type: runtime_capability"));
    assert!(content.contains("name: web-search"));
    assert!(content.ends_with("Use WebSearch and TeamCreate for this work."));
    let manifest: MaterializationManifest = serde_json::from_slice(
        &fs::read(
            revision_root(&service.store_root, "foreign-pack", &record.revision)
                .join("manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        manifest
            .files
            .iter()
            .any(|file| { file.path == "skills/research/SKILL.md" && file.generated })
    );
    assert!(
        manifest
            .files
            .iter()
            .any(|file| { file.path == "source/skills/research.md" && !file.generated })
    );
}

#[test]
fn command_materialization_rejects_generated_skill_above_descriptor_limit() {
    let service = PackageService::ephemeral("test").unwrap();
    let result = service
        .execute(PackageCommand::Install {
            request_id: "oversized-command".to_string(),
            package_id: "oversized-command-pack".to_string(),
            snapshot: PackageSnapshot {
                source_name: "oversized-command-pack".to_string(),
                entries: vec![PackageSnapshotEntry {
                    path: "skills/research.md".to_string(),
                    bytes: vec![b'a'; MAX_SOURCE_FILE_BYTES],
                    executable: false,
                }],
            },
        })
        .unwrap();

    assert!(!result.success);
    assert!(
        result
            .message
            .contains("generated command-style Skill exceeds descriptor file size limit")
    );
    assert!(
        !service
            .catalog()
            .packages
            .contains_key("oversized-command-pack")
    );
}

#[test]
fn command_skill_names_use_canonical_normalization() {
    let service = PackageService::ephemeral("test").unwrap();
    let record = service
        .execute(PackageCommand::Install {
            request_id: "command-normalization".to_string(),
            package_id: "command-normalization-pack".to_string(),
            snapshot: PackageSnapshot {
                source_name: "command-normalization-pack".to_string(),
                entries: [
                    "repo.review.md",
                    "release check.md",
                    "foo__bar.md",
                    "123.md",
                    "true.md",
                ]
                .into_iter()
                .map(|name| PackageSnapshotEntry {
                    path: format!("skills/{name}"),
                    bytes: b"Command body.".to_vec(),
                    executable: false,
                })
                .collect(),
            },
        })
        .unwrap()
        .package
        .unwrap();

    assert_eq!(
        record
            .exports
            .iter()
            .map(|export| export.skill_id.as_str())
            .collect::<Vec<_>>(),
        vec!["123", "foo-bar", "release-check", "repo-review", "true"]
    );

    let lease = service.acquire("command-normalization-pack").unwrap();
    for skill_id in ["123", "true"] {
        let document = fs::read_to_string(
            lease
                .content_root()
                .join(format!("skills/{skill_id}/SKILL.md")),
        )
        .unwrap();
        let metadata = parse_skill_metadata(
            &document,
            &Path::new(skill_id).join("SKILL.md"),
            SkillScope::Installed,
        )
        .unwrap();
        assert_eq!(metadata.id, skill_id);
    }
}
