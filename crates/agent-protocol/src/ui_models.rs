//! Safe Process-local model observation contract. No Connection restore revision or secrets.
use crate::{ReasoningControls, ReasoningEffort, UI_SURFACE_VERSION};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiModelCatalog {
    pub profile: String,
    pub models: Vec<UiModelChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiModelChoice {
    pub model: String,
    #[serde(default)]
    pub supported_reasoning_efforts: Vec<ReasoningEffort>,
    #[serde(default)]
    pub default_reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiModelBinding {
    pub profile: String,
    pub provider: String,
    pub model: String,
    pub reasoning: ReasoningControls,
    pub control_source: UiModelControlSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiModelControlSource {
    TurnOverride,
    AgentMachineOverride,
    AgentConfig,
    ModelDefault,
    ProviderDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiAdmittedModel {
    pub submission_id: String,
    /// None means legacy or unavailable capture, not launch defaults.
    pub binding: Option<UiModelBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiModelSnapshot {
    pub version: u16,
    /// Monotonic only within process_path. Not a Connection revision.
    pub publication_version: u64,
    pub process_path: String,
    pub known: bool,
    pub catalog: Option<UiModelCatalog>,
    pub selected_next: Option<UiModelBinding>,
    pub active: Option<UiModelBinding>,
    pub admitted: Vec<UiAdmittedModel>,
}
impl Default for UiModelSnapshot {
    fn default() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            publication_version: 0,
            process_path: String::new(),
            known: false,
            catalog: None,
            selected_next: None,
            active: None,
            admitted: Vec::new(),
        }
    }
}
impl UiModelSnapshot {
    pub fn is_valid(&self) -> bool {
        self.version == UI_SURFACE_VERSION
            && (!self.known
                || (self.publication_version > 0 && self.process_path.starts_with("/proc/")))
    }
    /// Reject older observations only within the same Process; recovery has a new owner.
    pub fn supersedes(&self, previous: &Self) -> bool {
        self.is_valid()
            && self.known
            && (self.process_path != previous.process_path
                || self.publication_version >= previous.publication_version)
    }
}
