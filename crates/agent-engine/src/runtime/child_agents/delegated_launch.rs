use super::{ChildNamespaceAssemblyPlan, ROUTE_MOUNT_PATH};
use crate::runtime::agent_loop::RuntimeLoopState;
use crate::runtime::delegation_capabilities::{
    DelegatedSpawnRejected, evaluate_delegated_namespace, namespace_summary_from_bindings,
};
use alan_agent_protocol::{DelegatedCapabilityDecision, DelegatedCapabilityRecovery, SpawnSpec};
use anyhow::Result;
use std::path::PathBuf;

pub(super) async fn evaluate_delegated_launch_capabilities(
    parent: &RuntimeLoopState,
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
    let mut described = plan.launch_context.namespace.describe();
    described.extend([
        (plan.agent_mount.clone(), alan_kernel::Access::ReadWrite),
        (plan.llm_mount.clone(), alan_kernel::Access::ReadWrite),
        (plan.srv_mount.clone(), alan_kernel::Access::ReadOnly),
        (plan.route_mount.clone(), alan_kernel::Access::ReadWrite),
    ]);
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
    parent: &RuntimeLoopState,
) -> Result<alan_agent_protocol::DelegatedNamespaceSummary> {
    let mut described = parent
        .namespace_environment()
        .launch_context()
        .map(|context| context.namespace.describe())
        .unwrap_or_default();
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
            .static_tool_names()
            .await?
            .into_iter()
            .map(|tool| format!("/bin/{tool}"))
            .collect(),
        parent
            .namespace_environment()
            .launch_context()
            .map(|context| PathBuf::from(&context.cwd)),
        Some(
            parent
                .core_config
                .connection_profile
                .clone()
                .unwrap_or_else(|| "default".to_string()),
        ),
    ))
}
