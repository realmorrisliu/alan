//! Dependency boundary (introduce-alan-shell §2.2, ADR-0025 D3): the shell is a
//! client and depends on `alan-ap` only among Alan crates — never a server or
//! backend (kernel, agentfs, llmfs, agent-engine, …). A front-end reads files,
//! writes `ctl`, and watches streams; it must not link the things it talks to.
//! (alan-kernel appears only as a dev-dependency for tests, not in `[dependencies]`.)

fn declared_dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line == "[dependencies]";
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let head = line.split('=').next().unwrap_or("").trim();
        let name = head.split('.').next().unwrap_or("").trim();
        if !name.is_empty() {
            deps.push(name.to_string());
        }
    }
    deps
}

#[test]
fn shell_depends_only_on_alan_ap_among_alan_crates() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("read shell Cargo.toml");

    let alan_deps: Vec<String> = declared_dependencies(&manifest)
        .into_iter()
        .filter(|d| d.starts_with("alan-"))
        .collect();

    assert_eq!(
        alan_deps,
        vec!["alan-ap".to_string()],
        "the shell is an aP-only client (ADR-0025 D3); found Alan deps {alan_deps:?}"
    );
}
