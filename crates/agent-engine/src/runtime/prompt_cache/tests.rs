use super::*;
use crate::prompts::{ensure_definition_bootstrap_files_at, ensure_memory_store_layout_at};
use crate::skills::{
    ResolvedSkillExecution, ScopedPackageDir, SkillActivationReason,
    SkillExecutionResolutionSource, SkillHostCapabilities, SkillOverride, SkillScope,
};
use sha2::{Digest, Sha256};

fn create_definition_skill(
    definition_root: &std::path::Path,
    dir_name: &str,
    skill_name: &str,
    description: &str,
    body: &str,
) {
    let skill_dir = definition_root.join("skills").join(dir_name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            r#"---
name: {skill_name}
description: {description}
---

{body}
"#
        ),
    )
    .unwrap();
}

fn create_definition_skill_with_frontmatter(
    definition_root: &std::path::Path,
    dir_name: &str,
    frontmatter: &str,
    body: &str,
) {
    let skill_dir = definition_root.join("skills").join(dir_name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\n{frontmatter}\n---\n\n{body}\n"),
    )
    .unwrap();
}

fn create_definition_child_agent(
    definition_root: &std::path::Path,
    package_dir: &str,
    agent_name: &str,
) {
    let agent_root = definition_root
        .join("skills")
        .join(package_dir)
        .join("agents")
        .join(agent_name);
    std::fs::create_dir_all(&agent_root).unwrap();
    std::fs::write(
        agent_root.join("agent.toml"),
        "llm_provider = \"openai_responses\"\n",
    )
    .unwrap();
}

fn capability_view_for_definition_root(
    definition_root: &std::path::Path,
) -> ResolvedCapabilityView {
    ResolvedCapabilityView::from_package_sources(
        vec![ScopedPackageDir {
            path: definition_root.join("skills"),
            scope: SkillScope::Descriptor,
        }],
        crate::skills::preinstalled_package_roots_for_tests(),
    )
}

fn prompt_cache_for_definition_root(
    definition_root: &std::path::Path,
    definition_persona_dirs: Vec<PathBuf>,
) -> PromptAssemblyCache {
    PromptAssemblyCache::with_fixed_capability_view(
        capability_view_for_definition_root(definition_root),
        definition_persona_dirs,
        SkillHostCapabilities::default(),
    )
}

fn prompt_cache_for_definition_root_with_overrides(
    definition_root: &std::path::Path,
    skill_overrides: Vec<SkillOverride>,
    definition_persona_dirs: Vec<PathBuf>,
) -> PromptAssemblyCache {
    PromptAssemblyCache::with_fixed_capability_view_and_overrides(
        capability_view_for_definition_root(definition_root),
        skill_overrides,
        definition_persona_dirs,
        SkillHostCapabilities::default(),
    )
}

include!("tests/cache_behavior.rs");
include!("tests/skill_behavior.rs");
