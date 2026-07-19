use crate::runtime::launch_config::AgentConfig;
use crate::runtime::transition::RuntimeLoopState;
use alan_agent_protocol::{GovernanceConfig, SpawnHandle, SpawnSpec, SpawnTarget};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

pub(super) fn ensure_child_connection_is_passed(
    parent: &RuntimeLoopState,
    requested: &str,
) -> Result<()> {
    let passed = parent.namespace_environment().llm_connection();
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
    let parent_definition_path = parent
        .descriptor(crate::AGENT_DEFINITION_DESCRIPTOR)
        .map(|descriptor| descriptor.path.clone());
    let mut launch_context = parent.child();
    launch_context.descriptors.clear();

    if !spec.has_handle(SpawnHandle::HostMounts) {
        let inherited_mounts = std::mem::take(&mut launch_context.host_mounts);
        if let Some(cwd) = child_cwd.as_deref()
            && inherited_mounts
                .iter()
                .any(|grant| grant.resolve_host_path(cwd).is_some())
        {
            bail!(
                "Child Agent Process launch cwd '{cwd}' requires the explicit host_mounts handle."
            );
        }
        if child_cwd.is_none()
            && inherited_mounts
                .iter()
                .any(|grant| grant.resolve_host_path(&launch_context.cwd).is_some())
        {
            launch_context.cwd = "/".to_string();
        }
        for grant in inherited_mounts {
            if parent_definition_path.as_deref() == Some(&grant.namespace_path) {
                launch_context.host_mounts.push(grant);
                continue;
            }
            launch_context.namespace.unmount(&grant.namespace_path);
        }
    }

    if let Some(cwd) = child_cwd {
        launch_context.cwd = cwd;
    }
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
        if !spec.has_handle(SpawnHandle::HostMounts)
            && let Some(parent_definition_path) = parent_definition_path.as_deref()
        {
            launch_context
                .host_mounts
                .retain(|grant| grant.namespace_path != parent_definition_path);
            launch_context.namespace.unmount(parent_definition_path);
        }
        launch_context.descriptors.insert(
            crate::AGENT_DEFINITION_DESCRIPTOR.to_string(),
            crate::ProcessDescriptor::with_file_tree(descriptor_path, file_tree.clone())?,
        );
    } else if let Some(ResolvedLaunchRoot { root_dir, .. }) = launch_root_dir {
        let source_path = parent
            .namespace_path(root_dir)
            .filter(|path| !parent.namespace.union_at(path).is_empty());
        if source_path.as_deref() != Some("/agent-definition")
            && let Some(source_path) = source_path
        {
            launch_context.namespace.unmount("/agent-definition");
            launch_context
                .namespace
                .bind("/agent-definition", &source_path);
        }
        launch_context.host_mounts.retain(|grant| {
            grant.namespace_path != "/agent-definition"
                && parent_definition_path.as_deref() != Some(&grant.namespace_path)
        });
        launch_context = launch_context
            .with_host_mount(crate::HostMountGrant::new(
                "/agent-definition",
                root_dir,
                alan_kernel::Access::ReadOnly,
            )?)
            .with_descriptor(
                crate::AGENT_DEFINITION_DESCRIPTOR,
                crate::ProcessDescriptor::new("/agent-definition")?,
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
    parent: &RuntimeLoopState,
    target: &SpawnTarget,
) -> Result<Option<ResolvedLaunchRoot>> {
    match target {
        SpawnTarget::DefinitionDescriptor { descriptor } => {
            let descriptor = parent
                .namespace_environment()
                .launch_context()
                .and_then(|context| context.descriptor(descriptor))
                .with_context(|| format!("parent Process has no `{descriptor}` descriptor"))?;
            let root = if descriptor.file_tree.is_some() {
                PathBuf::from(&descriptor.path)
            } else {
                parent
                    .namespace_environment()
                    .launch_context()
                    .and_then(|context| context.host_path(&descriptor.path))
                    .with_context(|| {
                        format!(
                            "Agent Definition descriptor {} has no explicit Host Mount backing",
                            descriptor.path
                        )
                    })?
            };
            Ok(Some(ResolvedLaunchRoot {
                root_dir: root,
                file_tree: descriptor.file_tree.clone(),
            }))
        }
        SpawnTarget::PackageChildAgent { .. } => {
            let export = parent
                .prompt_cache
                .capability_view()
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

pub(super) fn build_child_agent_config(parent: &RuntimeLoopState, spec: &SpawnSpec) -> AgentConfig {
    let mut child_agent_config = AgentConfig::from(parent.core_config.clone());
    child_agent_config.runtime_config = parent.runtime_config.clone();

    if !spec.has_handle(SpawnHandle::Memory) {
        child_agent_config.core_config.memory.store_dir = None;
    }

    if spec.has_handle(SpawnHandle::ApprovalScope) {
        child_agent_config.runtime_config.governance = parent.runtime_config.governance.clone();
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
