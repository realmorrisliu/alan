## Why

Alan hosts an agent as a **process-level OS inside the host OS**: you type `alan`
in a macOS/Linux terminal and enter an environment with its own shell and file
namespace. The next step in that vision is to `mount` real host directories (and,
later, programs) into the namespace under Plan 9 semantics, so the agent reaches
host resources through the namespace rather than through ambient host access.

The appeal is that isolation would then be **structural**: the agent can only
touch what is mounted, so we would not need a separate OS sandbox layer. That
intuition is half right and half a trap, and this change exists to pin down which
half is which before we build.

Today two worlds are disjoint:

- **The namespace world** (`alan-kernel`: `Namespace`, `MountFs`) enforces access
  *in-process* over aP file operations — a read-only mount cannot be written. But
  every mounted `FileServer` is virtual (`MemFs`, `AgentFs`, `ProcFs`, `SrvFs`).
  There is **no host-directory-backed file server**, so "mount a local directory"
  is not yet a real capability.
- **The sandbox world** (`alan-agent-engine`: `sandbox_backend`) already enforces
  confinement *at the OS level* for the `bash` / native-subprocess path — Seatbelt
  on macOS, Landlock+seccomp on Linux, with safe degradation. But its input is a
  single hard-coded `workspace_root` plus one `allow_network` boolean. It knows
  nothing about the namespace.

The key realization: **the agent's tools live in two different path spaces.** aP
file tools (read/write/edit) see the namespace (`/proc`, `/agent`, `/mnt/llm`,
mounted host dirs); `bash` is a host-native subprocess that sees the real host
filesystem and cannot see the namespace at all. Plan 9 does not have this split
because its own `bash` uses the namespace; here `bash` is a host binary. So a
namespace **cannot by itself** confine a native subprocess — that still needs an
OS sandbox. The two are not alternatives; they are two enforcement mechanisms
that should agree, driven from the same source of truth.

## What Changes

**This is an explore-stage framing document, not an implementable change.** It
records the settled design of "namespace-driven sandbox rule generation" and
sequences the concrete proposals that follow. No code lands under this change-id;
each sequenced proposal (P1/P2/P3) is its own OpenSpec change.

The thesis it fixes: a single **mount declaration list** is the source of truth,
and it projects into two enforcement mechanisms —

1. the **namespace** (in-process aP enforcement for file tools), and
2. a **sandbox manifest** (`SandboxSpec`) that the OS sandbox enforces for native
   subprocesses.

Landing path is **projection** (derive OS-sandbox rules from host-backed mounts),
reused on both platforms. Full read isolation via **reification** (a real
per-process filesystem view) is recorded as a future, Linux-only direction.

Sequenced downstream proposals:

- **P1 — Sandbox input refactor (pure, zero behavior change).** Replace
  `Sandbox`'s `workspace_root: PathBuf` with a `SandboxSpec { writable_roots,
  read_denylist, network }` seeded from a single-entry manifest (the workspace).
  Establishes the "two projections" seam without touching any file-system
  semantics. Lowest risk; lands first.
- **P2 — `HostDirFs` + `mount_host` + multi-entry manifest.** A host-directory-
  backed aP `FileServer`, and a declaration entry point that installs it into the
  namespace *and* records `(host_path, access)` into the manifest. "Mount a local
  directory" becomes real and flows into both projections at once.
- **P3+ — hardening & fidelity.** macOS Seatbelt sensitive-read denylist;
  agent-requestable `mount` routed through the `PolicyEngine` as an escalation;
  Linux reification for full read isolation.

## Capabilities

### New Capabilities

- None land under this change. It is exploratory framing; capabilities are defined
  by the downstream P1/P2/P3 changes it sequences.

### Modified Capabilities

- None.

## Impact

- Builds on `define-plan9-kernel-substrate` (aP protocol, `Namespace`, `MountFs`)
  and the existing `alan-agent-engine` `sandbox_backend` (Seatbelt/Landlock).
- Preserves layering: `alan-kernel` stays ignorant of host paths;
  `alan-agent-engine` stays hosting-agnostic (no dependency on `alan-kernel`). The
  projection wiring lives only in the `alan` composition root.
- Relates to ADR-0024 (the Plan 9 kernel model). Makes the "agent isolated inside
  Alan OS" claim precise and honest rather than aspirational.
- Non-goal: this does not make the namespace *replace* the OS sandbox. It makes
  them agree by projecting both from one declaration list.
