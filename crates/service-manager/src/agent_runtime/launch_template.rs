//! Invocation-scoped launch inputs; child construction selects its own capabilities.

use super::{
    AgentProcessConfig, ProcessLaunchContext, ProviderCapabilities, SkillHostCapabilities,
};

#[derive(Clone)]
pub(crate) struct RootAgentTemplate {
    pub(super) process: AgentProcessConfig,
    pub(crate) resume_persisted_rollout: bool,
    pub(super) launch_context: ProcessLaunchContext,
    pub(super) host_capabilities: SkillHostCapabilities,
    pub(super) generation_capabilities: ProviderCapabilities,
    pub(super) llm_connection: String,
    pub(super) input_shadow: Option<crate::InputShadowSelection>,
    pub(crate) guardian_shadow: Option<crate::GuardianShadowSelection>,
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
            input_shadow,
            guardian_shadow: None,
            launch_context,
            host_capabilities,
            generation_capabilities,
            llm_connection,
        }
    }
}
