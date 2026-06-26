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
//! writes the exec spec and starts it (now public); [`discard`] drops a pending
//! slot so a failed spawn leaks nothing into `/proc`.

use std::collections::BTreeMap;

use crate::Namespace;

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
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ExecSpec {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
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
}

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
        let pid = self.alloc_pid();
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
        Some(slot)
    }

    /// Discard a pending slot (spawn aborted/rejected). It was never public, so
    /// nothing leaks and no `/proc` watcher observed it.
    pub fn discard(&mut self, slot: Pid) {
        self.pending.remove(&slot);
    }

    /// Record a process's termination.
    pub fn exit(&mut self, pid: Pid, code: i32) {
        if let Some(proc) = self.processes.get_mut(&pid) {
            proc.status = Status::Exited;
            proc.exit_code = Some(code);
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
