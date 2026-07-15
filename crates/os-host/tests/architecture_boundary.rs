#[test]
fn host_has_no_service_supervision_or_fixed_composition() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!root.join("src/composition.rs").exists());
    let mut files = Vec::new();
    collect_rust_sources(&root.join("src"), &mut files);
    let source = files
        .into_iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect::<String>();
    for forbidden in [
        "ProcFs::new",
        "SrvFs::new",
        "spawn_with_namespace_environment",
        "FixedComposition",
        "FixedBootConfig",
    ] {
        assert!(!source.contains(forbidden), "Host retained `{forbidden}`");
    }
    assert!(source.contains("ServiceManager::boot"));
}

#[test]
fn host_owns_secret_materialization_while_connection_service_owns_metadata() {
    let host_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let service_root = host_root.parent().unwrap().join("service-manager/src");
    let mut service_files = Vec::new();
    collect_rust_sources(&service_root, &mut service_files);
    let service_source = service_files
        .into_iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect::<String>();
    for forbidden in [
        "struct SecretStore",
        "apply_profile_to_config",
        "credentials_dir",
    ] {
        assert!(
            !service_source.contains(forbidden),
            "Connection Service retained Host credential authority through `{forbidden}`"
        );
    }

    let host_adapter = std::fs::read_to_string(host_root.join("src/secret_store.rs")).unwrap();
    for required in [
        "struct SecretStore",
        "apply_profile_to_config",
        "secrets.toml",
    ] {
        assert!(
            host_adapter.contains(required),
            "Host credential adapter is missing `{required}`"
        );
    }
}

fn collect_rust_sources(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
