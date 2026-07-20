use crate::runtime::child_agents::ChildLaunchRuntime;
use crate::runtime::launch_config::AgentConfig;
use alan_agent_protocol::{GovernanceConfig, SpawnHandle, SpawnSpec, SpawnTarget};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

pub(super) fn ensure_child_connection_is_passed(
    parent: &ChildLaunchRuntime,
    requested: &str,
) -> Result<()> {
    let passed = parent.child_launch.connection_name();
    if requested != passed {
        bail!(
            "Connection '{requested}' was not passed to the child Agent Process by the parent Process; available Connection is '{passed}'."
        );
    }
    Ok(())
}

pub(super) fn build_child_launch_context(
    parent: &crate::ProcessLaunchContext,
    spec: &SpawnSpec,
    child_cwd: Option<String>,
    launch_root_dir: Option<&ResolvedLaunchRoot>,
) -> Result<crate::ProcessLaunchContext> {
    let memory_descriptor = parent.descriptor(crate::MEMORY_STORE_DESCRIPTOR).cloned();
    let mut launch_context = parent.child();
    launch_context.descriptors.clear();
    launch_context.cwd = child_cwd.unwrap_or_else(|| "/".to_string());
    if spec.has_handle(SpawnHandle::Memory) {
        if let Some(descriptor) = memory_descriptor {
            launch_context
                .descriptors
                .insert(crate::MEMORY_STORE_DESCRIPTOR.to_string(), descriptor);
        }
    } else {
        launch_context.namespace.unmount("/memory");
    }

    if let Some(ResolvedLaunchRoot {
        root_dir,
        file_tree: Some(file_tree),
    }) = launch_root_dir
    {
        let descriptor_path = root_dir
            .to_str()
            .context("package child Agent Executable descriptor path is not UTF-8")?;
        launch_context.descriptors.insert(
            crate::AGENT_DEFINITION_DESCRIPTOR.to_string(),
            crate::ProcessDescriptor::with_file_tree(descriptor_path, file_tree.clone())?,
        );
    } else if let Some(ResolvedLaunchRoot { root_dir, .. }) = launch_root_dir {
        bail!(
            "child Agent Definition {} must be passed as an immutable file-tree descriptor",
            root_dir.display()
        );
    }
    Ok(launch_context)
}

pub(super) fn validate_child_launch_contract(spec: &SpawnSpec) -> Result<Option<String>> {
    if spec.has_handle(SpawnHandle::Artifacts) || spec.launch.output_dir.is_some() {
        bail!(
            "Child Agent Process launches do not support artifact routing yet; omit SpawnHandle::Artifacts and launch.output_dir."
        );
    }

    if let Some(cwd) = spec.launch.cwd.as_deref()
        && !cwd.is_absolute()
    {
        bail!(
            "Child Agent Process launch cwd '{}' must be absolute.",
            cwd.display()
        );
    }

    let cwd = spec
        .launch
        .cwd
        .as_deref()
        .map(|cwd| {
            let cwd = cwd.to_str().with_context(|| {
                format!(
                    "Child Agent Process launch cwd '{}' must be valid Unicode.",
                    cwd.display()
                )
            })?;
            crate::process_launch::normalize_namespace_path(cwd)
                .with_context(|| format!("Invalid child Agent Process launch cwd '{}'.", cwd))
        })
        .transpose()?;

    Ok(cwd)
}

pub(super) fn resolve_launch_root_dir(
    parent: &ChildLaunchRuntime,
    target: &SpawnTarget,
) -> Result<Option<ResolvedLaunchRoot>> {
    match target {
        SpawnTarget::DefinitionDescriptor { descriptor } => {
            let launch_context = parent.child_launch.launch_context();
            let descriptor = launch_context
                .and_then(|context| context.descriptor(descriptor))
                .with_context(|| format!("parent Process has no `{descriptor}` descriptor"))?;
            let root = PathBuf::from(&descriptor.path);
            let file_tree = descriptor.file_tree.clone().with_context(|| {
                format!(
                    "Agent Definition descriptor {} has no immutable file-tree handle",
                    descriptor.path
                )
            })?;
            Ok(Some(ResolvedLaunchRoot {
                root_dir: root,
                file_tree: Some(file_tree),
            }))
        }
        SpawnTarget::PackageChildAgent { .. } => {
            let export = parent
                .capability_view
                .as_ref()
                .map(crate::skills::ResolvedCapabilityView::refresh)
                .and_then(|view| view.resolve_child_agent_export(target).cloned())
                .ok_or_else(|| {
                    anyhow::anyhow!("Unknown package child Agent Executable target: {target:?}")
                })?;
            Ok(Some(ResolvedLaunchRoot {
                root_dir: export.root_dir,
                file_tree: export.file_tree,
            }))
        }
    }
}

pub(super) struct ResolvedLaunchRoot {
    pub(super) root_dir: PathBuf,
    pub(super) file_tree: Option<crate::ProcessFileTree>,
}

pub(super) fn build_child_agent_config(
    parent: &ChildLaunchRuntime,
    spec: &SpawnSpec,
) -> AgentConfig {
    let mut child_agent_config = parent.base_agent_config.clone();

    if !spec.has_handle(SpawnHandle::Memory) {
        child_agent_config.core_config.memory.store_dir = None;
    }

    if spec.has_handle(SpawnHandle::ApprovalScope) {
        child_agent_config.runtime_config.governance =
            parent.base_agent_config.runtime_config.governance.clone();
    } else {
        child_agent_config.runtime_config.governance = GovernanceConfig::default();
    }

    if let Some(model) = spec.runtime_overrides.model.as_deref() {
        child_agent_config.set_model_override(model);
    }
    if let Some(effort) = spec.runtime_overrides.model_reasoning_effort {
        child_agent_config.set_model_reasoning_effort_override(Some(effort));
    }
    if let Some(policy_path) = spec.runtime_overrides.policy_path.clone() {
        child_agent_config.runtime_config.governance.policy_path = Some(policy_path);
    }

    child_agent_config
}
