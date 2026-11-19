use super::*;
use crate::runtime::memory_domains::{DomainRecallScope, MemoryDomain};
use tempfile::TempDir;

#[test]
fn identity_query_prefers_user_memory() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    fs::write(
        memory_dir.join("USER.md"),
        "# User Memory\nName: Morris Liu\n",
    )
    .unwrap();

    let bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text("Who am I?")]),
    )
    .expect("expected recall bundle");

    assert!(bundle.contains("/memory/USER.md"));
    assert!(bundle.contains("Morris Liu"));
}

#[test]
fn continuity_query_picks_handoff_and_episodic_record() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    fs::write(
        memory_dir.join("handoffs/LATEST.md"),
        "# Latest Handoff\nWe were refining the recall router.\n",
    )
    .unwrap();
    fs::create_dir_all(memory_dir.join("episodic/2026/04/15")).unwrap();
    fs::write(
        memory_dir.join("episodic/2026/04/15/sess-1.md"),
        "# Agent Process Activity\nRecall router work in progress.\n",
    )
    .unwrap();

    let bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text(
            "What were we doing in the previous Agent Process?",
        )]),
    )
    .expect("expected recall bundle");

    assert!(bundle.contains("/memory/handoffs/LATEST.md"));
    assert!(bundle.contains("/memory/episodic/2026/04/15/sess-1.md"));
}

#[test]
fn domain_query_can_pick_relevant_topic_page() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    fs::write(
        memory_dir.join("topics/memory-router.md"),
        "# Memory Router\nArchitecture decision: use lexical recall over pure-text files.\n",
    )
    .unwrap();

    let bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text(
            "What is the architecture decision for the memory router?",
        )]),
    )
    .expect("expected recall bundle");

    assert!(bundle.contains("/memory/topics/memory-router.md"));
    assert!(bundle.contains("lexical recall"));
}

#[test]
fn recent_query_fallback_ignores_memory_root_token_overlap() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("yesterday-root//memory");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    fs::create_dir_all(memory_dir.join("episodic/2026/04/16")).unwrap();
    for index in 1..=4 {
        fs::write(
            memory_dir.join(format!("topics/recent-match-{index}.md")),
            format!("# Topic Note\nwe did document topic match {index}\n"),
        )
        .unwrap();
    }
    fs::write(
        memory_dir.join("daily/2026-04-16.md"),
        "## 2026-04-16\nALAN_RECENT_RECALL\n",
    )
    .unwrap();
    for index in 1..=4 {
        fs::write(
            memory_dir.join(format!("episodic/2026/04/16/process-{index}.md")),
            format!("# Agent Process Activity\nALAN_RECENT_RECALL_{index}\n"),
        )
        .unwrap();
    }

    let bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text("What did we do yesterday?")]),
    )
    .expect("expected recall bundle");

    assert!(bundle.contains("## Runtime Recall Bundle"));
    assert!(bundle.contains("/memory/daily/2026-04-16.md"));
    assert!(bundle.contains("/memory/episodic/2026/04/16/process-4.md"));
    assert!(bundle.contains("ALAN_RECENT_RECALL_4"));
    assert!(!bundle.contains("/memory/topics/recent-match-4.md"));
}

#[test]
fn pure_chinese_and_mixed_cjk_english_queries_recall_relevant_docs() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    fs::write(
        memory_dir.join("USER.md"),
        "# User Memory\n用户偏好：技术讨论优先使用中文，并默认采用 Rust 语言。\n",
    )
    .unwrap();
    fs::write(
        memory_dir.join("topics/cargo-errors.md"),
        "# Cargo 构建排查\n遇到 Rust 项目里的 Cargo 报错时，先检查 feature 和 lockfile。\n",
    )
    .unwrap();
    fs::write(
        memory_dir.join("topics/cooking.md"),
        "# 食谱\n番茄炒蛋只需要鸡蛋和番茄。\n",
    )
    .unwrap();

    let zh_bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text("用户偏好用哪种语言？")]),
    )
    .expect("chinese recall bundle");
    assert!(zh_bundle.contains("/memory/USER.md"));
    assert!(zh_bundle.contains("优先使用中文"));
    assert!(!zh_bundle.contains("番茄炒蛋"));

    let mixed_bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text("Rust 项目里的 Cargo 报错")]),
    )
    .expect("mixed cjk+english recall bundle");
    assert!(mixed_bundle.contains("/memory/topics/cargo-errors.md"));
    assert!(mixed_bundle.contains("Cargo 报错"));
    assert!(!mixed_bundle.contains("番茄炒蛋"));
}

#[test]
fn recall_scrubs_secrets_and_bounds_snippets_to_1200_chars() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    let long_body = format!(
        "# Deployment Token Notes\n{}\nDeploymentAnchorNote with sk-ant-secret1234567890 and Bearer super_secret_bearer_token\n",
        "historical context line\n".repeat(120)
    );
    fs::write(memory_dir.join("topics/deploy.md"), long_body).unwrap();

    let bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text("DeploymentAnchorNote")]),
    )
    .expect("expected scrubbed bundle");

    assert!(bundle.contains("DeploymentAnchorNote"));
    assert!(!bundle.contains("sk-ant-secret1234567890"));
    assert!(!bundle.contains("super_secret_bearer_token"));
    assert!(bundle.contains("[REDACTED"));

    let hits = search_memory_store(&memory_dir, "default", "DeploymentAnchorNote", None, 5);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.chars().count() <= MAX_FILE_RECALL_CHARS);
    assert!(!hits[0].snippet.contains("sk-ant-secret1234567890"));
    assert!(!hits[0].snippet.contains("super_secret_bearer_token"));
}

#[test]
fn domain_isolated_recall_never_leaks_across_project_or_app_domains() {
    let temp = TempDir::new().unwrap();
    let stores_root = temp.path().join("stores");
    let proj_alpha = MemoryDomain::Project("alpha".to_string());
    let proj_beta = MemoryDomain::Project("beta".to_string());
    let app_slowpage = MemoryDomain::App("slowpage".to_string());

    let alpha_dir = proj_alpha.resolve_store_dir(&stores_root).unwrap();
    let beta_dir = proj_beta.resolve_store_dir(&stores_root).unwrap();
    let app_dir = app_slowpage.resolve_store_dir(&stores_root).unwrap();
    crate::prompts::ensure_memory_store_layout_at(&alpha_dir).unwrap();
    crate::prompts::ensure_memory_store_layout_at(&beta_dir).unwrap();
    crate::prompts::ensure_memory_store_layout_at(&app_dir).unwrap();

    fs::write(
        alpha_dir.join("topics/secret-alpha.md"),
        "# Alpha\nAlphaDomainUniqueMarker\n",
    )
    .unwrap();
    fs::write(
        app_dir.join("topics/app-notes.md"),
        "# Slowpage\nSlowpageAppUniqueMarker\n",
    )
    .unwrap();

    // Querying from project:beta must NEVER return project:alpha or app:slowpage.
    let beta_scope = DomainRecallScope::ActiveOnly(proj_beta);
    let leaked = build_turn_recall_bundle_for_domains(
        &stores_root,
        &beta_scope,
        Some(&[crate::tape::ContentPart::text(
            "AlphaDomainUniqueMarker SlowpageAppUniqueMarker",
        )]),
        None,
    )
    .unwrap();
    assert!(leaked.is_none(), "cross-domain leak detected: {leaked:?}");

    // Explicit query against project:alpha succeeds while still hiding app:slowpage.
    let alpha_scope = DomainRecallScope::ActiveOnly(proj_alpha);
    let alpha_bundle = build_turn_recall_bundle_for_domains(
        &stores_root,
        &alpha_scope,
        Some(&[crate::tape::ContentPart::text(
            "AlphaDomainUniqueMarker SlowpageAppUniqueMarker",
        )]),
        None,
    )
    .unwrap()
    .expect("alpha bundle");
    assert!(alpha_bundle.contains("AlphaDomainUniqueMarker"));
    assert!(!alpha_bundle.contains("SlowpageAppUniqueMarker"));
}

#[test]
fn collect_markdown_files_recursive_prioritizes_newest_paths_under_cap() {
    let temp = TempDir::new().unwrap();
    let episodic_dir = temp.path().join("memory-store/episodic");
    fs::create_dir_all(episodic_dir.join("2026/04/15")).unwrap();
    fs::create_dir_all(episodic_dir.join("2026/04/16")).unwrap();
    fs::write(
        episodic_dir.join("2026/04/15/process-older.md"),
        "# Agent Process Activity\nolder\n",
    )
    .unwrap();
    fs::write(
        episodic_dir.join("2026/04/16/process-newer.md"),
        "# Agent Process Activity\nnewer\n",
    )
    .unwrap();

    let canonical_memory_root = fs::canonicalize(temp.path().join("memory-store")).unwrap();
    let collected = collect_markdown_files_recursive(&episodic_dir, &canonical_memory_root, 1);

    assert_eq!(
        collected,
        vec![episodic_dir.join("2026/04/16/process-newer.md")]
    );
}

#[cfg(unix)]
#[test]
fn continuity_query_skips_symlinked_episodic_directories() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    fs::write(
        memory_dir.join("handoffs/LATEST.md"),
        "# Latest Handoff\nWe were refining the recall router.\n",
    )
    .unwrap();
    fs::create_dir_all(memory_dir.join("episodic/2026/04")).unwrap();

    let external_dir = temp.path().join("external-episodic");
    fs::create_dir_all(&external_dir).unwrap();
    fs::write(
        external_dir.join("process-leak.md"),
        "# Agent Process Activity\nZebraRecallLeak\n",
    )
    .unwrap();
    symlink(&external_dir, memory_dir.join("episodic/2026/04/link-out")).unwrap();

    let bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text(
            "What were we doing in the previous Agent Process about ZebraRecallLeak?",
        )]),
    )
    .expect("expected recall bundle");

    assert!(!bundle.contains("ZebraRecallLeak"));
    assert!(!bundle.contains("process-leak.md"));
}

#[cfg(unix)]
#[test]
fn continuity_query_skips_handoff_paths_outside_memory_root() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();

    let external_handoffs = temp.path().join("external-handoffs");
    fs::create_dir_all(&external_handoffs).unwrap();
    fs::write(
        external_handoffs.join("LATEST.md"),
        "# Latest Handoff\nZebraHandoffLeak\n",
    )
    .unwrap();

    fs::remove_dir_all(memory_dir.join("handoffs")).unwrap();
    symlink(&external_handoffs, memory_dir.join("handoffs")).unwrap();

    let bundle = build_turn_recall_bundle(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text(
            "What were we doing in the previous Agent Process about ZebraHandoffLeak?",
        )]),
    );

    assert!(
        bundle.is_none()
            || bundle
                .as_deref()
                .is_some_and(|bundle| !bundle.contains("ZebraHandoffLeak"))
    );
}
