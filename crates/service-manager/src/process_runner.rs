//! Dispatches executable Process images owned by Alan OS services.

use alan_kernel::{ProcessInvocation, ProcessOutcome, ProcessRunner};
use async_trait::async_trait;

use crate::{
    agent_runtime::AgentRuntimeService,
    quartermaster::{QUARTERMASTER_EXECUTABLE, QuartermasterProcessRunner},
};

#[derive(Clone)]
pub(crate) struct SystemProcessRunner {
    quartermaster: QuartermasterProcessRunner,
    agent_runtime: Option<std::sync::Weak<AgentRuntimeService>>,
    fallback: Option<alan_agent_engine::tools::ToolProcessRunner>,
}

impl SystemProcessRunner {
    pub(crate) fn new(
        agent_runtime: Option<std::sync::Weak<AgentRuntimeService>>,
        fallback: Option<alan_agent_engine::tools::ToolProcessRunner>,
    ) -> Self {
        Self {
            quartermaster: QuartermasterProcessRunner,
            agent_runtime,
            fallback,
        }
    }
}

#[async_trait]
impl ProcessRunner for SystemProcessRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        if !invocation
            .namespace
            .describe()
            .iter()
            .any(|(path, _)| path == &invocation.exec.executable)
        {
            return ProcessOutcome::exited(127, b"executable is not mounted\n");
        }
        if invocation.exec.executable == crate::agent_work::EXECUTABLE {
            return crate::agent_work::AgentWorkProcessRunner
                .run(invocation)
                .await;
        }
        if invocation.exec.executable == QUARTERMASTER_EXECUTABLE {
            return self.quartermaster.run(invocation).await;
        }
        if invocation.exec.executable == "/bin/alan-agent" {
            let Some(runtime) = self
                .agent_runtime
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
            else {
                return ProcessOutcome::exited(127, b"Agent Runtime Service is unavailable\n");
            };
            return runtime.run_agent_process(invocation).await;
        }
        match &self.fallback {
            Some(runner) => {
                let outcome = runner
                    .run_in_namespace(
                        alan_agent_engine::tools::ToolProcessInvocation {
                            pid: invocation.pid.0,
                            parent: invocation.parent.map(|pid| pid.0),
                            executable: invocation.exec.executable,
                            args: invocation.exec.args,
                        },
                        alan_ap::InProcessTransport::new(std::sync::Arc::new(
                            alan_kernel::MountFs::new(invocation.namespace),
                        )),
                    )
                    .await;
                ProcessOutcome::exited(outcome.exit_code, outcome.output)
            }
            None => ProcessOutcome::exited(127, b"executable has no Process image\n"),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
