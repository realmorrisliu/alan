#[test]
fn fixed_composition_is_located_for_service_manager_deletion() {
    assert_eq!(
        alan_os_host::TEMPORARY_FIXED_COMPOSITION_SUCCESSOR,
        "implement-minimal-service-manager"
    );
    let source =
        std::fs::read_to_string(format!("{}/src/composition.rs", env!("CARGO_MANIFEST_DIR")))
            .unwrap();
    assert!(source.contains("Temporary fixed Alan OS boot composition"));
    assert!(source.contains("ProcFs::new"));
    assert!(source.contains("SrvFs::new"));
}
