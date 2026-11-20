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
        crate::skills::preinstalled_package_roots_for_tests(definition_root),
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
include!("tests/skill_observation.rs");
include!("tests/skill_behavior.rs");

#[tokio::test]
async fn live_namespace_reads_mounted_knowledge_rejects_unmounted_and_clears_on_revoke() {
    use crate::runtime::transition::NamespaceRuntimeEnvironment;
    use crate::tools::{
        ToolExecutionAdapter, ToolExecutionAuthority, ToolExecutionBinding, ToolProcessRunner,
        ToolRegistry,
    };
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Debug)]
    struct FakeAdapter;
    impl ToolExecutionAdapter for FakeAdapter {
        fn namespace_cwd(&self) -> PathBuf {
            PathBuf::from("/mnt/project")
        }
        fn cwd(&self) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/tmp"))
        }
        fn resolve_path(&self, _ns: &Path, p: &Path) -> anyhow::Result<PathBuf> {
            Ok(p.to_path_buf())
        }
        fn visible_path(&self, host_path: &Path) -> PathBuf {
            host_path.to_path_buf()
        }
        fn project_text(&self, text: &str) -> String {
            text.to_string()
        }
        fn sandbox(&self) -> anyhow::Result<crate::tools::Sandbox> {
            anyhow::bail!("unused")
        }
    }

    #[derive(Debug)]
    struct FakeAuthority {
        active: Arc<AtomicBool>,
    }
    impl ToolExecutionAuthority for FakeAuthority {
        fn reconcile(
            &self,
            _pid: u64,
            binding: ToolExecutionBinding,
        ) -> anyhow::Result<ToolExecutionBinding> {
            Ok(binding)
        }
        fn authority_revision(&self, _pid: u64) -> Option<u64> {
            Some(1)
        }
        fn project_knowledge_roots(&self, _pid: u64) -> Vec<PathBuf> {
            if self.active.load(Ordering::SeqCst) {
                vec![PathBuf::from("/mnt/project")]
            } else {
                Vec::new()
            }
        }
    }

    let project_fs = alan_ap::reference::MemFs::with_read_only_files([
        (
            "AGENTS.md".to_string(),
            b"Run `just test` before reporting done.\nNEVER enable --trust.".to_vec(),
        ),
        ("CLAUDE.md".to_string(), b"Keep functions modular.".to_vec()),
    ]);
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/mnt/project",
        alan_ap::InProcessTransport::new(Arc::new(project_fs)),
        alan_kernel::Access::ReadOnly,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));

    let registry = ToolRegistry::with_config(Arc::new(crate::Config::default()));
    let runner = ToolProcessRunner::from_registry(&registry);
    let active = Arc::new(AtomicBool::new(false));
    runner.register_process_authority(
        7,
        Arc::new(FakeAuthority {
            active: active.clone(),
        }),
    );
    runner.register_process_binding(
        7,
        ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from("/mnt/project"),
            std::env::temp_dir(),
        )
        .with_adapter(Arc::new(FakeAdapter)),
    );
    let env = NamespaceRuntimeEnvironment::new(root, "/agent/7", "default")
        .with_tool_process_context(7, runner);

    // Inactive/revoked root must not inject project knowledge.
    assert!(env.read_project_knowledge_section().await.is_none());

    // When active mount is present, AGENTS.md and CLAUDE.md are bounded & framed non-authoritatively.
    active.store(true, Ordering::SeqCst);
    let section = env.read_project_knowledge_section().await.unwrap();
    assert!(section.contains("<project_knowledge authoritative=\"false\""));
    assert!(section.contains("Run `just test` before reporting done."));
    assert!(section.contains("Keep functions modular."));

    let mut cache = PromptAssemblyCache::new(Vec::new());
    cache.set_project_knowledge_section(Some(section));
    let assembled = cache.build(None).system_prompt;
    assert!(assembled.contains("<project_knowledge authoritative=\"false\""));
    assert!(assembled.contains("Run `just test` before reporting done."));

    // Revoking immediately drops project knowledge.
    active.store(false, Ordering::SeqCst);
    assert!(env.read_project_knowledge_section().await.is_none());
}
