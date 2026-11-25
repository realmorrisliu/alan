use std::path::Path;

use alan_agent_engine::{
    InboxDigestStatus, bind_project_dir, list_unread_inbox_digests,
    runtime::MAX_INBOX_SUMMARY_CHARS,
};
use alan_os_host::SystemStorePaths;

use super::manifest::{WorkflowManifest, WorkflowStore, WorkflowTrustPosture};
use super::tests::write_mock_script;
use super::{WorkflowAction, WorkflowArgs, run_with_store};

#[test]
fn manifest_project_key_is_optional_and_validated() {
    let legacy = r#"
schema_version = 1
id = "legacy-wf"
schedule = "every 1h"
prompt = "Check repo"
"#;
    let parsed: WorkflowManifest = toml::from_str(legacy).unwrap();
    parsed.validate().unwrap();
    assert!(parsed.project_key.is_none());
    let encoded = toml::to_string_pretty(&parsed).unwrap();
    assert!(!encoded.contains("project_key"), "{encoded}");

    for bad_key in ["", "  ", "../escape", "proj/sub", "proj space"] {
        let mut invalid = parsed.clone();
        invalid.project_key = Some(bad_key.to_string());
        assert!(
            invalid.validate().is_err(),
            "expected invalid project_key `{bad_key}` to fail validation"
        );
    }

    let mut valid = parsed;
    valid.project_key = Some("proj-abc123".to_string());
    valid.validate().unwrap();
}

#[tokio::test]
async fn execute_workflow_run_emits_redacted_bounded_inbox_digests_for_success_and_failure() {
    let temp = tempfile::tempdir().unwrap();
    let system = SystemStorePaths::from_root(&temp.path().join("System Store")).unwrap();
    let projects_root = system.projects().unwrap();
    let store = WorkflowStore::from_system_store(&system).unwrap();

    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let binding = bind_project_dir(&projects_root, &project_dir, None).unwrap();

    // 1. Success run with secrets and long text gets scrubbed and bounded in project inbox/.
    let long_tail = "z".repeat(MAX_INBOX_SUMMARY_CHARS + 300);
    let ok_bin = write_mock_script(
        temp.path(),
        "ok-alan.sh",
        &format!(
            r#"#!/bin/sh
printf '{{"type":"turn_finished","status":"completed","answer":"Checked repo; leaked sk-proj-secret123456 and api_key=rawpass999 {long_tail}"}}\n'
"#
        ),
    );
    let (init_out, init_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Init {
                id: "proj-ok".to_string(),
                schedule: "every 1h".to_string(),
                prompt: "Check project".to_string(),
                model_profile: None,
                project_key: Some(binding.project_key.clone()),
                mounts: vec![],
                packages: vec![],
                fileservers: vec![],
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
    assert!(init_out.contains(&binding.project_key));

    let (_out, run_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "proj-ok".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&ok_bin),
    )
    .await
    .unwrap();
    assert!(run_err.is_none(), "{run_err:?}");

    let unread = list_unread_inbox_digests(&projects_root, &binding.project_key).unwrap();
    assert_eq!(unread.len(), 1);
    let ok_digest = &unread[0];
    assert_eq!(ok_digest.workflow_id, "proj-ok");
    assert_eq!(ok_digest.status, InboxDigestStatus::Ok);
    assert!(ok_digest.digested_at.is_none());
    assert!(ok_digest.stream_name.is_none());
    assert!(!ok_digest.summary.contains("sk-proj-secret123456"));
    assert!(!ok_digest.summary.contains("rawpass999"));
    assert!(ok_digest.summary.contains("[REDACTED]"));
    assert!(ok_digest.summary.chars().count() <= MAX_INBOX_SUMMARY_CHARS);
    assert!(Path::new(&ok_digest.evidence_ref).is_file());

    // 2. Failing run still records a scrubbed Warning digest in inbox/.
    let fail_bin = write_mock_script(
        temp.path(),
        "fail-alan.sh",
        r#"#!/bin/sh
echo "fatal: token Authorization: Bearer secret-bearer-999 failed" >&2
exit 2
"#,
    );
    let fail_manifest = WorkflowManifest {
        schema_version: 1,
        id: "proj-fail".to_string(),
        schedule: "every 1h".to_string(),
        prompt: "Failing check".to_string(),
        model_profile: None,
        project_key: Some(binding.project_key.clone()),
        mounts: vec![],
        packages: vec![],
        fileservers: vec![],
        trust: WorkflowTrustPosture::Governed,
        json: true,
        timeout_secs: Some(30),
        trigger_allowlist: None,
        release_gate: None,
    };
    store.save_manifest(&fail_manifest, false).unwrap();

    let (_fail_out, fail_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "proj-fail".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&fail_bin),
    )
    .await
    .unwrap();
    assert!(fail_err.is_some());

    let unread_after = list_unread_inbox_digests(&projects_root, &binding.project_key).unwrap();
    assert_eq!(unread_after.len(), 2);
    let fail_digest = unread_after
        .iter()
        .find(|entry| entry.workflow_id == "proj-fail")
        .expect("failing run must emit inbox digest");
    assert_eq!(fail_digest.status, InboxDigestStatus::Warning);
    assert!(!fail_digest.summary.contains("secret-bearer-999"));
    assert!(fail_digest.summary.contains("[REDACTED]"));
    assert!(fail_digest.digested_at.is_none());

    // 3. Unbound workflow (project_key = None) does not write to project inbox.
    let unbound_manifest = WorkflowManifest {
        id: "unbound-wf".to_string(),
        project_key: None,
        ..fail_manifest
    };
    store.save_manifest(&unbound_manifest, false).unwrap();
    run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "unbound-wf".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&ok_bin),
    )
    .await
    .unwrap();
    assert_eq!(
        list_unread_inbox_digests(&projects_root, &binding.project_key)
            .unwrap()
            .len(),
        2
    );
}
