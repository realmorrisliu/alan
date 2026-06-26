//! Workspace state types — shared data structures for workspace state persistence.
//!
//! This module contains the **data types** for workspace state persistence.
//! These types are used by the `agentd` crate for managing workspace metadata.
//!
//! Note: The orchestration logic (session management, runtime lifecycle) lives in the
//! `agentd` crate, as it is a hosting concern rather than a core runtime concern.

mod state;

pub use state::{
    PersistedLlmProvider, WorkspaceConfigState, WorkspaceInfo, WorkspaceState, WorkspaceStatus,
};
