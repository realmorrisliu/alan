//! alan — AI Turing Machine CLI library.
//!
//! This crate provides direct CLI, shell-control, and host integration functionality.

pub mod cli;
pub mod host_mounts;
pub mod install_channel;
pub mod legacy_state;
pub mod system_store;

pub use system_store::{AgentRuntimeStorePaths, HostStorePaths, SystemStorePaths};
