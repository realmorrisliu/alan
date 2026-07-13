//! Alan OS Host: one system authority per user, device, and install channel.
//!
//! This crate owns platform lifetime, boot identity, System/Host Store paths,
//! native adapters, and local aP attachment. Alan OS service lifecycle belongs
//! to `alan-service-manager`.

mod boot;
pub mod host_mounts;
mod legacy_connections;
mod local;
pub mod paths;

pub use boot::HostBootConfig;
pub use legacy_connections::{
    ConnectionMigrationReport, LegacyConnectionPaths, migrate_legacy_connections,
};
pub use local::{
    AlanOsHost, AttachedNamespace, HostCommandPlane, HostEndpointPaths, HostProcessReference,
    HostReadiness, HostStatus, LocalAttachment, request_host_stop, run_host_process,
};
pub use paths::{AgentRuntimeStorePaths, HostStorePaths, SystemStorePaths};
