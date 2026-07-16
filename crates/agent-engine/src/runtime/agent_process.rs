//! Host-owned Agent Process assembly boundary.

use std::{path::PathBuf, sync::Arc};

#[cfg(test)]
use alan_ap::InProcessTransport;
use anyhow::{Result, bail};
use async_trait::async_trait;

use super::{NamespaceRuntimeEnvironment, ToolPackageManifest};

/// Complete child namespace plan selected by the parent Agent Process.
#[derive(Debug, Clone)]
pub struct ChildAgentProcessAssemblyPlan {
    pub agent_mount: String,
    pub llm_mount: String,
    pub llm_connection_name: String,
    pub srv_mount: String,
    pub route_mount: String,
    pub bin_tool_mounts: Vec<String>,
    pub tool_packages: Vec<ToolPackageManifest>,
    pub cwd: Option<PathBuf>,
    pub launch_context: crate::ProcessLaunchContext,
}

impl ChildAgentProcessAssemblyPlan {
    pub fn bin_tool_names(&self) -> impl Iterator<Item = &str> {
        self.bin_tool_mounts
            .iter()
            .filter_map(|mount| mount.strip_prefix("/bin/"))
    }

    pub fn llm_connection_name(&self) -> Result<String> {
        if self.llm_connection_name.is_empty() || self.llm_connection_name.contains('/') {
            bail!(
                "child namespace plan has invalid llm connection name '{}'",
                self.llm_connection_name
            );
        }
        Ok(self.llm_connection_name.clone())
    }
}

/// Inputs selected by the Agent Execution Engine for host-owned child assembly.
#[derive(Clone)]
pub struct ChildAgentProcessAssemblyRequest {
    pub plan: ChildAgentProcessAssemblyPlan,
    pub scratch_dir: Option<PathBuf>,
    pub executable: String,
    #[cfg(test)]
    pub llm_override: Option<InProcessTransport>,
}

/// Lifecycle authority retained by Agent Runtime Service after assembly.
#[async_trait]
pub trait AgentProcessLifecycle: std::fmt::Debug + Send + Sync {
    async fn finish(&self, exit_code: i32);
}

/// An Agent Process namespace assembled before its transition loop starts.
pub struct AssembledChildAgentProcess {
    pub pid: String,
    pub environment: NamespaceRuntimeEnvironment,
    pub observation_environment: NamespaceRuntimeEnvironment,
    pub lifecycle: Arc<dyn AgentProcessLifecycle>,
}

/// Process-bound Agent Runtime Service capability for creating child Agent Processes.
#[async_trait]
pub trait ChildAgentProcessAssembler: std::fmt::Debug + Send + Sync {
    async fn assemble(
        &self,
        request: ChildAgentProcessAssemblyRequest,
    ) -> Result<AssembledChildAgentProcess>;
}
