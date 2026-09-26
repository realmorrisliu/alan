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
    let process = AgentProcessConfig {
        store_bindings: Some(stores),
        ..AgentProcessConfig::default()
    };
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
    HostCommandPlane::new(paths)
        .approve_host_mount(request, project.path().to_owned())
        .await
        .unwrap();
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
    let local_cd = command(&first, "cd .. && printf root > local.txt").await;
    wait_idle(&first, &local_cd).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("local.txt")).unwrap(),
        "root"
    );
    let failed_cd = command(&first, "cd missing-directory").await;
    let after_cd = command(&second, "printf retained > still-here.txt").await;
    wait_idle(&second, &after_cd).await;
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/still-here.txt")).unwrap(),
        "retained"
    );
    let mut saw_failed_cd = false;
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
        if result["call_id"] == failed_cd {
            assert_eq!(result["exit_code"], 1);
            saw_failed_cd = true;
        }
    }
    assert!(
        saw_failed_cd,
        "failed cd must retain correlated failure evidence"
    );

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
        2,
        "explicit commands must not invoke generation"
    );
    let edit = input(&first, "agent", "write and edit the project file").await;
    wait_idle(&first, &edit).await;
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
    assert_eq!(probe.recorded_requests().len(), 9);
    let status_input = input(&first, "agent", "inspect work status").await;
    wait_idle(&first, &status_input).await;
    let status = action_output(&first, "work-status").await;
    assert_eq!(status["success"], true, "{status}");
    assert!(status["activity"].is_object(), "{status}");
    assert!(
        probe.recorded_requests()[0]
            .tools
            .iter()
            .any(|tool| tool.name == "agent_work")
    );
    stop.cancel();
    server.await.unwrap().unwrap();
}
