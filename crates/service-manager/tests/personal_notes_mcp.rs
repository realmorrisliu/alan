use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alan_agent_engine::runtime::mcp::{
    McpClient, McpServerConfig, McpServerSession, McpStdioTransportConfig, McpTransportKind,
};
use alan_agent_engine::tools::{Sandbox, SandboxSpec, Tool, ToolCapability, ToolRegistry};
use alan_agent_engine::{AgentProcessConfig, LlmClient};
use alan_llm::MockLlmProvider;
use alan_service_manager::{
    CLI_IMPORT_MOUNT_PATH, McpTransportSpec, PackageExportKind, PackageService,
    ProcessLaunchContext, ServiceManager, ServiceManagerConfig, run_q_command_with_package_service,
};
use serde_json::json;

fn package_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/personal-notes-mcp")
}

#[cfg(unix)]
#[tokio::test]
async fn q_install_personal_notes_mcp_has_zero_ambient_authority_and_works_via_stdio_mcp() {
    let store_dir = tempfile::tempdir().unwrap();
    let pkg_store = store_dir.path().join("packages");
    let service = PackageService::open(pkg_store.clone()).unwrap();

    // 1. Install `personal-notes-mcp` atomically via Quartermaster `/bin/q`.
    let install_result = run_q_command_with_package_service(
        &service,
        Some(&package_fixture_dir()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "personal-notes-mcp".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    let install_text = String::from_utf8_lossy(&install_result.output);
    assert_eq!(install_result.exit_code, 0, "{install_text}");
    assert!(
        install_text.contains("installed personal-notes-mcp"),
        "{install_text}"
    );

    let record = service.resolve("personal-notes-mcp").unwrap();
    assert_eq!(record.export_kind, PackageExportKind::Mcp);
    assert_eq!(record.mcp_exports.len(), 1);
    let mcp = &record.mcp_exports[0];
    assert_eq!(mcp.server_id, "personal-notes");
    assert_eq!(mcp.capability, ToolCapability::Read);
    let McpTransportSpec::Stdio {
        command,
        args,
        env,
        allow_network,
    } = &mcp.transport
    else {
        panic!("expected stdio transport for personal-notes-mcp");
    };
    assert_eq!(command, "mcp/personal-notes/bin");
    assert!(args.is_empty());
    assert!(
        !env.contains_key("NOTES_DIR"),
        "package manifest must grant zero ambient directory authority"
    );
    assert!(!allow_network);

    // 2. Verify zero ambient authority: booting a ServiceManager against this package store
    // without explicitly referencing `personal-notes-mcp` does NOT project `/lib/pkg/personal-notes-mcp`
    // or `/bin/mcp_personal_notes_*` onto Root Agent.
    let mut manager_cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    manager_cfg.package_store = Some(pkg_store.clone());
    let manager = ServiceManager::boot(manager_cfg).await.unwrap();
    let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(
        manager.local_entry().namespace_for_local_client(),
    ));
    assert!(shell.ls("/lib/pkg/personal-notes-mcp").await.is_err());
    if let Ok(bin_entries) = shell.ls("/bin").await {
        assert!(
            !bin_entries
                .iter()
                .any(|entry| entry.starts_with("mcp_personal_notes_")),
            "unreferenced MCP tools leaked into /bin: {bin_entries:?}"
        );
    }
    manager.shutdown().await.unwrap();

    // 3. Acquire materialized executable from Package Service lease and connect via StdioMcpTransport.
    let lease = service.acquire("personal-notes-mcp").unwrap();
    let pkg_shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(
        lease.file_server().unwrap(),
    ));
    assert!(
        pkg_shell
            .cat("/mcp/personal-notes/server.json")
            .await
            .is_ok()
    );
    let executable = pkg_store
        .join("revisions")
        .join("personal-notes-mcp")
        .join(&record.revision)
        .join("content")
        .join(command);
    assert!(executable.is_file(), "missing {}", executable.display());

    let scratch = tempfile::tempdir().unwrap();
    let sandbox = Sandbox::from_spec(SandboxSpec::seed(scratch.path().to_path_buf()));

    // 3a. Without explicit `NOTES_DIR` or `notes_dir`, `list_notes` fails closed (`isError: true`).
    let unconfigured_client = McpClient::connect(
        &McpServerConfig {
            server_id: mcp.server_id.clone(),
            description: mcp.description.clone(),
            transport: McpTransportKind::Stdio(McpStdioTransportConfig {
                executable: executable.clone(),
                args: args.clone(),
                env: BTreeMap::new(),
                cwd: scratch.path().to_path_buf(),
                scratch_dir: scratch.path().to_path_buf(),
                allow_network: false,
            }),
            capability: mcp.capability,
            timeout: Duration::from_secs(10),
        },
        &sandbox,
    )
    .await
    .unwrap();
    let unconfigured_res = unconfigured_client
        .call_tool("list_notes", json!({}))
        .await
        .unwrap();
    assert!(
        unconfigured_res.is_error,
        "missing NOTES_DIR must refuse ambient access: {unconfigured_res:?}"
    );
    unconfigured_client.shutdown().await;

    // 3b. With explicit `NOTES_DIR`, discover tools & call `list_notes`, `search_notes`, `read_note`.
    let workspace = tempfile::tempdir().unwrap();
    let outside_secret = workspace.path().join("secret.md");
    std::fs::write(&outside_secret, "# Secret\nDO NOT READ\n").unwrap();

    let notes_dir = workspace.path().join("notes");
    std::fs::create_dir_all(notes_dir.join("journal")).unwrap();
    std::fs::write(
        notes_dir.join("inbox.md"),
        "# Inbox\nBuy milk and review alan9 workflow logs.\n",
    )
    .unwrap();
    std::fs::write(
        notes_dir.join("journal/2026-11-19.md"),
        "# Daily Note\nShipped personal-notes-mcp over stdio MCP.\n",
    )
    .unwrap();
    std::fs::write(notes_dir.join("ignore.txt"), "not markdown").unwrap();
    std::os::unix::fs::symlink(&outside_secret, notes_dir.join("escape.md")).unwrap();

    let mut explicit_env = BTreeMap::new();
    explicit_env.insert("NOTES_DIR".to_string(), notes_dir.display().to_string());
    let server_config = McpServerConfig {
        server_id: mcp.server_id.clone(),
        description: mcp.description.clone(),
        transport: McpTransportKind::Stdio(McpStdioTransportConfig {
            executable,
            args: args.clone(),
            env: explicit_env,
            cwd: scratch.path().to_path_buf(),
            scratch_dir: scratch.path().to_path_buf(),
            allow_network: false,
        }),
        capability: mcp.capability,
        timeout: Duration::from_secs(10),
    };

    let client = McpClient::connect(&server_config, &sandbox).await.unwrap();
    let tools = client.list_tools().await.unwrap();
    let discovered_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        discovered_names,
        vec!["list_notes", "search_notes", "read_note"]
    );

    let listed = client.call_tool("list_notes", json!({})).await.unwrap();
    assert!(!listed.is_error, "{listed:?}");
    let listed_sc = listed.structured_content.unwrap();
    assert_eq!(listed_sc["count"], 2);
    let paths: Vec<&str> = listed_sc["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["inbox.md", "journal/2026-11-19.md"]);

    let searched = client
        .call_tool("search_notes", json!({ "query": "alan9" }))
        .await
        .unwrap();
    assert!(!searched.is_error, "{searched:?}");
    let search_sc = searched.structured_content.unwrap();
    assert_eq!(search_sc["count"], 1);
    assert_eq!(search_sc["matches"][0]["path"], "inbox.md");

    let read = client
        .call_tool("read_note", json!({ "path": "journal/2026-11-19.md" }))
        .await
        .unwrap();
    assert!(!read.is_error, "{read:?}");
    let read_sc = read.structured_content.unwrap();
    assert!(
        read_sc["content"]
            .as_str()
            .unwrap()
            .contains("Shipped personal-notes-mcp")
    );

    // Traversal and escaping symlinks fail closed
    let traversal = client
        .call_tool("read_note", json!({ "path": "../secret.md" }))
        .await
        .unwrap();
    assert!(traversal.is_error, "{traversal:?}");

    let symlink_escape = client
        .call_tool("read_note", json!({ "path": "escape.md" }))
        .await
        .unwrap();
    assert!(symlink_escape.is_error, "{symlink_escape:?}");

    client.shutdown().await;

    // 4. Verify `McpServerSession` projects canonical `mcp_personal_notes_*` tools cleanly.
    let session = Arc::new(McpServerSession::new(server_config));
    let projected = session
        .discover_tools(&sandbox, "notes-scope")
        .await
        .unwrap();
    let projected_names: Vec<&str> = projected.iter().map(|t| t.name()).collect();
    assert_eq!(
        projected_names,
        vec![
            "mcp_personal_notes_list_notes",
            "mcp_personal_notes_search_notes",
            "mcp_personal_notes_read_note",
        ]
    );
    for tool in &projected {
        assert_eq!(tool.capability(&json!({})), ToolCapability::Read);
        tool.manifest().unwrap();
    }
    session.shutdown().await;
}
