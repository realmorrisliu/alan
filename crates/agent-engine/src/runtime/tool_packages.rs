use alan_agent_protocol::ToolCapability;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::tools::{Tool, ToolLocality};

const TOOL_MANIFEST_VERSION: u16 = 1;

/// Namespace-owned model and execution metadata for one Tool package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolPackageManifest {
    pub version: u16,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub capability: ToolCapability,
    pub locality: ToolPackageLocality,
    pub timeout_secs: usize,
    pub execution: ToolExecutionHints,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolPackageLocality {
    Global,
    WorkspaceLocal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolExecutionHints {
    pub arguments: String,
    pub result: String,
}

impl ToolPackageManifest {
    pub(crate) fn is_workspace_local(&self) -> bool {
        self.locality == ToolPackageLocality::WorkspaceLocal
    }

    pub(crate) fn model_definition(&self) -> alan_llm::ToolDefinition {
        alan_llm::ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        }
    }

    pub(crate) fn from_tool(tool: &dyn Tool, timeout_secs: usize) -> Result<Self> {
        let manifest = Self {
            version: TOOL_MANIFEST_VERSION,
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters_schema(),
            capability: tool.capability(&serde_json::Value::Null),
            locality: match tool.locality() {
                ToolLocality::Global => ToolPackageLocality::Global,
                ToolLocality::WorkspaceLocal => ToolPackageLocality::WorkspaceLocal,
            },
            timeout_secs,
            execution: ToolExecutionHints {
                arguments: "json_first_arg".to_string(),
                result: "stdout_json".to_string(),
            },
        };
        manifest.validate_for_name(tool.name())?;
        Ok(manifest)
    }

    pub(crate) fn validate_for_name(&self, mounted_name: &str) -> Result<()> {
        if self.version != TOOL_MANIFEST_VERSION {
            bail!("unsupported Tool manifest version {}", self.version);
        }
        if self.name != mounted_name || self.name.is_empty() || self.name.contains('/') {
            bail!("Tool manifest name does not match mounted package '{mounted_name}'");
        }
        if self.description.trim().is_empty() || !self.parameters.is_object() {
            bail!("Tool manifest '{mounted_name}' lacks model metadata");
        }
        if self.timeout_secs == 0 {
            bail!("Tool manifest '{mounted_name}' has a zero timeout hint");
        }
        if self.execution.arguments != "json_first_arg" || self.execution.result != "stdout_json" {
            bail!("Tool manifest '{mounted_name}' has unsupported execution hints");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ToolContext, ToolResult};

    struct ExampleTool;

    impl Tool for ExampleTool {
        fn name(&self) -> &str {
            "example"
        }
        fn description(&self) -> &str {
            "Example Tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(&self, _: serde_json::Value, _: &ToolContext) -> ToolResult {
            Box::pin(async { Ok(serde_json::Value::Null) })
        }
    }

    #[test]
    fn manifest_round_trips_and_validates_against_mount_name() {
        let manifest = ToolPackageManifest::from_tool(&ExampleTool, 30).unwrap();
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let decoded: ToolPackageManifest = serde_json::from_slice(&bytes).unwrap();
        decoded.validate_for_name("example").unwrap();
        assert!(decoded.validate_for_name("other").is_err());
    }
}
