//! Prompt assembly from explicitly supplied Agent Definition and Memory Store content.

use crate::config::Config;
use std::path::PathBuf;

/// Build the base prompt plus an explicitly configured Memory Store.
///
/// Agent Definition persona directories are launch descriptors and are supplied by the
/// runtime-specific assembly path, never discovered from Host directories here.
pub fn build_agent_system_prompt(config: &Config, domain_prompt: &str) -> String {
    let memory_context = config
        .memory
        .enabled
        .then_some(config.memory.store_dir.as_ref())
        .flatten()
        .filter(|path| path.exists())
        .map(|path| super::memory::render_memory_store_context(path));
    build_agent_system_prompt_from_sections(domain_prompt, &[], memory_context.as_deref())
}

#[allow(
    dead_code,
    reason = "persona-directory adapter remains available to focused white-box tests during descriptor migration"
)]
pub(crate) fn build_agent_system_prompt_from_persona_dirs(
    domain_prompt: &str,
    definition_persona_dirs: &[PathBuf],
) -> String {
    build_agent_system_prompt_from_sections(domain_prompt, definition_persona_dirs, None)
}

fn build_agent_system_prompt_from_sections(
    domain_prompt: &str,
    definition_persona_dirs: &[PathBuf],
    memory_context: Option<&str>,
) -> String {
    let definition_context = definition_persona_dirs
        .iter()
        .any(|path| path.exists())
        .then(|| {
            super::definition::render_definition_persona_context_from_dirs(definition_persona_dirs)
        });
    build_agent_system_prompt_with_sections(
        domain_prompt,
        definition_context.as_deref(),
        memory_context,
    )
}

pub(crate) fn build_agent_system_prompt_with_sections(
    domain_prompt: &str,
    definition_context: Option<&str>,
    memory_context: Option<&str>,
) -> String {
    let mut prompt = String::new();
    append_prompt_section(&mut prompt, super::RUNTIME_BASE_PROMPT);
    append_prompt_section(&mut prompt, super::SYSTEM_PROMPT);
    append_prompt_section(&mut prompt, domain_prompt);
    if let Some(definition_context) = definition_context {
        append_prompt_section(&mut prompt, definition_context);
    }
    if let Some(memory_context) = memory_context {
        append_prompt_section(&mut prompt, memory_context);
    }
    prompt
}

fn append_prompt_section(prompt: &mut String, section: &str) {
    let trimmed = section.trim();
    if trimmed.is_empty() {
        return;
    }
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    prompt.push_str(trimmed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembly_keeps_fixed_order() {
        let prompt = build_agent_system_prompt_with_sections(
            "DOMAIN_SENTINEL",
            Some("DEFINITION_SENTINEL"),
            Some("MEMORY_SENTINEL"),
        );
        let domain = prompt.find("DOMAIN_SENTINEL").unwrap();
        let definition = prompt.find("DEFINITION_SENTINEL").unwrap();
        let memory = prompt.find("MEMORY_SENTINEL").unwrap();
        assert!(domain < definition && definition < memory);
    }

    #[test]
    fn no_definition_directory_is_discovered_from_memory_store() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("persona")).unwrap();
        std::fs::write(temp.path().join("persona/AGENTS.md"), "implicit persona").unwrap();
        let config = Config {
            memory: crate::config::MemoryConfig {
                enabled: false,
                store_dir: Some(temp.path().to_path_buf()),
                strict_store: true,
            },
            ..Config::default()
        };
        assert!(!build_agent_system_prompt(&config, "domain").contains("implicit persona"));
    }
}
