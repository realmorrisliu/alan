use super::*;

pub(super) async fn run_deferred_runtime_actions(state: &mut RuntimeLoopState) -> usize {
    let cancel = CancellationToken::new();
    let actions = state.machine.drain_deferred_runtime_actions();
    let count = actions.len();
    for action in actions {
        assert_eq!(
            crate::runtime::transition::run_deferred_runtime_action_with_cancel(
                state, action, &cancel,
            )
            .await,
            crate::runtime::transition::DeferredRuntimeActionExit::Completed,
            "run deferred runtime action"
        );
    }
    count
}

pub(super) fn prompt_cache_for_definition_root(
    definition_root: &std::path::Path,
    definition_persona_dirs: Vec<std::path::PathBuf>,
) -> crate::runtime::prompt_cache::PromptAssemblyCache {
    let capability_view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
        path: definition_root.join("skills"),
        scope: SkillScope::Descriptor,
    }]);
    crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
        capability_view,
        definition_persona_dirs,
        crate::skills::SkillHostCapabilities::default(),
    )
}

pub(super) fn create_repo_skill(
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
