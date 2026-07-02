//! Kernel bootstrap root (§7.3).
//!
//! Alan Kernel starts with only the substrate: `/proc`, `/srv`, and the namespace
//! engine that presents those mounts as one root. User-space init / Service
//! Manager assembles `/agent`, `/bin`, `/lib`, `/man`, and `/mnt` later by
//! starting file servers and mounting their `/srv` handles.

use std::sync::Arc;

use alan_ap::InProcessTransport;

use crate::{Access, Credentials, MountFs, Namespace, ProcFs, SrvFs};

/// The minimal Alan Kernel root namespace.
pub struct KernelRoot {
    procfs: Arc<ProcFs>,
    srvfs: Arc<SrvFs>,
    root: Arc<MountFs>,
}

impl Default for KernelRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelRoot {
    /// Bring up the substrate-only root: `/proc`, `/srv`, and the namespace
    /// engine. No higher-level user-space tree is mounted here.
    pub fn new() -> Self {
        let procfs = ProcFs::new();
        let srvfs = Arc::new(SrvFs::new());

        let mut namespace = Namespace::new();
        namespace.mount(
            "/proc",
            InProcessTransport::new(Arc::new(procfs.clone())),
            Access::ReadWrite,
        );
        namespace.mount(
            "/srv",
            InProcessTransport::new(srvfs.clone()),
            Access::ReadWrite,
        );
        let procfs = Arc::new(procfs.for_spawner(None, namespace.clone(), Credentials::system()));
        namespace.unmount("/proc");
        namespace.mount(
            "/proc",
            InProcessTransport::new(procfs.clone()),
            Access::ReadWrite,
        );

        Self {
            procfs,
            srvfs,
            root: Arc::new(MountFs::new(namespace)),
        }
    }

    /// A single aP handle over the boot root namespace.
    pub fn transport(&self) -> InProcessTransport {
        InProcessTransport::new(self.root.clone())
    }

    /// The boot `/proc` device.
    pub fn procfs(&self) -> Arc<ProcFs> {
        self.procfs.clone()
    }

    /// The boot `/srv` device.
    pub fn srvfs(&self) -> Arc<SrvFs> {
        self.srvfs.clone()
    }
}
