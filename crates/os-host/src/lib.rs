//! Alan OS Host: one system authority per user, device, and install channel.
//!
//! This crate owns Kernel lifetime, fixed boot composition, boot identity,
//! readiness, System Store paths, and local aP attachment. The fixed
//! composition is temporary and must be deleted when Service Manager owns boot.

mod composition;
pub mod host_mounts;
mod legacy_connections;
mod local;
pub mod paths;

pub use composition::{FixedBootConfig, TEMPORARY_FIXED_COMPOSITION_SUCCESSOR};
pub use legacy_connections::{
    ConnectionMigrationReport, LegacyConnectionPaths, migrate_legacy_connections,
};
pub use local::{
    AlanOsHost, AttachedNamespace, HostEndpointPaths, HostProcessReference, HostReadiness,
    HostStatus, LocalAttachment, request_host_stop, run_host_process,
};
pub use paths::{AgentRuntimeStorePaths, HostStorePaths, SystemStorePaths};
