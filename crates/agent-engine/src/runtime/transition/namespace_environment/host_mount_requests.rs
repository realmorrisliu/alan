use anyhow::{Context, Result, bail};

use super::{NamespaceHostMountRequests, client::NamespaceClient};

const HOST_MOUNT_REQUEST_CLONE: &str = "/mnt/host-mount/requests/clone";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostMountTerminalStatus {
    Approved,
    Rejected,
    Cancelled,
    Failed,
}

impl HostMountTerminalStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostMountTerminalResult {
    pub(crate) status: HostMountTerminalStatus,
    pub(crate) grant_reference: Option<String>,
    pub(crate) error: Option<String>,
}

impl NamespaceHostMountRequests {
    pub(crate) async fn create(&self, document: &[u8]) -> Result<String> {
        let request_id = NamespaceClient::new(self.root.clone())
            .clone_with_document(HOST_MOUNT_REQUEST_CLONE, document)
            .await
            .context("commit logical Host Mount request")?;
        validate_request_id(&request_id)
            .context("Host Mount Service returned an invalid request reference")?;
        Ok(request_id)
    }

    pub(crate) async fn terminal_result(
        &self,
        request_id: &str,
    ) -> Result<Option<HostMountTerminalResult>> {
        validate_request_id(request_id)?;
        let client = NamespaceClient::new(self.root.clone());
        let status_path = format!("/mnt/host-mount/requests/{request_id}/status");
        let Some(status) = client.try_read_file(&status_path).await? else {
            return Ok(Some(HostMountTerminalResult {
                status: HostMountTerminalStatus::Failed,
                grant_reference: None,
                error: Some("Host Mount request is no longer available".to_string()),
            }));
        };
        let status = String::from_utf8(status).context("Host Mount request status is not utf8")?;
        match status.trim() {
            "pending" => Ok(None),
            "approved" => {
                let grant = read_optional_text(
                    &client,
                    &format!("/mnt/host-mount/requests/{request_id}/grant"),
                )
                .await?;
                if grant.is_none() {
                    return Ok(Some(HostMountTerminalResult {
                        status: HostMountTerminalStatus::Failed,
                        grant_reference: None,
                        error: Some(
                            "Host Mount Service approved the request without a grant reference"
                                .to_string(),
                        ),
                    }));
                }
                Ok(Some(HostMountTerminalResult {
                    status: HostMountTerminalStatus::Approved,
                    grant_reference: grant,
                    error: None,
                }))
            }
            "rejected" | "cancelled" | "failed" => {
                let terminal_status = match status.trim() {
                    "rejected" => HostMountTerminalStatus::Rejected,
                    "cancelled" => HostMountTerminalStatus::Cancelled,
                    _ => HostMountTerminalStatus::Failed,
                };
                let error = read_optional_text(
                    &client,
                    &format!("/mnt/host-mount/requests/{request_id}/error"),
                )
                .await?
                .or_else(|| {
                    Some(format!(
                        "Host Mount request was {}",
                        terminal_status.as_str()
                    ))
                });
                Ok(Some(HostMountTerminalResult {
                    status: terminal_status,
                    grant_reference: None,
                    error,
                }))
            }
            other => bail!("unknown Host Mount request status `{other}`"),
        }
    }

    pub(crate) async fn cancel(&self, request_id: &str) -> Result<HostMountTerminalResult> {
        validate_request_id(request_id)?;
        let status_path = format!("/mnt/host-mount/requests/{request_id}/status");
        let client = NamespaceClient::new(self.root.clone());
        if let Err(cancel_error) = client.write_document(&status_path, b"cancelled\n").await {
            if let Some(terminal) = self.terminal_result(request_id).await? {
                return Ok(terminal);
            }
            return Err(cancel_error).context("cancel pending Host Mount Service request");
        }
        self.terminal_result(request_id)
            .await?
            .context("cancelled Host Mount Service request did not reach terminal status")
    }
}

async fn read_optional_text(client: &NamespaceClient, path: &str) -> Result<Option<String>> {
    let Some(bytes) = client.try_read_file(path).await? else {
        return Ok(None);
    };
    let value = String::from_utf8(bytes).with_context(|| format!("{path} is not utf8"))?;
    Ok((!value.trim().is_empty()).then(|| value.trim().to_string()))
}

fn validate_request_id(request_id: &str) -> Result<()> {
    if request_id.is_empty()
        || request_id.contains('/')
        || matches!(request_id, "." | "..")
        || request_id.chars().any(char::is_whitespace)
    {
        bail!("invalid Host Mount request reference");
    }
    Ok(())
}
