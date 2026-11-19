use crate::{
    agent_machine::AgentMachine,
    config::GuardianConfig,
    policy::{PolicyEngine, TrustPosture},
    runtime::transition::{NamespaceAgentFiles, NamespaceGeneration, NamespaceToolAuthorization},
};

/// Explicit inputs for one Tool policy and approval transition.
pub(crate) struct ToolAuthorizationRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) policy_engine: &'a PolicyEngine,
    pub(super) governance: &'a alan_agent_protocol::GovernanceConfig,
    pub(super) generation: NamespaceGeneration,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) llm_request_timeout_secs: u64,
    pub(super) trust_posture: TrustPosture,
    pub(super) guardian_profile: Option<&'a str>,
    pub(super) guardian_config: GuardianConfig,
    pub(super) authority: Option<NamespaceToolAuthorization>,
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
            guardian_config: GuardianConfig::default(),
            authority: None,
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

    pub(crate) fn with_guardian_config(mut self, guardian_config: GuardianConfig) -> Self {
        self.guardian_config = guardian_config;
        self
    }

    pub(crate) fn with_authority(mut self, authority: NamespaceToolAuthorization) -> Self {
        self.authority = Some(authority);
        self
    }
}
