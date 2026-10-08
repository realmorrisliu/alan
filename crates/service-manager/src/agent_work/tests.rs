use super::*;
use alan_kernel::{Access, Credentials, ExecSpec, Namespace, Pid};
use std::collections::BTreeMap;

fn invocation(namespace: Namespace, args: &[&str]) -> ProcessInvocation {
    ProcessInvocation {
        pid: Pid(2),
        parent: Some(Pid(1)),
        credentials: Credentials::user("test"),
        namespace,
        exec: ExecSpec {
            executable: EXECUTABLE.into(),
            args: args.iter().map(|s| (*s).into()).collect(),
            namespace: Default::default(),
            descriptors: BTreeMap::new(),
        },
    }
}

#[tokio::test]
async fn help_and_manifest_explain_invocation_root_and_submit_semantics() {
    let manifest: Value = serde_json::from_slice(&manifest()).unwrap();
    assert_eq!(manifest["description"], HELP);
    assert_eq!(
        manifest["parameters"]["properties"]["target"]["description"],
        "root (this invocation's Root Agent Process, which may be the caller itself) or a visible Agent PID"
    );
    let outcome = AgentWorkProcessRunner
        .run(invocation(Namespace::new(), &["--help"]))
        .await;
    assert_eq!(outcome.exit_code, 0);
    let result: Value = serde_json::from_slice(&outcome.output).unwrap();
    assert_eq!(result["help"], HELP);
    for clarification in [
        "root is this invocation's Root Agent Process and may be the caller itself",
        "Submit queues input for that target, including legitimate self-scheduling",
        "does not notify an external operator",
        "is not an external handoff or report",
    ] {
        assert!(HELP.contains(clarification));
    }
    println!("manifest description: {}", manifest["description"]);
    println!(
        "target description: {}",
        manifest["parameters"]["properties"]["target"]["description"]
    );
    println!("--help output: {}", result);
}

#[test]
fn tool_schema_matches_action_argument_requirements() {
    let manifest: Value = serde_json::from_slice(&manifest()).unwrap();
    let validator = jsonschema::validator_for(&manifest["parameters"]).unwrap();
    for action in [
        "status",
        "result",
        "continue",
        "discard",
        "submit",
        "cancel",
        "select_owner",
    ] {
        let mut value = json!({"action":action,"target":"root"});
        assert_eq!(
            validator.is_valid(&value),
            !matches!(action, "submit" | "cancel" | "select_owner")
        );
        if action == "submit" {
            value["text"] = json!("do work");
        }
        if action == "cancel" {
            value["submission_id"] = json!(uuid::Uuid::new_v4());
        }
        if action == "select_owner" {
            value["request"] = owner_request();
        }
        assert!(validator.is_valid(&value));
        assert!(serde_json::from_value::<Action>(value.clone()).is_ok());
        let mut extra = value.clone();
        extra[if action == "select_owner" {
            "text"
        } else {
            "request"
        }] = owner_request();
        assert!(!validator.is_valid(&extra));
        value[if action == "cancel" {
            "text"
        } else {
            "submission_id"
        }] = json!("unexpected");
        assert!(!validator.is_valid(&value));
    }
}

#[tokio::test]
async fn work_commands_use_only_visible_agent_files_and_preserve_receipt_semantics() {
    let manifest: alan_agent_engine::runtime::ToolPackageManifest =
        serde_json::from_slice(&manifest()).unwrap();
    manifest.validate_for_name("agent_work").unwrap();
    let fs = Arc::new(alan_agentfs::AgentFs::new());
    let mut namespace = Namespace::new();
    namespace.mount("/agent/7", InProcessTransport::new(fs), Access::ReadWrite);
    let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(
        namespace.clone(),
    ))));
    let runner = AgentWorkProcessRunner;
    let status = runner
        .run(invocation(namespace.clone(), &["status", "7"]))
        .await;
    assert_eq!(status.exit_code, 0);
    assert!(serde_json::from_slice::<Value>(&status.output).unwrap()["activity"].is_object());
    let submitted = runner
        .run(invocation(
            namespace.clone(),
            &[r#"{"action":"submit","target":"7","text":"do work"}"#],
        ))
        .await;
    assert_eq!(submitted.exit_code, 0);
    let receipt: Value = serde_json::from_slice(&submitted.output).unwrap();
    assert_eq!(receipt["status"], "submitted");
    let id = receipt["submission_id"].as_str().unwrap();
    uuid::Uuid::parse_str(id).unwrap();
    let input = shell.cat("/agent/7/io/input").await.unwrap();
    let input = String::from_utf8_lossy(&input);
    assert!(
        input.contains(id) && input.contains("do work") && input.contains("\"intent\":\"agent\"")
    );
    let upper_id = id.to_ascii_uppercase();
    for args in [
        vec!["cancel", "7", &upper_id],
        vec!["continue", "7"],
        vec!["discard", "7"],
    ] {
        let outcome = runner.run(invocation(namespace.clone(), &args)).await;
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(
            serde_json::from_slice::<Value>(&outcome.output).unwrap()["status"],
            "requested"
        );
    }
    let events = String::from_utf8(shell.cat("/agent/7/events").await.unwrap()).unwrap();
    assert!(events.contains(&format!("ctl:queue-v1 interrupt {id}")));
    for args in [
        vec!["status", "root"],
        vec!["submit", "7/../8", "escape"],
        vec!["cancel", "7", "bad-id"],
    ] {
        assert_ne!(
            runner
                .run(invocation(namespace.clone(), &args))
                .await
                .exit_code,
            0
        );
    }
    let mut read_only = Namespace::new();
    read_only.mount(
        "/agent/7",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        Access::ReadOnly,
    );
    let denied = runner
        .run(invocation(read_only, &["submit", "7", "forbidden"]))
        .await;
    assert_eq!(denied.exit_code, 1);
    assert!(
        String::from_utf8_lossy(&denied.output).contains("access"),
        "{}",
        String::from_utf8_lossy(&denied.output)
    );
    let missing = crate::process_runner::SystemProcessRunner::new(None, None)
        .run(invocation(namespace.clone(), &["status", "7"]))
        .await;
    assert_eq!(missing.exit_code, 127);
    namespace.mount(
        EXECUTABLE,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    let installed = crate::process_runner::SystemProcessRunner::new(None, None)
        .run(invocation(namespace, &["status", "7"]))
        .await;
    assert_eq!(installed.exit_code, 0);
}

fn owner_request() -> Value {
    json!({"version":1,"question":"Which crate defines HostDirFs?",
        "evaluator_profile":"eval","candidates":[{"id":"hostfs","sources":[{
            "path":"/mnt/project/lib.rs","start_line":1,"end_line":1}]}]})
}

#[tokio::test]
async fn owner_work_receipt_is_a_control_admission_not_a_completed_result() {
    let fs = Arc::new(alan_agentfs::AgentFs::new());
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/7",
        InProcessTransport::new(fs.clone()),
        Access::ReadWrite,
    );
    let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(
        namespace.clone(),
    ))));
    let request =
        json!({"action":"select_owner","target":"7","request":owner_request()}).to_string();
    let result = AgentWorkProcessRunner
        .run(invocation(namespace.clone(), &[&request]))
        .await;
    assert_eq!(result.exit_code, 0);
    let receipt: Value = serde_json::from_slice(&result.output).unwrap();
    assert_eq!(receipt["status"], "submitted");
    let events = String::from_utf8(shell.cat("/agent/7/events").await.unwrap()).unwrap();
    assert!(events.contains("ctl:owner-work-v1 "));
    assert!(events.contains(receipt["submission_id"].as_str().unwrap()));
    assert!(shell.cat("/agent/7/io/input").await.unwrap().is_empty());
    let latest = AgentWorkProcessRunner
        .run(invocation(namespace.clone(), &["result", "7"]))
        .await;
    let latest: Value = serde_json::from_slice(&latest.output).unwrap();
    assert!(latest["projection"]["work"].is_null());
    let mut invalid = owner_request();
    invalid["candidates"][0]["sources"][0]["path"] = json!("/Users/secret");
    let invalid = json!({"action":"select_owner","target":"7","request":invalid}).to_string();
    let denied = AgentWorkProcessRunner
        .run(invocation(namespace.clone(), &[&invalid]))
        .await;
    assert_eq!(denied.exit_code, 1);
    assert_eq!(
        String::from_utf8(shell.cat("/agent/7/events").await.unwrap()).unwrap(),
        events
    );
    let mut read_only = Namespace::new();
    read_only.mount("/agent/7", InProcessTransport::new(fs), Access::ReadOnly);
    assert_eq!(
        AgentWorkProcessRunner
            .run(invocation(read_only, &[&request]))
            .await
            .exit_code,
        1
    );
}

include!("commit_tests.rs");
