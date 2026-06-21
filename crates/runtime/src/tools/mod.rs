//! Tool registry and execution infrastructure.
//!
//! This module provides the `Tool` trait, `ToolRegistry`, `ToolContext`,
//! and `Sandbox` abstractions. Concrete tool implementations live in the
//! `alan-tools` crate.

mod context;
mod registry;
mod sandbox;
mod sandbox_backend;

pub use context::{ToolContext, ToolExecutionBinding};
pub use registry::{Tool, ToolLocality, ToolRegistry, ToolResult};
pub use sandbox::{ExecResult, Sandbox};
pub use sandbox_backend::{SandboxBackendKind, confines_network, detect_backend, seatbelt_profile};
