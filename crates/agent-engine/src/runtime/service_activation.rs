//! Host-owned service activation authority injected into the Root Agent Process.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Requested mount access mode for an external fileserver package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceMountAccess {
    ReadOnly,
    #[default]
    ReadWrite,
}

impl ServiceMountAccess {
    /// Stable lowercase wire label used in tool payloads and prompts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }

    /// Human-readable hyphenated label used in context and confirmation summaries.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::ReadWrite => "read-write",
        }
    }

    /// Returns whether `self` already grants at least `requested` access.
    pub const fn satisfies(self, requested: Self) -> bool {
        matches!(
            (self, requested),
            (Self::ReadWrite, Self::ReadOnly | Self::ReadWrite) | (Self::ReadOnly, Self::ReadOnly)
        )
    }
}

pub(crate) const fn network_scope_label(allow_network: bool) -> &'static str {
    if allow_network {
        "network"
    } else {
        "no-network"
    }
}

/// Validated request to activate and hot-mount an installed fileserver package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceActivationRequest {
    pub package_id: String,
    #[serde(default)]
    pub access: ServiceMountAccess,
    #[serde(default)]
    pub allow_network: bool,
    pub reason: String,
}

/// Descriptor of an installed fileserver package visible to the Root Agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableFileserverPackage {
    pub package_id: String,
    pub service_id: String,
    pub mount_path: String,
    pub description: Option<String>,
    pub allow_network: bool,
    pub network_domains: Vec<String>,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_access: Option<ServiceMountAccess>,
    #[serde(default)]
    pub active_allow_network: bool,
}

/// Result of activating and hot-mounting a fileserver package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivatedServiceMount {
    pub package_id: String,
    pub service_id: String,
    pub mount_path: String,
    pub description: String,
    pub access: ServiceMountAccess,
    pub allow_network: bool,
    pub persisted: bool,
}

/// Root-only authority for inspecting and activating installed fileserver packages.
#[async_trait]
pub trait ServiceActivationAuthority: Send + Sync {
    /// List installed fileserver packages with their active mount state.
    fn list_available_fileservers(&self) -> Vec<AvailableFileserverPackage>;

    /// Validate `request` against installed fileserver packages and policy bounds.
    fn inspect_request(
        &self,
        request: &ServiceActivationRequest,
    ) -> Result<AvailableFileserverPackage, String>;

    /// Return the active `(access, allow_network)` grant for `service_id` if running and mounted.
    fn active_service_grant(&self, _service_id: &str) -> Option<(ServiceMountAccess, bool)> {
        None
    }

    /// Start (or upgrade/reuse) and hot-mount the requested fileserver package.
    async fn activate_fileserver(
        &self,
        request: &ServiceActivationRequest,
        persist_for_root: bool,
    ) -> Result<ActivatedServiceMount, String>;
}

/// Render concise dynamic turn-context guidance for available fileserver packages.
pub(crate) fn render_service_activation_context(
    packages: &[AvailableFileserverPackage],
) -> Option<String> {
    if packages.is_empty() {
        return None;
    }

    let mut sorted = packages.to_vec();
    sorted.sort_by(|left, right| left.package_id.cmp(&right.package_id));

    let mut active_lines = Vec::new();
    let mut inactive_lines = Vec::new();

    for pkg in &sorted {
        let status_note = if pkg.active {
            let access_label = pkg
                .active_access
                .map(ServiceMountAccess::display_label)
                .unwrap_or("unknown");
            let network_label = network_scope_label(pkg.active_allow_network);
            format!("active: {access_label}, {network_label}")
        } else if pkg.allow_network {
            "network capable".to_string()
        } else {
            "local only".to_string()
        };
        let desc = pkg
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!(" — {s}"))
            .unwrap_or_default();
        let line = format!(
            "- `{}` -> `{}` ({status_note}){desc}",
            pkg.package_id, pkg.mount_path
        );
        if pkg.active {
            active_lines.push(line);
        } else {
            inactive_lines.push(line);
        }
    }

    let mut sections = vec!["Fileserver services (Root Agent):".to_string()];
    if !active_lines.is_empty() {
        sections.push(format!("Active mounts:\n{}", active_lines.join("\n")));
    }
    if !inactive_lines.is_empty() {
        sections.push(format!(
            "Inactive available packages (call `request_service` to activate and hot-mount into `/mnt/<service>`; do not probe host CLIs like `bash gh` directly):\n{}",
            inactive_lines.join("\n")
        ));
    }

    Some(sections.join("\n"))
}
