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
