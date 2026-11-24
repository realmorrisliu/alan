use super::*;
use crate::{
    FileserverMountAccess, PackageCommand, PackageSnapshot, PackageSnapshotEntry, RootBindingMode,
};
use alan_ap::ErrorCode;
use alan_llm::GenerationResponse;
use serde_json::json;

const MOCK_GITHUB_STDIO_FILESERVER: &str = r#"#!/usr/bin/env python3
import json
import os
import sys

STATUS_PAYLOAD = json.dumps({"service": "github", "pid": os.getpid()}).encode("utf-8")
FIDS = {}
WRITES = []

def stat_for(node):
    if node == "root":
        return {"name": "", "qid": {"kind": "dir", "version": 0, "path": 1}, "length": 10, "executable": False, "writable": False}
    if node == "status":
        return {"name": "status", "qid": {"kind": "file", "version": 0, "path": 2}, "length": len(STATUS_PAYLOAD), "executable": False, "writable": False}
    if node == "ctl":
        return {"name": "ctl", "qid": {"kind": "file", "version": 0, "path": 3}, "length": 0, "executable": False, "writable": True}
    return None

def handle(req):
    op = req["op"]
    if op == "walk":
        fid, newfid, names = req["fid"], req["newfid"], req["names"]
        base = "root" if fid == 0 else FIDS.get(fid, {}).get("node")
        if base is None:
            return False, "not_found"
        if not names:
            FIDS[newfid] = {"node": base, "mode": None}
            return True, {"op": "walk", "qid": stat_for(base)["qid"]}
        if base == "root" and len(names) == 1 and names[0] in ("status", "ctl"):
            FIDS[newfid] = {"node": names[0], "mode": None}
            return True, {"op": "walk", "qid": stat_for(names[0])["qid"]}
        return False, "not_found"
    if op == "open":
        fid, mode = req["fid"], req["mode"]
        if fid == 0:
            return True, {"op": "open", "qid": stat_for("root")["qid"]}
        st = FIDS.get(fid)
        if st is None:
            return False, "not_found"
        if st["node"] == "status" and mode != "read":
            return False, "no_access"
        st["mode"] = mode
        return True, {"op": "open", "qid": stat_for(st["node"])["qid"]}
    if op == "stat":
        fid = req["fid"]
        node = "root" if fid == 0 else FIDS.get(fid, {}).get("node")
        if node is None:
            return False, "not_found"
        return True, {"op": "stat", "stat": stat_for(node)}
    if op == "read":
        fid, off, cnt = req["fid"], req["offset"], req["count"]
        node = "root" if fid == 0 else FIDS.get(fid, {}).get("node")
        if node == "root":
            raw = b"ctl\nstatus"
        elif node == "status":
            raw = STATUS_PAYLOAD
        elif node == "ctl":
            raw = b""
        else:
            return False, "not_found"
        return True, {"op": "read", "data": list(raw[off:off + cnt])}
    if op == "write":
        st = FIDS.get(req["fid"])
        if st is None or st["node"] != "ctl" or st["mode"] not in ("write", "read_write"):
            return False, "no_access"
        WRITES.append(bytes(req["data"]))
        return True, {"op": "write", "count": len(req["data"])}
    if op == "clunk":
        if req["fid"] != 0:
            FIDS.pop(req["fid"], None)
        return True, {"op": "clunk"}
    return False, "unsupported"

while True:
    line = sys.stdin.readline()
    if not line:
        break
    if not line.strip():
        continue
    frame = json.loads(line)
    ok, body = handle(frame["request"])
    resp = {"status": "ok", "tag": frame["tag"], "response": body} if ok else {"status": "error", "tag": frame["tag"], "code": body}
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#;

fn install_github_fileserver_package(store_root: &std::path::Path) -> Arc<PackageService> {
    let service = PackageService::open(store_root.to_path_buf()).unwrap();
    let snap = PackageSnapshot {
        source_name: "github-fileserver".to_string(),
        entries: vec![
            PackageSnapshotEntry {
                path: "alan-package.toml".to_string(),
                bytes: br#"
schema_version = 1
kind = "fileserver"
description = "Reference GitHub fileserver"

[fileserver]
id = "github"
description = "Mounts GitHub under /mnt/github"
entrypoint = "bin/githubfs"
allow_network = true
network_domains = ["api.github.com", "github.com"]

[fileserver.transport]
type = "stdio"
"#
                .to_vec(),
                executable: false,
            },
            PackageSnapshotEntry {
                path: "bin/githubfs".to_string(),
                bytes: MOCK_GITHUB_STDIO_FILESERVER.as_bytes().to_vec(),
                executable: true,
            },
        ],
    };
    service
        .execute(PackageCommand::Install {
            request_id: "install-github-fs".to_string(),
            package_id: "github-fileserver".to_string(),
            snapshot: snap,
        })
        .unwrap();
    service
}

fn tool_call_response(id: &str, name: &str, args: serde_json::Value) -> GenerationResponse {
    GenerationResponse {
        content: String::new(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: vec![alan_llm::ToolCall {
            id: Some(id.to_string()),
            name: name.to_string(),
            arguments: args,
        }],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

fn text_response(text: &str) -> GenerationResponse {
    GenerationResponse {
        content: text.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

async fn wait_for_pending_request_id(
    shell: &alan_shell::Shell,
    seen: &mut std::collections::HashSet<String>,
) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(entries) = shell.ls("/agent/root/requests").await {
                for id in entries {
                    if matches!(id.as_str(), "" | "events" | "clone") || seen.contains(&id) {
                        continue;
                    }
                    let status = shell
                        .cat(&format!("/agent/root/requests/{id}/status"))
                        .await
                        .ok();
                    let prompt = shell
                        .cat(&format!("/agent/root/requests/{id}/prompt"))
                        .await
                        .ok();
                    if status
                        .as_deref()
                        .is_some_and(|raw| String::from_utf8_lossy(raw).trim() == "pending")
                        && prompt.is_some_and(|p| !p.is_empty())
                    {
                        seen.insert(id.clone());
                        return id;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for pending confirmation request")
}

async fn wait_for_output_contains(shell: &alan_shell::Shell, needle: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = shell.cat("/agent/root/io/output").await
                && String::from_utf8_lossy(&bytes).contains(needle)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for output `{needle}`"));
}

#[tokio::test]
async fn request_service_hot_mounts_github_fileserver_for_session_and_persists_on_allow_persistent()
{
    let temp = tempfile::tempdir().unwrap();
    let pkg_store = temp.path().join("packages");
    let packages = install_github_fileserver_package(&pkg_store);
    assert!(
        packages
            .root_binding("github-fileserver")
            .unwrap()
            .is_none()
    );

    let mock = MockLlmProvider::new().with_responses(vec![
        // Turn 1: request_service -> reject
        tool_call_response(
            "call_reject",
            "request_service",
            json!({
                "package_id": "github-fileserver",
                "access": "read_only",
                "allow_network": false,
                "reason": "Inspect GitHub"
            }),
        ),
        text_response("turn1-declined"),
        // Turn 2: request_service -> approve (session only, read_only)
        tool_call_response(
            "call_session",
            "request_service",
            json!({
                "package_id": "github-fileserver",
                "access": "read_only",
                "allow_network": false,
                "reason": "Read GitHub status for this session"
            }),
        ),
        text_response("turn2-session-active"),
        // Turn 3: request_service -> allow_persistent (upgrade to read_write + network)
        tool_call_response(
            "call_persist",
            "request_service",
            json!({
                "package_id": "github-fileserver",
                "access": "read_write",
                "allow_network": true,
                "reason": "Write GitHub ctl persistently"
            }),
        ),
        text_response("turn3-persistent-active"),
        // Turn 4 (opened child session): verify request_service is excluded
        text_response("child-session-ok"),
    ]);

    let mut cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(mock.clone()),
        ToolRegistry::new(),
    );
    cfg.package_store = Some(pkg_store.clone());
    cfg.process.agent_config.core_config.memory.enabled = false;
    cfg.process.agent_config.core_config.streaming_mode = alan_agent_engine::StreamingMode::Off;

    let manager = ServiceManager::boot(cfg).await.unwrap();
    let root_shell = alan_shell::Shell::new(manager.root_namespace.clone());

    // Initially inactive and unmounted in both Root and LocalEntryService.
    assert_eq!(root_shell.ls("/mnt/github").await, Err(ErrorCode::NotFound));
    let local_before = alan_shell::Shell::new(InProcessTransport::new(
        manager.local_entry().namespace_for_local_client(),
    ));
    assert_eq!(
        local_before.ls("/mnt/github").await,
        Err(ErrorCode::NotFound)
    );

    // Turn 1: Reject leaves /mnt/github unmounted.
    let mut seen_requests = std::collections::HashSet::new();
    root_shell
        .write("/agent/root/io/input", b"Turn 1 check github")
        .await
        .unwrap();
    let req1 = wait_for_pending_request_id(&root_shell, &mut seen_requests).await;
    root_shell
        .write(
            &format!("/agent/root/requests/{req1}/response"),
            br#"{"choice":"reject"}"#,
        )
        .await
        .unwrap();
    wait_for_output_contains(&root_shell, "turn1-declined").await;
    assert_eq!(root_shell.ls("/mnt/github").await, Err(ErrorCode::NotFound));
    assert!(
        packages
            .root_binding("github-fileserver")
            .unwrap()
            .is_none()
    );

    // Turn 2: Approve (session-only, read_only) hot-mounts /mnt/github into Root + LocalEntryService
    // while leaving root_binding == None.
    root_shell
        .write("/agent/root/io/input", b"Turn 2 activate for session")
        .await
        .unwrap();
    let req2 = wait_for_pending_request_id(&root_shell, &mut seen_requests).await;
    let prompt2 = String::from_utf8(
        root_shell
            .cat(&format!("/agent/root/requests/{req2}/prompt"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        prompt2.trim(),
        "Activate 'github-fileserver' at /mnt/github (read-only, no-network)?"
    );
    root_shell
        .write(
            &format!("/agent/root/requests/{req2}/response"),
            br#"{"choice":"approve"}"#,
        )
        .await
        .unwrap();
    wait_for_output_contains(&root_shell, "turn2-session-active").await;

    let root_status = root_shell.cat("/mnt/github/status").await.unwrap();
    assert!(String::from_utf8_lossy(&root_status).contains("github"));
    assert_eq!(
        root_shell
            .write("/mnt/github/ctl", b"watch acme/alan")
            .await,
        Err(ErrorCode::NoAccess)
    );
    let local_session = alan_shell::Shell::new(InProcessTransport::new(
        manager.local_entry().namespace_for_local_client(),
    ));
    assert!(local_session.cat("/mnt/github/status").await.is_ok());
    assert!(
        packages
            .root_binding("github-fileserver")
            .unwrap()
            .is_none()
    );

    // Turn 3: allow_persistent upgrades to ReadWrite + network and persists RootBindingMode::Active.
    root_shell
        .write("/agent/root/io/input", b"Turn 3 upgrade persistently")
        .await
        .unwrap();
    let req3 = wait_for_pending_request_id(&root_shell, &mut seen_requests).await;
    let prompt3 = String::from_utf8(
        root_shell
            .cat(&format!("/agent/root/requests/{req3}/prompt"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        prompt3.trim(),
        "Upgrade 'github-fileserver' at /mnt/github (read-write, network)?"
    );
    root_shell
        .write(
            &format!("/agent/root/requests/{req3}/response"),
            br#"{"choice":"allow_persistent"}"#,
        )
        .await
        .unwrap();
    wait_for_output_contains(&root_shell, "turn3-persistent-active").await;

    root_shell
        .write("/mnt/github/ctl", b"watch acme/alan")
        .await
        .unwrap();
    let local_persisted = alan_shell::Shell::new(InProcessTransport::new(
        manager.local_entry().namespace_for_local_client(),
    ));
    local_persisted
        .write("/mnt/github/ctl", b"watch acme/alan")
        .await
        .unwrap();

    let binding = packages
        .root_binding("github-fileserver")
        .unwrap()
        .expect("allow_persistent must persist RootPackageBinding");
    assert_eq!(binding.mode, RootBindingMode::Active);
    let fs_activation = binding.fileserver.unwrap();
    assert_eq!(fs_activation.access, FileserverMountAccess::ReadWrite);
    assert!(fs_activation.allow_network);

    // Non-primary sessions must never receive `request_service`.
    let session = manager
        .open_session(SessionLaunchOptions::default())
        .await
        .unwrap();
    let session_shell = alan_shell::Shell::new(session.namespace.clone());
    session_shell
        .write(
            &format!("/agent/{}/io/input", session.pid.0),
            b"Check tools in secondary session",
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = session_shell
                .cat(&format!("/agent/{}/io/output", session.pid.0))
                .await
                && String::from_utf8_lossy(&bytes).contains("child-session-ok")
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("secondary session should complete");
    let recorded = mock.recorded_requests();
    let root_req = &recorded[0];
    assert!(
        root_req
            .tools
            .iter()
            .any(|tool| tool.name == "request_service")
    );
    let initial_context = root_req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == alan_llm::MessageRole::Context)
        .expect("expected initial service activation context");
    assert!(
        initial_context
            .content
            .contains("- `github-fileserver` -> `/mnt/github` (network capable) — Mounts GitHub under /mnt/github"),
        "{}",
        initial_context.content
    );
    let turn3_req = recorded
        .iter()
        .find(|req| {
            req.messages
                .iter()
                .any(|m| m.content.contains("Turn 3 upgrade persistently"))
        })
        .expect("expected Turn 3 request");
    let turn3_context = turn3_req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == alan_llm::MessageRole::Context)
        .expect("expected Turn 3 dynamic context");
    assert!(
        turn3_context.content.contains(
            "- `github-fileserver` -> `/mnt/github` (active: read-only, no-network) — Mounts GitHub under /mnt/github"
        ),
        "{}",
        turn3_context.content
    );
    let turn3_tool_reply_req = recorded
        .iter()
        .find(|req| {
            req.messages.iter().any(|m| {
                m.role == alan_llm::MessageRole::Tool
                    && m.content.contains("\"status\":\"activated\"")
            })
        })
        .expect("expected Turn 3 post-activation tool response");
    let activated_msg = turn3_tool_reply_req
        .messages
        .iter()
        .find(|m| {
            m.role == alan_llm::MessageRole::Tool && m.content.contains("\"status\":\"activated\"")
        })
        .unwrap();
    assert!(
        activated_msg
            .content
            .contains("\"description\":\"Mounts GitHub under /mnt/github\""),
        "{}",
        activated_msg.content
    );
    let child_req = recorded.last().unwrap();
    assert!(
        !child_req
            .tools
            .iter()
            .any(|tool| tool.name == "request_service")
    );

    session.close().await.unwrap();
    manager.shutdown().await.unwrap();
}
