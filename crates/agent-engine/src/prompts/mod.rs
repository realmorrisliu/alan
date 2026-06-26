//! Prompt management module.
//!
//! This module provides access to prompt templates that guide the agent's behavior.
//! Prompts are embedded at compile time for zero-cost runtime access.

mod assembler;
mod loader;
mod memory;
mod workspace;

pub(crate) use assembler::build_agent_system_prompt_with_workspace_sections;
pub use assembler::{build_agent_system_prompt, build_agent_system_prompt_for_workspace};
pub use loader::PromptLoader;
pub use memory::{MEMORY_DAILY_DIRNAME, ensure_workspace_memory_layout_at};
#[allow(unused_imports)]
pub(crate) use memory::{
    MEMORY_INBOX_DIRNAME, MEMORY_TOPICS_DIRNAME, MEMORY_USER_FILENAME, WORKSPACE_MEMORY_FILENAME,
    render_workspace_memory_context, workspace_memory_tracked_paths,
};
pub use workspace::ensure_workspace_bootstrap_files_at;
#[allow(unused_imports)]
pub(crate) use workspace::{
    render_workspace_persona_context, render_workspace_persona_context_from_dirs,
    workspace_persona_tracked_paths, workspace_persona_tracked_paths_from_dirs,
};

// ============================================================================
// Compile-time embedded prompts
// ============================================================================

/// Runtime base prompt - hard constraints that cannot be overridden
pub const RUNTIME_BASE_PROMPT: &str = include_str!("../../prompts/runtime_base.md");

/// Main system prompt defining the agent's role and behavior
pub const SYSTEM_PROMPT: &str = include_str!("../../prompts/system.md");

/// Compaction prompt for summarizing older conversation history
pub const COMPACT_PROMPT: &str = r#"You are performing a CONTEXT CHECKPOINT COMPACTION. Create a handoff summary for another LLM that will resume the task.

Include:
- Current progress and key decisions made
- Important context, constraints, or user preferences
- What remains to be done (clear next steps)
- Any critical data, examples, or references needed to continue
- If a previous compaction summary is included, integrate its key points into your new summary

Be concise, structured, and focused on helping the next LLM seamlessly continue the work.
"#;

/// Memory-flush prompt for persisting durable context before automatic compaction.
pub const MEMORY_FLUSH_PROMPT: &str = include_str!("../../prompts/memory_flush.md");

/// Memory-promotion prompt for model-assisted turn-end durable write planning.
pub const MEMORY_PROMOTION_PROMPT: &str = include_str!("../../prompts/memory_promotion.md");
