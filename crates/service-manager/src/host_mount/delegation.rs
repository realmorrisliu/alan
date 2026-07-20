use std::collections::BTreeSet;
use std::sync::Arc;

use alan_agent_engine::{SpawnHostMount, SpawnMountAccess};
use alan_kernel::Pid;
use anyhow::{Context, Result, ensure};

use super::{
    HostMountAccess, HostMountExport, State, namespace_paths_strictly_overlap,
    validate_mount_namespace_path,
};

pub(super) struct DelegatedProjection {
    pub(super) grant_id: String,
    pub(super) target: String,
    pub(super) access: HostMountAccess,
    pub(super) export: Arc<dyn HostMountExport>,
}

pub(super) fn resolve_child_delegations(
    state: &State,
    parent_pid: Pid,
    requested: &[SpawnHostMount],
) -> Result<Vec<DelegatedProjection>> {
    validate_child_mount_requests(requested)?;
    requested
        .iter()
        .map(|selection| {
            let grant = state
                .grants
                .get(&selection.grant)
                .with_context(|| format!("unknown Host Mount grant `{}`", selection.grant))?;
            ensure!(grant.public.active, "Host Mount grant is revoked");
            let parent_projection = grant
                .projections
                .iter()
                .find(|projection| projection.pid == parent_pid)
                .with_context(|| {
                    format!(
                        "Process {parent_pid:?} does not hold Host Mount grant `{}`",
                        selection.grant
                    )
                })?;
            let requested_access = spawn_mount_access(selection.access);
            ensure!(
                parent_projection.access == HostMountAccess::ReadWrite
                    || requested_access == HostMountAccess::ReadOnly,
                "child Host Mount delegation cannot amplify parent access"
            );
            ensure!(
                grant.public.access == HostMountAccess::ReadWrite
                    || requested_access == HostMountAccess::ReadOnly,
                "child Host Mount delegation cannot amplify grant access"
            );
            Ok(DelegatedProjection {
                grant_id: selection.grant.clone(),
                target: selection.target.to_string_lossy().to_string(),
                access: requested_access,
                export: grant.export.clone(),
            })
        })
        .collect()
}

pub(super) fn validate_child_mount_requests(requested: &[SpawnHostMount]) -> Result<()> {
    let mut grant_ids = BTreeSet::new();
    let mut targets: Vec<String> = Vec::with_capacity(requested.len());
    for selection in requested {
        ensure!(
            !selection.grant.trim().is_empty(),
            "child Host Mount grant reference is empty"
        );
        ensure!(
            grant_ids.insert(selection.grant.as_str()),
            "child Host Mount grant `{}` is delegated more than once",
            selection.grant
        );
        let target = selection
            .target
            .to_str()
            .context("child Host Mount target is not UTF-8")?;
        validate_mount_namespace_path(target)?;
        ensure!(
            !targets
                .iter()
                .any(|existing| namespace_paths_overlap(target, existing)),
            "child Host Mount target overlaps another delegated target"
        );
        targets.push(target.to_string());
    }
    Ok(())
}

fn spawn_mount_access(access: SpawnMountAccess) -> HostMountAccess {
    match access {
        SpawnMountAccess::ReadOnly => HostMountAccess::ReadOnly,
        SpawnMountAccess::ReadWrite => HostMountAccess::ReadWrite,
    }
}

fn namespace_paths_overlap(left: &str, right: &str) -> bool {
    left == right || namespace_paths_strictly_overlap(left, right)
}
