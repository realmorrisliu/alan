use std::fs;
use std::path::Path;

#[test]
fn manifest_excludes_adapter_renderer_and_executor_dependencies() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("read alan-kernel manifest");

    for forbidden in [
        "alan-runtime",
        "alan-protocol",
        "axum",
        "ratatui",
        "crossterm",
        "reqwest",
        "tokio",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "alan-kernel must not depend on {forbidden}"
        );
    }
}
