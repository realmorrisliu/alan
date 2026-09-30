use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

/// Authority requested for a user-selected project directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectAccess {
    /// Tools may inspect files but cannot change them.
    ReadOnly,
    /// Tools may inspect and change files.
    ReadWrite,
}

impl ProjectAccess {
    pub(in crate::file_backed) fn toggle(self) -> Self {
        match self {
            Self::ReadOnly => Self::ReadWrite,
            Self::ReadWrite => Self::ReadOnly,
        }
    }

    pub(in crate::file_backed) fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::ReadWrite => "read-write",
        }
    }
}

/// Local TUI command passed to the native Host adapter.
#[derive(Clone, Debug)]
pub enum ProjectControl {
    /// Request and approve the explicitly selected native directory.
    Mount {
        host_path: PathBuf,
        access: ProjectAccess,
    },
    /// Revoke a project grant after the Process has left its cwd.
    Revoke { grant_id: String },
}

/// Visible metadata for a project grant; it contains no native Host path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMountReceipt {
    /// Host Mount grant identity.
    pub grant_id: String,
    /// Process-visible mount path.
    pub namespace_path: String,
    /// Short label shown by the terminal UI.
    pub label: String,
    /// Effective grant scope.
    pub access: ProjectAccess,
}

/// Result returned by the same-invocation Host project control plane.
#[derive(Clone, Debug)]
pub enum ProjectControlResult {
    /// Directory grant was projected into the Root Agent.
    Mounted {
        /// Visible grant details without the native path.
        receipt: ProjectMountReceipt,
        /// Canonical Host root used only by this local UI for file completion.
        completion_root: PathBuf,
    },
    /// Grant was revoked.
    Revoked,
}

/// Future type used to keep platform Host details behind the CLI adapter.
pub type ProjectControlFuture =
    Pin<Box<dyn Future<Output = Result<ProjectControlResult>> + Send + 'static>>;

/// Host-local project operations supplied by the CLI to the TUI.
pub type ProjectControlHandler =
    Arc<dyn Fn(ProjectControl) -> ProjectControlFuture + Send + Sync + 'static>;
