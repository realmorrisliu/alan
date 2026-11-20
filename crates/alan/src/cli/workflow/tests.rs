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

fn write_mock_script(dir: &Path, name: &str, body: &str) -> PathBuf {
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
