# aP on Linux: filesystem integration research

Disposition update: the user subsequently selected native Host shell execution
with explicit aP access. Linux/FUSE options below are deferred research, not the
accepted implementation recommendation. See [decision report](decision-report.md).

Date: 2026-09-24. Status: exploratory evidence, not an accepted architecture change.
No prototype, runtime change, or performance measurement was performed.

## Current contract

- [Canonical substrate specification](../../specs/plan9-kernel-substrate/spec.md)
  defines aP as its own protocol, explicitly permits a future 9P gateway, and
  requires per-Process namespace capability boundaries.
- [`FileServer`](../../../crates/ap/src/server.rs) exposes walk/open/read/write/
  stat/create/remove/clunk. It already has an exported/imported byte transport.
- [`types.rs`](../../../crates/ap/src/types.rs) defines independent fids, clone
  allocation on open, resumable blocking streams, and mount-derived authority.
- Complete documents commit at clunk; malformed documents must return a commit
  error and start no interaction. This includes Process spawn and Agent input.
- The current trait has no general fsync, poll/readiness, truncate, rename,
  symlink, or permission-changing operations. POSIX projection therefore needs
  an explicit supported subset; it cannot be presumed a lossless adapter.

## Integration choices

| Route | Feasibility and boundary | Initial judgment |
| --- | --- | --- |
| Linux VFS → existing FUSE driver → userspace aP bridge | Ordinary Linux programs issue filesystem syscalls; the bridge calls existing aP services. No new kernel driver required. | First candidate for a bounded feasibility experiment. |
| Linux VFS → existing v9fs → userspace 9P/aP gateway | Existing Linux 9P client, with a real translation gateway; aP bytes are not 9P wire bytes. | Second candidate if 9P transport or remote mounting is valuable. |
| Linux VFS → new native aP kernel filesystem | Technically possible through VFS registration, inode and file operations. | Defer until measured limitations justify kernel maintenance. |

FUSE is explicitly a userspace filesystem mechanism with a kernel component;
an aP bridge would run Alan services in userspace while exposing their trees
through Linux VFS. [Linux FUSE overview](https://docs.kernel.org/filesystems/fuse/fuse.html)

Linux v9fs supports 9P2000, 9P2000.u and 9P2000.L, with TCP, Unix and virtio
among its transports. Its UID-based attach modes do not automatically represent
Alan Process capabilities. A gateway must select an authorized aP attachment.
[Linux v9fs](https://docs.kernel.org/filesystems/9p.html)

A native filesystem can register with Linux VFS, but still inherits Linux file
descriptor semantics. It does not by itself solve the close/commit mismatch.
Linux does not promise a stable internal driver interface, adding maintenance
cost. [VFS](https://docs.kernel.org/filesystems/vfs.html),
[kernel interfaces](https://docs.kernel.org/process/stable-api-nonsense.html)

## First blocker: commit-on-clunk is not ordinary close

FUSE `flush` may occur repeatedly for one open because of dup/fork, and cannot
identify the final close. `release` occurs after the last reference, including
memory mappings, but its error does not reach the closing application. `fsync`
is an explicit synchronization operation, not a synonym for either callback.
[libfuse operations](https://libfuse.github.io/doxygen/structfuse__operations.html)

Consequently, mapping every flush to clunk can prematurely commit a partial
request; mapping release to clunk loses the synchronous commit-error contract.
This is a semantic blocker to claiming transparent, complete aP compatibility,
not a blocker to exposing read-only trees or ordinary data files.

Candidate resolutions requiring a separate decision:

- Retain native aP clients for transactional writes; initially project reads.
- Define a POSIX submission interface with an explicit commit and readable
  result. Repeated commit must not repeat effects, and writes after commit need
  defined behavior. A helper may be necessary for reliable acknowledgment.
- Change the underlying framing contract only after examining all affected
  services. Merely choosing FUSE or 9P does not authorize this change.

The current v9fs behavior for Tclunk errors must be checked in the exact chosen
kernel and with a failure-injection test; no transparency claim is made here.

## Streams, cache, identity, and executable files

- Dynamic streams should first be evaluated with FUSE direct I/O, which bypasses
  page cache and read-ahead. Shared mmap is disabled by default in this mode.
  Writeback cache may delay writes and assumes changes pass through FUSE, which
  conflicts with independently changing service state unless explicitly handled.
  [FUSE I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html)
- FUSE supports poll and notification, but current aP has no general readiness
  method. Blocking read, interruption, reconnect offsets and poll integration
  need separate checks. Do not assume an ordinary cached regular file gives
  correct tail behavior. [libfuse operations](https://libfuse.github.io/doxygen/structfuse__operations.html)
- v9fs cache=none disables caching and mmap; loose caches need not observe
  server changes. Streams and executable content may require different cache
  policies. [Linux v9fs](https://docs.kernel.org/filesystems/9p.html)
- FUSE request context supplies UID/GID/thread ID, sometimes zero across namespace
  boundaries. These are not Alan credentials; trusting a raw PID lookup alone
  would leave identity/lifetime questions unresolved.
  [libfuse request context](https://libfuse.github.io/doxygen/structfuse__ctx.html)
- Linux mount namespaces give processes different mount views. A prototype can
  test separate authorized mounts/attachments, but namespace visibility alone
  does not settle inherited descriptors or access to the underlying transport.
  [Linux mount namespaces](https://man7.org/linux/man-pages/man7/mount_namespaces.7.html)
- A projected executable flag does not make an Alan executable launch contract
  Linux-executable. Linux execve needs a recognized executable or interpreter
  script; ELF interpreters/libraries must also exist. Keep native binaries on a
  normal filesystem initially; use explicit launch adapters for Alan commands.
  [Linux execve](https://man7.org/linux/man-pages/man2/execve.2.html)

## Minimal feasibility gates, in order

1. Mount a read-only status tree: ls/stat/cat, changing status, error translation,
   and concurrent access must work without stale reads or leaking fids.
2. Exercise a document writer with several writes, malformed input, dup/fork,
   intermediate close, final close, fsync, SIGKILL and bridge disconnect.
   Pass requires a defined commit boundary and observable success/failure;
   early execution, duplicate execution or silent commit failure blocks writable rollout.
3. Exercise two independent stream readers, resume offsets, blocking reads,
   cancellation, service death and unrelated concurrent requests. Pass requires
   no lost bytes, stranded requests or cross-request starvation.
4. Give two callers different authority, including same-UID callers. Probe
   other mounts, guessed paths, inherited fds and transport access. Any access
   beyond granted authority blocks an execution environment claim.
5. Run a mature shell pipeline using ordinary native binaries and projected
   reads. Test cwd, cancellation, exit status and output evidence separately
   from Agent/Alan executable launch integration.
6. Only after semantic gates pass, measure latency, throughput, memory and mount
   counts under representative load. Compare FUSE and a 9P gateway only if an
   observed limitation warrants a second implementation.

Passing these gates establishes filesystem integration evidence; it does not
establish Linux ownership of Alan Process lifecycle or approve a Linux-based OS.
