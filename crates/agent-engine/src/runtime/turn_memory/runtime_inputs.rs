use std::path::PathBuf;

use crate::agent_machine::AgentMachine;

/// Explicit inputs for finalizing one turn's Memory Store state.
pub(crate) struct TurnMemoryRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) memory_dir: Option<PathBuf>,
    pub(super) process_path: String,
    pub(super) project_cwd: Option<String>,
    pub(super) llm_request_timeout_secs: u64,
}

impl<'a> TurnMemoryRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        memory_dir: Option<PathBuf>,
        process_path: String,
        llm_request_timeout_secs: u64,
    ) -> Self {
        Self {
            machine,
            memory_dir,
            process_path,
            project_cwd: None,
            llm_request_timeout_secs,
        }
    }

    pub(crate) fn with_project_cwd(mut self, project_cwd: Option<String>) -> Self {
        self.project_cwd = project_cwd;
        self
    }
}
