use super::*;

#[test]
fn embedded_packages_match_directory_snapshots_and_seeded_revisions() {
    use alan_agent_engine::skills::{
        preinstalled_skill_package_ids, preinstalled_skill_package_sources,
    };
    let sources = preinstalled_skill_package_sources();
    assert_eq!(
        preinstalled_skill_package_ids().collect::<Vec<_>>(),
        sources
            .iter()
            .map(|source| source.package_id)
            .collect::<Vec<_>>()
    );
    let fixture = tempfile::tempdir().unwrap();
    let embedded_store = PackageService::ephemeral("test").unwrap();
    let directory_store = PackageService::ephemeral("test").unwrap();
    for source in sources {
        let root = fixture.path().join(source.source_name);
        for file in &source.files {
            assert!(!file.path.starts_with("tooling"));
            let target = root.join(file.path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, file.bytes).unwrap();
            #[cfg(unix)]
            if file.executable {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let embedded = PackageSnapshot::from_preinstalled(&source).unwrap();
        let directory = PackageSnapshot::from_directory(&root).unwrap();
        assert_eq!(embedded, directory);
        embedded_store
            .seed_preinstalled(source.package_id, embedded)
            .unwrap();
        directory_store
            .seed_preinstalled(source.package_id, directory)
            .unwrap();
        let left = embedded_store
            .cached_catalog()
            .packages
            .remove(source.package_id)
            .unwrap();
        let right = directory_store
            .cached_catalog()
            .packages
            .remove(source.package_id)
            .unwrap();
        assert_eq!(left.revision, right.revision);
        assert_eq!(
            left.materialized_fingerprint,
            right.materialized_fingerprint
        );
        assert_eq!(left.exports, right.exports);
        assert!(!left.exports.is_empty());
    }
}

#[test]
fn embedded_entries_use_the_same_snapshot_validation_boundary() {
    use alan_agent_engine::skills::{PreinstalledSkillFile, PreinstalledSkillPackageSource};
    for path in ["../escape", "/absolute", "a//b", "a/./b", ".git/config"] {
        let source = PreinstalledSkillPackageSource {
            package_id: "invalid",
            source_name: "invalid",
            files: vec![PreinstalledSkillFile {
                path: Path::new(path),
                bytes: b"ignored",
                executable: false,
            }],
        };
        assert!(
            PackageSnapshot::from_preinstalled(&source).is_err(),
            "{path}"
        );
    }
    let file = PreinstalledSkillFile {
        path: Path::new("SKILL.md"),
        bytes: b"body",
        executable: false,
    };
    for files in [
        Vec::new(),
        vec![file.clone(), file.clone()],
        vec![file; 4097],
    ] {
        let source = PreinstalledSkillPackageSource {
            package_id: "invalid",
            source_name: "invalid",
            files,
        };
        assert!(PackageSnapshot::from_preinstalled(&source).is_err());
    }
}
