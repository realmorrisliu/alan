//! Alan Kernel — the Plan 9 substrate.
//!
//! The kernel owns exactly three things (ADR-0024 D9): the per-process
//! **namespace engine** (mount/bind/union/walk), the **process table** (one
//! `Process` category), and the synthetic devices **`/proc`** and **`/srv`**.
//! It depends only on [`alan_ap`] and knows nothing of agents, LLM providers,
//! tape, memory, tools, or any product concept — those are user-space file
//! servers above it (ADR-0025 D1). This is what makes "the kernel changes least"
//! a structural fact rather than a hope.
//!
//! The kernel is **ephemeral** (D7): the process table, namespaces, and fids are
//! runtime state that starts empty on restart. Durability is a property of
//! storage-backed file servers, never of the kernel.

mod bootstrap;
mod mountfs;
mod namespace;
mod process;
mod procfs;
mod srvfs;

pub use bootstrap::KernelRoot;
pub use mountfs::{LiveNamespace, MountFs};
pub use namespace::{Access, Namespace, Resolved, Unreachable};
pub use process::{
    Credentials, ExecNamespaceAccess, ExecNamespaceManifest, ExecNamespaceMount, ExecSpec, Pid,
    Process, ProcessTable, Status,
};
pub use procfs::{ProcFs, ProcessInvocation, ProcessOutcome, ProcessRunner};
pub use srvfs::SrvFs;
