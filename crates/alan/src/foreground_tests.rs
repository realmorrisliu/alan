use super::*;
use alan_agent_protocol::InputIntent;
use alan_tui::composer::{Composer, HistoryEntry, load_history};

fn config(store: &alan_os_host::SystemStorePaths) -> alan_tui::FileBackedRunConfig {
    interactive_config(
        alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
            alan_kernel::Namespace::new(),
        ))),
        Some("model".into()),
        Some(PathBuf::from("/project-candidate")),
        store,
    )
    .unwrap()
}

#[test]
fn foreground_history_is_product_store_owned_and_restarts_typed() {
    let dir = tempfile::tempdir().unwrap();
    let product = alan_os_host::SystemStorePaths::from_data_dir(dir.path()).unwrap();
    let isolated =
        alan_os_host::SystemStorePaths::from_data_dir(&dir.path().join("other-host")).unwrap();
    let first = config(&product);
    assert_eq!(first.agent_path, "/agent/root");
    assert_eq!(first.effective_model.as_deref(), Some("model"));
    assert_eq!(
        first.project_candidate,
        Some(PathBuf::from("/project-candidate"))
    );
    assert!(first.host_file_completion_root.is_none());
    assert!(first.require_interactive_terminal);
    let path = first
        .history_path
        .expect("shipped interactive history configured");
    assert_eq!(
        path,
        product
            .service("shell-ui")
            .unwrap()
            .join("composer-history")
    );
    assert_ne!(Some(path.clone()), config(&isolated).history_path);
    let entries = [
        HistoryEntry {
            body: "!literal agent\n  ".into(),
            intent: InputIntent::Agent,
        },
        HistoryEntry {
            body: "printf '%s' 'a b'\n  ".into(),
            intent: InputIntent::Command,
        },
    ];
    let mut composer = Composer::with_history(load_history(&path, 1000), Some(path));
    for entry in &entries {
        composer.remember_input(&entry.body, entry.intent);
        composer.remember_input(&entry.body, entry.intent);
    }
    let restarted_path = config(&product).history_path.unwrap();
    assert_eq!(load_history(&restarted_path, 1000), entries);
    assert!(load_history(&config(&isolated).history_path.unwrap(), 1000).is_empty());
}

#[test]
fn connection_configured_tracks_root_model_presence() {
    let dir = tempfile::tempdir().unwrap();
    let product = alan_os_host::SystemStorePaths::from_data_dir(dir.path()).unwrap();
    let configured = config(&product);
    assert_eq!(configured.connection_configured, Some(true));

    let unconfigured = interactive_config(
        alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
            alan_kernel::Namespace::new(),
        ))),
        None,
        None,
        &product,
    )
    .unwrap();
    assert_eq!(unconfigured.connection_configured, Some(false));
}

#[test]
fn should_auto_mount_cwd_excludes_root_home_and_tmp_roots_but_allows_project_dirs() {
    assert!(!should_auto_mount_cwd(Path::new("/")));
    assert!(!should_auto_mount_cwd(Path::new("/tmp")));
    assert!(!should_auto_mount_cwd(&std::env::temp_dir()));
    if let Some(home) = dirs::home_dir() {
        assert!(!should_auto_mount_cwd(&home));
    }
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("my-project");
    std::fs::create_dir_all(&project).unwrap();
    assert!(should_auto_mount_cwd(&project));
    assert!(!should_auto_mount_cwd(&temp.path().join("missing-dir")));
}

#[test]
fn interactive_config_defaults_update_notice_to_none_and_passive_check_uses_system_store() {
    let dir = tempfile::tempdir().unwrap();
    let product = alan_os_host::SystemStorePaths::from_data_dir(dir.path()).unwrap();
    let cfg = config(&product);
    assert!(cfg.update_notice.is_none());
    assert_eq!(
        cfg.release_channel.as_deref(),
        Some(cli::update::current_binary_channel().as_str())
    );
    assert!(cli::update::prepare_passive_tui_update_notice(&product, false).is_none());
    assert_eq!(
        cli::update::passive_check::passive_check_cache_path(&product).unwrap(),
        product.service("updater").unwrap().join("last-check.json")
    );

    cli::update::write_configured_channel(&product, cli::update::ReleaseChannel::Beta).unwrap();
    assert_eq!(config(&product).release_channel.as_deref(), Some("beta"));
}

fn init_fake_git_repo(dir: &Path, remote_url: &str) {
    let git_dir = dir.join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(
        git_dir.join("config"),
        format!("[remote \"origin\"]\n\turl = {remote_url}\n"),
    )
    .unwrap();
}

fn mock_boot_config() -> HostBootConfig {
    HostBootConfig::ephemeral(
        alan_agent_engine::AgentProcessConfig::default(),
        alan_agent_engine::LlmClient::new(alan_llm::MockLlmProvider::new()),
        alan_agent_engine::ToolRegistry::new(),
    )
}

#[test]
fn foreground_startup_loads_read_only_ledger_for_bound_dir_only() {
    use alan_agent_engine::{
        InboxDigestEntry, InboxDigestStatus, WorkstreamPersistence, WorkstreamRecord,
        bind_project_dir, complete_workstream, ledger_path, mark_inbox_digest_digested,
        upsert_workstream, write_inbox_digest,
    };

    let temp = tempfile::tempdir().unwrap();
    let system =
        alan_os_host::SystemStorePaths::from_root(&temp.path().join("System Store")).unwrap();
    let projects_root = system.projects().unwrap();

    let bound_repo = temp.path().join("primary/alan");
    let foreign_repo = temp.path().join("foreign/alan");
    let unbound_repo = temp.path().join("unbound");
    std::fs::create_dir_all(&bound_repo).unwrap();
    std::fs::create_dir_all(&foreign_repo).unwrap();
    std::fs::create_dir_all(&unbound_repo).unwrap();
    init_fake_git_repo(&bound_repo, "git@github.com:example/alan.git");
    init_fake_git_repo(&foreign_repo, "git@github.com:example/alan.git");

    let binding = bind_project_dir(&projects_root, &bound_repo, Some("grant-alan".into())).unwrap();

    // Even before ledger.json exists, read-only load does not create ledger.json.
    let initial_cfg =
        apply_project_startup_context(mock_boot_config(), &projects_root, Some(&bound_repo), false)
            .unwrap();
    assert!(initial_cfg.project_startup_context().is_some());
    assert!(
        !ledger_path(&projects_root, &binding.project_key)
            .unwrap()
            .exists()
    );

    // Populate a persistent idle stream, an archived one-off stream, and read/unread inbox entries.
    let persistent = WorkstreamRecord::new(
        "ui-polish",
        "Refine status bar",
        WorkstreamPersistence::Persistent,
    );
    upsert_workstream(&projects_root, &binding.project_key, persistent).unwrap();
    complete_workstream(
        &projects_root,
        &binding.project_key,
        "ui-polish",
        Some("ckpt://1".into()),
        Some("rollouts/1.jsonl".into()),
    )
    .unwrap();

    let one_off = WorkstreamRecord::new("quick-fix", "Fix typo", WorkstreamPersistence::OneOff);
    upsert_workstream(&projects_root, &binding.project_key, one_off).unwrap();
    complete_workstream(
        &projects_root,
        &binding.project_key,
        "quick-fix",
        None,
        None,
    )
    .unwrap();

    write_inbox_digest(
        &projects_root,
        &binding.project_key,
        InboxDigestEntry::new(
            "wf-unread",
            "nightly",
            None,
            InboxDigestStatus::Ok,
            "Nightly checks passed",
            "/tmp/runs.ndjson",
        ),
    )
    .unwrap();
    write_inbox_digest(
        &projects_root,
        &binding.project_key,
        InboxDigestEntry::new(
            "wf-read",
            "weekly",
            None,
            InboxDigestStatus::Ok,
            "Already acknowledged",
            "/tmp/runs.ndjson",
        ),
    )
    .unwrap();
    mark_inbox_digest_digested(&projects_root, &binding.project_key, "wf-read").unwrap();

    let ledger_before =
        std::fs::read(ledger_path(&projects_root, &binding.project_key).unwrap()).unwrap();

    // 1. Bound directory loads read-only ProjectLedger + unread inbox digests.
    let bound_cfg =
        apply_project_startup_context(mock_boot_config(), &projects_root, Some(&bound_repo), false)
            .unwrap();
    assert!(!bound_cfg.ephemeral_context());
    let startup = bound_cfg
        .project_startup_context()
        .expect("bound directory must load ProjectStartupContext");
    assert_eq!(startup.store.project_key, binding.project_key);
    assert_eq!(startup.store.grant_ref.as_deref(), Some("grant-alan"));
    assert_eq!(startup.ledger.active_streams().len(), 1);
    assert_eq!(startup.ledger.active_streams()[0].name, "ui-polish");
    assert_eq!(startup.unread_inbox.len(), 1);
    assert_eq!(startup.unread_inbox[0].run_id, "wf-unread");
    let rendered = startup.render_read_only_context().unwrap();
    assert!(rendered.contains("authoritative=\"false\" read_only=\"true\""));
    assert!(rendered.contains("ui-polish"));
    assert!(rendered.contains("Nightly checks passed"));
    assert!(!rendered.contains("Already acknowledged"));

    // Ledger file bytes were not mutated by startup load.
    let ledger_after =
        std::fs::read(ledger_path(&projects_root, &binding.project_key).unwrap()).unwrap();
    assert_eq!(ledger_before, ledger_after);

    // 2. Foreign checkout with same Git remote and basename NEVER auto-binds.
    let foreign_cfg = apply_project_startup_context(
        mock_boot_config(),
        &projects_root,
        Some(&foreign_repo),
        false,
    )
    .unwrap();
    assert!(foreign_cfg.project_startup_context().is_none());

    // 3. Unbound directory does nothing.
    let unbound_cfg = apply_project_startup_context(
        mock_boot_config(),
        &projects_root,
        Some(&unbound_repo),
        false,
    )
    .unwrap();
    assert!(unbound_cfg.project_startup_context().is_none());

    // 4. --ephemeral skips Project Ledger loading completely.
    let ephemeral_cfg = apply_project_startup_context(
        mock_boot_config().with_ephemeral_rollout(true),
        &projects_root,
        Some(&bound_repo),
        true,
    )
    .unwrap();
    assert!(ephemeral_cfg.ephemeral_context());
    assert!(ephemeral_cfg.project_startup_context().is_none());

    // Even calling with_project_startup_context directly on an ephemeral boot config stays None.
    let forced_ephemeral = mock_boot_config()
        .with_ephemeral_rollout(true)
        .with_project_startup_context(Some(startup.clone()));
    assert!(forced_ephemeral.ephemeral_context());
    assert!(forced_ephemeral.project_startup_context().is_none());

    // 5. --no-resume-update preserves Project Ledger loading and !ephemeral_context.
    let no_resume_cfg = apply_project_startup_context(
        mock_boot_config().with_no_resume_update(true),
        &projects_root,
        Some(&bound_repo),
        false,
    )
    .unwrap();
    assert!(no_resume_cfg.no_resume_update());
    assert!(!no_resume_cfg.ephemeral_context());
    assert!(no_resume_cfg.project_startup_context().is_some());
}

#[tokio::test]
async fn boot_foreground_accepts_project_startup_context_and_respects_ephemeral() {
    use alan_agent_engine::{ProjectStartupContext, bind_project_dir};

    let temp = tempfile::tempdir().unwrap();
    let projects_root = temp.path().join("System Store/services/projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let binding = bind_project_dir(&projects_root, &repo, None).unwrap();
    let startup = ProjectStartupContext::load_read_only(&projects_root, &binding).unwrap();

    let host_with_ctx = mock_boot_config()
        .with_project_startup_context(Some(startup.clone()))
        .boot_foreground()
        .await
        .unwrap();
    host_with_ctx.shutdown().await.unwrap();

    let host_ephemeral = mock_boot_config()
        .with_ephemeral_rollout(true)
        .with_project_startup_context(Some(startup))
        .boot_foreground()
        .await
        .unwrap();
    host_ephemeral.shutdown().await.unwrap();
}

#[test]
fn oneshot_grant_cwd_resolves_grants_subdirs_and_fails_closed_on_ungranted_cwd() {
    use alan_os_host::SavedProjectGrantStore;
    use alan_service_manager::HostMountAccess;

    let temp = tempfile::tempdir().unwrap();
    let store = SavedProjectGrantStore::from_path(temp.path().join("grants.json"));
    let repo = temp.path().join("repo");
    let sub = repo.join("crates/foo");
    let docs = temp.path().join("docs");
    let ungranted = temp.path().join("ungranted");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::create_dir_all(&ungranted).unwrap();

    let canonical_repo = std::fs::canonicalize(&repo).unwrap();
    let canonical_sub = std::fs::canonicalize(&sub).unwrap();
    let canonical_docs = std::fs::canonicalize(&docs).unwrap();

    store
        .save_grant("docs", &docs, HostMountAccess::ReadOnly, None, Vec::new())
        .unwrap();
    store
        .save_grant("repo", &repo, HostMountAccess::ReadWrite, None, Vec::new())
        .unwrap();

    // No grants and no explicit --cwd: stays None so non-workspace prompts work.
    assert_eq!(
        oneshot_cwd::resolve_oneshot_grant_cwd(Some(&store), &[], None, Some(&ungranted)).unwrap(),
        None
    );

    // Explicit --cwd without --grant fails closed and suggests the matching saved grant.
    let err = oneshot_cwd::resolve_oneshot_grant_cwd(Some(&store), &[], Some(&sub), None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--grant repo"), "{err}");
    assert!(!err.contains("attach with the TTY renderer"), "{err}");

    // Explicit --cwd with no saved grant suggests `alan host grant save` + `--grant`.
    let err_new = oneshot_cwd::resolve_oneshot_grant_cwd(Some(&store), &[], Some(&ungranted), None)
        .unwrap_err()
        .to_string();
    assert!(err_new.contains("alan host grant save"), "{err_new}");
    assert!(err_new.contains("--grant <id>"), "{err_new}");

    // --grant with unrelated ambient cwd prefers the ReadWrite grant root.
    let resolved_rw = oneshot_cwd::resolve_oneshot_grant_cwd(
        Some(&store),
        &["docs".to_string(), "repo".to_string()],
        None,
        Some(&ungranted),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        resolved_rw.namespace_path,
        PathBuf::from("/mnt/grants/repo")
    );
    assert_eq!(resolved_rw.host_path, canonical_repo);

    // --grant with ambient cwd inside grant maps to subdirectory.
    let resolved_ambient = oneshot_cwd::resolve_oneshot_grant_cwd(
        Some(&store),
        &["repo".to_string()],
        None,
        Some(&sub),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        resolved_ambient.namespace_path,
        PathBuf::from("/mnt/grants/repo/crates/foo")
    );
    assert_eq!(resolved_ambient.host_path, canonical_sub);

    // --grant with explicit --cwd inside ReadOnly grant maps to that grant.
    let resolved_docs = oneshot_cwd::resolve_oneshot_grant_cwd(
        Some(&store),
        &["docs".to_string(), "repo".to_string()],
        Some(Path::new("/mnt/grants/docs")),
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        resolved_docs.namespace_path,
        PathBuf::from("/mnt/grants/docs")
    );
    assert_eq!(resolved_docs.host_path, canonical_docs);

    // --grant with explicit --cwd outside activated grants fails closed.
    assert!(
        oneshot_cwd::resolve_oneshot_grant_cwd(
            Some(&store),
            &["repo".to_string()],
            Some(&ungranted),
            None,
        )
        .is_err()
    );
}
