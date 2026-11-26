use crate::{
    agent_machine::AgentMachine,
    config::GuardianConfig,
    policy::{PolicyEngine, TrustPosture},
    runtime::transition::{NamespaceAgentFiles, NamespaceGeneration, NamespaceToolAuthorization},
};

/// Explicit inputs for one Tool policy and approval transition.
pub(crate) struct ToolAuthorizationRuntime<'a> {
    pub(crate) machine: &'a mut AgentMachine,
    pub(crate) policy_engine: &'a PolicyEngine,
    pub(crate) governance: &'a alan_agent_protocol::GovernanceConfig,
    pub(crate) generation: NamespaceGeneration,
    pub(crate) agent_files: NamespaceAgentFiles,
    pub(crate) llm_request_timeout_secs: u64,
    pub(crate) trust_posture: TrustPosture,
    pub(crate) guardian_profile: Option<&'a str>,
    pub(crate) smart_evaluator_profile: Option<&'a str>,
    pub(crate) guardian_config: GuardianConfig,
    pub(crate) authority: Option<NamespaceToolAuthorization>,
    pub(crate) tool_policy_hook: Option<std::sync::Arc<dyn crate::runtime::ToolPolicyHook>>,
}

impl<'a> ToolAuthorizationRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        policy_engine: &'a PolicyEngine,
        governance: &'a alan_agent_protocol::GovernanceConfig,
        generation: NamespaceGeneration,
        agent_files: NamespaceAgentFiles,
        llm_request_timeout_secs: u64,
    ) -> Self {
        Self {
            machine,
            policy_engine,
            governance,
            generation,
            agent_files,
            llm_request_timeout_secs,
            trust_posture: TrustPosture::Governed,
            guardian_profile: None,
            smart_evaluator_profile: None,
            guardian_config: GuardianConfig::default(),
            authority: None,
            tool_policy_hook: None,
        }
    }

    pub(crate) fn with_trust_posture(mut self, trust_posture: TrustPosture) -> Self {
        self.trust_posture = trust_posture;
        self
    }

    pub(crate) fn with_guardian_profile(mut self, guardian_profile: Option<&'a str>) -> Self {
        self.guardian_profile = guardian_profile;
        self
    }

    pub(crate) fn with_smart_evaluator_profile(
        mut self,
        smart_evaluator_profile: Option<&'a str>,
    ) -> Self {
        self.smart_evaluator_profile = smart_evaluator_profile;
        self
    }

    pub(crate) fn with_guardian_config(mut self, guardian_config: GuardianConfig) -> Self {
        self.guardian_config = guardian_config;
        self
    }

    pub(crate) fn with_authority(mut self, authority: NamespaceToolAuthorization) -> Self {
        self.authority = Some(authority);
        self
    }

    pub(crate) fn with_tool_policy_hook(
        mut self,
        hook: Option<std::sync::Arc<dyn crate::runtime::ToolPolicyHook>>,
    ) -> Self {
        self.tool_policy_hook = hook;
        self
    }
}
