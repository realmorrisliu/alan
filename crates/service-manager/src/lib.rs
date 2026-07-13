//! Alan OS Service Manager and the mandatory system services it supervises.
//!
//! Alan OS Host creates the Kernel and starts one [`ServiceManager`] Process.
//! This crate owns every later system Process, Boot Unit state, readiness,
//! supervision, local entry, Host Mount grants, and Connection metadata.

mod boot_unit;
mod connection;
mod control_fs;
mod flat_fs;
mod host_mount;
mod local_entry;
mod runtime;

pub use boot_unit::{
    BootDescriptor, BootManifest, BootMount, BootUnit, MountAccess, RestartPolicy,
};
pub use connection::{
    ConnectionService, NativeConnectionAction, NativeConnectionRequest, NativeConnectionResponse,
};
pub use control_fs::{
    ManagerState, RestartDecision, ServiceManagerFs, SystemStatus, UnitSnapshot, UnitStatus,
};
pub use host_mount::{
    HostMountAccess, HostMountApplicatorFactory, HostMountExport, HostMountExportAdapter,
    HostMountGrantRecord, HostMountRequest, HostMountService, UnavailableHostMountExportAdapter,
};
pub use local_entry::LocalEntryService;
pub use runtime::{
    BOOT_ID_PATH, BOOT_STATE_PATH, LlmClientFactory, ServiceManager, ServiceManagerConfig,
};
