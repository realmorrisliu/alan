//! Dependency boundary (substrate §8.3, ADR-0025 D1): `alan-kernel` depends only
//! on `alan-ap` among Alan crates, and never on the agent runtime, the legacy
//! session protocol, provider clients, memory stores, sandbox backends, or
//! renderers. Enforced structurally so "the kernel changes least" is a fact, not
//! a hope — and so the retired V1 ontology cannot creep back in.

use std::path::Path;

/// Parse the crate names declared in the `[dependencies]` table of a Cargo
/// manifest. Handles both `name = { ... }` and `name.workspace = true` forms.
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
        // The dependency name is the token before `=` or the first `.`.
        let head = line.split('=').next().unwrap_or("").trim();
        let name = head.split('.').next().unwrap_or("").trim();
        if !name.is_empty() {
            deps.push(name.to_string());
        }
    }
    deps
}

#[test]
fn kernel_depends_only_on_alan_ap_among_alan_crates() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("read kernel Cargo.toml");

    let alan_deps: Vec<String> = declared_dependencies(&manifest)
        .into_iter()
        .filter(|d| d.starts_with("alan-"))
        .collect();

    assert_eq!(
        alan_deps,
        vec!["alan-ap".to_string()],
        "alan-kernel must depend only on alan-ap among Alan crates (ADR-0025 D1); found {alan_deps:?}"
    );
}

#[test]
fn kernel_does_not_declare_runtime_provider_or_renderer_crates() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("read kernel Cargo.toml");
    let deps = declared_dependencies(&manifest);

    // Crates whose presence would mean the kernel learned about agents, the
    // legacy protocol, providers, or a renderer.
    let forbidden = [
        "alan-runtime",
        "alan-agent-engine",
        "alan-protocol",
        "alan-agent-protocol",
        "alan-llm",
        "alan-tools",
        "reqwest",
        "ratatui",
        "crossterm",
    ];
    for crate_name in forbidden {
        assert!(
            !deps.iter().any(|d| d == crate_name),
            "alan-kernel must not depend on `{crate_name}` (substrate §8.3)"
        );
    }
}

#[test]
fn kernel_source_does_not_reference_the_runtime_or_legacy_protocol() {
    // A leak would show up as a `use` of these crates or the retired ontology.
    let forbidden_tokens = [
        "alan_runtime",
        "alan_protocol",
        "alan_agent_engine",
        "alan_agent_protocol",
        "agent_capability",
        "ViewModel",
        "renderer_host",
    ];

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    visit_rs_files(&src, &mut |path, contents| {
        for token in forbidden_tokens {
            if contents.contains(token) {
                offenders.push(format!("{}: {token}", path.display()));
            }
        }
    });

    assert!(
        offenders.is_empty(),
        "kernel source leaks retired/runtime concepts: {offenders:?}"
    );
}

fn visit_rs_files(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_rs_files(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(contents) = std::fs::read_to_string(&path)
        {
            f(&path, &contents);
        }
    }
}
