//! Child Agent Process launch capabilities passed through the namespace boundary.

use std::sync::Arc;

use super::NamespaceChildLaunch;

impl NamespaceChildLaunch {
    pub(crate) fn connection_name(&self) -> &str {
        &self.llm_connection
    }

    pub(crate) fn launch_context(&self) -> Option<&crate::ProcessLaunchContext> {
        self.launch_context.as_ref()
    }

    pub(crate) fn assembler(&self) -> Option<Arc<dyn crate::runtime::ChildAgentProcessAssembler>> {
        self.child_process_assembler.clone()
    }
}
