//! Alan OS Service Manager and the mandatory system services it supervises.
//!
//! Alan OS Host creates the Kernel and starts one [`ServiceManager`] Process.
//! This crate owns every later system Process, Boot Unit state, readiness,
//! supervision, local entry, Host Mount grants, Connection metadata, and
//! Package Service lifecycle.

mod agent_runtime;
mod agent_work;
mod boot_unit;
mod connection;
mod connection_profile;
mod control_fs;
mod fileserver_service;
mod flat_fs;
pub mod githubfs;
mod host_mount;
mod local_entry;
pub mod manpages;
mod package;
mod process_launch;
mod process_runner;
mod process_spawn;
mod quartermaster;
mod runtime;
mod temporary_store;

pub use boot_unit::{
    BootDescriptor, BootManifest, BootMount, BootUnit, MountAccess, RestartPolicy,
};
pub use connection::{
    ConnectionService, NativeConnectionAction, NativeConnectionRequest, NativeConnectionResponse,
    provider_packages,
};
pub use connection_profile::{
    ConnectionCredential, ConnectionProfile, ConnectionStoreBindings, ConnectionsFile,
    CredentialKind, ProviderDescriptor, ResolvedConnectionProfile, default_credential_backend,
    first_party_provider_catalog, normalize_profile_settings, provider_catalog,
    sanitize_identifier, validate_profile_settings,
};
pub use control_fs::{
    ManagerState, RestartDecision, ServiceManagerFs, SystemStatus, UnitSnapshot, UnitStatus,
};
pub use fileserver_service::{FileserverLaunchGrant, FileserverManager, FileserverStatus};
pub use host_mount::{
    HostMountAccess, HostMountExport, HostMountExportAdapter, HostMountGrantRecord,
    HostMountRequest, HostMountService, HostMountStatus, HostMountToolProjection,
    UnavailableHostMountExportAdapter,
};
pub use local_entry::LocalEntryService;
pub use package::{
    FileserverPackageExport, FileserverTransportSpec, McpPackageExport, McpTransportSpec,
    PackageCatalog, PackageCommand, PackageCommandResult, PackageExport, PackageExportKind,
    PackageKind, PackageRecord, PackageReferenceLease, PackageService, PackageSnapshot,
    PackageSnapshotEntry, PackageState, PackageSyncEntry, ProviderAuthMode, ProviderPackageExport,
    ToolPackageExport, validate_package_id, validate_package_store_for_migration,
};
pub use process_launch::ProcessLaunchContext;
pub use quartermaster::{
    CLI_IMPORT_MOUNT_PATH, QuartermasterCommandResult, run_q_command_with_package_service,
};
pub use runtime::{
    BOOT_ID_PATH, BOOT_STATE_PATH, InputShadowSelection, LlmClientFactory, ServiceManager,
    ServiceManagerConfig, SessionHandle, SessionLaunchOptions,
};
