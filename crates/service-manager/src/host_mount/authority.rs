//! `ToolExecutionAuthority` projection and direct delegated-grant activation for `HostMountService`.

use std::path::PathBuf;
use std::sync::Arc;

use alan_agent_engine::GitPushDelegationScope;
use alan_agent_engine::tools::{ToolExecutionAuthority, ToolExecutionBinding};
use alan_kernel::Pid;
use anyhow::{Context, Result, ensure};

use super::{
    Grant, HostMountAccess, HostMountExport, HostMountGrantRecord, HostMountService,
    HostMountToolProjection, Projection, audit, namespace_paths_strictly_overlap,
    validate_mount_namespace_path,
};

impl HostMountService {
    /// Activate a pre-authorized native Host Mount directly onto `pid` with narrow
    /// standing `git push` scopes (for Host-validated saved project delegations).
    #[allow(
        clippy::too_many_arguments,
        reason = "Direct host-validated grant projection takes explicit handle metadata, access, and git push scopes"
    )]
    pub fn grant_direct_with_scopes(
        &self,
        pid: Pid,
        id: String,
        label: String,
        namespace_path: String,
        access: HostMountAccess,
        export: Arc<dyn HostMountExport>,
        provenance: impl Into<String>,
        git_push_scopes: Vec<GitPushDelegationScope>,
    ) -> Result<HostMountGrantRecord> {
        ensure!(!id.trim().is_empty(), "Host Mount grant id is empty");
        ensure!(!label.trim().is_empty(), "Host Mount label is empty");
        validate_mount_namespace_path(&namespace_path)?;

        let mut state = self.state.lock().unwrap();
        let namespace = state
            .processes
            .get(&pid)
            .cloned()
            .with_context(|| format!("unknown Process {pid:?}"))?;
        ensure!(
            !state.grants.contains_key(&id) && !state.requests.contains_key(&id),
            "duplicate Host Mount id `{id}`"
        );
        ensure!(
            !state.grants.iter().any(|(_, existing)| {
                existing.public.active
                    && existing.projections.iter().any(|projection| {
                        projection.pid == pid
                            && (projection.namespace_path == namespace_path
                                || namespace_paths_strictly_overlap(
                                    &namespace_path,
                                    &projection.namespace_path,
                                ))
                    })
            }),
            "Host Mount namespace path overlaps an active Process projection"
        );

        let record = HostMountGrantRecord {
            id: id.clone(),
            label,
            namespace_path: namespace_path.clone(),
            access,
            provenance: provenance.into(),
            active: true,
        };
        namespace.replace_mount(&namespace_path, export.file_tree(), access.kernel());
        state.grants.insert(
            id.clone(),
            Grant {
                public: record.clone(),
                owner: pid,
                export,
                projections: vec![Projection {
                    pid,
                    namespace,
                    namespace_path,
                    access,
                }],
                git_push_scopes,
            },
        );
        audit(
            &mut state,
            "grant_direct",
            &id,
            "host-delegation".to_string(),
            vec![pid.0],
        );
        drop(state);
        self.bump_generation();
        self.append_service_event(
            serde_json::json!({
                "type": "grant_activated",
                "grant_id": id,
            }),
            &[pid.0],
        );
        Ok(record)
    }
}

impl ToolExecutionAuthority for HostMountService {
    fn presentation_scope(&self, pid: u64) -> Option<String> {
        let state = self.state.lock().ok()?;
        let grants = state
            .grants
            .iter()
            .flat_map(|(id, grant)| {
                grant
                    .projections
                    .iter()
                    .filter(move |p| grant.public.active && p.pid.0 == pid)
                    .map(move |p| (id, &p.namespace_path, p.access))
            })
            .collect::<Vec<_>>();
        if grants.is_empty() {
            return None;
        }
        serde_json::to_string(&(self.generation(), grants)).ok()
    }

    fn authority_revision(&self, _pid: u64) -> Option<u64> {
        Some(u64::from(self.generation()))
    }

    fn git_push_delegations(&self, pid: u64) -> Vec<GitPushDelegationScope> {
        let Ok(state) = self.state.lock() else {
            return Vec::new();
        };
        let pid = Pid(pid);
        state
            .grants
            .values()
            .filter(|grant| {
                grant.public.active
                    && grant
                        .projections
                        .iter()
                        .any(|projection| projection.pid == pid)
            })
            .flat_map(|grant| grant.git_push_scopes.iter().cloned())
            .collect()
    }

    fn project_knowledge_roots(&self, pid: u64) -> Vec<PathBuf> {
        let Ok(state) = self.state.lock() else {
            return Vec::new();
        };
        let pid = Pid(pid);
        let mut roots = state
            .grants
            .values()
            .filter(|grant| grant.public.active)
            .flat_map(|grant| {
                grant
                    .projections
                    .iter()
                    .filter(|projection| projection.pid == pid)
                    .map(|projection| PathBuf::from(&projection.namespace_path))
            })
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        roots
    }

    fn reconcile(
        &self,
        pid: u64,
        mut binding: ToolExecutionBinding,
    ) -> Result<ToolExecutionBinding> {
        let pid = Pid(pid);
        let requested_namespace_cwd = binding.namespace_cwd.clone();
        let state = self.state.lock().unwrap();
        let mut cwd_grants = Vec::new();
        let projections = state
            .grants
            .values()
            .filter_map(|grant| {
                if !grant.public.active {
                    return None;
                }
                grant
                    .projections
                    .iter()
                    .find(|projection| projection.pid == pid)
                    .map(|projection| {
                        cwd_grants.push((
                            PathBuf::from(&projection.namespace_path),
                            grant.public.id.clone(),
                        ));
                        HostMountToolProjection {
                            namespace_path: PathBuf::from(&projection.namespace_path),
                            access: projection.access,
                            export: grant.export.clone(),
                        }
                    })
            })
            .collect::<Vec<_>>();
        drop(state);

        binding.clear_adapter();
        if !projections.is_empty() {
            binding.set_adapter(
                self.adapter
                    .tool_execution_adapter(&projections, &requested_namespace_cwd)?,
            );
        }
        let selected_grant = cwd_grants
            .iter()
            .filter(|(path, _)| binding.namespace_cwd.starts_with(path))
            .max_by_key(|(path, _)| path.components().count())
            .map(|(_, id)| id.clone());
        ensure!(
            binding
                .cwd_grant_id
                .as_ref()
                .is_none_or(|id| Some(id) == selected_grant.as_ref()),
            "Process cwd grant was revoked or replaced; choose an explicit directory"
        );
        binding.cwd_grant_id = selected_grant;
        Ok(binding)
    }
}
