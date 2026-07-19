use std::path::PathBuf;

use crate::agent_machine::AgentMachine;

use super::super::transition::{NamespaceAgentFiles, NamespaceGeneration};

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompactionSettings {
    pub(super) trigger_messages: usize,
    pub(super) keep_last: usize,
    pub(super) context_window_tokens: u32,
    pub(super) soft_trigger_ratio: f32,
    pub(super) hard_trigger_ratio: f32,
}

impl CompactionSettings {
    pub(crate) fn new(
        trigger_messages: usize,
        keep_last: usize,
        context_window_tokens: u32,
        soft_trigger_ratio: f32,
        hard_trigger_ratio: f32,
    ) -> Self {
        Self {
            trigger_messages,
            keep_last,
            context_window_tokens,
            soft_trigger_ratio,
            hard_trigger_ratio,
        }
    }
}

pub(crate) struct CompactionMemory {
    pub(super) enabled: bool,
    pub(super) store_dir: Option<PathBuf>,
    pub(super) process_path: String,
}

impl CompactionMemory {
    pub(crate) fn new(enabled: bool, store_dir: Option<PathBuf>, process_path: String) -> Self {
        Self {
            enabled,
            store_dir,
            process_path,
        }
    }
}

pub(crate) struct CompactionRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) generation: NamespaceGeneration,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) settings: CompactionSettings,
    pub(super) memory: CompactionMemory,
}

impl<'a> CompactionRuntime<'a> {
    pub(crate) fn new(
        machine: &'a mut AgentMachine,
        generation: NamespaceGeneration,
        agent_files: NamespaceAgentFiles,
        settings: CompactionSettings,
        memory: CompactionMemory,
    ) -> Self {
        Self {
            machine,
            generation,
            agent_files,
            settings,
            memory,
        }
    }
}
