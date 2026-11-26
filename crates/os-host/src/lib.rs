//! Alan OS Host: one independent system instance per Alan invocation.
//!
//! This crate owns platform lifetime, boot identity, System/Host Store paths,
//! native adapters, and local aP attachment. Alan OS service lifecycle belongs
//! to `alan-service-manager`.

mod boot;
pub mod cow_workspaces;
pub mod embed;
pub mod host_mounts;
pub mod installation;
mod legacy_connections;
mod local;
pub mod macos_nfs_reification;
mod one_shot;
pub mod paths;
pub mod reset;
mod secret_store;

pub use boot::HostBootConfig;
pub use cow_workspaces::{
    CloneFallbackPolicy, ConflictReason, CowArenaWorkspaceAdapter, CowBranchWorkspaceAdapter,
    CowHostRuntime, CowWorkspaceConfig, CowWorkspaceError, CowWorkspaceIsolationAdapter,
    CowWorkspaceManager, CowWorkspaceSummary, CreateWorkspaceOptions, DiffPathEntry, DiffSummary,
    ExitFlushReport, FileChangeKind, FlushedWorkspaceEvidence, GcReport, GitBranchExportReport,
    IntentProvenance, PatchExportReport, SyncBackReport, SyncConflict, WorkspaceBacking,
    WorkspaceRef,
};
pub use embed::{
    AgentSessionHandle, EmbedEvent, EmbedEventStream, EmbeddedAlan, EmbeddedConfig, HookDecision,
    HostMountAccess, HostMountGrantRecord, LlmSource, RootAgentClient, SessionOptions,
    SharedLlmProvider, StorageMode, SystemPromptMode, ToolPolicyHook,
};
pub use host_mounts::saved_grants::{
    SavedGitPushRule, SavedProjectGrantRecord, SavedProjectGrantStore,
};
pub use legacy_connections::{
    ConnectionMigrationReport, LegacyConnectionPaths, migrate_legacy_connections,
};
pub use local::{
    AlanOsHost, AttachedNamespace, HostCommandPlane, HostEndpointPaths, HostProcessReference,
    HostProjectMount, HostReadiness, HostStatus, LocalAttachment, ProjectMountRejected,
    UnsupportedProcesslessAttachment, request_host_stop, run_host_process,
};
pub use macos_nfs_reification::{
    NfsLoopbackServer, NfsServerConfig, NfsService, ReifiedMountGcReport, ReifiedMountHandle,
    ReifiedMountManager, gc_stale_reified_mounts, mount_nfs_options,
};
pub use one_shot::complete_one_shot_prompt;
pub use paths::{AgentRuntimeStorePaths, HostStorePaths, SystemStorePaths};
pub use reset::{ResetReport, ResetScope, reset_alan_stores};
pub use secret_store::{EphemeralAdcFile, SecretStore, apply_profile_to_config};
