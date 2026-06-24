//! Built-in Alan Agent app projection layer for Alan OS.
//!
//! This crate maps current daemon-backed Alan Agent sessions and protocol events
//! into Alan Kernel semantic objects, buffers, views, commands, tasks, artifacts,
//! and evidence. It does not own provider execution, daemon sessions, memory
//! stores, sandbox execution, or terminal rendering.

mod model;
mod projection;

pub use model::{
    ALAN_AGENT_APP_ID, AgentWorkspaceChildRunEvent, AgentWorkspaceChildRunStatus,
    AgentWorkspaceCommandIds, AgentWorkspaceEffectKind, AgentWorkspaceEvidenceInput,
    AgentWorkspaceHydratedMessage, AgentWorkspaceIds, AgentWorkspaceMemoryObservation,
    AgentWorkspaceMemoryObservationKind, AgentWorkspaceModel, AgentWorkspaceObjectIds,
    AgentWorkspaceObjectRole, AgentWorkspaceRolloutRecord, AgentWorkspaceSessionMetadata,
    AgentWorkspaceSurfaceIds, COMPATIBILITY_SESSION_ADAPTER,
};
pub use projection::{AgentWorkspaceProjector, AgentWorkspaceSnapshots};
