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

    /// Mount an explicitly selected Host directory into this invocation's Root Agent.
    pub async fn mount_project(
        &self,
        host_path: PathBuf,
        access: HostMountAccess,
    ) -> Result<HostProjectMount> {
        let status = self.paths.read_status()?;
        ensure!(
            status.supports_project_mount(),
            "Alan OS Host does not support project selection (local protocol {}); restart this Alan invocation",
            status.local_attachment_protocol_version
        );
        let response = self
            .call(LocalRequest::MountProject { host_path, access })
            .await?;
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
