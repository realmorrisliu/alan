//! Tool registry and execution infrastructure.
//!
//! This module provides the `Tool` trait, `ToolRegistry`, `ToolContext`,
//! and `Sandbox` abstractions. Concrete tool implementations live in the
//! `alan-tools` crate.

mod context;
mod registry;
mod reified_namespace;
mod sandbox;
mod sandbox_backend;

pub use alan_agent_protocol::ToolCapability;
pub use alan_ap;

/// System and control-plane namespace prefixes clamped to read-only in a Tool's
/// least-privilege namespace view. Explicitly granted data mounts (Host Mounts,
/// `/memory`, EmbeddedAlan `/mnt/<app>`, etc.) retain their Kernel-granted access.
pub const TOOL_READ_ONLY_SYSTEM_PREFIXES: &[&str] = &[
    "/proc",
    "/agent",
    "/srv",
    "/lib",
    "/man",
    "/bin",
    "/mnt/llm",
    "/mnt/route",
    "/mnt/connections",
    "/mnt/service-manager",
    "/mnt/package",
    "/mnt/packages",
    "/mnt/host-mount",
    "/mnt/host-mounts",
];
#[cfg(test)]
pub(crate) use context::test_execution_binding;
pub use context::{
    ToolContext, ToolExecutionAdapter, ToolExecutionAuthority, ToolExecutionBinding,
};
pub use registry::{
    Tool, ToolProcessInvocation, ToolProcessOutcome, ToolProcessRunner, ToolRegistry, ToolResult,
};
pub use reified_namespace::{
    DEFAULT_PRIMARY_MOUNT_NAMESPACE_PATH, DEFAULT_SCRATCH_TMP_NAMESPACE_PATH,
    LinuxReifiedNamespaceRunner, ReifiedExecutionSubstrateMount, ReifiedHostMount,
    ReifiedMountAccess, ReifiedMountDeclaration, ReifiedMountSource, ReifiedNamespaceCommandSpec,
    ReifiedNamespacePlan, ReifiedNamespacePlanError, ReifiedNamespacePlanInput,
    ReifiedNamespaceRunError, ReifiedNamespaceRunner, ReifiedScratchTmpMount,
    default_execution_substrate,
};
pub(crate) use sandbox::parse_standalone_cd;
pub use sandbox::{
    ExecResult, NetworkPosture, OutputProgressCallback, Sandbox, SandboxHostMount, SandboxSpec,
};
pub use sandbox_backend::{
    LinuxReificationCapability, LinuxReificationCapabilityReport, LinuxReificationStatus,
    LinuxReifiedNamespaceBackendReadiness, SandboxBackendKind, active_backend_name,
    active_backend_path_mode, confines_network, detect_backend, detect_projection_backend,
    linux_reified_namespace_backend_readiness, os_backend_active,
    preferred_linux_backend_with_reification, preferred_linux_backend_with_reification_and_runner,
    probe_linux_reification, seatbelt_profile,
};
