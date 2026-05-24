//! Prompt assembly logic.

use crate::config::Config;
use std::path::{Path, PathBuf};

/// Build agent system prompt with proper assembly order:
/// 1. Runtime Base (hard constraints - cannot be overridden)
/// 2. System Prompt (default identity and behavior)
/// 3. Domain Prompt (skills/domain overlays loaded dynamically)
/// 4. Workspace Profile (persona files)
///
/// This convenience helper resolves the current AgentRoot persona overlays from
/// the caller's available workspace context. Runtime launches should prefer the
/// explicit persona dirs resolved on `ResolvedAgentDefinition`.
pub fn build_agent_system_prompt(config: &Config, domain_prompt: &str) -> String {
    let workspace_persona_dirs = resolved_workspace_persona_dirs(config, None);
    let memory_context = resolved_workspace_memory_dir(config, None)
        .filter(|path| path.exists())
        .map(|path| super::memory::render_workspace_memory_context(&path));
    build_agent_system_prompt_from_sections(
        domain_prompt,
        &workspace_persona_dirs,
        memory_context.as_deref(),
    )
}

/// Convenience helper that resolves AgentRoot persona overlays from an
/// explicit workspace path plus the current global home.
pub fn build_agent_system_prompt_for_workspace(
    config: &Config,
    domain_prompt: &str,
    workspace_dir: Option<&Path>,
) -> String {
    let workspace_persona_dirs = resolved_workspace_persona_dirs(config, workspace_dir);
    let memory_context = resolved_workspace_memory_dir(config, workspace_dir)
        .filter(|path| path.exists())
        .map(|path| super::memory::render_workspace_memory_context(&path));
    build_agent_system_prompt_from_sections(
        domain_prompt,
        &workspace_persona_dirs,
        memory_context.as_deref(),
    )
}

#[allow(dead_code)]
pub(crate) fn build_agent_system_prompt_from_persona_dirs(
    domain_prompt: &str,
    workspace_persona_dirs: &[PathBuf],
) -> String {
    build_agent_system_prompt_from_sections(domain_prompt, workspace_persona_dirs, None)
}

fn build_agent_system_prompt_from_sections(
    domain_prompt: &str,
    workspace_persona_dirs: &[PathBuf],
    memory_context: Option<&str>,
) -> String {
    let workspace_context = if workspace_persona_dirs.iter().any(|path| path.exists()) {
        Some(super::workspace::render_workspace_persona_context_from_dirs(workspace_persona_dirs))
    } else {
        None
    };
    build_agent_system_prompt_with_workspace_sections(
        domain_prompt,
        workspace_context.as_deref(),
        memory_context,
    )
}

pub(crate) fn resolved_workspace_persona_dirs(
    config: &Config,
    workspace_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let workspace_root_dir = workspace_dir
        .map(normalize_workspace_root)
        .or_else(|| infer_workspace_root_from_config(config));
    crate::ResolvedAgentRoots::with_home_paths(
        crate::AlanHomePaths::detect(),
        workspace_root_dir.as_deref(),
        None,
    )
    .persona_dirs()
}

#[allow(dead_code)]
pub(crate) fn build_agent_system_prompt_with_workspace_context(
    domain_prompt: &str,
    workspace_context: Option<&str>,
) -> String {
    build_agent_system_prompt_with_workspace_sections(domain_prompt, workspace_context, None)
}

pub(crate) fn build_agent_system_prompt_with_workspace_sections(
    domain_prompt: &str,
    workspace_context: Option<&str>,
    memory_context: Option<&str>,
) -> String {
    let mut prompt = String::new();

    append_prompt_section(&mut prompt, super::RUNTIME_BASE_PROMPT);
    append_prompt_section(&mut prompt, super::SYSTEM_PROMPT);
    append_prompt_section(&mut prompt, domain_prompt);
    if let Some(workspace_context) = workspace_context {
        append_prompt_section(&mut prompt, workspace_context);
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

fn infer_workspace_root_from_config(config: &Config) -> Option<PathBuf> {
    let path = config.memory.workspace_dir.as_deref()?;
    infer_workspace_root_from_memory_dir(path).or_else(|| Some(normalize_workspace_root(path)))
}

fn resolved_workspace_memory_dir(config: &Config, workspace_dir: Option<&Path>) -> Option<PathBuf> {
    if !config.memory.enabled {
        return None;
    }

    workspace_dir
        .map(|path| {
            let workspace_root = normalize_workspace_root(path);
            crate::workspace_memory_dir_for_channel_from_alan_dir(
                &crate::workspace_alan_dir(workspace_root.as_path()),
                crate::InstallChannel::detect_current(),
            )
        })
        .or_else(|| config.memory.workspace_dir.clone())
}

fn infer_workspace_root_from_memory_dir(path: &Path) -> Option<PathBuf> {
    if path.file_name()? != std::ffi::OsStr::new("memory") {
        return None;
    }
    let parent = path.parent()?;
    if parent.file_name()? == std::ffi::OsStr::new(".alan") {
        return parent.parent().map(Path::to_path_buf);
    }
    if parent
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == std::ffi::OsStr::new("runtime"))
    {
        let alan_dir = parent.parent()?.parent()?;
        if alan_dir.file_name()? == std::ffi::OsStr::new(".alan") {
            return alan_dir.parent().map(Path::to_path_buf);
        }
    }
    None
}

fn normalize_workspace_root(path: &Path) -> PathBuf {
    let is_alan_dir = path
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(".alan"))
        .unwrap_or(false);
    if is_alan_dir {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use std::fs;
    use tempfile::TempDir;

    fn test_config_with_workspace(temp_dir: &TempDir) -> Config {
        let mut config = Config::default();
        config.memory.workspace_dir = Some(temp_dir.path().join(".alan/memory"));
        config.memory.strict_workspace = false;
        config
    }

    #[test]
    fn test_build_agent_system_prompt_includes_runtime_base() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config_with_workspace(&temp_dir);
        let workspace_persona_dirs = resolved_workspace_persona_dirs(&config, None);

        let prompt =
            build_agent_system_prompt_from_persona_dirs("Domain Prompt", &workspace_persona_dirs);

        assert!(prompt.contains("Runtime Base Constraints"));
        assert!(prompt.contains("No Self-Modification"));
        assert!(prompt.contains("alan System Prompt"));
        assert!(prompt.contains("Domain Prompt"));
    }

    #[test]
    fn test_build_agent_system_prompt_injects_workspace_context() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config_with_workspace(&temp_dir);
        let persona_dir = temp_dir.path().join(".alan/agents/default/persona");
        crate::prompts::ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        let workspace_persona_dirs = resolved_workspace_persona_dirs(&config, None);

        let prompt =
            build_agent_system_prompt_from_persona_dirs("Domain Prompt", &workspace_persona_dirs);

        assert!(prompt.contains("Workspace Persona Context"));
        assert!(prompt.contains("### SOUL.md"));
        assert!(persona_dir.join("SOUL.md").exists());
        assert!(persona_dir.join("ROLE.md").exists());
    }

    #[test]
    fn test_build_agent_system_prompt_uses_existing_workspace_file_content() {
        let temp_dir = TempDir::new().unwrap();
        let persona_dir = temp_dir.path().join(".alan/agents/default/persona");
        fs::create_dir_all(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        fs::write(persona_dir.join("SOUL.md"), "custom persona instructions").unwrap();

        let config = test_config_with_workspace(&temp_dir);
        let workspace_persona_dirs = resolved_workspace_persona_dirs(&config, None);
        let prompt =
            build_agent_system_prompt_from_persona_dirs("Domain Prompt", &workspace_persona_dirs);

        assert!(prompt.contains("custom persona instructions"));
    }

    #[test]
    fn test_build_agent_system_prompt_truncates_large_workspace_content() {
        let temp_dir = TempDir::new().unwrap();
        crate::prompts::ensure_workspace_bootstrap_files_at(temp_dir.path()).unwrap();
        let large = "a".repeat(10_000);
        fs::write(temp_dir.path().join("ROLE.md"), large).unwrap();

        let workspace_context = crate::prompts::render_workspace_persona_context(temp_dir.path());
        let prompt = build_agent_system_prompt_with_workspace_context(
            "Domain Prompt",
            Some(&workspace_context),
        );

        assert!(prompt.contains("[...truncated, read ROLE.md for full content...]"));
    }

    #[test]
    fn test_prompt_assembly_order() {
        let temp_dir = TempDir::new().unwrap();
        let persona_dir = temp_dir.path().join(".alan/agents/default/persona");
        fs::create_dir_all(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        fs::write(persona_dir.join("SOUL.md"), "SOUL content").unwrap();

        let config = test_config_with_workspace(&temp_dir);
        let workspace_persona_dirs = resolved_workspace_persona_dirs(&config, None);
        let prompt =
            build_agent_system_prompt_from_persona_dirs("DOMAIN content", &workspace_persona_dirs);

        let runtime_base_pos = prompt.find("Runtime Base Constraints").unwrap();
        let system_pos = prompt.find("alan System Prompt").unwrap();
        let skills_pos = prompt.find("DOMAIN content").unwrap();
        let workspace_pos = prompt.find("Workspace Persona Context").unwrap();

        assert!(runtime_base_pos < system_pos);
        assert!(system_pos < skills_pos);
        assert!(skills_pos < workspace_pos);
    }

    #[test]
    fn test_build_agent_system_prompt_includes_memory_persistence_guidance() {
        let temp_dir = TempDir::new().unwrap();
        let persona_dir = temp_dir.path().join(".alan/agents/default/persona");
        let memory_dir = temp_dir.path().join(".alan/memory");
        fs::create_dir_all(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let config = test_config_with_workspace(&temp_dir);
        let workspace_persona_dirs = resolved_workspace_persona_dirs(&config, None);
        let prompt =
            build_agent_system_prompt_from_persona_dirs("DOMAIN content", &workspace_persona_dirs);

        assert!(
            prompt.contains("persist it to the appropriate workspace memory or user-context file")
        );
        assert!(prompt.contains("Do not re-read them with tools by default"));
    }

    #[test]
    fn test_build_agent_system_prompt_includes_memory_bootstrap_when_memory_dir_exists() {
        let temp_dir = TempDir::new().unwrap();
        let persona_dir = temp_dir.path().join(".alan/agents/default/persona");
        let memory_dir = temp_dir.path().join(".alan/memory");
        fs::create_dir_all(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        fs::write(memory_dir.join("USER.md"), "# User Memory\n- Morris\n").unwrap();

        let config = test_config_with_workspace(&temp_dir);
        let prompt = build_agent_system_prompt(&config, "DOMAIN content");

        assert!(prompt.contains("Workspace Memory Bootstrap"));
        assert!(prompt.contains("Resolved from:"));
        assert!(prompt.contains("Write updates to:"));
        assert!(prompt.contains("# User Memory"));
    }

    #[test]
    fn test_build_agent_system_prompt_omits_memory_bootstrap_when_memory_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let persona_dir = temp_dir.path().join(".alan/agents/default/persona");
        let memory_dir = temp_dir.path().join(".alan/memory");
        fs::create_dir_all(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        fs::write(memory_dir.join("USER.md"), "# User Memory\n- Morris\n").unwrap();

        let mut config = test_config_with_workspace(&temp_dir);
        config.memory.enabled = false;
        let prompt = build_agent_system_prompt(&config, "DOMAIN content");

        assert!(!prompt.contains("Workspace Memory Bootstrap"));
        assert!(!prompt.contains("# User Memory"));
    }

    #[test]
    fn test_build_agent_system_prompt_does_not_create_workspace_files() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config_with_workspace(&temp_dir);
        let workspace_persona_dirs = resolved_workspace_persona_dirs(&config, None);

        let prompt =
            build_agent_system_prompt_from_persona_dirs("Domain Prompt", &workspace_persona_dirs);

        assert!(prompt.contains("Domain Prompt"));
        assert!(
            !temp_dir
                .path()
                .join(".alan/agents/default/persona/SOUL.md")
                .exists()
        );
        assert!(
            !temp_dir
                .path()
                .join(".alan/agents/default/persona/ROLE.md")
                .exists()
        );
    }

    #[test]
    fn test_resolved_workspace_persona_dirs_prefer_workspace_from_memory_dir() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.memory.workspace_dir = Some(temp_dir.path().join(".alan/memory"));

        let persona_dirs = resolved_workspace_persona_dirs(&config, None);

        assert_eq!(
            persona_dirs.last(),
            Some(&temp_dir.path().join(".alan/agents/default/persona"))
        );
    }

    #[test]
    fn test_resolved_workspace_persona_dirs_treats_dot_alan_as_workspace_state_dir() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path().join("workspace");
        let persona_dirs = resolved_workspace_persona_dirs(
            &Config::default(),
            Some(&workspace_root.join(".alan")),
        );

        assert_eq!(
            persona_dirs.last(),
            Some(&workspace_root.join(".alan/agents/default/persona"))
        );
    }
}
