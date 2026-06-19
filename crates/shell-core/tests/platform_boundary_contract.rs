#[test]
fn shell_core_manifest_does_not_depend_on_platform_ui_or_hosting_crates() {
    let cargo_toml = include_str!("../Cargo.toml");
    let forbidden = [
        "AppKit",
        "SwiftUI",
        "GTK",
        "gtk",
        "Ghostty",
        "GhosttyKit",
        "axum",
        "cocoa",
        "objc",
        "winit",
    ];

    for needle in forbidden {
        assert!(
            !cargo_toml.contains(needle),
            "alan-shell-core must not depend on platform or hosting crate `{needle}`"
        );
    }
}
