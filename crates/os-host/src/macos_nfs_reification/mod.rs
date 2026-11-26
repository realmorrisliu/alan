//! Zero-root macOS loopback NFSv3 bridge for read-only aP namespace reification (Track B).
//!
//! Exports any `Arc<dyn alan_ap::FileServer>` over a pure-Rust loopback TCP server
//! bound strictly to `127.0.0.1:0` (serving MOUNT v3 and NFS v3 on the same port without
//! `rpcbind` or root privileges), mounts it read-only via `/sbin/mount_nfs`, guarantees
//! `NFS3ERR_ROFS` on all mutating procedures, and cleans up mounts on drop/exit and
//! dead-PID startup GC.

mod ap_bridge;
pub mod mount;
pub mod nfs_server;
pub mod rpc;
pub mod xdr;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_mount;

pub use mount::{
    ReifiedMountGcReport, ReifiedMountHandle, ReifiedMountManager, gc_stale_reified_mounts,
    mount_nfs_options,
};
pub use nfs_server::{NfsLoopbackServer, NfsServerConfig, NfsService};
