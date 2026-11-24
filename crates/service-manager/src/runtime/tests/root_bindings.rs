use super::*;
use crate::{
    FileserverMountAccess, PackageCommand, PackageSnapshot, PackageSnapshotEntry, RootBindingMode,
    RootFileserverActivation, RootPackageBinding,
};
use alan_ap::ErrorCode;

const PID_STATUS_STDIO_FILESERVER: &str = r#"#!/usr/bin/env python3
import json
import os
import sys

PID_PAYLOAD = json.dumps({"pid": os.getpid()}).encode("utf-8")
FIDS = {}
WRITES = []

def stat_for(node):
    if node == "root":
        return {"name": "", "qid": {"kind": "dir", "version": 0, "path": 1}, "length": 10, "executable": False, "writable": False}
    if node == "status":
        return {"name": "status", "qid": {"kind": "file", "version": 0, "path": 2}, "length": len(PID_PAYLOAD), "executable": False, "writable": False}
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
            FIDS[newfid] = {"node": base, "mode": None, "buf": bytearray()}
            return True, {"op": "walk", "qid": stat_for(base)["qid"]}
        if base == "root" and len(names) == 1 and names[0] in ("status", "ctl"):
            FIDS[newfid] = {"node": names[0], "mode": None, "buf": bytearray()}
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
            raw = PID_PAYLOAD
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

fn install_demo_fileserver_package(store_root: &std::path::Path) {
    let service = PackageService::open(store_root.to_path_buf()).unwrap();
    let snap = PackageSnapshot {
        source_name: "demo-plugin".to_string(),
        entries: vec![
            PackageSnapshotEntry {
                path: "alan-package.toml".to_string(),
                bytes: br#"
schema_version = 1
kind = "fileserver"
description = "Demo plugin with Companion Skill"

[fileserver]
id = "demo_srv"
entrypoint = "bin/server.py"
allow_network = false

[fileserver.transport]
type = "stdio"
"#
                .to_vec(),
                executable: false,
            },
            PackageSnapshotEntry {
                path: "bin/server.py".to_string(),
                bytes: PID_STATUS_STDIO_FILESERVER.as_bytes().to_vec(),
                executable: true,
            },
            PackageSnapshotEntry {
                path: "skills/demo-guide/SKILL.md".to_string(),
                bytes: b"---\nname: demo-guide\ndescription: Demo Companion Skill.\n---\nFollow demo.\n".to_vec(),
                executable: false,
            },
        ],
    };
    let installed = service
        .execute(PackageCommand::Install {
            request_id: "install-demo".to_string(),
            package_id: "demo-plugin".to_string(),
            snapshot: snap,
        })
        .unwrap();
    assert!(installed.success, "{}", installed.message);
}

#[cfg(unix)]
#[tokio::test]
async fn skills_only_and_active_fileserver_root_bindings_enforce_lifecycle_and_mounts() {
    let temp = tempfile::tempdir().unwrap();
    let pkg_store = temp.path().join("packages");
    install_demo_fileserver_package(&pkg_store);

    // 1. `SkillsOnly`: Companion Skill mounts at `/lib/pkg/demo-plugin` without spawning fileserver.
    let mut skills_only_cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    )
    .with_root_package_bindings(vec![RootPackageBinding {
        package_id: "demo-plugin".to_string(),
        mode: RootBindingMode::SkillsOnly,
        fileserver: None,
        updated_at: chrono::Utc::now(),
    }]);
    skills_only_cfg.package_store = Some(pkg_store.clone());

    let manager_ro_knowledge = ServiceManager::boot(skills_only_cfg).await.unwrap();
    assert!(
        manager_ro_knowledge
            .fileserver_manager()
            .list()
            .await
            .is_empty()
    );
    let root_shell = alan_shell::Shell::new(manager_ro_knowledge.root_namespace.clone());
    assert!(
        root_shell
            .ls("/lib/pkg/demo-plugin/skills/demo-guide")
            .await
            .is_ok()
    );
    assert_eq!(
        root_shell.cat("/mnt/demo_srv/status").await,
        Err(ErrorCode::NotFound)
    );
    let local_shell = alan_shell::Shell::new(InProcessTransport::new(
        manager_ro_knowledge
            .local_entry()
            .namespace_for_local_client(),
    ));
    assert_eq!(
        local_shell.cat("/mnt/demo_srv/status").await,
        Err(ErrorCode::NotFound)
    );
    manager_ro_knowledge.shutdown().await.unwrap();

    // 2. `Active` fileserver binding (`ReadOnly`): Companion Skill + `/srv/demo_srv` + `/mnt/demo_srv`
    // in BOTH Root Agent AND `LocalEntryService`, and `shutdown()` kills the fileserver process.
    let mut active_ro_cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    )
    .with_explicit_root_package_bindings(vec![RootPackageBinding {
        package_id: "demo-plugin".to_string(),
        mode: RootBindingMode::Active,
        fileserver: Some(RootFileserverActivation {
            access: FileserverMountAccess::ReadOnly,
            allow_network: false,
            host_grant_ids: Vec::new(),
        }),
        updated_at: chrono::Utc::now(),
    }]);
    active_ro_cfg.package_store = Some(pkg_store.clone());

    let manager_active = ServiceManager::boot(active_ro_cfg).await.unwrap();
    let fs_list = manager_active.fileserver_manager().list().await;
    assert_eq!(fs_list.len(), 1);
    assert!(fs_list[0].running);
    let child_pid = fs_list[0].pid.expect("running fileserver pid");

    let root_shell = alan_shell::Shell::new(manager_active.root_namespace.clone());
    assert!(
        root_shell
            .ls("/lib/pkg/demo-plugin/skills/demo-guide")
            .await
            .is_ok()
    );
    let status_json: serde_json::Value =
        serde_json::from_slice(&root_shell.cat("/mnt/demo_srv/status").await.unwrap()).unwrap();
    assert_eq!(status_json["pid"].as_u64(), Some(u64::from(child_pid)));
    // ReadOnly binding rejects write to `/mnt/demo_srv/ctl`.
    assert_eq!(
        root_shell.write("/mnt/demo_srv/ctl", b"ping").await,
        Err(ErrorCode::NoAccess)
    );

    // LocalEntryService local client (used by TUI `/issue`) also sees `/mnt/demo_srv`.
    let local_shell = alan_shell::Shell::new(InProcessTransport::new(
        manager_active.local_entry().namespace_for_local_client(),
    ));
    let local_status: serde_json::Value =
        serde_json::from_slice(&local_shell.cat("/mnt/demo_srv/status").await.unwrap()).unwrap();
    assert_eq!(local_status["pid"].as_u64(), Some(u64::from(child_pid)));

    manager_active.shutdown().await.unwrap();

    // Confirm fileserver OS process exited cleanly on shutdown.
    // SAFETY: signal 0 probes process existence without delivering any signal to `child_pid`.
    let rc = unsafe { libc::kill(child_pid as libc::pid_t, 0) };
    assert_eq!(rc, -1, "fileserver process {child_pid} must be terminated");

    // 3. Explicit missing package binding fails closed even if a prior valid binding succeeded.
    let mut fail_cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    )
    .with_explicit_root_package_bindings(vec![
        RootPackageBinding {
            package_id: "demo-plugin".to_string(),
            mode: RootBindingMode::Active,
            fileserver: Some(RootFileserverActivation {
                access: FileserverMountAccess::ReadWrite,
                allow_network: false,
                host_grant_ids: Vec::new(),
            }),
            updated_at: chrono::Utc::now(),
        },
        RootPackageBinding {
            package_id: "missing-package".to_string(),
            mode: RootBindingMode::Active,
            fileserver: None,
            updated_at: chrono::Utc::now(),
        },
    ]);
    fail_cfg.package_store = Some(pkg_store);
    assert!(ServiceManager::boot(fail_cfg).await.is_err());
}

fn install_test_tool_package(store_root: &std::path::Path, package_id: &str, tool_names: &[&str]) {
    let service = PackageService::open(store_root.to_path_buf()).unwrap();
    let mut manifest =
        String::from("schema_version = 1\nkind = \"tool\"\ndescription = \"Test tool package\"\n");
    let mut entries = Vec::new();
    for tool in tool_names {
        manifest.push_str(&format!(
            "\n[[tools]]\nname = \"{tool}\"\ndescription = \"{tool}\"\nentrypoint = \"bin/{tool}.sh\"\nparameters = {{ type = \"object\" }}\n",
        ));
        entries.push(PackageSnapshotEntry {
            path: format!("bin/{tool}.sh"),
            bytes: b"#!/bin/sh\necho '{\"ok\":true}'\n".to_vec(),
            executable: true,
        });
    }
    entries.push(PackageSnapshotEntry {
        path: "alan-package.toml".to_string(),
        bytes: manifest.into_bytes(),
        executable: false,
    });
    let res = service
        .execute(PackageCommand::Install {
            request_id: format!("install-{package_id}"),
            package_id: package_id.to_string(),
            snapshot: PackageSnapshot {
                source_name: package_id.to_string(),
                entries,
            },
        })
        .unwrap();
    assert!(res.success, "{}", res.message);
}

#[tokio::test]
async fn durable_root_binding_collision_rolls_back_all_partial_tool_projections() {
    let temp = tempfile::tempdir().unwrap();
    let pkg_store = temp.path().join("packages");
    install_test_tool_package(&pkg_store, "a-pre-tool", &["beta_tool"]);
    install_test_tool_package(&pkg_store, "b-multi-tool", &["alpha_tool", "beta_tool"]);
    install_test_tool_package(&pkg_store, "c-clean-tool", &["gamma_tool"]);

    let package_service = PackageService::open(pkg_store).unwrap();
    let srvfs = Arc::new(alan_kernel::SrvFs::new());
    let fileserver_manager = Arc::new(crate::FileserverManager::new(
        package_service.clone(),
        srvfs.clone(),
    ));
    let system_namespace = alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new());
    let local_entry =
        crate::LocalEntryService::new(alan_kernel::ProcFs::new(), system_namespace.clone());
    let mut launch_context = ProcessLaunchContext::root();

    let bindings = ["a-pre-tool", "b-multi-tool", "c-clean-tool"]
        .into_iter()
        .map(|id| RootPackageBinding {
            package_id: id.to_string(),
            mode: RootBindingMode::Active,
            fileserver: None,
            updated_at: chrono::Utc::now(),
        })
        .collect::<Vec<_>>();

    crate::runtime::packages::apply_root_package_bindings(
        &package_service,
        &fileserver_manager,
        &srvfs,
        &system_namespace,
        &local_entry,
        &mut launch_context,
        alan_agent_engine::TrustPosture::Governed,
        &bindings,
        &[],
    )
    .await
    .unwrap();

    let mounted_paths: Vec<String> = launch_context
        .namespace
        .describe()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert!(
        !mounted_paths.iter().any(|p| p == "/bin/alpha_tool"),
        "{mounted_paths:?}"
    );
    assert!(
        !mounted_paths.iter().any(|p| p == "/lib/exec/alpha_tool"),
        "{mounted_paths:?}"
    );
    assert!(
        !mounted_paths.iter().any(|p| p == "/lib/pkg/b-multi-tool"),
        "{mounted_paths:?}"
    );
    assert!(mounted_paths.iter().any(|p| p == "/bin/beta_tool"));
    assert!(mounted_paths.iter().any(|p| p == "/bin/gamma_tool"));

    let tool_names: Vec<&str> = launch_context
        .packaged_tools
        .iter()
        .map(|t| t.name())
        .collect();
    assert_eq!(tool_names, vec!["beta_tool", "gamma_tool"]);

    let manifest_names: Vec<&str> = launch_context
        .packaged_tool_manifests
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(manifest_names, vec!["beta_tool", "gamma_tool"]);

    let ref_ids: Vec<&str> = launch_context
        .package_references
        .iter()
        .map(|r| r.package_id.as_str())
        .collect();
    assert_eq!(ref_ids, vec!["a-pre-tool", "c-clean-tool"]);
}

#[tokio::test]
async fn boot_preinstalls_github_fileserver_catalog_only_without_ambient_authority() {
    let cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    let manager = ServiceManager::boot(cfg).await.unwrap();
    let local_shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(
        manager.local_entry().namespace_for_local_client(),
    ));

    let srv_entries = local_shell.ls("/srv").await.unwrap();
    assert!(
        !srv_entries.contains(&"github".to_string()),
        "catalog-only preinstalled github-fileserver must not publish /srv/github: {srv_entries:?}"
    );
    assert_eq!(
        local_shell.ls("/mnt/github").await,
        Err(ErrorCode::NotFound)
    );
    let root_shell = alan_shell::Shell::new(manager.root_namespace.clone());
    let root_pkg_entries = root_shell.ls("/lib/pkg").await.unwrap();
    assert!(
        !root_pkg_entries.contains(&"github-fileserver".to_string()),
        "catalog-only preinstalled github-fileserver must not be projected into Root: {root_pkg_entries:?}"
    );
    assert_eq!(root_shell.ls("/mnt/github").await, Err(ErrorCode::NotFound));

    let catalog_bytes = local_shell.cat("/mnt/package/catalog").await.unwrap();
    let catalog: crate::PackageCatalog = serde_json::from_slice(&catalog_bytes).unwrap();
    let gh = catalog
        .packages
        .get("github-fileserver")
        .expect("github-fileserver preinstalled in catalog");
    assert_eq!(gh.kind, crate::PackageKind::Preinstalled);
    assert_eq!(gh.export_kind, crate::PackageExportKind::Fileserver);
    assert!(
        manager
            .package
            .root_binding("github-fileserver")
            .unwrap()
            .is_none()
    );

    manager.shutdown().await.unwrap();
}
