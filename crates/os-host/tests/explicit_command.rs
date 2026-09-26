//! Governed explicit input through the real Host and native shell.
use alan_agent_engine::{
    AgentProcessConfig, AgentRuntimeStoreBindings, LlmClient, ToolCall, ToolRegistry,
};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_os_host::{
    AlanOsHost, HostBootConfig, HostCommandPlane, HostEndpointPaths, LocalAttachment,
};
use alan_shell::Shell;
use serde_json::{Value, json};
use std::time::Duration;
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

async fn submit_command(shell: &Shell, body: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let record =
        json!({"version":1,"submission_id":id,"intent":"command","mode":"follow_up","body":body});
    shell
        .write(
            "/agent/root/io/input",
            format!("alan-input-v1\n{record}").as_bytes(),
        )
        .await
        .unwrap();
    id
}

async fn command_result(shell: &Shell, id: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            for action in shell.ls("/agent/root/actions").await.unwrap() {
                if !action.starts_with('a') {
                    continue;
                }
                let base = format!("/agent/root/actions/{action}");
                let bytes = shell.cat(&format!("{base}/result")).await.unwrap();
                let Ok(mut result) = serde_json::from_slice::<Value>(&bytes) else {
                    continue;
                };
                if result["call_id"] == id {
                    let status = shell.cat(&format!("{base}/status")).await.unwrap();
                    if status == b"completed" || status == b"failed" {
                        result["process"] = json!(
                            String::from_utf8(shell.cat(&format!("{base}/process")).await.unwrap())
                                .unwrap()
                        );
                        return result;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("correlated command completion")
}

async fn command(shell: &Shell, body: &str) -> Value {
    let id = submit_command(shell, body).await;
    command_result(shell, &id).await
}

#[tokio::test]
async fn native_commands_change_cwd_and_preserve_scripts_without_generation() {
    let runtime = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    let mut mount = response();
    mount.tool_calls.push(ToolCall { id: Some("mount-project".into()), name: "request_mount".into(),
        arguments: json!({"label":"Project","namespace_path":"/mnt/project","access":"read_write","reason":"native command test"}) });
    let provider = MockLlmProvider::new().with_responses(vec![mount, response()]);
    let probe = provider.clone();
    let mut tools = ToolRegistry::new();
    tools.register(alan_tools::BashTool::new());
    let stores = AgentRuntimeStoreBindings {
        rollouts: runtime.path().join("rollouts"),
        checkpoints: runtime.path().join("checkpoints"),
        cache: runtime.path().join("cache"),
        tmp: runtime.path().join("tmp"),
        metadata: runtime.path().join("metadata"),
    };
    for path in [
        &stores.rollouts,
        &stores.checkpoints,
        &stores.cache,
        &stores.tmp,
        &stores.metadata,
    ] {
        std::fs::create_dir_all(path).unwrap();
    }
    let process = AgentProcessConfig {
        store_bindings: Some(stores),
        ..AgentProcessConfig::from(alan_agent_engine::Config {
            context_window_tokens: Some(128_000),
            ..Default::default()
        })
    };
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let host = AlanOsHost::boot(
        HostBootConfig::ephemeral("test", process, LlmClient::new(provider), tools),
        paths.clone(),
    )
    .await
    .unwrap();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move { host.serve_until(stopped.cancelled_owned()).await });
    let shell = Shell::new(
        LocalAttachment::new(paths.clone())
            .connect()
            .await
            .unwrap()
            .root,
    );
    shell
        .write("/agent/root/io/input", b"mount project")
        .await
        .unwrap();
    let request = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(id) = shell
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
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let activity: Value = serde_json::from_slice(
                &shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
            )
            .unwrap();
            if probe.recorded_requests().len() == 2 && activity["state"] == "idle" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(command(&shell, "cd /mnt/project/src").await["exit_code"], 0);
    assert_eq!(command(&shell, "printf '%s\\n' 'first value' | tr 'a-z' 'A-Z' > result.txt\nprintf '%s\\n' second >> result.txt").await["exit_code"], 0);
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/result.txt")).unwrap(),
        "FIRST VALUE\nsecond\n"
    );
    assert_eq!(
        command(&shell, "cd .. && printf root > local.txt").await["exit_code"],
        0
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("local.txt")).unwrap(),
        "root"
    );
    assert_eq!(command(&shell, "cd missing").await["exit_code"], 1);
    assert_eq!(command(&shell, "cd $HOME").await["exit_code"], 1);
    assert_eq!(
        command(&shell, "printf retained > still-here.txt").await["exit_code"],
        0
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/still-here.txt")).unwrap(),
        "retained"
    );
    assert_eq!(
        command(&shell, "printf partial > partial.txt; exit 7").await["exit_code"],
        7
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/partial.txt")).unwrap(),
        "partial"
    );
    let running = command(&shell, "printf saved > before-interrupt.txt; sleep 30");
    let interrupt = async {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !project.path().join("src/before-interrupt.txt").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("native command started");
        let queued = submit_command(&shell, "printf queued > queued.txt").await;
        shell
            .write("/agent/root/machine/ctl", b"interrupt")
            .await
            .unwrap();
        queued
    };
    let (interrupted, queued) = tokio::join!(running, interrupt);
    assert_ne!(interrupted["exit_code"], 0);
    let process = interrupted["process"]
        .as_str()
        .expect("spawned Process evidence");
    assert!(process.starts_with("/proc/"), "{interrupted}");
    assert_eq!(
        shell.cat(&format!("{process}/status")).await.unwrap(),
        b"exited\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/before-interrupt.txt")).unwrap(),
        "saved"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !project.path().join("queued.txt").exists(),
        "interrupt must hold the next native command"
    );
    let activity: Value =
        serde_json::from_slice(&shell.cat("/agent/root/machine/ui/activity").await.unwrap())
            .unwrap();
    assert_eq!(activity["state"], "paused");
    shell
        .write("/agent/root/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    assert_eq!(command_result(&shell, &queued).await["exit_code"], 0);
    assert_eq!(
        std::fs::read_to_string(project.path().join("queued.txt")).unwrap(),
        "queued"
    );
    assert_eq!(
        command(&shell, "printf alive > after-interrupt.txt").await["exit_code"],
        0
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/after-interrupt.txt")).unwrap(),
        "alive"
    );
    assert_eq!(
        probe.recorded_requests().len(),
        2,
        "commands must not generate Agent responses"
    );
    stop.cancel();
    server.await.unwrap().unwrap();
}
