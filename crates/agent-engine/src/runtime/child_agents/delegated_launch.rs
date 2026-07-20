use alan_agent_protocol::{
    DelegatedCapabilityDecision, DelegatedCapabilityRecovery, ProcessNamespaceAccess,
    ProcessNamespaceMount, SpawnSpec,
};
use anyhow::Result;

use super::ChildNamespacePlan;
use crate::runtime::delegation_capabilities::{
    DelegatedSpawnRejected, evaluate_delegated_namespace, namespace_summary_from_bindings,
};

pub(super) fn evaluate_delegated_launch_capabilities(
    spec: &mut SpawnSpec,
    plan: &ChildNamespacePlan,
    parent_mounts: &[ProcessNamespaceMount],
    parent_cwd: &std::path::Path,
) -> Result<Option<DelegatedCapabilityDecision>> {
    let Some(context) = spec.delegated.as_ref() else {
        return Ok(None);
    };
    let decision = evaluate_delegated_namespace(
        &spec.launch.task,
        &context.requirements,
        plan.namespace_summary(),
        &namespace_summary_from_bindings(
            parent_mounts
                .iter()
                .map(|mount| mount.path.clone())
                .collect(),
            parent_mounts
                .iter()
                .filter(|mount| mount.access == ProcessNamespaceAccess::ReadWrite)
                .map(|mount| mount.path.clone())
                .collect(),
            parent_mounts
                .iter()
                .filter(|mount| mount.path.starts_with("/bin/"))
                .map(|mount| mount.path.clone())
                .collect(),
            Some(parent_cwd.to_path_buf()),
            Some(plan.llm_connection_name.clone()),
        ),
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
