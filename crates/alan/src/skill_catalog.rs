use alan_agent_engine::skills::{
    SkillHostCapabilities, SkillsRegistry, build_skill_host_capabilities,
};
use alan_agent_engine::{
    AgentRootKind, Config, ResolvedAgentDefinition, ToolRegistry, WorkspaceRuntimeConfig,
};
use alan_tools::{create_core_tools, register_builtin_tool_catalog};
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct SkillCatalogContext {
    pub resolved: ResolvedAgentDefinition,
    pub registry: SkillsRegistry,
    pub host_capabilities: SkillHostCapabilities,
}

pub fn resolve_skill_catalog_context(
    runtime_config: &WorkspaceRuntimeConfig,
) -> Result<SkillCatalogContext> {
    let resolved = ResolvedAgentDefinition::from_runtime_config(runtime_config)?;
    let registry =
        SkillsRegistry::load_capability_view(&resolved.capability_view, &resolved.skill_overrides)?;
    let host_capabilities =
        resolve_skill_host_capabilities(&runtime_config.agent_config.core_config, &resolved)?;
    Ok(SkillCatalogContext {
        resolved,
        registry,
        host_capabilities,
    })
}

pub fn resolve_skill_host_capabilities(
    base_config: &Config,
    resolved: &ResolvedAgentDefinition,
) -> Result<SkillHostCapabilities> {
    let mut core_config = base_config.clone();
    if !resolved.config_overlay_paths.is_empty() {
        core_config = core_config.with_agent_root_overlays(&resolved.config_overlay_paths)?;
    }
    let mut tools = ToolRegistry::with_config(Arc::new(core_config));
    if resolved.workspace_root_dir.is_some() {
        register_builtin_tool_catalog(&mut tools);
        for tool in create_core_tools() {
            tools.register_boxed(tool);
        }
    }
    let delegated_supported = !resolved
        .roots
        .roots()
        .iter()
        .any(|root| matches!(root.kind, AgentRootKind::LaunchRoot));
    Ok(build_skill_host_capabilities(
        tools.list_tools().into_iter().map(str::to_string),
        delegated_supported,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::skills::SkillScope;
    use alan_agent_engine::{AgentRootPaths, ResolvedAgentRoots};
    use tempfile::TempDir;

    #[test]
    fn top_level_catalogs_support_delegated_invocation() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let resolved = ResolvedAgentDefinition {
            roots: ResolvedAgentRoots::default(),
            workspace_root_dir: Some(workspace_root.clone()),
            workspace_alan_dir: Some(workspace_root.join(".alan")),
            agent_name: None,
            config_overlay_paths: Vec::new(),
            persona_dirs: Vec::new(),
            capability_view: alan_agent_engine::skills::ResolvedCapabilityView::from_package_dirs(
                vec![alan_agent_engine::skills::ScopedPackageDir {
                    path: workspace_root.join(".alan/agents/default/skills"),
                    scope: SkillScope::Repo,
                }],
            ),
            skill_overrides: Vec::new(),
            default_policy_path: None,
            writable_root_dir: None,
            writable_config_path: None,
            writable_persona_dir: None,
        };

        let capabilities = resolve_skill_host_capabilities(&Config::default(), &resolved).unwrap();

        assert!(capabilities.supports_delegated_skill_invocation());
        assert!(capabilities.supports_required_tool("invoke_delegated_skill"));
    }

    #[test]
    fn launch_roots_do_not_support_delegated_invocation() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let launch_root = temp.path().join("child-agent");
        std::fs::create_dir_all(&workspace_root).unwrap();
        std::fs::create_dir_all(&launch_root).unwrap();
        let resolved = ResolvedAgentDefinition {
            roots: ResolvedAgentRoots::default()
                .with_appended_root(AgentRootPaths::new(AgentRootKind::LaunchRoot, launch_root)),
            workspace_root_dir: Some(workspace_root.clone()),
            workspace_alan_dir: Some(workspace_root.join(".alan")),
            agent_name: None,
            config_overlay_paths: Vec::new(),
            persona_dirs: Vec::new(),
            capability_view: alan_agent_engine::skills::ResolvedCapabilityView::default(),
            skill_overrides: Vec::new(),
            default_policy_path: None,
            writable_root_dir: None,
            writable_config_path: None,
            writable_persona_dir: None,
        };

        let capabilities = resolve_skill_host_capabilities(&Config::default(), &resolved).unwrap();

        assert!(!capabilities.supports_delegated_skill_invocation());
        assert!(!capabilities.supports_required_tool("invoke_delegated_skill"));
    }
}
