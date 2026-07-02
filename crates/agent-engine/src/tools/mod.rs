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
    ReifiedExecutionSubstrateMount, ReifiedHostMount, ReifiedMountAccess, ReifiedMountDeclaration,
    ReifiedMountSource, ReifiedNamespacePlan, ReifiedNamespacePlanError, ReifiedNamespacePlanInput,
    ReifiedScratchTmpMount, default_execution_substrate,
};
pub use sandbox::{ExecResult, NetworkPosture, Sandbox, SandboxSpec};
pub use sandbox_backend::{
    LinuxReificationCapability, LinuxReificationCapabilityReport, LinuxReificationStatus,
    SandboxBackendKind, active_backend_name, confines_network, detect_backend, os_backend_active,
    preferred_linux_backend_with_reification, probe_linux_reification, seatbelt_profile,
};
