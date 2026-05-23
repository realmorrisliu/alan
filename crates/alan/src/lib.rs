//! alan — AI Turing Machine CLI & daemon library.
//!
//! This crate provides both the CLI binary and daemon server functionality.

pub mod cli;
pub mod daemon;
pub mod host_config;
pub mod install_channel;
pub mod registry;
mod skill_catalog;

/// Output mode for `alan ask`.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OutputMode {
    /// Human-readable streaming output (default)
    Text,
    /// Raw NDJSON event stream for agent/automation consumption
    Json,
    /// Silent streaming; emit accumulated text only at turn end
    Quiet,
}

pub use registry::{WorkspaceEntry, WorkspaceRegistry, generate_workspace_id};

// Re-export daemon components for advanced use
pub use daemon::{
    runtime_manager::{RuntimeInfo, RuntimeManager, RuntimeManagerConfig},
    session_store::{SessionBinding, SessionStore},
    workspace_resolver::{ResolvedWorkspace, WorkspaceResolver},
};
