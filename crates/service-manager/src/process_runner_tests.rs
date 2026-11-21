use std::collections::BTreeMap;
use std::sync::Arc;

use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, ExecSpec, Namespace, Pid};

use super::*;

fn agent_invocation(mounted: bool) -> ProcessInvocation {
    let mut namespace = Namespace::new();
    if mounted {
        namespace.mount(
            "/bin/alan-agent",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
    }
    ProcessInvocation {
        pid: Pid(2),
        parent: Some(Pid(1)),
        credentials: Credentials::user("agent"),
        namespace,
        exec: ExecSpec {
            executable: "/bin/alan-agent".to_string(),
            args: Vec::new(),
            namespace: alan_kernel::ExecNamespaceManifest::default(),
            descriptors: BTreeMap::new(),
        },
    }
}

struct NamespaceReadProbe;

impl alan_agent_engine::tools::Tool for NamespaceReadProbe {
    fn name(&self) -> &str {
        "namespace_read_probe"
    }

    fn description(&self) -> &str {
        "Read own Action output through the invocation namespace"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","required":["path"],
            "properties":{"path":{"type":"string"}}})
    }

    fn execute(
        &self,
        args: serde_json::Value,
        context: &alan_agent_engine::tools::ToolContext,
    ) -> alan_agent_engine::tools::ToolResult {
        context
            .read_namespace_evidence(args["path"].as_str().unwrap(), 0, 4096)
            .unwrap()
    }
}

#[tokio::test]
async fn tool_dispatch_uses_actual_parent_and_invocation_namespace_without_host_authority() {
    use alan_agent_engine::tools::{ToolExecutionBinding, ToolRegistry};
    let mut registry = ToolRegistry::new();
    registry.register(NamespaceReadProbe);
    let tools = registry.process_runner();
    for owner in [8, 9] {
        tools.register_process_binding(
            owner,
            ToolExecutionBinding::awaiting_host_projection("/".into(), "".into()),
        );
    }
    let runner = SystemProcessRunner::new(None, Some(tools));
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin/namespace_read_probe",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    for owner in [8, 9] {
        namespace.mount(
            &format!("/agent/{owner}/actions/a2"),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "output",
                b"retained original".to_vec(),
            ))),
            Access::ReadOnly,
        );
    }
    for (parent, exit) in [(8, 0), (9, 1)] {
        let outcome = runner
            .run(ProcessInvocation {
                pid: Pid(20),
                parent: Some(Pid(parent)),
                credentials: Credentials::user("agent"),
                namespace: namespace.clone(),
                exec: ExecSpec {
                    executable: "/bin/namespace_read_probe".into(),
                    args: vec![
                        serde_json::json!({"path":"/agent/8/actions/a2/output"}).to_string(),
                    ],
                    namespace: alan_kernel::ExecNamespaceManifest::default(),
                    descriptors: BTreeMap::new(),
                },
            })
            .await;
        assert_eq!(outcome.exit_code, exit);
        let result: serde_json::Value = serde_json::from_slice(&outcome.output).unwrap();
        if exit == 0 {
            assert_eq!(result["content"], "retained original");
        } else {
            assert!(
                result["error"]
                    .as_str()
                    .unwrap()
                    .contains("receiving Agent")
            );
        }
    }
}

#[tokio::test]
async fn agent_process_image_requires_the_executable_mount_before_dispatch() {
    let runner = SystemProcessRunner::new(None, None);

    let missing = runner.run(agent_invocation(false)).await;
    assert_eq!(missing.exit_code, 127);
    assert_eq!(missing.output, b"executable is not mounted\n");

    let mut broad = agent_invocation(false);
    broad.namespace.mount(
        "/bin",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    let broad = runner.run(broad).await;
    assert_eq!(broad.exit_code, 127);
    assert_eq!(broad.output, b"executable is not mounted\n");

    let unavailable = runner.run(agent_invocation(true)).await;
    assert_eq!(unavailable.exit_code, 127);
    assert_eq!(
        unavailable.output,
        b"Agent Runtime Service is unavailable\n"
    );
}

struct NamespaceWriteProbe;

impl alan_agent_engine::tools::Tool for NamespaceWriteProbe {
    fn name(&self) -> &str {
        "namespace_write_probe"
    }

    fn description(&self) -> &str {
        "Attempt an aP write through the Tool Process namespace"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": { "path": { "type": "string" } }
        })
    }

    fn execute(
        &self,
        args: serde_json::Value,
        context: &alan_agent_engine::tools::ToolContext,
    ) -> alan_agent_engine::tools::ToolResult {
        let transport = context.namespace().cloned();
        let path = args["path"].as_str().unwrap_or("").to_string();
        Box::pin(async move {
            let transport = transport.ok_or_else(|| anyhow::anyhow!("missing namespace"))?;
            let names: Vec<String> = path
                .split('/')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect();
            let fid = alan_ap::Fid(777);
            transport
                .call(alan_ap::Request::Walk {
                    fid: alan_ap::Fid::ROOT,
                    newfid: fid,
                    names,
                })
                .await?;
            let stat_writable = match transport.call(alan_ap::Request::Stat { fid }).await? {
                alan_ap::Response::Stat { stat } => stat.writable,
                _ => false,
            };
            let open_res = transport
                .call(alan_ap::Request::Open {
                    fid,
                    mode: alan_ap::OpenMode::Write,
                })
                .await;
            let _ = transport.call(alan_ap::Request::Clunk { fid }).await;
            open_res?;
            Ok(serde_json::json!({ "writable": stat_writable }))
        })
    }
}

#[tokio::test]
async fn tool_dispatch_clamps_system_mounts_read_only_while_keeping_data_mounts_writable() {
    use alan_agent_engine::tools::{ToolExecutionBinding, ToolRegistry};
    use alan_ap::{Fid, FileKind, FileServer};

    async fn tree_with_file(name: &str) -> InProcessTransport {
        let fs = Arc::new(alan_ap::reference::MemFs::writable());
        fs.create(Fid::ROOT, Fid(1), name, FileKind::File)
            .await
            .unwrap();
        fs.clunk(Fid(1)).await.unwrap();
        InProcessTransport::new(fs)
    }

    let mut registry = ToolRegistry::new();
    registry.register(NamespaceWriteProbe);
    let tools = registry.process_runner();
    tools.register_process_binding(
        8,
        ToolExecutionBinding::awaiting_host_projection("/".into(), "".into()),
    );
    let runner = SystemProcessRunner::new(None, Some(tools));

    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin/namespace_write_probe",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    namespace.mount("/proc/8", tree_with_file("ctl").await, Access::ReadWrite);
    namespace.mount("/agent/8", tree_with_file("ctl").await, Access::ReadWrite);
    namespace.mount(
        "/mnt/connections",
        tree_with_file("ctl").await,
        Access::ReadWrite,
    );
    namespace.mount(
        "/memory",
        tree_with_file("USER.md").await,
        Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/slowpage",
        tree_with_file("draft.md").await,
        Access::ReadWrite,
    );

    for (path, expected_exit) in [
        ("/proc/8/ctl", 1),
        ("/agent/8/ctl", 1),
        ("/mnt/connections/ctl", 1),
        ("/memory/USER.md", 0),
        ("/mnt/slowpage/draft.md", 0),
    ] {
        let outcome = runner
            .run(ProcessInvocation {
                pid: Pid(21),
                parent: Some(Pid(8)),
                credentials: Credentials::user("agent"),
                namespace: namespace.clone(),
                exec: ExecSpec {
                    executable: "/bin/namespace_write_probe".into(),
                    args: vec![serde_json::json!({ "path": path }).to_string()],
                    namespace: alan_kernel::ExecNamespaceManifest::default(),
                    descriptors: BTreeMap::new(),
                },
            })
            .await;
        assert_eq!(
            outcome.exit_code,
            expected_exit,
            "path={path}, output={}",
            String::from_utf8_lossy(&outcome.output)
        );
    }
}
