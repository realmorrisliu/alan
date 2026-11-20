use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use alan_agent_protocol::{GovernanceConfig, ToolCapability};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::ProcessFileTree;
use crate::policy::{PolicyAction, PolicyContext, PolicyEngine};
use crate::tools::{Sandbox, SandboxSpec, Tool, ToolContext, ToolExecutionBinding};

use super::*;

fn scratch_context(scratch: &std::path::Path) -> ToolContext {
    let binding = ToolExecutionBinding::awaiting_host_projection(
        std::path::PathBuf::from("/workspace"),
        scratch.to_path_buf(),
    );
    ToolContext::from_binding(binding, Arc::new(crate::Config::default()))
}

fn write_executable_script(dir: &std::path::Path, name: &str, script: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[cfg(unix)]
#[tokio::test]
async fn stdio_mcp_client_handles_handshake_pagination_call_env_sanitization_and_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("mcp.pid");
    let script = format!(
        r#"#!/bin/sh
set -eu
echo $$ > "{pid_path}"
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"protocolVersion":"2025-03-26","serverInfo":{{"name":"mock-stdio","version":"0.1.0"}}}}}}\n' "$id"
      ;;
    *'"method":"notifications/initialized"'*)
      ;;
    *'"method":"tools/list"'*)
      id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      case "$line" in
        *'"cursor":"page-2"'*)
          printf '{{"jsonrpc":"2.0","id":%s,"result":{{"tools":[{{"name":"echo.tool","description":"Echo env check","inputSchema":{{"type":"object","properties":{{"msg":{{"type":"string"}}}},"required":["msg"]}}}}]}}}}\n' "$id"
          ;;
        *)
          printf '{{"jsonrpc":"2.0","id":%s,"result":{{"tools":[{{"name":"ping","description":"Ping tool","inputSchema":{{"type":"object"}}}}],"nextCursor":"page-2"}}}}\n' "$id"
          ;;
      esac
      ;;
    *'"method":"tools/call"'*)
      id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      leaked="${{TEST_PARENT_API_KEY:-clean}}"
      safe="${{PUBLIC_FLAG:-unset}}"
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"content":[{{"type":"text","text":"ok"}}],"structuredContent":{{"parent_secret":"%s","public_flag":"%s"}}}}}}\n' "$id" "$leaked" "$safe"
      ;;
  esac
done
"#,
        pid_path = pid_file.display()
    );
    let executable = write_executable_script(temp.path(), "mock-stdio-mcp.sh", &script);

    // SAFETY: unit test sets a uniquely named dummy variable before spawning the isolated stdio child
    // to verify env sanitization strips inherited host secrets.
    unsafe {
        std::env::set_var("TEST_PARENT_API_KEY", "sk-should-never-leak");
    }

    let mut static_env = BTreeMap::new();
    static_env.insert("PUBLIC_FLAG".to_string(), "enabled".to_string());

    let config = McpServerConfig {
        server_id: "demo-stdio".to_string(),
        description: Some("Mock stdio server".to_string()),
        transport: McpTransportKind::Stdio(McpStdioTransportConfig {
            executable,
            args: Vec::new(),
            env: static_env,
            cwd: temp.path().to_path_buf(),
            scratch_dir: temp.path().join("scratch"),
            allow_network: false,
        }),
        capability: ToolCapability::Unknown,
        timeout: Duration::from_secs(5),
    };

    let sandbox = Sandbox::from_spec(SandboxSpec::seed(temp.path().to_path_buf()));
    let session = Arc::new(McpServerSession::new(config));
    let tools = session.discover_tools(&sandbox, "scope-a").await.unwrap();
    let names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
    assert_eq!(
        names,
        vec!["mcp_demo_stdio_ping", "mcp_demo_stdio_echo_tool"]
    );

    let ctx = scratch_context(&temp.path().join("scratch"));
    let result = tools[1].execute(json!({"msg": "hi"}), &ctx).await.unwrap();
    assert_eq!(
        result,
        json!({"parent_secret": "clean", "public_flag": "enabled"})
    );

    let pid_text = std::fs::read_to_string(&pid_file).unwrap();
    let child_pid: i32 = pid_text.trim().parse().unwrap();
    session.shutdown().await;
    // Allow OS signal delivery to reap the terminated process group.
    tokio::time::sleep(Duration::from_millis(50)).await;
    // SAFETY: `libc::kill(pid, 0)` only queries process existence cleanly without modifying state.
    let alive = unsafe { libc::kill(child_pid, 0) } == 0;
    assert!(!alive, "MCP stdio subprocess should terminate on shutdown");
}

#[derive(Default)]
struct HttpMockState {
    seen_session_headers: Vec<Option<String>>,
    methods: Vec<String>,
}

async fn spawn_http_mcp_server(sse_for_call: bool) -> (String, Arc<Mutex<HttpMockState>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(Mutex::new(HttpMockState::default()));
    let server_state = Arc::clone(&state);

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let state = Arc::clone(&server_state);
            tokio::spawn(async move {
                let mut raw = vec![0u8; 64 * 1024];
                let mut filled = 0usize;
                loop {
                    let n = socket.read(&mut raw[filled..]).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    filled += n;
                    let text = String::from_utf8_lossy(&raw[..filled]);
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        let headers_part = &text[..header_end];
                        let content_len = headers_part
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
                        let body_start = header_end + 4;
                        if filled >= body_start + content_len {
                            let body = &raw[body_start..body_start + content_len];
                            let rpc: Value = serde_json::from_slice(body).unwrap();
                            let method = rpc["method"].as_str().unwrap_or("").to_string();
                            let session_header = headers_part.lines().find_map(|line| {
                                let (name, val) = line.split_once(':')?;
                                if name.trim().eq_ignore_ascii_case("mcp-session-id") {
                                    Some(val.trim().to_string())
                                } else {
                                    None
                                }
                            });
                            {
                                let mut guard = state.lock().await;
                                guard.seen_session_headers.push(session_header);
                                guard.methods.push(method.clone());
                            }
                            let id = rpc.get("id").cloned();
                            let response = match method.as_str() {
                                "initialize" => {
                                    let payload = json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "protocolVersion": "2025-03-26",
                                            "serverInfo": { "name": "http-mock", "version": "1.0" }
                                        }
                                    })
                                    .to_string();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: sess-abc-123\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        payload.len(),
                                        payload
                                    )
                                }
                                "notifications/initialized" => {
                                    "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                                }
                                "tools/list" => {
                                    let payload = json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "tools": [{
                                                "name": "lookup-doc",
                                                "description": "Lookup via HTTP",
                                                "inputSchema": {
                                                    "type": "object",
                                                    "properties": { "query": { "type": "string" } },
                                                    "required": ["query"]
                                                }
                                            }]
                                        }
                                    })
                                    .to_string();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        payload.len(),
                                        payload
                                    )
                                }
                                "tools/call" => {
                                    let query = rpc["params"]["arguments"]["query"]
                                        .as_str()
                                        .unwrap_or("missing");
                                    let rpc_res = json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "content": [{ "type": "text", "text": query }],
                                            "structuredContent": { "answer": format!("http:{query}") }
                                        }
                                    })
                                    .to_string();
                                    if sse_for_call {
                                        let sse_body = format!(
                                            "event: message\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{{\"progress\":1}}}}\n\nevent: message\ndata: {rpc_res}\n\n"
                                        );
                                        format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                            sse_body.len(),
                                            sse_body
                                        )
                                    } else {
                                        format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                            rpc_res.len(),
                                            rpc_res
                                        )
                                    }
                                }
                                _ => {
                                    let payload = json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "error": { "code": -32601, "message": "method not found" }
                                    })
                                    .to_string();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        payload.len(),
                                        payload
                                    )
                                }
                            };
                            let _ = socket.write_all(response.as_bytes()).await;
                            let _ = socket.shutdown().await;
                            return;
                        }
                    }
                }
            });
        }
    });

    (format!("http://{addr}/mcp"), state)
}

#[tokio::test]
async fn streamable_http_mcp_handles_json_sse_and_session_id() {
    for sse_mode in [false, true] {
        let (url, state) = spawn_http_mcp_server(sse_mode).await;
        let temp = tempfile::tempdir().unwrap();
        let config = McpServerConfig {
            server_id: "remote-http".to_string(),
            description: Some("Remote Streamable HTTP MCP".to_string()),
            transport: McpTransportKind::StreamableHttp(McpHttpTransportConfig {
                url,
                headers: BTreeMap::new(),
            }),
            capability: ToolCapability::Network,
            timeout: Duration::from_secs(5),
        };
        let sandbox = Sandbox::from_spec(SandboxSpec::seed(temp.path().to_path_buf()));
        let session = Arc::new(McpServerSession::new(config));
        let tools = session
            .discover_tools(&sandbox, "scope-http")
            .await
            .unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "mcp_remote_http_lookup_doc");
        assert_eq!(tools[0].capability(&Value::Null), ToolCapability::Network);

        let manifest = tools[0].manifest().unwrap();
        manifest
            .validate_for_name("mcp_remote_http_lookup_doc")
            .unwrap();
        assert_eq!(manifest.capability, ToolCapability::Network);

        let ctx = scratch_context(temp.path());
        let outcome = tools[0]
            .execute(json!({"query": "alan9"}), &ctx)
            .await
            .unwrap();
        assert_eq!(outcome, json!({"answer": "http:alan9"}));

        let recorded = state.lock().await;
        assert_eq!(
            recorded.methods,
            vec![
                "initialize",
                "notifications/initialized",
                "tools/list",
                "tools/call"
            ]
        );
        assert_eq!(
            recorded.seen_session_headers,
            vec![
                None,
                Some("sess-abc-123".to_string()),
                Some("sess-abc-123".to_string()),
                Some("sess-abc-123".to_string()),
            ]
        );
    }
}

#[test]
fn projected_mcp_tools_use_ordinary_policy_governance_without_ambient_trust() {
    let policy_yaml = r#"
rules:
  - id: deny-unknown
    tool: "*"
    capability: unknown
    action: deny
    reason: "unknown capability tools are blocked"
  - id: escalate-network
    tool: "*"
    capability: network
    action: escalate
    reason: "network MCP requires operator confirmation"
default_action: allow
"#;
    let mut files = BTreeMap::new();
    files.insert("policy.yaml".to_string(), policy_yaml.as_bytes().to_vec());
    let tree = ProcessFileTree::new(files).unwrap();
    let policy =
        PolicyEngine::load_for_governance_from_file_tree(&tree, &GovernanceConfig::default());

    let stdio_decision = policy.evaluate(PolicyContext {
        tool_name: "mcp_demo_ping",
        arguments: &Value::Null,
        capability: ToolCapability::Unknown,
        cwd: None,
    });
    assert_eq!(stdio_decision.action, PolicyAction::Deny);

    let http_decision = policy.evaluate(PolicyContext {
        tool_name: "mcp_remote_lookup_doc",
        arguments: &Value::Null,
        capability: ToolCapability::Network,
        cwd: None,
    });
    assert_eq!(http_decision.action, PolicyAction::Escalate);

    let default_autonomous = PolicyEngine::autonomous().evaluate(PolicyContext {
        tool_name: "mcp_demo_ping",
        arguments: &Value::Null,
        capability: ToolCapability::Unknown,
        cwd: None,
    });
    assert_eq!(default_autonomous.action, PolicyAction::Escalate);
}

#[test]
fn endpoint_and_header_validators_reject_embedded_credentials() {
    assert!(validate_http_endpoint_url("https://user:pass@example.com/mcp", "MCP").is_err());
    assert!(validate_http_endpoint_url("ftp://example.com/mcp", "MCP").is_err());
    assert!(validate_http_endpoint_url("https://example.com/mcp", "MCP").is_ok());

    let mut bad_headers = BTreeMap::new();
    bad_headers.insert("Authorization".to_string(), "Bearer sk-123".to_string());
    assert!(validate_safe_http_headers(&bad_headers, "MCP").is_err());

    let mut bad_env = BTreeMap::new();
    bad_env.insert("ANTHROPIC_API_KEY".to_string(), "sk-123".to_string());
    assert!(validate_safe_stdio_env(&bad_env).is_err());
}
