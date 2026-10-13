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
#[path = "process_runner_tests.rs"]
mod tests;
