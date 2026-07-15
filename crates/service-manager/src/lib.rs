//! Alan OS Service Manager and the mandatory system services it supervises.
//!
//! Alan OS Host creates the Kernel and starts one [`ServiceManager`] Process.
//! This crate owns every later system Process, Boot Unit state, readiness,
//! supervision, local entry, Host Mount grants, Connection metadata, and
//! Package Service lifecycle.

mod agent_runtime;
mod boot_unit;
mod connection;
mod connection_profile;
mod control_fs;
mod flat_fs;
mod host_mount;
mod local_entry;
mod package;
mod quartermaster;
mod runtime;

pub use boot_unit::{
    BootDescriptor, BootManifest, BootMount, BootUnit, MountAccess, RestartPolicy,
};
pub use connection::{
    ConnectionService, NativeConnectionAction, NativeConnectionRequest, NativeConnectionResponse,
};
pub use connection_profile::{
    ConnectionCredential, ConnectionProfile, ConnectionStoreBindings, ConnectionsFile,
    CredentialKind, ProviderDescriptor, ResolvedConnectionProfile, default_credential_backend,
    normalize_profile_settings, provider_catalog, sanitize_identifier, validate_profile_settings,
};
pub use control_fs::{
    ManagerState, RestartDecision, ServiceManagerFs, SystemStatus, UnitSnapshot, UnitStatus,
};
pub use host_mount::{
    HostMountAccess, HostMountApplicatorFactory, HostMountExport, HostMountExportAdapter,
    HostMountGrantRecord, HostMountRequest, HostMountService, UnavailableHostMountExportAdapter,
};
pub use local_entry::LocalEntryService;
pub use package::{
    PackageCatalog, PackageCommand, PackageCommandResult, PackageExport, PackageKind,
    PackageRecord, PackageReferenceLease, PackageService, PackageSnapshot, PackageSnapshotEntry,
    PackageState,
};
pub use runtime::{
    BOOT_ID_PATH, BOOT_STATE_PATH, LlmClientFactory, ServiceManager, ServiceManagerConfig,
};
