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

use std::collections::{BTreeMap, VecDeque};

use alan_ap::VersionTable;

use crate::{Access, Namespace};

/// Conventional exit code assigned when a process is cancelled via `/proc/<pid>/ctl`
/// or via `link_parent` cascade cancellation.
pub const CANCELLED_EXIT_CODE: i32 = 130;

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
    pub namespace: ExecNamespaceManifest,
    /// Numeric Process descriptors bound to paths in the committed namespace.
    #[serde(default)]
    pub descriptors: BTreeMap<u32, String>,
    /// When true, exiting this process's parent for any reason deterministically
    /// cancels this process.
    #[serde(default)]
    pub link_parent: bool,
}

/// The versioned namespace mount set a spawner expects a `/proc/clone` commit
/// to use.
///
/// The kernel still receives the actual namespace from the spawner context; this
/// manifest is a commit-time check that the exec document and pending clone
/// snapshot have the same generation and describe the same or a narrower
/// capability set.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ExecNamespaceManifest {
    pub generation: u32,
    #[serde(default)]
    pub mounts: Vec<ExecNamespaceMount>,
}

impl ExecNamespaceManifest {
    /// Build the inspectable manifest for a kernel namespace.
    pub fn from_namespace(namespace: &Namespace) -> Self {
        Self::from_snapshot(namespace, 0)
    }

    /// Build an inspectable manifest bound to one namespace generation.
    pub fn from_snapshot(namespace: &Namespace, generation: u32) -> Self {
        let mounts = namespace
            .describe()
            .into_iter()
            .map(|(path, access)| ExecNamespaceMount::new(path, access.into()))
            .collect::<Vec<_>>();
        Self { generation, mounts }.normalized()
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
        // Stable path ordering keeps same-path union contributors in mount order.
        mounts.sort_by(|left, right| left.path.cmp(&right.path));
        Self {
            generation: self.generation,
            mounts,
        }
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

/// Why a process terminated, rendered in `/proc/<pid>/wait`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    Normal,
    Failed,
    Cancelled,
}

impl ExitReason {
    /// Classify a non-cancellation exit code into `Normal` (`0`) or `Failed` (non-zero).
    pub fn from_exit_code(code: i32) -> Self {
        if code == 0 {
            Self::Normal
        } else {
            Self::Failed
        }
    }
}

/// One process termination recorded by [`ProcessTable::exit_with_reason`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessExit {
    pub pid: Pid,
    pub exit_code: i32,
    pub reason: ExitReason,
}

/// Terminal JSON record published on `/proc/<pid>/wait`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProcessWaitRecord {
    pub pid: u64,
    pub status: String,
    pub exit_code: i32,
    pub reason: ExitReason,
}

impl ProcessWaitRecord {
    pub fn from_exit(exit: ProcessExit) -> Self {
        Self {
            pid: exit.pid.0,
            status: "exited".to_string(),
            exit_code: exit.exit_code,
            reason: exit.reason,
        }
    }

    /// Encode this record as a single newline-delimited JSON line.
    pub fn to_json_line(&self) -> Vec<u8> {
        let mut bytes =
            serde_json::to_vec(self).expect("ProcessWaitRecord serialization is infallible");
        bytes.push(b'\n');
        bytes
    }
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
    pub exit_reason: Option<ExitReason>,
}

/// A pending process slot allocated by clone-begin but not yet committed. It
/// carries the child's intended credentials and namespace and is private to the
/// spawner until commit.
struct Pending {
    parent: Option<Pid>,
    credentials: Credentials,
    namespace: Namespace,
    namespace_generation: u32,
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
        self.clone_begin_with_namespace_generation(parent, credentials, |_| (namespace, 0))
    }

    pub fn clone_begin_with_namespace(
        &mut self,
        parent: Option<Pid>,
        credentials: Credentials,
        build_namespace: impl FnOnce(Pid) -> Namespace,
    ) -> Option<Pid> {
        self.clone_begin_with_namespace_generation(parent, credentials, |pid| {
            (build_namespace(pid), 0)
        })
    }

    pub fn clone_begin_with_namespace_generation(
        &mut self,
        parent: Option<Pid>,
        credentials: Credentials,
        build_namespace: impl FnOnce(Pid) -> (Namespace, u32),
    ) -> Option<Pid> {
        let pid = self.alloc_pid();
        let (namespace, namespace_generation) = build_namespace(pid);
        self.pending.insert(
            pid,
            Pending {
                parent,
                credentials,
                namespace,
                namespace_generation,
            },
        );
        Some(pid)
    }

    /// Commit a pending slot with its exec spec, starting the process and making
    /// `/proc/<pid>` public. Returns `None` if `slot` is not a pending slot.
    ///
    /// If `exec.link_parent` is `true` and the spawning parent has already
    /// exited before commit, the child commits directly into the cancelled
    /// terminal state so no race window can leave a linked orphan running.
    pub fn commit(&mut self, slot: Pid, exec: ExecSpec) -> Option<Pid> {
        let pending = self.pending.remove(&slot)?;
        let parent_already_exited = exec.link_parent
            && pending
                .parent
                .and_then(|parent_pid| self.processes.get(&parent_pid))
                .is_some_and(|parent| parent.status == Status::Exited);
        let (status, exit_code, exit_reason) = if parent_already_exited {
            (
                Status::Exited,
                Some(CANCELLED_EXIT_CODE),
                Some(ExitReason::Cancelled),
            )
        } else {
            (Status::Running, None, None)
        };
        self.processes.insert(
            slot,
            Process {
                pid: slot,
                parent: pending.parent,
                credentials: pending.credentials,
                namespace: pending.namespace,
                exec,
                status,
                exit_code,
                exit_reason,
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

    /// Generation of the spawner namespace snapshot captured for this clone.
    pub fn pending_namespace_generation(&self, slot: Pid) -> Option<u32> {
        self.pending
            .get(&slot)
            .map(|pending| pending.namespace_generation)
    }

    /// Replace a pending slot's namespace before commit. Used when an exec
    /// manifest deliberately narrows the inherited namespace.
    pub fn replace_pending_namespace(&mut self, slot: Pid, namespace: Namespace) -> Option<()> {
        self.pending.get_mut(&slot)?.namespace = namespace;
        Some(())
    }

    /// Record a process's termination and cascade cancellation to any live
    /// descendants linked via [`ExecSpec::link_parent`].
    ///
    /// Terminal state is recorded once: a later cancel/termination notification
    /// for an already-exited process is ignored, so the real exit code is not
    /// clobbered.
    pub fn exit(&mut self, pid: Pid, code: i32) {
        self.exit_with_reason(pid, code, ExitReason::from_exit_code(code));
    }

    /// Cancel a live process (`exit_code = 130`, `reason = Cancelled`) and
    /// cascade cancellation to any live descendants linked via
    /// [`ExecSpec::link_parent`].
    pub fn cancel(&mut self, pid: Pid) -> Vec<ProcessExit> {
        self.exit_with_reason(pid, CANCELLED_EXIT_CODE, ExitReason::Cancelled)
    }

    /// Record a process's termination with an explicit [`ExitReason`],
    /// deterministically cascading cancellation in breadth-first `Pid` order to
    /// all live descendants linked via [`ExecSpec::link_parent`].
    ///
    /// Returns every process that transitioned from `Running` to `Exited` (empty
    /// if `pid` was absent or already exited).
    pub fn exit_with_reason(
        &mut self,
        pid: Pid,
        code: i32,
        reason: ExitReason,
    ) -> Vec<ProcessExit> {
        let Some(proc) = self.processes.get_mut(&pid) else {
            return Vec::new();
        };
        if proc.status == Status::Exited {
            return Vec::new();
        }
        proc.status = Status::Exited;
        proc.exit_code = Some(code);
        proc.exit_reason = Some(reason);
        // The process's status/exit changed: bump its per-process generation.
        self.versions.bump(pid.0);

        let mut exited = vec![ProcessExit {
            pid,
            exit_code: code,
            reason,
        }];
        let mut queue = VecDeque::from([pid]);

        while let Some(exited_parent) = queue.pop_front() {
            let linked_children: Vec<Pid> = self
                .processes
                .iter()
                .filter_map(|(&child_pid, child)| {
                    (child.status != Status::Exited
                        && child.parent == Some(exited_parent)
                        && child.exec.link_parent)
                        .then_some(child_pid)
                })
                .collect();

            for child_pid in linked_children {
                if let Some(child) = self.processes.get_mut(&child_pid)
                    && child.status != Status::Exited
                {
                    child.status = Status::Exited;
                    child.exit_code = Some(CANCELLED_EXIT_CODE);
                    child.exit_reason = Some(ExitReason::Cancelled);
                    self.versions.bump(child_pid.0);
                    exited.push(ProcessExit {
                        pid: child_pid,
                        exit_code: CANCELLED_EXIT_CODE,
                        reason: ExitReason::Cancelled,
                    });
                    queue.push_back(child_pid);
                }
            }
        }

        exited
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
