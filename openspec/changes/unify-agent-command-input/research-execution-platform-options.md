# Execution platform options and feasibility gates

Disposition update: the user subsequently selected native Host shell execution
with explicit aP access. Linux/FUSE options below are deferred research, not the
accepted implementation recommendation. See [decision report](decision-report.md).

Status: research and validation plan, 2026-09-24. No prototype was executed.
This explores alternatives to the accepted ADR-0058 implementation direction;
it does not supersede that ADR, authorize migration, or change canonical specs.

## Independent decisions

1. Command language: bounded argv grammar, reusable AST parser, or a complete
   existing shell interpreter. Parsing and executing shell constructs are separate.
2. Execution substrate: current native adapters, Linux workers, or Linux as the
   primary platform for all Agent Runtime services.
3. Service exposure: native aP, FUSE projection, a 9P gateway, or a new kernel client.

## Options

| Option | Shape | Main benefit | Main cost / falsification test |
| --- | --- | --- | --- |
| A | Native Alan, reusable parser, current aP executors | Least architecture change | Must maintain supported command semantics; reject unsupported AST before effects |
| B | Native Alan control services plus Linux command workers | Mature shell/tool environment without moving all services | Identity, cancellation, cwd and evidence span the boundary; test loss and restart |
| C | Linux-hosted alan9 services, existing shell, FUSE service projection | One Linux execution world for commands and agents | Redefine Kernel/Process ownership, qualify FUSE semantics and host integration |
| D | Linux-hosted services via aP-to-9P gateway and v9fs | Reuse upstream kernel 9P client | aP is not 9P; translation and VFS semantics still require qualification |
| E | Linux kernel-native aP filesystem client | Potential direct aP protocol implementation | Kernel maintenance and Linux VFS semantics remain; unjustified before measured FUSE limits |

A is a baseline, not assumed long-term winner. B is only attractive as a durable
product architecture with explicit native/worker responsibilities; do not build
it merely as a temporary bridge to C. C is the preferred research candidate if
ordinary Unix tools should share the agent's world. D is a fallback to investigate
when FUSE-specific costs are demonstrated. E has no current evidence-based need.

Rust parsing candidates: [brush-parser](https://docs.rs/brush-parser/latest/brush_parser/)
provides POSIX/bash tokenizer and AST; [yash-syntax](https://docs.rs/yash-syntax/latest/yash_syntax/)
provides shell AST/parser; [shlex](https://docs.rs/shlex/latest/shlex/) splits shell-like
words but is not a complete shell grammar. Dependency cost and dialect compatibility
are unmeasured. Parser reuse does not supply a process executor or aP file semantics.

## Linux filesystem projection

Candidate path:

    ordinary shell / tools
      -> Linux syscalls and VFS
      -> existing kernel FUSE driver
      -> userspace aP filesystem adapter
      -> existing aP FileServer or wire client
      -> AgentFS / Memory Stores / other services

The kernel handles FUSE, not aP; the adapter translates to aP. This needs no custom
kernel patch when the deployed kernel and mount policy support FUSE. A Rust
implementation could evaluate [fuser](https://docs.rs/fuser/latest/fuser/), without
assuming its API or dependencies are already selected.
[Kernel FUSE overview](https://docs.kernel.org/filesystems/fuse/fuse.html).

Read-only file projection is a smaller claim than complete protocol equivalence.
aP commit-on-clunk, clone allocation, blocking streams and process-scoped authority
need explicit mapping. See the companion [filesystem study](research-ap-linux-filesystems.md).
FUSE exposes files; it does not make an aP executable record into a Linux ELF file
or make Linux fork/exec children appear automatically in Alan's Process table.

## Ownership questions before migration

- Linux owns actual process scheduling, kernel PID lifecycle, wait status and signals.
  Decide which Alan logical execution identities remain necessary and how they
  reference Linux processes, including children spawned internally by shell scripts.
  Do not retain two independent authorities for the same actual process.
- Keep Linux /proc intact in the experiment. Mount the existing aP root below
  /mnt/alan9 initially. Production /agent, /srv and logical Process paths require
  a separate naming/ownership decision; an experimental prefix is not the target.
- One global FUSE mount under one UID does not prove per-Agent authority. Compare
  isolated mount views and capability-scoped adapter instances/connections; test
  cached lookups, open handles, FD inheritance and grant revocation explicitly.
- A whole shell script may be a single authorized action that spawns many Linux
  children. It does not give per-command or per-syscall policy review automatically.
  Determine whether script-level authority is the desired product contract.
- Ordinary project trees and toolchains can stay on native Linux filesystems.
  Project only service trees through FUSE; test whole-root FUSE only if justified.

## Validation order and stop conditions

| Gate | Smallest experiment | Evidence required / stop condition |
| --- | --- | --- |
| 0: semantics | Table mapping aP operations to VFS, and actual Process to logical identity | Explicit ownership and every unsupported operation identified; no claim that paths alone provide authority |
| 1: read projection | One AgentFS-like tree, stat/ls/cat and concurrent live reads in disposable Linux | Correct offsets, cancellation and terminal EOF; no blocking reader starves control requests |
| 2: transactional writes | Multi-write request with dup/fork/close, malformed input, crash and reconnect | No partial or duplicate side effect; reliable result correlation and observable failure; stop full transparent-write claim if close cannot provide it |
| 3: boundaries | Two workloads with different visible trees and read/write grants | Negative access cases pass, cached data/FDs cannot exceed stated rights, revocation behavior documented |
| 4: real shell | Existing shell reads mounted state, processes real project data in a pipeline, changes cwd and produces failing output | Agent can inspect results, correct exit status, all relevant children terminate on cancellation and unknown effects never replay |
| 5: platforms | Same userland on Linux and macOS Linux VM, shared folder and service restart | Same semantic suite passes; mount privileges recorded, no blanket privileged container as product requirement |
| 6: product cost | Compare A and C on the same workload | Record cold/warm startup, idle RSS/CPU, command latency, metadata throughput, event latency and file-sharing cost before choosing |

Run gates 1-4 in an existing disposable Linux environment before investing in VM
packaging. A failure in gate 2 may support a deliberately limited FUSE surface plus
native aP clients for transactional operations, or an explicit new commit/result
contract. Either is a product decision; do not silently weaken the current contract.
Do not treat an idempotency key alone as proof of exactly-once external effects.
Freeze workload and acceptance budgets before performance comparison. Retain exact
build/kernel versions, traces, operation/result IDs and cleanup evidence.

## Deployment feasibility

Apple's [Virtualization framework](https://developer.apple.com/documentation/virtualization/creating-and-running-a-linux-virtual-machine)
supports Linux guests. Begin with an existing distribution and stock supported
kernel, not kernel trimming or a custom distro. Linux containers share the host
kernel, while VMs have their own; test capabilities and mount policy separately.
[Docker container/VM explanation](https://docs.docker.com/get-started/docker-concepts/the-basics/what-is-a-container/).

Host folder sharing is a different adapter from AgentFS projection.
[virtio-fs](https://docs.kernel.org/filesystems/virtiofs.html) lets a guest access
exported host directories; it does not automatically export aP services.
User code executes with guest Linux tools, not native macOS binaries or APIs.
Credentials, keychain operations and other macOS capabilities still need explicit
host-owned adapters. Host-sharing performance must be measured on real repositories.

## Primary sources for mechanisms

- [FUSE callbacks](https://libfuse.github.io/doxygen/structfuse__operations.html):
  release errors are ignored; flush can repeat and is not fsync.
- [FUSE IO modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html): cache and
  direct IO behavior must match service freshness and stream requirements.
- [Linux v9fs](https://docs.kernel.org/filesystems/9p.html): existing kernel 9P client;
  aP translation is still required.
- [Mount namespaces](https://man7.org/linux/man-pages/man7/mount_namespaces.7.html):
  per-namespace mount views, not an automatic aP credential mapping.
- [cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html): process resource
  grouping/control; [seccomp](https://docs.kernel.org/userspace-api/seccomp_filter.html)
  filters syscalls but is not a complete sandbox on its own.
- Local boundary: crates/ap/src/server.rs defines FileServer operations including
  blocking stream read and commit-on-clunk; crates/kernel/src/lib.rs defines the
  current namespace/Process/proc/srv ownership. These are the migration baseline.
