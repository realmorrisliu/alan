use super::*;

/// Same-user project mount plus its canonical native root for local completion.
#[derive(Clone, Debug)]
pub struct HostProjectMount {
    /// Logical Host Mount grant projected into the Root Agent.
    pub grant: HostMountGrantRecord,
    /// Canonical host directory, returned only to the local TUI adapter.
    pub host_path: PathBuf,
}

impl HostCommandPlane {
    pub async fn approve_host_mount(
        &self,
        request_id: impl Into<String>,
        host_path: PathBuf,
    ) -> Result<HostMountGrantRecord> {
        self.call(LocalRequest::ApproveHostMount {
            request_id: request_id.into(),
            host_path,
        })
        .await?
        .grant
        .context("Host Mount approval returned no grant")
    }

    /// Reconcile an explicit project selection against the same operation and Host boot.
    pub async fn mount_project(
        &self,
        operation_id: Uuid,
        expected_boot: Uuid,
        host_path: PathBuf,
        access: HostMountAccess,
    ) -> Result<HostProjectMount> {
        let status = self.paths.read_status()?;
        if !status.supports_project_mount() || status.boot_id != expected_boot {
            return Err(ProjectMountRejected("Host changed or project reconciliation is unsupported; restart this Alan invocation".into()).into());
        }
        // A correlated server error is a known rejection; transport loss is uncertain.
        let mut stream = tokio::net::UnixStream::connect(&self.paths.socket).await?;
        write_local_request(
            &mut stream,
            &LocalRequest::MountProject(ProjectMountRequest {
                operation_id,
                expected_boot,
                host_path,
                access,
            }),
        )
        .await?;
        let response = read_local_response(&mut stream).await?;
        ensure!(
            response.boot_id == expected_boot && response.operation_id == Some(operation_id),
            "Host project response belongs to another boot or operation"
        );
        if let Some(error) = response.error {
            ensure!(
                response.grant.is_none() && response.host_path.is_none(),
                "contradictory Host project response"
            );
            return Err(ProjectMountRejected(error).into());
        }
        Ok(HostProjectMount {
            grant: response
                .grant
                .context("Host project mount returned no grant")?,
            host_path: response
                .host_path
                .context("Host project mount returned no canonical directory")?,
        })
    }

    pub async fn cancel_host_mount(&self, request_id: impl Into<String>) -> Result<()> {
        self.call(LocalRequest::CancelHostMount {
            request_id: request_id.into(),
        })
        .await?;
        Ok(())
    }

    pub async fn revoke_host_mount(&self, grant_id: impl Into<String>) -> Result<()> {
        self.call(LocalRequest::RevokeHostMount {
            grant_id: grant_id.into(),
        })
        .await?;
        Ok(())
    }
}

/// A correlated rejection of this request; it does not revoke a prior mismatched operation.
#[derive(Debug)]
pub struct ProjectMountRejected(pub String);
impl std::fmt::Display for ProjectMountRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ProjectMountRejected {}

// ponytail: invocation-local results grow with selections; index durable receipts if long sessions warrant it.
#[derive(Default)]
pub(super) struct ProjectOperations(std::collections::HashMap<Uuid, ProjectOperation>);
struct ProjectOperation {
    host_path: PathBuf,
    access: HostMountAccess,
    root_pid: alan_kernel::Pid,
    outcome: std::result::Result<HostProjectMount, String>,
}
impl ProjectOperations {
    pub(super) fn mount(
        &mut self,
        service: &alan_service_manager::HostMountService,
        root_pid: alan_kernel::Pid,
        boot: Uuid,
        request: ProjectMountRequest,
    ) -> LocalResponse {
        let ProjectMountRequest {
            operation_id,
            expected_boot,
            host_path,
            access,
        } = request;
        let result = (|| {
            ensure!(
                expected_boot == boot,
                "project operation belongs to another Host boot"
            );
            if let Some(prior) = self.0.get(&operation_id) {
                ensure!(
                    prior.host_path == host_path
                        && prior.access == access
                        && prior.root_pid == root_pid,
                    "project operation identity cannot change directory, access or Root"
                );
            } else {
                let outcome = (|| {
                    let canonical = crate::host_mounts::canonical_host_path(&host_path)?;
                    ensure!(
                        canonical.is_dir(),
                        "selected project path is not a directory"
                    );
                    let request = service.request_project_mount(root_pid, access)?;
                    let grant = crate::host_mounts::approve_host_mount(
                        service,
                        &request,
                        &canonical,
                        "native-project-chooser",
                        "local-user",
                    )?;
                    Ok(HostProjectMount {
                        grant,
                        host_path: canonical,
                    })
                })()
                .map_err(|e: anyhow::Error| e.to_string());
                // Retain the result before writing any client response.
                self.0.insert(
                    operation_id,
                    ProjectOperation {
                        host_path,
                        access,
                        root_pid,
                        outcome,
                    },
                );
            }
            let prior = self.0.get(&operation_id).unwrap();
            let mounted = prior.outcome.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
            let grant = service
                .grant_record(&mounted.grant.id)
                .context("project grant unavailable")?;
            ensure!(
                grant.active,
                "project grant was revoked; operation cannot reauthorize it"
            );
            Ok(HostProjectMount {
                grant,
                host_path: mounted.host_path.clone(),
            })
        })();
        LocalResponse {
            operation_id: Some(operation_id),
            boot_id: boot,
            grant: result.as_ref().ok().map(|m| m.grant.clone()),
            host_path: result.as_ref().ok().map(|m| m.host_path.clone()),
            error: result.err().map(|e: anyhow::Error| e.to_string()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct ProjectMountRequest {
    pub operation_id: Uuid,
    pub expected_boot: Uuid,
    pub host_path: PathBuf,
    pub access: HostMountAccess,
}

#[cfg(test)]
mod tests;
