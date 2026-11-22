use super::*;
use crate::cli::workflow::logs::{
    MAX_LOG_ENTRIES_PER_WORKFLOW, WorkflowRunLogEntry, append_run_log, read_run_logs,
};
use crate::cli::workflow::manifest::{
    WorkflowManifest, WorkflowStore, WorkflowTrustPosture, validate_workflow_id,
};
use crate::cli::workflow::schedule::{
    install_launchd_plist, render_cron_entry, render_launchd_plist,
};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub(super) fn write_mock_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn manifest_validation_enforces_ids_explicit_mounts_and_defaults() {
    let valid = WorkflowManifest {
        schema_version: 1,
        id: "daily-notes".to_string(),
        schedule: "0 9 * * 1-5".to_string(),
        prompt: "Summarize new inbox notes".to_string(),
        model_profile: None,
        mounts: vec!["notes".to_string()],
        trust: WorkflowTrustPosture::default(),
        json: true,
        timeout_secs: None,
        trigger_allowlist: None,
        release_gate: None,
    };
    assert_eq!(valid.trust, WorkflowTrustPosture::Governed);
    valid.validate().unwrap();

    // Reject path traversal & uppercase ids
    for bad_id in ["../evil", "Daily", "-bad", "bad-", "a/b", ""] {
        assert!(validate_workflow_id(bad_id).is_err(), "{bad_id}");
    }

    // Reject raw host paths in mounts
    for bad_mount in ["/Users/alice/notes", "~/notes", "../notes", "a/b"] {
        let mut m = valid.clone();
        m.mounts = vec![bad_mount.to_string()];
        assert!(m.validate().is_err(), "{bad_mount}");
    }

    // Reject unknown TOML fields
    let parsed: Result<WorkflowManifest, _> = toml::from_str(
        r#"
id = "x"
schedule = "every 10m"
prompt = "hi"
unexpected_field = true
"#,
    );
    assert!(parsed.is_err());

    // Reject empty prompt or invalid timeout
    let mut empty_prompt = valid.clone();
    empty_prompt.prompt = "   ".to_string();
    assert!(empty_prompt.validate().is_err());

    let mut zero_timeout = valid;
    zero_timeout.timeout_secs = Some(0);
    assert!(zero_timeout.validate().is_err());
}

#[test]
fn schedule_parsing_and_launchd_cron_rendering_cover_intervals_and_calendar_cron() {
    let temp = tempfile::tempdir().unwrap();
    let store = WorkflowStore::from_root(temp.path().join("workflows")).unwrap();
    let bin = Path::new("/usr/local/bin/alan");

    let interval_manifest = WorkflowManifest {
        schema_version: 1,
        id: "inbox-sweep".to_string(),
        schedule: "every 15m".to_string(),
        prompt: "Check inbox".to_string(),
        model_profile: None,
        mounts: vec![],
        trust: WorkflowTrustPosture::Governed,
        json: true,
        timeout_secs: Some(60),
        trigger_allowlist: None,
        release_gate: None,
    };
    let plist = render_launchd_plist(&interval_manifest, bin, &store).unwrap();
    assert!(plist.contains("<string>app.alanworks.workflow.inbox-sweep</string>"));
    assert!(plist.contains("<key>StartInterval</key>\n  <integer>900</integer>"));
    assert!(plist.contains("<key>RunAtLoad</key>\n  <false/>"));

    let cron_line = render_cron_entry(&interval_manifest, bin).unwrap();
    assert!(cron_line.contains("*/15 * * * * /usr/local/bin/alan workflow run inbox-sweep"));

    // Cron with weekday 7 (Sunday) + range + XML special chars in bin path
    let cron_manifest = WorkflowManifest {
        schema_version: 1,
        id: "weekly-digest".to_string(),
        schedule: "30 8 * * 0,7".to_string(),
        prompt: "Weekly digest".to_string(),
        model_profile: None,
        mounts: vec![],
        trust: WorkflowTrustPosture::Governed,
        json: true,
        timeout_secs: None,
        trigger_allowlist: None,
        release_gate: None,
    };
    let xml_bin = Path::new("/tmp/a&b/alan");
    let cron_plist = render_launchd_plist(&cron_manifest, xml_bin, &store).unwrap();
    assert!(cron_plist.contains("<string>/tmp/a&amp;b/alan</string>"));
    assert!(cron_plist.contains("<key>StartCalendarInterval</key>"));
    assert!(cron_plist.contains("<key>Minute</key>\n    <integer>30</integer>"));
    assert!(cron_plist.contains("<key>Hour</key>\n    <integer>8</integer>"));
    assert!(cron_plist.contains("<key>Weekday</key>\n    <integer>0</integer>"));
    // Deduplicated 0 and 7 -> single dict, not array
    assert!(
        !cron_plist.contains("<key>StartCalendarInterval</key>\n  <array>"),
        "{cron_plist}"
    );

    // Sub-minute interval works for launchd but fails cleanly for cron
    let fast_manifest = WorkflowManifest {
        schedule: "30s".to_string(),
        ..cron_manifest
    };
    assert!(render_launchd_plist(&fast_manifest, bin, &store).is_ok());
    assert!(render_cron_entry(&fast_manifest, bin).is_err());

    // Opt-in LaunchAgents installation writes safely to explicit home
    let fake_home = temp.path().join("home");
    let installed_path = install_launchd_plist("inbox-sweep", &plist, &fake_home).unwrap();
    assert_eq!(
        installed_path,
        fake_home.join("Library/LaunchAgents/app.alanworks.workflow.inbox-sweep.plist")
    );
    assert_eq!(std::fs::read_to_string(&installed_path).unwrap(), plist);
}

#[test]
fn run_logs_scrub_secrets_and_enforce_bounded_retention() {
    let temp = tempfile::tempdir().unwrap();
    let store = WorkflowStore::from_root(temp.path().join("workflows")).unwrap();

    for idx in 0..(MAX_LOG_ENTRIES_PER_WORKFLOW + 15) {
        let entry = WorkflowRunLogEntry {
            run_id: format!("run-{idx}"),
            workflow_id: "scrub-test".to_string(),
            started_at: "2026-11-19T00:00:00Z".to_string(),
            finished_at: "2026-11-19T00:00:01Z".to_string(),
            duration_ms: 42,
            exit_code: 0,
            status: "completed".to_string(),
            summary: format!(
                "run {idx} sk-proj-supersecret123456 AIzaSyAbCdEfGhIjKlMnOp \
                 Authorization: Bearer secret-bearer-token-999 api_key=rawsecret123"
            ),
            stderr_excerpt: "ghp_1234567890abcdef1234567890abcdef".to_string(),
            ephemeral: true,
            trust: "governed".to_string(),
            model_profile: None,
            tool_calls: 1,
            usage: None,
        };
        append_run_log(&store, &entry).unwrap();
    }

    let logs = read_run_logs(&store, "scrub-test", 500).unwrap();
    assert_eq!(logs.len(), MAX_LOG_ENTRIES_PER_WORKFLOW);
    assert_eq!(
        logs[0].run_id,
        format!("run-{}", MAX_LOG_ENTRIES_PER_WORKFLOW + 14)
    );

    let raw_file = std::fs::read_to_string(store.run_log_path("scrub-test").unwrap()).unwrap();
    for (idx, fixture) in [
        "sk-proj-supersecret123456",
        "AIzaSyAbCdEfGhIjKlMnOp",
        "secret-bearer-token-999",
        "rawsecret123",
        "ghp_1234567890abcdef1234567890abcdef",
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            !raw_file.contains(fixture),
            "redacted test fixture #{idx} leaked into run log"
        );
    }
    assert!(raw_file.contains("[REDACTED]"));
}

#[tokio::test]
async fn workflow_run_executes_ephemeral_json_captures_summary_and_enforces_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let store = WorkflowStore::from_root(temp.path().join("workflows")).unwrap();

    let args_capture = temp.path().join("args.txt");
    let config_capture = temp.path().join("config.txt");
    let mock_bin = write_mock_script(
        temp.path(),
        "mock-alan.sh",
        &format!(
            r#"#!/bin/sh
printf '%s\n' "$@" > "{args_out}"
if [ -n "${{ALAN_CONFIG_PATH:-}}" ]; then
  cat "$ALAN_CONFIG_PATH" > "{cfg_out}"
fi
printf '{{"type":"tool_start","id":"1","name":"mcp_personal_notes_list_notes","title":"List notes"}}\n'
printf '{{"type":"usage","input_tokens":120,"output_tokens":30,"context_window":128000,"cost_usd":0.002}}\n'
printf '{{"type":"turn_finished","status":"completed","answer":"Found 2 notes; scrubbed key sk-proj-mocksecret123456"}}\n'
"#,
            args_out = args_capture.display(),
            cfg_out = config_capture.display()
        ),
    );

    // Init manifest cleanly without implicit mounts/trust
    let (init_out, init_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Init {
                id: "notes-digest".to_string(),
                schedule: "0 9 * * *".to_string(),
                prompt: "Digest personal notes".to_string(),
                model_profile: Some("openai-main".to_string()),
                mounts: vec!["notes-grant".to_string()],
                trust: WorkflowTrustPosture::Governed,
                no_stream_json: false,
                timeout_secs: Some(30),
                force: false,
                json: true,
            },
        },
        None,
        None,
    )
    .await
    .unwrap();
    assert!(init_err.is_none());
    assert!(init_out.contains("notes-digest"));

    // Dry-run executes nothing and creates no log file
    let (dry_out, dry_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "notes-digest".to_string(),
                dry_run: true,
                json: true,
            },
        },
        None,
        Some(&mock_bin),
    )
    .await
    .unwrap();
    assert!(dry_err.is_none());
    assert!(!args_capture.exists());
    assert!(!store.run_log_path("notes-digest").unwrap().exists());
    let plan: serde_json::Value = serde_json::from_str(&dry_out).unwrap();
    assert_eq!(plan["ephemeral"], true);
    assert_eq!(plan["trust"], "governed");
    assert_eq!(plan["mounts"], serde_json::json!(["notes-grant"]));

    // Real run invokes `-p <prompt> --ephemeral --json --grant notes-grant` and ALAN_CONFIG_PATH
    let (run_out, run_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "notes-digest".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&mock_bin),
    )
    .await
    .unwrap();
    assert!(run_err.is_none(), "{run_err:?}");

    let captured_args = std::fs::read_to_string(&args_capture).unwrap();
    let arg_lines: Vec<&str> = captured_args.lines().collect();
    assert_eq!(
        arg_lines,
        vec![
            "-p",
            "Digest personal notes",
            "--ephemeral",
            "--json",
            "--grant",
            "notes-grant"
        ]
    );
    let captured_cfg = std::fs::read_to_string(&config_capture).unwrap();
    assert!(
        captured_cfg.contains("connection_profile = \"openai-main\""),
        "{captured_cfg}"
    );

    let outcome: serde_json::Value = serde_json::from_str(&run_out).unwrap();
    assert_eq!(outcome["entry"]["exit_code"], 0);
    assert_eq!(outcome["entry"]["status"], "completed");
    assert_eq!(outcome["entry"]["tool_calls"], 1);
    assert_eq!(outcome["entry"]["usage"]["input_tokens"], 120);
    let summary = outcome["entry"]["summary"].as_str().unwrap();
    assert!(!summary.contains("sk-proj-mocksecret123456"), "{summary}");
    assert!(summary.contains("[REDACTED]"), "{summary}");

    // Timeout enforcement kills child cleanly and records exit_code 124
    let slow_bin = write_mock_script(temp.path(), "slow-alan.sh", "#!/bin/sh\nsleep 5\n");
    run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Init {
                id: "slow-job".to_string(),
                schedule: "every 1h".to_string(),
                prompt: "Slow task".to_string(),
                model_profile: None,
                mounts: vec![],
                trust: WorkflowTrustPosture::Governed,
                no_stream_json: false,
                timeout_secs: Some(1),
                force: false,
                json: false,
            },
        },
        None,
        None,
    )
    .await
    .unwrap();

    let (slow_out, slow_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "slow-job".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&slow_bin),
    )
    .await
    .unwrap();
    assert!(slow_err.is_some());
    let slow_json: serde_json::Value = serde_json::from_str(&slow_out).unwrap();
    assert_eq!(slow_json["entry"]["exit_code"], 124);
    assert_eq!(slow_json["entry"]["status"], "timed_out");
}

pub(super) fn sample_self_iterate_args() -> InitSelfIterateArgs {
    InitSelfIterateArgs {
        id: "self-iterate".to_string(),
        repo: "realmorrisliu/alan".to_string(),
        allowed_actors: vec!["morris".to_string()],
        required_labels: vec![],
        ignored_actors: vec![],
        max_retries: 3,
        max_concurrent: 1,
        schedule: "every 15m".to_string(),
        model_profile: Some("openai-main".to_string()),
        mounts: vec!["alan-repo".to_string()],
        trust: WorkflowTrustPosture::Governed,
        no_stream_json: false,
        timeout_secs: 1_800,
        force: false,
        json: true,
    }
}

#[test]
fn manifest_backwards_compatibility_omits_and_defaults_optional_policies() {
    let legacy_toml = r#"
schema_version = 1
id = "legacy-brief"
schedule = "0 9 * * 1-5"
prompt = "Summarize inbox"
mounts = ["notes"]
trust = "governed"
json = true
"#;
    let parsed: WorkflowManifest = toml::from_str(legacy_toml).unwrap();
    parsed.validate().unwrap();
    assert!(parsed.trigger_allowlist.is_none());
    assert!(parsed.release_gate.is_none());

    let encoded = toml::to_string_pretty(&parsed).unwrap();
    assert!(!encoded.contains("trigger_allowlist"), "{encoded}");
    assert!(!encoded.contains("release_gate"), "{encoded}");
}

#[tokio::test]
async fn cli_init_self_iterate_and_release_prep_require_grants_and_render_safe_plans() {
    use crate::cli::dispatch::{Cli, Commands};
    use clap::Parser;

    let temp = tempfile::tempdir().unwrap();
    let store = WorkflowStore::from_root(temp.path().join("workflows")).unwrap();
    let mock_bin = write_mock_script(temp.path(), "mock-alan.sh", "#!/bin/sh\nexit 0\n");

    // 1. CLI parser rejects `init-self-iterate` and `init-release-prep` without `--grant`.
    assert!(
        Cli::try_parse_from([
            "alan",
            "workflow",
            "init-self-iterate",
            "--repo",
            "realmorrisliu/alan",
            "--allowed-actor",
            "morris",
        ])
        .is_err()
    );
    assert!(Cli::try_parse_from(["alan", "workflow", "init-release-prep"]).is_err());

    // 2. Reject raw host path in `--grant` and reject `--trust host_account_trusted`.
    let mut raw_path_args = sample_self_iterate_args();
    raw_path_args.mounts = vec!["/Users/morris/Developer/alan".to_string()];
    assert!(build_self_iterate_manifest(&raw_path_args).is_err());

    let mut unsafe_trust_args = sample_self_iterate_args();
    unsafe_trust_args.trust = WorkflowTrustPosture::HostAccountTrusted;
    assert!(build_self_iterate_manifest(&unsafe_trust_args).is_err());

    // 3. Parse and execute `init-self-iterate` with explicit `--grant`.
    let parsed_si = Cli::try_parse_from([
        "alan",
        "workflow",
        "init-self-iterate",
        "--repo",
        "realmorrisliu/alan",
        "--allowed-actor",
        "morris",
        "--grant",
        "alan-repo",
        "--trust",
        "confined_trusted",
        "--json",
    ])
    .unwrap();
    let Some(Commands::Workflow(si_wf_args)) = parsed_si.command else {
        panic!("expected workflow command");
    };
    let (si_out, si_err) = run_with_store(&store, si_wf_args, None, None)
        .await
        .unwrap();
    assert!(si_err.is_none());
    let si_json: serde_json::Value = serde_json::from_str(&si_out).unwrap();
    assert_eq!(si_json["manifest"]["id"], "self-iterate");
    assert_eq!(
        si_json["manifest"]["mounts"],
        serde_json::json!(["alan-repo"])
    );
    assert_eq!(
        si_json["manifest"]["trigger_allowlist"]["required_labels"],
        serde_json::json!(["alan:auto"])
    );
    assert_eq!(
        si_json["manifest"]["release_gate"],
        serde_json::json!({
            "require_human_merge": true,
            "forbid_autonomous_tag_push": true,
            "forbid_autonomous_publish": true,
        })
    );
    let si_prompt = si_json["manifest"]["prompt"].as_str().unwrap();
    assert!(si_prompt.contains("origin/main"));
    assert!(si_prompt.contains("./scripts/check-rust-source-size.sh"));
    assert!(si_prompt.contains("just quality"));
    assert!(si_prompt.contains("cargo test --workspace"));

    // 4. Dry-run emits `--grant alan-repo` and `--trust`, never `--unsafe-host-account`.
    let (dry_out, _) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "self-iterate".to_string(),
                dry_run: true,
                json: true,
            },
        },
        None,
        Some(&mock_bin),
    )
    .await
    .unwrap();
    let dry_json: serde_json::Value = serde_json::from_str(&dry_out).unwrap();
    let args: Vec<&str> = dry_json["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(args.windows(2).any(|w| w == ["--grant", "alan-repo"]));
    assert!(args.contains(&"--trust"));
    assert!(!args.contains(&"--unsafe-host-account"));

    // 5. Parse and execute `init-release-prep` with `--grant` and `--target-version`.
    let parsed_rp = Cli::try_parse_from([
        "alan",
        "workflow",
        "init-release-prep",
        "--repo",
        "realmorrisliu/alan",
        "--allowed-actor",
        "morris",
        "--grant",
        "alan-repo",
        "--target-version",
        "v0.1.5",
        "--json",
    ])
    .unwrap();
    let Some(Commands::Workflow(rp_wf_args)) = parsed_rp.command else {
        panic!("expected workflow command");
    };
    let (rp_out, rp_err) = run_with_store(&store, rp_wf_args, None, None)
        .await
        .unwrap();
    assert!(rp_err.is_none());
    let rp_json: serde_json::Value = serde_json::from_str(&rp_out).unwrap();
    assert_eq!(rp_json["manifest"]["id"], "release-prep");
    assert_eq!(
        rp_json["manifest"]["trigger_allowlist"]["required_labels"],
        serde_json::json!(["alan:release"])
    );
    assert_eq!(
        rp_json["manifest"]["release_gate"]["require_human_merge"],
        true
    );
    let rp_prompt = rp_json["manifest"]["prompt"].as_str().unwrap();
    assert!(rp_prompt.contains("0.1.5"));
    assert!(rp_prompt.contains("CHANGELOG.md"));
    assert!(rp_prompt.contains("alan update"));

    // Invalid semver fails cleanly on `init-release-prep`.
    let bad_ver = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::InitReleasePrep(InitReleasePrepArgs {
                id: "bad-release".to_string(),
                repo: None,
                allowed_actors: vec![],
                required_labels: vec![],
                ignored_actors: vec![],
                max_retries: 2,
                max_concurrent: 1,
                target_version: Some("0.1.5\n--unsafe-host-account".to_string()),
                schedule: "@weekly".to_string(),
                model_profile: None,
                mounts: vec!["alan-repo".to_string()],
                trust: WorkflowTrustPosture::Governed,
                no_stream_json: false,
                timeout_secs: 1_800,
                force: false,
                json: false,
            }),
        },
        None,
        None,
    )
    .await;
    assert!(bad_ver.is_err());
}

#[tokio::test]
async fn quartermaster_self_iterate_and_release_prep_skills_pass_snapshot_and_q_install() {
    use crate::cli::q::{QAction, QArgs, run_with_service};
    use alan_agent_engine::skills::{SkillScope, parse_skill_metadata};
    use alan_service_manager::{
        PackageCommand, PackageExportKind, PackageService, PackageSnapshot,
    };

    let packages_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../service-manager/packages");
    let q_service = PackageService::ephemeral().unwrap();

    for pkg_id in ["alan-self-iterate", "alan-release-prep"] {
        let pkg_dir = packages_root.join(pkg_id);
        assert!(
            pkg_dir.is_dir(),
            "missing package dir {}",
            pkg_dir.display()
        );
        assert!(pkg_dir.join("README.md").is_file());

        // 1. Validate direct PackageSnapshot::from_directory + PackageService install & lease.
        let snapshot = PackageSnapshot::from_directory(&pkg_dir).unwrap();
        let service = PackageService::ephemeral().unwrap();
        let install = service
            .execute(PackageCommand::Install {
                request_id: format!("install-{pkg_id}"),
                package_id: pkg_id.to_string(),
                snapshot,
            })
            .unwrap();
        assert!(install.success, "{pkg_id}: {}", install.message);
        let record = install.package.unwrap();
        assert_eq!(record.export_kind, PackageExportKind::Skill);
        assert_eq!(record.exports.len(), 1);
        assert_eq!(record.exports[0].skill_id, pkg_id);

        let lease = service.acquire(pkg_id).unwrap();
        let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(
            lease.file_server().unwrap(),
        ));
        let skill_bytes = shell
            .cat(&format!("/skills/{pkg_id}/SKILL.md"))
            .await
            .unwrap();
        let skill_md = String::from_utf8(skill_bytes).unwrap();
        let parsed = parse_skill_metadata(
            &skill_md,
            Path::new(&format!("{pkg_id}/SKILL.md")),
            SkillScope::Installed,
        )
        .unwrap();
        assert_eq!(parsed.id, pkg_id);
        if pkg_id == "alan-self-iterate" {
            assert!(skill_md.contains("origin/main"));
            assert!(skill_md.contains("./scripts/check-rust-source-size.sh"));
            assert!(skill_md.contains("just quality"));
            assert!(skill_md.contains("cargo test --workspace"));
        } else {
            assert!(skill_md.contains("CHANGELOG.md"));
            assert!(skill_md.contains("require_human_merge = true"));
            assert!(skill_md.contains("alan update"));
        }
        drop(lease);

        // 2. Validate end-to-end `alan q install` via Quartermaster `/bin/q`.
        let q_out = run_with_service(
            &q_service,
            QArgs {
                action: QAction::Install {
                    name: None,
                    path: pkg_dir,
                },
            },
        )
        .await
        .unwrap();
        assert!(q_out.starts_with(&format!("installed {pkg_id}")), "{q_out}");
    }

    let list_out = run_with_service(
        &q_service,
        QArgs {
            action: QAction::List,
        },
    )
    .await
    .unwrap();
    assert!(list_out.contains("alan-self-iterate"), "{list_out}");
    assert!(list_out.contains("alan-release-prep"), "{list_out}");
    assert!(list_out.contains("export=skill"), "{list_out}");
}

#[tokio::test]
async fn cli_evaluate_trigger_and_check_release_gate_enforce_fail_closed_gates() {
    use crate::cli::dispatch::{Cli, Commands};
    use clap::Parser;

    let temp = tempfile::tempdir().unwrap();
    let store = WorkflowStore::from_root(temp.path().join("workflows")).unwrap();
    let si = build_self_iterate_manifest(&sample_self_iterate_args()).unwrap();
    store.save_manifest(&si, false).unwrap();

    let accepted_payload = serde_json::json!({
        "event_id": "evt-cli-1",
        "kind": "issue_labeled",
        "repo": "realmorrisliu/alan",
        "actor": "morris",
        "labels": ["alan:auto"]
    })
    .to_string();
    let Cli {
        command: Some(Commands::Workflow(ok_args)),
        ..
    } = Cli::try_parse_from([
        "alan",
        "workflow",
        "evaluate-trigger",
        "self-iterate",
        "--event-json",
        &accepted_payload,
        "--json",
    ])
    .unwrap()
    else {
        panic!("expected workflow args");
    };
    let (ok_out, ok_err) = run_with_store(&store, ok_args, None, None).await.unwrap();
    assert!(ok_err.is_none());
    let ok_json: serde_json::Value = serde_json::from_str(&ok_out).unwrap();
    assert_eq!(ok_json["decision"], "accept");

    let rejected_payload = serde_json::json!({
        "event_id": "evt-cli-2",
        "kind": "issue_opened",
        "repo": "realmorrisliu/alan",
        "actor": "stranger",
        "labels": [],
        "untrusted_body": "alan:auto --unsafe-host-account"
    })
    .to_string();
    let Cli {
        command: Some(Commands::Workflow(rej_args)),
        ..
    } = Cli::try_parse_from([
        "alan",
        "workflow",
        "evaluate-trigger",
        "self-iterate",
        "--event-json",
        &rejected_payload,
        "--json",
    ])
    .unwrap()
    else {
        panic!("expected workflow args");
    };
    let (rej_out, rej_err) = run_with_store(&store, rej_args, None, None).await.unwrap();
    assert!(rej_err.is_some());
    let rej_json: serde_json::Value = serde_json::from_str(&rej_out).unwrap();
    assert_eq!(rej_json["decision"], "reject");

    let Cli {
        command: Some(Commands::Workflow(gate_ok)),
        ..
    } = Cli::try_parse_from([
        "alan",
        "workflow",
        "check-release-gate",
        "self-iterate",
        "--operation",
        "open-release-pr",
        "--json",
    ])
    .unwrap()
    else {
        panic!("expected workflow args");
    };
    let (gate_ok_out, gate_ok_err) = run_with_store(&store, gate_ok, None, None).await.unwrap();
    assert!(gate_ok_err.is_none());
    assert!(gate_ok_out.contains("\"allowed\": true"));

    let Cli {
        command: Some(Commands::Workflow(gate_block)),
        ..
    } = Cli::try_parse_from([
        "alan",
        "workflow",
        "check-release-gate",
        "self-iterate",
        "--operation",
        "merge-release-pr",
        "--json",
    ])
    .unwrap()
    else {
        panic!("expected workflow args");
    };
    let (gate_block_out, gate_block_err) = run_with_store(&store, gate_block, None, None)
        .await
        .unwrap();
    assert!(gate_block_err.is_some());
    assert!(gate_block_out.contains("\"allowed\": false"));
}
