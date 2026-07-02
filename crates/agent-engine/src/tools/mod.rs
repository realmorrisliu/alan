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

pub use context::{ToolContext, ToolExecutionBinding};
pub use registry::{Tool, ToolLocality, ToolRegistry, ToolResult};
pub use reified_namespace::{
    DEFAULT_SCRATCH_TMP_NAMESPACE_PATH, DEFAULT_WORKSPACE_NAMESPACE_PATH,
    LinuxReifiedNamespaceRunner, ReifiedExecutionSubstrateMount, ReifiedHostMount,
    ReifiedMountAccess, ReifiedMountDeclaration, ReifiedMountSource, ReifiedNamespaceCommandSpec,
    ReifiedNamespacePlan, ReifiedNamespacePlanError, ReifiedNamespacePlanInput,
    ReifiedNamespaceRunError, ReifiedNamespaceRunner, ReifiedScratchTmpMount,
    default_execution_substrate,
};
pub use sandbox::{ExecResult, NetworkPosture, Sandbox, SandboxSpec};
pub use sandbox_backend::{
    LinuxReificationCapability, LinuxReificationCapabilityReport, LinuxReificationStatus,
    LinuxReifiedNamespaceBackendReadiness, SandboxBackendKind, active_backend_name,
    active_backend_path_mode, confines_network, detect_backend, detect_projection_backend,
    linux_reified_namespace_backend_readiness, os_backend_active,
    preferred_linux_backend_with_reification, preferred_linux_backend_with_reification_and_runner,
    probe_linux_reification, seatbelt_profile,
};
