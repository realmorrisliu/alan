//! Real Host, native mount and shell ordering across independent aP clients.
use std::time::Duration;

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolCall, ToolRegistry};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_os_host::{
    AlanOsHost, HostBootConfig, HostCommandPlane, HostEndpointPaths, LocalAttachment,
};
use alan_shell::Shell;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn response() -> GenerationResponse {
    GenerationResponse {
        content: "ready".into(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: vec![],
        tool_calls: vec![],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: vec![],
    }
}

async fn command(shell: &Shell, body: &str) -> String {
    input(shell, "command", body).await
}

async fn input(shell: &Shell, intent: &str, body: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let mut bytes = b"alan-input-v1\n".to_vec();
    bytes.extend(
        serde_json::to_vec(&json!({
            "version":1, "submission_id":id, "intent":intent, "mode":"follow_up", "body":body,
        }))
        .unwrap(),
    );
    shell.write("/agent/root/io/input", &bytes).await.unwrap();
    id
}

async fn wait_idle(shell: &Shell, id: &str) {
    let mut last_activity = Value::Null;
    let mut saw_input = false;
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let tape =
                String::from_utf8(shell.cat("/agent/root/machine/tape").await.unwrap()).unwrap();
            let activity: Value = serde_json::from_slice(
                &shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
            )
            .unwrap();
            saw_input = tape.contains(id);
            last_activity = activity.clone();
            if saw_input
                && activity["state"] == "idle"
                && activity["active_submission"].is_null()
                && activity["pending_submissions"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "input did not settle: {id}, tape={saw_input}, activity={last_activity}"
    );
}

async fn action_output(shell: &Shell, call_id: &str) -> Value {
    for action in shell.ls("/agent/root/actions").await.unwrap() {
        if !action.starts_with('a') {
            continue;
        }
        let result: Value = serde_json::from_slice(
            &shell
                .cat(&format!("/agent/root/actions/{action}/result"))
                .await
                .unwrap(),
        )
        .unwrap();
        if result["call_id"] == call_id {
            return serde_json::from_slice(
                &shell
                    .cat(&format!("/agent/root/actions/{action}/output"))
                    .await
                    .unwrap(),
            )
            .unwrap();
        }
    }
    panic!("no Action for {call_id}")
}

#[tokio::test]
async fn two_clients_share_native_command_cwd_and_preserve_shell_script_semantics() {
    let runtime = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    let mut mount = response();
    mount.tool_calls.push(ToolCall {
        id: Some("mount-project".into()), name: "request_mount".into(),
        arguments: json!({"label":"Project", "namespace_path":"/mnt/project", "access":"read_write", "reason":"native command integration"}),
    });
    let file_call = |id: &str, name: &str, arguments| {
        let mut reply = response();
        reply.tool_calls.push(ToolCall {
            id: Some(id.into()),
            name: name.into(),
            arguments,
        });
        reply
    };
    let provider = MockLlmProvider::new().with_responses(vec![
        mount,
        response(),
        file_call(
            "mount-other",
            "request_mount",
            json!({"label":"Other", "namespace_path":"/mnt/other", "access":"read_write", "reason":"cross-grant cwd integration"}),
        ),
        response(),
        file_call(
            "write-project",
            "write_file",
            json!({"path":"agent.txt", "content":"original"}),
        ),
        file_call(
            "edit-project",
            "edit_file",
            json!({"path":"agent.txt", "old_string":"original", "new_string":"edited"}),
        ),
        response(),
        file_call("read-native", "read_file", json!({"path":"agent.txt"})),
        response(),
        file_call(
            "stale-edit",
            "edit_file",
            json!({"path":"agent.txt", "old_string":"edited", "new_string":"must not overwrite"}),
        ),
        response(),
        file_call(
            "work-status",
            "agent_work",
            json!({"action":"status", "target":"root"}),
        ),
        response(),
    ]);
    let probe = provider.clone();
    let mut tools = ToolRegistry::new();
    tools.register(alan_tools::BashTool::new());
    tools.register(alan_tools::ReadFileTool::new());
    tools.register(alan_tools::WriteFileTool::new());
    tools.register(alan_tools::EditFileTool::new());
    let stores = alan_agent_engine::AgentRuntimeStoreBindings {
        rollouts: runtime.path().join("rollouts"),
        checkpoints: runtime.path().join("checkpoints"),
        cache: runtime.path().join("cache"),
        tmp: runtime.path().join("tmp"),
        metadata: runtime.path().join("metadata"),
    };
    for dir in [
        &stores.rollouts,
        &stores.checkpoints,
        &stores.cache,
        &stores.tmp,
        &stores.metadata,
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let mut process = AgentProcessConfig {
        store_bindings: Some(stores.clone()),
        // This integration checks command dispatch, not automatic Tape compaction.
        ..AgentProcessConfig::from(alan_agent_engine::Config {
            context_window_tokens: Some(128_000),
            ..Default::default()
        })
    };
    process
        .agent_config
        .runtime_config
        .compaction_trigger_messages = 1_000;
    let config = HostBootConfig::ephemeral("test", process, LlmClient::new(provider), tools);
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let host = AlanOsHost::boot(config, paths.clone()).await.unwrap();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move { host.serve_until(stopped.cancelled_owned()).await });
    let first = Shell::new(
        LocalAttachment::new(paths.clone())
            .connect()
            .await
            .unwrap()
            .root,
    );
    let second = Shell::new(
        LocalAttachment::new(paths.clone())
            .connect()
            .await
            .unwrap()
            .root,
    );
    first
        .write("/agent/root/io/input", b"mount project")
        .await
        .unwrap();
    let request = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(id) = first
                .ls("/mnt/host-mount/requests")
                .await
                .unwrap()
                .into_iter()
                .find(|id| !matches!(id.as_str(), "clone" | "events"))
            {
                break id;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    HostCommandPlane::new(paths.clone())
        .approve_host_mount(request.clone(), project.path().to_owned())
        .await
        .unwrap();
    let mount_other = input(&second, "agent", "mount another directory").await;
    let other_request = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(id) = second
                .ls("/mnt/host-mount/requests")
                .await
                .unwrap()
                .into_iter()
                .find(|id| !matches!(id.as_str(), "clone" | "events") && id != &request)
            {
                break id;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    HostCommandPlane::new(paths.clone())
        .approve_host_mount(other_request, other.path().to_owned())
        .await
        .unwrap();
    wait_idle(&second, &mount_other).await;
    let cd = command(&first, "cd /mnt/project/src").await;
    // Separate connections admit both inputs before either client waits for completion.
    let script = command(&second, "printf '%s\\n' 'first value' | tr 'a-z' 'A-Z' > result.txt\nprintf '%s\\n' 'second' >> result.txt").await;
    wait_idle(&second, &script).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/result.txt")).unwrap(),
        "FIRST VALUE\nsecond\n"
    );
    assert!(!project.path().join("result.txt").exists());
    let tape = String::from_utf8(first.cat("/agent/root/machine/tape").await.unwrap()).unwrap();
    assert!(tape.find(&cd).unwrap() < tape.find(&script).unwrap());
    let switch = command(&first, "cd /mnt/other").await;
    let write_other = command(&second, "printf other > cross-grant.txt").await;
    wait_idle(&second, &write_other).await;
    assert_eq!(
        std::fs::read_to_string(other.path().join("cross-grant.txt")).unwrap(),
        "other"
    );
    assert!(!project.path().join("src/cross-grant.txt").exists());
    let switch_back = command(&second, "cd /mnt/project/src").await;
    let write_project = command(&first, "printf project > cross-grant.txt").await;
    wait_idle(&first, &write_project).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/cross-grant.txt")).unwrap(),
        "project"
    );
    assert_eq!(
        std::fs::read_to_string(other.path().join("cross-grant.txt")).unwrap(),
        "other"
    );
    let tape = String::from_utf8(first.cat("/agent/root/machine/tape").await.unwrap()).unwrap();
    for (cd, write) in [(&switch, &write_other), (&switch_back, &write_project)] {
        assert!(tape.find(cd).unwrap() < tape.find(write).unwrap());
        assert_eq!(action_output(&first, cd).await["stderr"], "");
        assert_eq!(action_output(&first, write).await["success"], true);
    }
    let local_cd = command(&first, "cd .. && printf root > local.txt").await;
    wait_idle(&first, &local_cd).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("local.txt")).unwrap(),
        "root"
    );
    let failed_cd = command(&first, "cd missing-directory").await;
    let unsupported_cd = command(&second, "cd $HOME").await;
    let after_cd = command(&second, "printf retained > still-here.txt").await;
    wait_idle(&second, &after_cd).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/still-here.txt")).unwrap(),
        "retained"
    );
    let mut failed_cds = Vec::new();
    for action in first.ls("/agent/root/actions").await.unwrap() {
        if !action.starts_with('a') {
            continue;
        }
        let result: Value = serde_json::from_slice(
            &first
                .cat(&format!("/agent/root/actions/{action}/result"))
                .await
                .unwrap(),
        )
        .unwrap();
        if result["call_id"] == failed_cd || result["call_id"] == unsupported_cd {
            assert_eq!(result["exit_code"], 1);
            failed_cds.push(result["call_id"].as_str().unwrap().to_owned());
        }
    }
    assert!(failed_cds.contains(&failed_cd));
    assert!(failed_cds.contains(&unsupported_cd));

    let active = command(
        &first,
        "printf started > started.txt; sleep 30; printf late > late.txt",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !project.path().join("src/started.txt").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native command did not start");
    let queued = command(&second, "printf queued > queued.txt").await;
    first
        .write(
            "/agent/root/machine/ctl",
            format!("queue-v1 interrupt {active}").as_bytes(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let activity: Value = serde_json::from_slice(
                &first.cat("/agent/root/machine/ui/activity").await.unwrap(),
            )
            .unwrap();
            if activity["queue_paused"] == true && activity["active_submission"].is_null() {
                assert_eq!(activity["pending_submissions"][0]["submission_id"], queued);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancellation did not pause the remaining input");
    assert!(project.path().join("src/started.txt").exists());
    assert!(!project.path().join("src/late.txt").exists());
    assert!(!project.path().join("src/queued.txt").exists());
    assert_eq!(
        action_output(&first, &active).await["stderr"],
        "Command interrupted; completed changes are preserved"
    );
    second
        .write("/agent/root/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    wait_idle(&second, &queued).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/queued.txt")).unwrap(),
        "queued"
    );
    assert_eq!(
        probe.recorded_requests().len(),
        4,
        "explicit commands must not invoke generation"
    );
    let captured = command(
        &first,
        "pwd > cwd.txt; pwd; printf 'diagnostic: ' >&2; pwd >&2; exit 7",
    )
    .await;
    wait_idle(&first, &captured).await;
    let captured_output = action_output(&second, &captured).await;
    assert_eq!(captured_output["stdout"], ".\n", "{captured_output}");
    assert_eq!(captured_output["stderr"], "diagnostic: .\n");
    assert_eq!(captured_output["exit_code"], 7);
    assert_eq!(captured_output["success"], false);
    let native_cwd = std::fs::canonicalize(project.path().join("src")).unwrap();
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/cwd.txt")).unwrap(),
        format!("{}\n", native_cwd.display()),
        "redirected project data must retain native shell output"
    );
    assert_eq!(probe.recorded_requests().len(), 4);
    let long = command(&second, "printf '%040000d' 0").await;
    wait_idle(&second, &long).await;
    assert_eq!(
        action_output(&first, &long).await["stdout"]
            .as_str()
            .unwrap()
            .len(),
        40_000
    );
    assert_eq!(probe.recorded_requests().len(), 4);
    let edit = input(&first, "agent", "write and edit the project file").await;
    wait_idle(&first, &edit).await;
    let requests = probe.recorded_requests();
    let prior_result = requests[4]
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some(captured.as_str()))
        .expect("later Agent generation must receive the command result");
    let prior_result: Value = serde_json::from_str(&prior_result.content).unwrap();
    assert_eq!(prior_result["stdout"], ".\n");
    assert_eq!(prior_result["stderr"], "diagnostic: .\n");
    assert_eq!(prior_result["exit_code"], 7);
    let long_result = requests[4]
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some(long.as_str()))
        .expect("long command result must remain available to later Agent input");
    let projection: Value = serde_json::from_str(&long_result.content).unwrap();
    assert_eq!(projection["type"], "evidence_projection");
    assert_eq!(projection["truncation"]["full_content_recoverable"], true);
    assert!(long_result.content.len() < 30_000);
    let reference = projection["reference"]["path"].as_str().unwrap();
    let full: Value = serde_json::from_slice(&second.cat(reference).await.unwrap()).unwrap();
    assert_eq!(full["stdout"], "0".repeat(40_000));
    assert_eq!(full["exit_code"], 0);
    let model_messages = serde_json::to_string(&requests[4].messages).unwrap();
    assert!(!model_messages.contains(native_cwd.to_str().unwrap()));
    let native = command(&second, "cat agent.txt > observed.txt; git diff --no-index --no-ext-diff --no-textconv -- /dev/null agent.txt > diff.txt; printf native > agent.txt").await;
    wait_idle(&second, &native).await;
    assert_eq!(
        action_output(&second, &native).await["success"],
        true,
        "{:?}",
        action_output(&second, &native).await
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/observed.txt")).unwrap(),
        "edited"
    );
    assert!(
        std::fs::read_to_string(project.path().join("src/diff.txt"))
            .unwrap()
            .contains("+edited")
    );
    assert!(!project.path().join("agent.txt").exists());
    let read = input(&first, "agent", "read the native edit").await;
    wait_idle(&first, &read).await;
    assert_eq!(
        action_output(&first, "read-native").await["content"],
        "native"
    );
    let stale = input(&first, "agent", "try an edit against stale content").await;
    wait_idle(&first, &stale).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/agent.txt")).unwrap(),
        "native"
    );
    assert!(
        action_output(&first, "stale-edit").await["error"]
            .as_str()
            .unwrap()
            .contains("Search text not found")
    );
    assert_eq!(probe.recorded_requests().len(), 11);
    let status_input = input(&first, "agent", "inspect work status").await;
    wait_idle(&first, &status_input).await;
    assert_eq!(
        probe.recorded_requests().len(),
        13,
        "status turn generation count"
    );
    let status = action_output(&first, "work-status").await;
    assert_eq!(status["success"], true, "{status}");
    assert!(status["activity"].is_object(), "{status}");
    assert!(
        probe.recorded_requests()[0]
            .tools
            .iter()
            .any(|tool| tool.name == "agent_work")
    );
    let active = command(
        &first,
        "printf started >> restart-started.txt; sleep 30; printf late > restart-late.txt",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !project.path().join("src/restart-started.txt").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pending = command(&second, "printf pending > restart-pending.txt").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let activity: Value = serde_json::from_slice(
                &first.cat("/agent/root/machine/ui/activity").await.unwrap(),
            )
            .unwrap();
            if activity["pending_submissions"][0]["submission_id"] == pending {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid_path = "/mnt/service-manager/units/root-agent/pid";
    let old_pid = String::from_utf8(first.cat(pid_path).await.unwrap()).unwrap();
    // Fault injection uses an ordinary Shell Process; processless clients retain
    // their read-only /proc projection.
    let control = Shell::new(
        LocalAttachment::new(paths.clone())
            .connect_shell_process()
            .await
            .unwrap()
            .root,
    );
    control
        .write(&format!("/proc/{}/ctl", old_pid.trim()), b"cancel")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(pid) = first.cat(pid_path).await
                && std::str::from_utf8(&pid)
                    .ok()
                    .and_then(|pid| pid.trim().parse::<u64>().ok())
                    .is_some_and(|pid| pid > 0)
                && pid != old_pid.as_bytes()
                && first
                    .cat("/mnt/service-manager/units/root-agent/status")
                    .await
                    .is_ok_and(|status| status.starts_with(b"ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let recovered: Value =
        serde_json::from_slice(&first.cat("/agent/root/machine/ui/activity").await.unwrap())
            .unwrap();
    assert_eq!(recovered["queue_paused"], true);
    assert!(recovered["active_submission"].is_null());
    assert_eq!(
        recovered["pending_submissions"][0]["submission_id"],
        pending
    );
    assert!(!recovered.to_string().contains(&active));
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/restart-started.txt")).unwrap(),
        "started",
        "active native command must not replay"
    );
    assert!(!project.path().join("src/restart-late.txt").exists());
    assert!(!project.path().join("src/restart-pending.txt").exists());
    stop.cancel();
    server.await.unwrap().unwrap();
    let mut tools = ToolRegistry::new();
    tools.register(alan_tools::BashTool::new());
    let provider = MockLlmProvider::new();
    let reboot_probe = provider.clone();
    let config = HostBootConfig::ephemeral(
        "test",
        AgentProcessConfig {
            store_bindings: Some(stores),
            ..AgentProcessConfig::default()
        },
        LlmClient::new(provider),
        tools,
    );
    let host = AlanOsHost::boot(config, paths.clone()).await.unwrap();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move { host.serve_until(stopped.cancelled_owned()).await });
    let first = Shell::new(LocalAttachment::new(paths).connect().await.unwrap().root);
    let recovered: Value =
        serde_json::from_slice(&first.cat("/agent/root/machine/ui/activity").await.unwrap())
            .unwrap();
    assert_eq!(recovered["queue_paused"], true);
    assert!(recovered["active_submission"].is_null());
    assert_eq!(
        recovered["pending_submissions"][0]["submission_id"],
        pending
    );
    first
        .write("/agent/root/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    wait_idle(&first, &pending).await;
    let failure = action_output(&first, &pending).await;
    assert!(
        failure.to_string().contains("explicit directory"),
        "{failure}"
    );
    assert!(!project.path().join("src/restart-pending.txt").exists());
    assert_eq!(probe.recorded_requests().len(), 13);
    assert!(reboot_probe.recorded_requests().is_empty());
    stop.cancel();
    server.await.unwrap().unwrap();
}
