use anyhow::{Context, Result, bail};
use std::time::Duration;

use super::{NamespaceHostMountRequests, client::NamespaceClient};

const HOST_MOUNT_REQUEST_CLONE: &str = "/mnt/host-mount/requests/clone";
const HOST_MOUNT_TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(10);
const HOST_MOUNT_DECISION_SETTLE_TIMEOUT: Duration = Duration::from_secs(30);

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
            let terminal = tokio::time::timeout(HOST_MOUNT_DECISION_SETTLE_TIMEOUT, async {
                loop {
                    if let Some(terminal) = self.terminal_result(request_id).await? {
                        return Ok::<_, anyhow::Error>(terminal);
                    }
                    tokio::time::sleep(HOST_MOUNT_TERMINAL_POLL_INTERVAL).await;
                }
            })
            .await;
            return match terminal {
                Ok(result) => result,
                Err(_) => Err(cancel_error).context(
                    "Host Mount Service request stayed pending after cancellation was rejected",
                ),
            };
        }
        self.terminal_result(request_id)
            .await?
            .context("cancelled Host Mount Service request did not reach terminal status")
    }

    /// Revalidate requested child Host Mount grants against `/mnt/host-mount/grants` when present.
    pub(crate) async fn validate_live_grants(
        &self,
        mounts: &[alan_agent_protocol::SpawnHostMount],
    ) -> Result<Option<String>> {
        if mounts.is_empty() {
            return Ok(None);
        }
        let client = NamespaceClient::new(self.root.clone());
        let Some(grant_ids) = client
            .try_read_directory_names("/mnt/host-mount/grants")
            .await?
        else {
            return Ok(None);
        };
        for mount in mounts {
            if !grant_ids.iter().any(|id| id == &mount.grant) {
                return Ok(Some(format!(
                    "Host Mount grant '{}' is missing or revoked on the parent Process.",
                    mount.grant
                )));
            }
            let record_path = format!("/mnt/host-mount/grants/{}/record", mount.grant);
            let Some(bytes) = client.try_read_file(&record_path).await? else {
                return Ok(Some(format!(
                    "Host Mount grant '{}' record is unavailable.",
                    mount.grant
                )));
            };
            let record: serde_json::Value = serde_json::from_slice(&bytes)
                .with_context(|| format!("parse Host Mount grant record {record_path}"))?;
            if !record["active"].as_bool().unwrap_or(false) {
                return Ok(Some(format!(
                    "Host Mount grant '{}' has been revoked.",
                    mount.grant
                )));
            }
            if mount.access == alan_agent_protocol::SpawnMountAccess::ReadWrite
                && record["access"].as_str() != Some("read_write")
            {
                return Ok(Some(format!(
                    "Host Mount grant '{}' cannot widen read_only grant to read_write.",
                    mount.grant
                )));
            }
            if let Some(ns_path) = record["namespace_path"].as_str() {
                let root = std::path::Path::new(ns_path);
                if root != std::path::Path::new("/") && !mount.target.starts_with(root) {
                    return Ok(Some(format!(
                        "Host Mount target '{}' is outside grant '{}' scope '{}'.",
                        mount.target.display(),
                        mount.grant,
                        ns_path
                    )));
                }
            }
        }
        Ok(None)
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
