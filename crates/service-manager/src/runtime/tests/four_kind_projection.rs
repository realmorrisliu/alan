use super::*;
use alan_agent_engine::runtime::tool_packages::ToolPackageManifest;
use alan_agent_engine::tools::{
    ToolCapability, ToolExecutionBinding, ToolProcessInvocation, ToolRegistry,
};
use alan_kernel::{Credentials, Namespace};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_http_mcp_projection_mock() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut raw = vec![0u8; 32 * 1024];
                let mut filled = 0usize;
                loop {
                    let n = socket.read(&mut raw[filled..]).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    filled += n;
                    let text = String::from_utf8_lossy(&raw[..filled]);
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        let headers = &text[..header_end];
                        let len = headers
                            .lines()
                            .find_map(|line| {
                                let (name, val) = line.split_once(':')?;
                                if name.trim().eq_ignore_ascii_case("content-length") {
                                    val.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        if filled >= header_end + 4 + len {
                            let rpc: serde_json::Value =
                                serde_json::from_slice(&raw[header_end + 4..header_end + 4 + len])
                                    .unwrap();
                            let method = rpc["method"].as_str().unwrap_or("");
                            let id = rpc.get("id").cloned();
                            let resp = match method {
                                "initialize" => {
                                    let body = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": { "protocolVersion": "2025-03-26" }
                                    })
                                    .to_string();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        body.len(),
                                        body
                                    )
                                }
                                "notifications/initialized" => {
                                    "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                                }
                                "tools/list" => {
                                    let body = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "tools": [{
                                                "name": "status",
                                                "description": "Remote status",
                                                "inputSchema": { "type": "object", "properties": {} }
                                            }]
                                        }
                                    })
                                    .to_string();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        body.len(),
                                        body
                                    )
                                }
                                "tools/call" => {
                                    let body = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "content": [{ "type": "text", "text": "healthy" }],
                                            "structuredContent": { "remote_status": "healthy" }
                                        }
                                    })
                                    .to_string();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        body.len(),
                                        body
                                    )
                                }
                                _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                            };
                            let _ = socket.write_all(resp.as_bytes()).await;
                            let _ = socket.shutdown().await;
                            return;
                        }
                    }
                }
            });
        }
    });
    format!("http://{addr}/mcp")
}

#[cfg(unix)]
#[tokio::test]
async fn tool_and_mcp_packages_project_only_to_explicitly_referencing_processes() {
    let packages = crate::PackageService::ephemeral().unwrap();

    // 1. Install packaged `tool` (`json_echo`). Also attempts escape write if allowed — scratch isolates it.
    let tool_src = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tool_src.path().join("bin")).unwrap();
    std::fs::write(
        tool_src.path().join("alan-package.toml"),
        r#"
kind = "tool"
description = "JSON echo tool"

[tool]
name = "json_echo"
description = "Echo JSON argument safely"
entrypoint = "bin/echo.sh"
parameters = { type = "object", properties = { msg = { type = "string" } }, required = ["msg"] }
capability = "unknown"
"#,
    )
    .unwrap();
    std::fs::write(
        tool_src.path().join("bin/echo.sh"),
        r#"#!/bin/sh
printf '{"echoed":%s,"confined":true}\n' "$1"
"#,
    )
    .unwrap();
    let res = crate::run_q_command_with_package_service(
        &packages,
        Some(tool_src.path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "echo-tool-pack".to_string(),
            crate::CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(res.exit_code, 0, "{}", String::from_utf8_lossy(&res.output));

    // 2. Install `mcp` package combining stdio + streamable_http servers.
    let http_url = spawn_http_mcp_projection_mock().await;
    let mcp_src = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(mcp_src.path().join("srv")).unwrap();
    std::fs::write(
        mcp_src.path().join("alan-package.toml"),
        format!(
            r#"
kind = "mcp"
description = "Mixed stdio + HTTP MCP bundle"

[[mcps]]
id = "stdio-srv"
[mcps.transport]
type = "stdio"
command = "srv/mcp.sh"

[[mcps]]
id = "http-srv"
[mcps.transport]
type = "streamable_http"
url = "{http_url}"
"#
        ),
    )
    .unwrap();
    std::fs::write(
        mcp_src.path().join("srv/mcp.sh"),
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-03-26"}}\n' "$id"
      ;;
    *'"method":"tools/list"'*)
      id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"ping","description":"Stdio ping","inputSchema":{"type":"object"}}]}}\n' "$id"
      ;;
    *'"method":"tools/call"'*)
      id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"ok"}],"structuredContent":{"stdio_ping":"pong"}}}\n' "$id"
      ;;
  esac
done
"#,
    )
    .unwrap();
    let res = crate::run_q_command_with_package_service(
        &packages,
        Some(mcp_src.path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "demo-mcp-pack".to_string(),
            crate::CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(res.exit_code, 0, "{}", String::from_utf8_lossy(&res.output));

    // Process A explicitly references both packages; Process B references neither!
    let mut ctx_a =
        ProcessLaunchContext::new(Namespace::new(), Credentials::user("proc-a"), "/").unwrap();
    let ctx_b =
        ProcessLaunchContext::new(Namespace::new(), Credentials::user("proc-b"), "/").unwrap();
    project_package_reference_async(&packages, &mut ctx_a, "echo-tool-pack")
        .await
        .unwrap();
    project_package_reference_async(&packages, &mut ctx_a, "demo-mcp-pack")
        .await
        .unwrap();

    // Verify namespace isolation: Process A sees /bin/* and /lib/exec/*/manifest; Process B sees NONE!
    let shell_a = alan_shell::Shell::new(alan_ap::InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(ctx_a.namespace.clone()),
    )));
    let shell_b = alan_shell::Shell::new(alan_ap::InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(ctx_b.namespace.clone()),
    )));
    assert!(shell_b.ls("/bin").await.is_err());

    let bin_entries = shell_a.ls("/bin").await.unwrap();
    assert!(
        bin_entries.contains(&"json_echo".to_string()),
        "{bin_entries:?}"
    );
    assert!(
        bin_entries.contains(&"mcp_stdio_srv_ping".to_string()),
        "{bin_entries:?}"
    );
    assert!(
        bin_entries.contains(&"mcp_http_srv_status".to_string()),
        "{bin_entries:?}"
    );

    let stdio_manifest: ToolPackageManifest = serde_json::from_slice(
        &shell_a
            .cat("/lib/exec/mcp_stdio_srv_ping/manifest")
            .await
            .unwrap(),
    )
    .unwrap();
    stdio_manifest
        .validate_for_name("mcp_stdio_srv_ping")
        .unwrap();
    assert_eq!(stdio_manifest.capability, ToolCapability::Unknown);

    let http_manifest: ToolPackageManifest = serde_json::from_slice(
        &shell_a
            .cat("/lib/exec/mcp_http_srv_status/manifest")
            .await
            .unwrap(),
    )
    .unwrap();
    http_manifest
        .validate_for_name("mcp_http_srv_status")
        .unwrap();
    assert_eq!(http_manifest.capability, ToolCapability::Network);

    // Execute via ToolProcessRunner for Process A (pid = 101) vs Process B (pid = 102).
    let runner = ToolRegistry::new().process_runner();
    let scratch_a = tempfile::tempdir().unwrap();
    let scratch_b = tempfile::tempdir().unwrap();
    runner.register_process_binding(
        101,
        ToolExecutionBinding::awaiting_host_projection("/".into(), scratch_a.path().to_path_buf()),
    );
    runner.register_process_binding(
        102,
        ToolExecutionBinding::awaiting_host_projection("/".into(), scratch_b.path().to_path_buf()),
    );
    runner
        .register_process_tools(101, ctx_a.packaged_tools.clone())
        .unwrap();

    let tool_out = runner
        .run(ToolProcessInvocation {
            pid: 1001,
            parent: Some(101),
            executable: "/bin/json_echo".to_string(),
            args: vec![r#"{"msg":"hello"}"#.to_string()],
        })
        .await;
    assert_eq!(
        tool_out.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&tool_out.output)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&tool_out.output).unwrap();
    assert_eq!(parsed["confined"], serde_json::json!(true));
    assert_eq!(parsed["echoed"]["msg"], serde_json::json!("hello"));

    let mcp_stdio_out = runner
        .run(ToolProcessInvocation {
            pid: 1002,
            parent: Some(101),
            executable: "/bin/mcp_stdio_srv_ping".to_string(),
            args: vec!["{}".to_string()],
        })
        .await;
    assert_eq!(mcp_stdio_out.exit_code, 0);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&mcp_stdio_out.output).unwrap(),
        serde_json::json!({ "stdio_ping": "pong" })
    );

    let mcp_http_out = runner
        .run(ToolProcessInvocation {
            pid: 1003,
            parent: Some(101),
            executable: "/bin/mcp_http_srv_status".to_string(),
            args: vec!["{}".to_string()],
        })
        .await;
    assert_eq!(mcp_http_out.exit_code, 0);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&mcp_http_out.output).unwrap(),
        serde_json::json!({ "remote_status": "healthy" })
    );

    // Non-referencing Process B must NOT be able to invoke Process A's packaged tools!
    let unauthorized = runner
        .run(ToolProcessInvocation {
            pid: 1004,
            parent: Some(102),
            executable: "/bin/json_echo".to_string(),
            args: vec![r#"{"msg":"nope"}"#.to_string()],
        })
        .await;
    assert_eq!(unauthorized.exit_code, 127);

    // Unregistering Process A removes packaged tool access cleanly.
    runner.unregister_process(101);
    let after_exit = runner
        .run(ToolProcessInvocation {
            pid: 1005,
            parent: Some(101),
            executable: "/bin/json_echo".to_string(),
            args: vec![r#"{"msg":"nope"}"#.to_string()],
        })
        .await;
    assert_eq!(after_exit.exit_code, 127);
}
