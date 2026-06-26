//! Dependency boundary (ADR-0025 D3): a file server depends on `alan-ap` plus
//! its own backend only — never on the kernel, another file server, or a client.
//! llmfs's backend is `alan-llm`; it must not reach into `alan-kernel`, another
//! `*fs`, or a UI/client crate.

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
fn llmfs_depends_on_alan_ap_and_its_backend_only() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("read llmfs Cargo.toml");
    let alan_deps: Vec<String> = declared_dependencies(&manifest)
        .into_iter()
        .filter(|d| d.starts_with("alan-"))
        .collect();

    // The protocol plus its backend (alan-llm) — nothing else among Alan crates.
    assert_eq!(
        alan_deps,
        vec!["alan-ap".to_string(), "alan-llm".to_string()],
        "llmfs is a file server: alan-ap + its alan-llm backend only (ADR-0025 D3); found {alan_deps:?}"
    );
}
