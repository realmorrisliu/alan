use super::*;

#[test]
fn prepare_fileserver_launch_grant_requires_github_fileserver_identity_and_network_for_host_token()
{
    let granted = prepare_fileserver_launch_grant(
        "github-fileserver",
        "github",
        FileserverMountAccess::ReadOnly,
        true,
        TrustPosture::Governed,
        None,
        None,
    );
    assert!(granted.allow_host_gh_token);

    for (pkg, svc, net) in [
        ("github-fileserver", "github", false),
        ("third-party-pack", "github", true),
        ("github-fileserver", "other", true),
    ] {
        let denied = prepare_fileserver_launch_grant(
            pkg,
            svc,
            FileserverMountAccess::ReadOnly,
            net,
            TrustPosture::Governed,
            None,
            None,
        );
        assert!(
            !denied.allow_host_gh_token,
            "expected allow_host_gh_token=false for ({pkg}, {svc}, net={net})"
        );
    }
}

#[test]
fn prepare_fileserver_launch_grant_requires_gdrive_fileserver_identity_and_network_for_auth() {
    let granted = prepare_fileserver_launch_grant(
        "gdrive-fileserver",
        "gdrive",
        FileserverMountAccess::ReadWrite,
        true,
        TrustPosture::Governed,
        None,
        None,
    );
    let auth_env = granted
        .runtime_env
        .get("ALAN_GDRIVEFS_AUTH_STORAGE")
        .expect("gdrive-fileserver with allow_network=true must set ALAN_GDRIVEFS_AUTH_STORAGE");
    let auth_parent = Path::new(auth_env).parent().unwrap();
    assert!(
        granted.host_mounts.iter().any(|mount| {
            mount.namespace_path == Path::new("/host/gdrive-auth")
                && mount.host_path == auth_parent
                && mount.access == ReifiedMountAccess::ReadWrite
        }),
        "expected ReadWrite /host/gdrive-auth mount in {:?}",
        granted.host_mounts
    );

    for (pkg, svc, net) in [
        ("gdrive-fileserver", "gdrive", false),
        ("third-party-pack", "gdrive", true),
        ("gdrive-fileserver", "other", true),
    ] {
        let denied = prepare_fileserver_launch_grant(
            pkg,
            svc,
            FileserverMountAccess::ReadWrite,
            net,
            TrustPosture::Governed,
            None,
            None,
        );
        assert!(
            !denied
                .runtime_env
                .contains_key("ALAN_GDRIVEFS_AUTH_STORAGE"),
            "expected no ALAN_GDRIVEFS_AUTH_STORAGE for ({pkg}, {svc}, net={net})"
        );
        assert!(
            !denied
                .host_mounts
                .iter()
                .any(|mount| mount.namespace_path == Path::new("/host/gdrive-auth")),
            "expected no /host/gdrive-auth mount for ({pkg}, {svc}, net={net})"
        );
    }

    let temp = tempfile::tempdir().unwrap();
    let explicit = temp.path().join("custom/gdrive/auth.json");
    std::fs::create_dir_all(explicit.parent().unwrap()).unwrap();
    let (resolved_file, resolved_dir) = resolve_gdrive_auth_storage_paths(
        Some(explicit.to_str().unwrap()),
        Some(Path::new("/Users/unused")),
        None,
    )
    .unwrap();
    let expected_dir = canonical_or_owned(explicit.parent().unwrap());
    assert_eq!(resolved_dir, expected_dir);
    assert_eq!(resolved_file, expected_dir.join("auth.json"));
}

#[test]
fn preinstalled_github_fileserver_snapshot_matches_source_and_seeds_catalog_only() {
    let pkg_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/github-fileserver");
    let embedded = preinstalled_github_fileserver_snapshot().unwrap();
    #[cfg(unix)]
    {
        let from_dir = PackageSnapshot::from_directory(&pkg_dir).unwrap();
        assert_eq!(embedded, from_dir);
    }
    assert!(
        alan_agent_engine::skills::preinstalled_skill_package_ids()
            .all(|id| id != GITHUB_FILESERVER_PACKAGE_ID)
    );

    let service = PackageService::ephemeral().unwrap();
    seed_preinstalled_packages(&service).unwrap();

    let record = service.resolve(GITHUB_FILESERVER_PACKAGE_ID).unwrap();
    assert_eq!(record.kind, PackageKind::Preinstalled);
    assert_eq!(record.export_kind, PackageExportKind::Fileserver);
    assert_eq!(
        record.fileserver_export.as_ref().unwrap().service_id,
        "github"
    );
    assert!(
        service
            .root_binding(GITHUB_FILESERVER_PACKAGE_ID)
            .unwrap()
            .is_none(),
        "preinstalled github-fileserver must seed catalog-only with no Root binding"
    );

    let listed = service
        .execute(crate::PackageCommand::List {
            request_id: "list-preinstalled".to_string(),
        })
        .unwrap();
    assert!(listed.success);
    let catalog = listed.catalog.unwrap();
    let entry = catalog.packages.get(GITHUB_FILESERVER_PACKAGE_ID).unwrap();
    assert_eq!(entry.kind, PackageKind::Preinstalled);
    assert_eq!(entry.export_kind, PackageExportKind::Fileserver);

    // Enabling after preinstalled seeding works without a local source checkout.
    let binding = service
        .bind_for_root(
            GITHUB_FILESERVER_PACKAGE_ID,
            RootBindingMode::Active,
            Some(crate::RootFileserverActivation {
                access: FileserverMountAccess::ReadWrite,
                allow_network: true,
                host_grant_ids: Vec::new(),
            }),
        )
        .unwrap();
    assert_eq!(binding.mode, RootBindingMode::Active);
    assert!(binding.fileserver.as_ref().unwrap().allow_network);
}

#[test]
fn seed_preinstalled_packages_preserves_operator_installed_github_fileserver() {
    let pkg_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/github-fileserver");
    let snapshot = PackageSnapshot::from_directory(&pkg_dir).unwrap();
    let service = PackageService::ephemeral().unwrap();
    let installed = service
        .execute(crate::PackageCommand::Install {
            request_id: "operator-install-gh".to_string(),
            package_id: GITHUB_FILESERVER_PACKAGE_ID.to_string(),
            snapshot,
        })
        .unwrap();
    assert!(installed.success, "{}", installed.message);

    seed_preinstalled_packages(&service).unwrap();
    let record = service.resolve(GITHUB_FILESERVER_PACKAGE_ID).unwrap();
    assert_eq!(record.kind, PackageKind::Installed);
}
