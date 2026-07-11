//! alan — AI Turing Machine CLI library.
//!
//! This crate provides direct CLI, shell-control, and host integration functionality.

pub mod cli;
pub mod host_mounts;
pub mod install_channel;
pub mod registry;
mod skill_catalog;

pub use registry::{WorkspaceEntry, WorkspaceRegistry, generate_workspace_id};
