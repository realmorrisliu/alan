use super::{ChildNamespaceAssemblyPlan, ROUTE_MOUNT_PATH};
use crate::runtime::child_agents::ChildLaunchRuntime;
use crate::runtime::delegation_capabilities::{
    DelegatedSpawnRejected, evaluate_delegated_namespace, namespace_summary_from_bindings,
};
use alan_agent_protocol::{DelegatedCapabilityDecision, DelegatedCapabilityRecovery, SpawnSpec};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub(super) async fn evaluate_delegated_launch_capabilities(
    parent: &ChildLaunchRuntime,
    spec: &mut SpawnSpec,
    plan: &ChildNamespaceAssemblyPlan,
) -> Result<Option<DelegatedCapabilityDecision>> {
    let Some(context) = spec.delegated.as_ref() else {
        return Ok(None);
    };
    let requirements = context.requirements.clone();
    let child_namespace = namespace_summary_from_child_plan(plan);
    let parent_namespace = namespace_summary_from_parent(parent).await?;
    let decision = evaluate_delegated_namespace(
        &spec.launch.task,
        &requirements,
        child_namespace,
        &parent_namespace,
    );

    match decision.recovery {
        DelegatedCapabilityRecovery::Satisfied => Ok(Some(decision)),
        DelegatedCapabilityRecovery::Narrowed => {
            if let Some(narrowed_task) = decision.narrowed_task.clone() {
                spec.launch.task = narrowed_task;
            }
            Ok(Some(decision))
        }
        DelegatedCapabilityRecovery::ParentPath
        | DelegatedCapabilityRecovery::AskUser
        | DelegatedCapabilityRecovery::Limitation => {
            Err(DelegatedSpawnRejected { decision }.into())
        }
    }
}

fn namespace_summary_from_child_plan(
    plan: &ChildNamespaceAssemblyPlan,
) -> alan_agent_protocol::DelegatedNamespaceSummary {
    let namespace = plan.launch_context.namespace_snapshot();
    let mut described = vec![
        (plan.agent_mount.clone(), alan_kernel::Access::ReadWrite),
        (plan.llm_mount.clone(), alan_kernel::Access::ReadWrite),
        (plan.srv_mount.clone(), alan_kernel::Access::ReadOnly),
        (plan.route_mount.clone(), alan_kernel::Access::ReadWrite),
    ];
    described.extend(plan.host_mounts.iter().map(|mount| {
        (
            mount.target.to_string_lossy().to_string(),
            match mount.access {
                alan_agent_protocol::SpawnMountAccess::ReadOnly => alan_kernel::Access::ReadOnly,
                alan_agent_protocol::SpawnMountAccess::ReadWrite => alan_kernel::Access::ReadWrite,
            },
        )
    }));
    described.extend(
        plan.launch_context
            .package_references
            .iter()
            .filter_map(|reference| {
                namespace
                    .resolve(&reference.namespace_path)
                    .ok()
                    .map(|resolved| (reference.namespace_path.clone(), resolved.access))
            }),
    );
    described.extend(
        plan.launch_context
            .descriptors
            .values()
            .filter(|descriptor| {
                !plan
                    .launch_context
                    .package_references
                    .iter()
                    .any(|reference| {
                        Path::new(&descriptor.path).starts_with(&reference.namespace_path)
                    })
            })
            .filter_map(|descriptor| {
                namespace
                    .resolve(&descriptor.path)
                    .ok()
                    .map(|resolved| (descriptor.path.clone(), resolved.access))
            }),
    );
    described.sort_by(|left, right| {
        left.0.cmp(&right.0).then_with(|| {
            matches!(left.1, alan_kernel::Access::ReadWrite)
                .cmp(&matches!(right.1, alan_kernel::Access::ReadWrite))
        })
    });
    described.dedup();
    namespace_summary_from_bindings(
        described.iter().map(|(path, _)| path.clone()).collect(),
        described
            .iter()
            .filter(|(_, access)| *access == alan_kernel::Access::ReadWrite)
            .map(|(path, _)| path.clone())
            .collect(),
        plan.bin_tool_mounts.clone(),
        plan.cwd.clone(),
        Some(plan.llm_connection_name.clone()),
    )
}

async fn namespace_summary_from_parent(
    parent: &ChildLaunchRuntime,
) -> Result<alan_agent_protocol::DelegatedNamespaceSummary> {
    let launch_context = parent.child_launch.launch_context();
    let mut described = launch_context
        .map(|context| context.namespace.describe())
        .unwrap_or_default();
    let cwd = launch_context.map(|context| PathBuf::from(&context.cwd));
    described.extend([
        ("/agent".to_string(), alan_kernel::Access::ReadWrite),
        ("/mnt/llm".to_string(), alan_kernel::Access::ReadWrite),
        ("/srv".to_string(), alan_kernel::Access::ReadOnly),
        (ROUTE_MOUNT_PATH.to_string(), alan_kernel::Access::ReadWrite),
    ]);
    Ok(namespace_summary_from_bindings(
        described.iter().map(|(path, _)| path.clone()).collect(),
        described
            .iter()
            .filter(|(_, access)| *access == alan_kernel::Access::ReadWrite)
            .map(|(path, _)| path.clone())
            .collect(),
        parent
            .tool_execution
            .static_tool_names()
            .await?
            .into_iter()
            .map(|tool| format!("/bin/{tool}"))
            .collect(),
        cwd,
        Some(
            parent
                .base_agent_config
                .core_config
                .connection_profile
                .clone()
                .unwrap_or_else(|| "default".to_string()),
        ),
    ))
}
