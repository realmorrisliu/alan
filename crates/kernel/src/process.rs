//! The process table and the single `Process` category (substrate §6.4–§6.5).
//!
//! There is exactly one process category. Whether a process is an agent, a
//! tool, a service, or the root agent is observable only at the file/namespace
//! layer (its `/proc`/`/agent` files and what it mounts), never as a kernel type
//! (ADR-0024 D3). The table is ephemeral runtime state and starts empty on
//! restart (D7); durable identity lives in storage-backed file servers, keyed by
//! path, not by the ephemeral pid.
//!
//! Spawn is staged so process creation can be driven by aP clone-via-open
//! (§7.1a): [`clone_begin`](ProcessTable::clone_begin) allocates a **pending**
//! slot that is not yet in the public listing; [`commit`](ProcessTable::commit)
//! writes the exec spec and starts it (now public);
//! [`discard`](ProcessTable::discard) drops a pending slot so a failed spawn
//! leaks nothing into `/proc`.

use std::collections::BTreeMap;

use alan_ap::VersionTable;

use crate::{Access, Namespace};

/// A process identity. Ephemeral: never reused as a durable reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pid(pub u64);

/// Who a process runs as. The kernel keeps this minimal; richer identity is a
/// file-server concern above the substrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub uname: String,
}

impl Credentials {
    /// The system credential used by boot/Service Manager processes.
    pub fn system() -> Self {
        Self {
            uname: "system".to_string(),
        }
    }

    pub fn user(name: &str) -> Self {
        Self {
            uname: name.to_string(),
        }
    }
}

/// What a process runs: an executable path and its arguments. The executable is
/// a command file bound into the namespace (`/bin/...`), not an RPC method. It
/// deserializes from the exec-spec document written to `/proc/clone` (§7.1a).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ExecSpec {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub namespace: Option<ExecNamespaceManifest>,
    /// Numeric Process descriptors bound to paths in the committed namespace.
    #[serde(default)]
    pub descriptors: BTreeMap<u32, String>,
}

/// The namespace mount set the spawner expects a `/proc/clone` commit to use.
///
/// The kernel still receives the actual namespace from the spawner context; this
/// manifest is a commit-time check that the exec document and inherited pending
/// namespace describe the same or a narrower capability set.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ExecNamespaceManifest {
    #[serde(default)]
    pub mounts: Vec<ExecNamespaceMount>,
}

impl ExecNamespaceManifest {
    /// Build the inspectable manifest for a kernel namespace.
    pub fn from_namespace(namespace: &Namespace) -> Self {
        let mounts = namespace
            .describe()
            .into_iter()
            .map(|(path, access)| ExecNamespaceMount::new(path, access.into()))
            .collect::<Vec<_>>();
        Self { mounts }.normalized()
    }

    pub(crate) fn namespace_subset_from(&self, namespace: &Namespace) -> Option<Namespace> {
        namespace.restrict_to_mounts(&self.normalized_access_mounts())
    }

    fn normalized_access_mounts(&self) -> Vec<(String, Access)> {
        self.normalized()
            .mounts
            .into_iter()
            .map(|mount| (mount.path, mount.access.into()))
            .collect()
    }

    fn normalized(&self) -> Self {
        let mut mounts = self.mounts.clone();
        mounts.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.access.cmp(&right.access))
        });
        Self { mounts }
    }
}

/// One mount declaration in an exec namespace manifest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ExecNamespaceMount {
    pub path: String,
    pub access: ExecNamespaceAccess,
}

impl ExecNamespaceMount {
    pub fn new(path: impl Into<String>, access: ExecNamespaceAccess) -> Self {
        Self {
            path: path.into(),
            access,
        }
    }
}

/// The access rights rendered in `/proc/<pid>/namespace`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
pub enum ExecNamespaceAccess {
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
}

impl From<Access> for ExecNamespaceAccess {
    fn from(access: Access) -> Self {
        match access {
            Access::ReadOnly => Self::ReadOnly,
            Access::ReadWrite => Self::ReadWrite,
        }
    }
}

impl From<ExecNamespaceAccess> for Access {
    fn from(access: ExecNamespaceAccess) -> Self {
        match access {
            ExecNamespaceAccess::ReadOnly => Self::ReadOnly,
            ExecNamespaceAccess::ReadWrite => Self::ReadWrite,
        }
    }
}

/// A process's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Spawned and running its image.
    Running,
    /// Terminated; see [`Process::exit_code`].
    Exited,
}

/// One process-table entry — the whole kernel ontology of a running thing.
pub struct Process {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub credentials: Credentials,
    pub namespace: Namespace,
    pub exec: ExecSpec,
    pub status: Status,
    pub exit_code: Option<i32>,
}

/// A pending process slot allocated by clone-begin but not yet committed. It
/// carries the child's intended credentials and namespace and is private to the
/// spawner until commit.
struct Pending {
    parent: Option<Pid>,
    credentials: Credentials,
    namespace: Namespace,
}

/// The ephemeral process table.
pub struct ProcessTable {
    next: u64,
    pending: BTreeMap<Pid, Pending>,
    processes: BTreeMap<Pid, Process>,
    /// qid generations: key `0` (no pid is ever `0` — `next` starts at 1) is the
    /// public listing generation (bumped when a process appears); key `pid.0` is a
    /// per-process generation (bumped when that process's state changes, e.g. on
    /// exit). A `/proc` view reads these so a cached qid/version goes stale.
    versions: VersionTable,
}

/// The `VersionTable` key for the public `/proc` listing generation (no real pid
/// is `0`, so it never collides with a per-process key).
const LISTING_KEY: u64 = 0;

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTable {
    pub fn new() -> Self {
        Self {
            next: 1,
            pending: BTreeMap::new(),
            processes: BTreeMap::new(),
            versions: VersionTable::new(),
        }
    }

    /// The qid version of the public `/proc` listing (bumped when a process
    /// becomes public).
    pub fn listing_generation(&self) -> u32 {
        self.versions.get(LISTING_KEY)
    }

    /// The qid version of a process's files (bumped when its state changes).
    pub fn generation(&self, pid: Pid) -> u32 {
        self.versions.get(pid.0)
    }

    /// Bump a committed process's file generation after externally visible
    /// metadata changes.
    pub fn bump_generation(&mut self, pid: Pid) {
        if self.processes.contains_key(&pid) {
            self.versions.bump(pid.0);
        }
    }

    fn alloc_pid(&mut self) -> Pid {
        let pid = Pid(self.next);
        self.next += 1;
        pid
    }

    /// Begin a spawn: allocate a pending slot with the child's parent,
    /// credentials, and namespace. The returned pid is private to the caller and
    /// is **not** yet listed in public `/proc`.
    pub fn clone_begin(
        &mut self,
        parent: Option<Pid>,
        namespace: Namespace,
        credentials: Credentials,
    ) -> Option<Pid> {
        self.clone_begin_with_namespace(parent, credentials, |_| namespace)
    }

    pub fn clone_begin_with_namespace(
        &mut self,
        parent: Option<Pid>,
        credentials: Credentials,
        build_namespace: impl FnOnce(Pid) -> Namespace,
    ) -> Option<Pid> {
        let pid = self.alloc_pid();
        let namespace = build_namespace(pid);
        self.pending.insert(
            pid,
            Pending {
                parent,
                credentials,
                namespace,
            },
        );
        Some(pid)
    }

    /// Commit a pending slot with its exec spec, starting the process and making
    /// `/proc/<pid>` public. Returns `None` if `slot` is not a pending slot.
    pub fn commit(&mut self, slot: Pid, exec: ExecSpec) -> Option<Pid> {
        let pending = self.pending.remove(&slot)?;
        self.processes.insert(
            slot,
            Process {
                pid: slot,
                parent: pending.parent,
                credentials: pending.credentials,
                namespace: pending.namespace,
                exec,
                status: Status::Running,
                exit_code: None,
            },
        );
        // A process became public: the /proc listing changed.
        self.versions.bump(LISTING_KEY);
        Some(slot)
    }

    /// Discard a pending slot (spawn aborted/rejected). It was never public, so
    /// nothing leaks and no `/proc` watcher observed it.
    pub fn discard(&mut self, slot: Pid) {
        self.pending.remove(&slot);
    }

    /// Borrow the namespace of a pending clone slot before it is committed.
    pub fn pending_namespace(&self, slot: Pid) -> Option<&Namespace> {
        self.pending.get(&slot).map(|pending| &pending.namespace)
    }

    /// Replace a pending slot's namespace before commit. Used when an exec
    /// manifest deliberately narrows the inherited namespace.
    pub fn replace_pending_namespace(&mut self, slot: Pid, namespace: Namespace) -> Option<()> {
        self.pending.get_mut(&slot)?.namespace = namespace;
        Some(())
    }

    /// Record a process's termination. Terminal state is recorded once: a later
    /// cancel/termination notification for an already-exited process is ignored,
    /// so the real exit code is not clobbered.
    pub fn exit(&mut self, pid: Pid, code: i32) {
        if let Some(proc) = self.processes.get_mut(&pid)
            && proc.status != Status::Exited
        {
            proc.status = Status::Exited;
            proc.exit_code = Some(code);
            // The process's status/exit changed: bump its per-process generation.
            self.versions.bump(pid.0);
        }
    }

    /// The publicly visible pids, in pid order. Pending slots are excluded.
    pub fn list(&self) -> Vec<Pid> {
        self.processes.keys().copied().collect()
    }

    /// Borrow a committed process by pid.
    pub fn get(&self, pid: Pid) -> Option<&Process> {
        self.processes.get(&pid)
    }
}
