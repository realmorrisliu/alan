//! Child Agent Process launch capabilities passed through the namespace boundary.

use super::NamespaceChildLaunch;

impl NamespaceChildLaunch {
    pub(crate) fn connection_name(&self) -> &str {
        &self.llm_connection
    }

    pub(crate) fn namespace_cwd(&self) -> &std::path::Path {
        &self.namespace_cwd
    }

    pub(crate) fn process_files(&self) -> &super::NamespaceProcessFiles {
        &self.process_files
    }

    pub(crate) fn observation_handles(
        &self,
        pid: &str,
    ) -> (super::NamespaceAgentFiles, super::NamespaceProcessFiles) {
        let agent_path = format!("/agent/{pid}");
        (
            super::NamespaceAgentFiles {
                root: self.process_files.root.clone(),
                agent_path: agent_path.clone(),
                input_offset: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                control_offset: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            },
            super::NamespaceProcessFiles {
                root: self.process_files.root.clone(),
                agent_path,
            },
        )
    }
}
