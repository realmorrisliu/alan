use alan_agent_engine::{AgentProcessConfig, skills::SkillHostCapabilities};
use alan_ap::InProcessTransport;
use alan_kernel::Pid;
use alan_llm::ProviderCapabilities;
use anyhow::Result;

use crate::ProcessLaunchContext;

#[derive(Clone)]
pub(crate) struct RootAgentTemplate {
    pub(super) process: AgentProcessConfig,
    pub(crate) resume_persisted_rollout: bool,
    pub(crate) resume_rollout_id: Option<String>,
    /// Never read or overwrite the selected-rollout channel for this invocation.
    pub(crate) ephemeral_rollout: bool,
    /// Skip writing `metadata/root-rollout` while keeping normal memory, context, and `--resume`.
    pub(crate) no_resume_update: bool,
    pub(super) launch_context: ProcessLaunchContext,
    pub(super) host_capabilities: SkillHostCapabilities,
    pub(super) generation_capabilities: ProviderCapabilities,
    pub(super) llm_connection: String,
    pub(crate) allow_profile_switch: bool,
    pub(super) input_shadow: Option<crate::InputShadowSelection>,
    pub(super) task_budget_binding: alan_agent_engine::TaskBudgetBinding,
}

impl RootAgentTemplate {
    pub(crate) fn new(
        process: AgentProcessConfig,
        launch_context: ProcessLaunchContext,
        host_capabilities: SkillHostCapabilities,
        generation_capabilities: ProviderCapabilities,
        llm_connection: String,
        resume_persisted_rollout: bool,
        input_shadow: Option<crate::InputShadowSelection>,
    ) -> Self {
        Self {
            task_budget_binding: Default::default(),
            process,
            resume_persisted_rollout,
            resume_rollout_id: None,
            ephemeral_rollout: false,
            no_resume_update: false,
            input_shadow,
            launch_context,
            host_capabilities,
            generation_capabilities,
            llm_connection,
            allow_profile_switch: false,
        }
    }

    /// Allow this Root Agent Process to switch between ready Connection profiles.
    pub(crate) fn with_allow_profile_switch(mut self, allow: bool) -> Self {
        self.allow_profile_switch = allow;
        self
    }

    /// Keep this invocation's rollout out of the `metadata/root-rollout` selector.
    pub(crate) fn with_ephemeral_rollout(mut self, ephemeral_rollout: bool) -> Self {
        self.ephemeral_rollout = ephemeral_rollout;
        self
    }

    /// Skip updating `metadata/root-rollout` on Root Agent startup.
    pub(crate) fn with_no_resume_update(mut self, no_resume_update: bool) -> Self {
        self.no_resume_update = no_resume_update;
        self
    }

    /// Pin an explicit persisted rollout ID to recover on Root Agent startup.
    pub(crate) fn with_resume_rollout_id(mut self, resume_rollout_id: Option<String>) -> Self {
        self.resume_rollout_id = resume_rollout_id;
        self
    }
}

pub(crate) struct RootAgentProcess {
    pub(super) pid: Pid,
    pub(super) namespace: InProcessTransport,
    pub(super) ready: Option<tokio::sync::oneshot::Receiver<std::result::Result<(), String>>>,
    pub(super) stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl RootAgentProcess {
    pub(crate) fn pid(&self) -> Pid {
        self.pid
    }

    pub(crate) fn namespace(&self) -> InProcessTransport {
        self.namespace.clone()
    }

    pub(crate) async fn wait_until_ready(&mut self) -> Result<()> {
        let Some(ready) = self.ready.take() else {
            return Ok(());
        };
        match ready.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(anyhow::anyhow!(message)),
            Err(_) => Err(anyhow::anyhow!(
                "Agent Runtime Service stopped before Root Agent readiness"
            )),
        }
    }
}
