//! Dependency boundary (ADR-0025): `FileLlmProvider` is a *client* of llmfs — it
//! talks aP and implements the `alan-llm` trait, so it depends on `alan-ap` +
//! `alan-llm` only. It must NOT link the `alan-llmfs` server crate (it reaches it
//! over aP, like any client), nor the kernel, another file server, or a UI.

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
fn file_provider_depends_on_ap_and_llm_only() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("read llmfs-client Cargo.toml");
    let alan_deps: Vec<String> = declared_dependencies(&manifest)
        .into_iter()
        .filter(|d| d.starts_with("alan-"))
        .collect();

    assert_eq!(
        alan_deps,
        vec!["alan-ap".to_string(), "alan-llm".to_string()],
        "FileLlmProvider is an aP client of llmfs: alan-ap + alan-llm only, not the server; found {alan_deps:?}"
    );
}
