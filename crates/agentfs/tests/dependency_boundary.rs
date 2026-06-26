//! Dependency boundary (introduce-alan-kernel-runtime §10, ADR-0025 D3): a file
//! server depends on `alan-ap` plus its own backend only — never on the kernel,
//! another file server, or a client. agentfs's backend is the agent engine /
//! protocol; it must not reach down into `alan-kernel` or sideways into another
//! `*fs`/client crate. This keeps the kernel free of agent knowledge (the leak
//! would otherwise show up as agentfs pulling the kernel in).

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
fn agentfs_does_not_depend_on_the_kernel_other_servers_or_clients() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("read agentfs Cargo.toml");
    let deps = declared_dependencies(&manifest);

    let forbidden = [
        "alan-kernel",
        "alan-llmfs",
        "alan-binfs",
        "alan-memfs",
        "alan-shell",
        "alan-terminal-ui",
    ];
    for crate_name in forbidden {
        assert!(
            !deps.iter().any(|d| d == crate_name),
            "agentfs is a file server: it must not depend on `{crate_name}` (ADR-0025 D3)"
        );
    }
}
