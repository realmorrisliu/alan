## Context

Two subsystems that both express "what may the agent touch" exist today, disjoint:

- `alan-kernel` — `Namespace::mount(at, tree: InProcessTransport, access)` builds
  an aP path → `FileServer` + `Access` table; `MountFs` presents it as one file
  server and enforces `Access` in-process (a `ReadOnly` mount rejects mutating
  ops). `describe()` returns `(namespace_path, Access)`. **Every mount is a
  virtual server; the namespace records no host provenance.**
- `alan-agent-engine::tools::sandbox_backend` — generates a Seatbelt SBPL profile
  (macOS) or applies a Landlock ruleset in a `pre_exec` hook (Linux) that confines
  writes to `workspace_root` (+ temp) and denies network by default, with safe
  degradation (no OS backend ⇒ bash/network must escalate, never silently run).
  Input is one hard-coded `workspace_root` + `allow_network: bool`.

`alan-agent-engine` does **not** depend on `alan-kernel`. `bash` runs as a host
subprocess and sees the host filesystem, not the namespace.

The trigger for this work is the product intuition: "mount host dirs into the
namespace, and the agent is structurally isolated — no seatbelt needed." That is
where the design must be careful.

## The Core Thesis

A namespace confines a process **only if every resource access goes through it**.
In real Plan 9 this holds because the kernel mediates all syscalls. Hosted inside
macOS/Linux, the host kernel does not know our namespace, and a native subprocess
issues raw host syscalls that bypass aP entirely. So:

- aP file tools → confined by the **namespace** (in-process, already true).
- `bash` / native subprocess → confined only by an **OS sandbox** (Seatbelt/
  Landlock), never by the namespace.

The fix is not to pick one. It is to drive both from one declaration:

```
                 ┌──────────────────────────────┐
                 │   MOUNT DECLARATION LIST      │   ← single source of truth
                 │   (built at session assembly, │      (human-declared)
                 │    in the `alan` comp. root)  │
                 │                               │
                 │   /mnt/project → HostDir(RW,  │
                 │                  /Users/…/proj)│
                 │   /agent       → AgentFs      │   (virtual, no host path)
                 │   /mnt/llm     → LlmFs        │   (virtual, no host path)
                 └───────────────┬───────────────┘
                                 │
                 ┌───────────────┴───────────────┐
                 ▼                               ▼
      ┌────────────────────┐          ┌────────────────────────┐
      │   NAMESPACE (aP)   │          │  SANDBOX MANIFEST       │
      │   MountFs enforces │          │  SandboxSpec {          │
      │   Access in-proc.  │          │    writable_roots,      │
      │                    │          │    read_denylist,       │
      │  ALL mounts appear │          │    network }            │
      │  (virtual + host)  │          │  ← only HOST-backed     │
      └─────────┬──────────┘          │    entries contribute   │
                │                     └───────────┬────────────┘
                ▼                                 ▼
      aP file tools                     Seatbelt / Landlock
      (read/write/edit)                 confining `bash`
```

Virtual mounts (`/agent`, `/mnt/llm`) contribute nothing to the sandbox manifest —
which is exactly right, because `bash` cannot see them anyway. No special-casing.

## Goals / Non-Goals

**Goals**

- Make the namespace the single policy source; make the OS sandbox a *projection*
  of it, so the two enforcement mechanisms agree by construction.
- Make "mount a local directory" a real capability (via `HostDirFs`).
- Keep an honest, precise statement of what isolation the agent actually has.

**Non-Goals**

- Make the namespace *replace* the OS sandbox (it cannot, for native subprocesses).
- Full read isolation at landing (deferred; see Decision 3 and 6).
- A unified path space between aP tools and `bash` (that is reification — future).
- Agent-requestable mounts at landing (future escalation feature).

## Decisions

### D1. Explore, not a single proposal

The prerequisite (`HostDirFs`) does not exist and the namespace↔sandbox relation
was not yet a fixed contract. Writing this as one implementable proposal would
freeze unresolved boundaries into tasks. So this change is exploratory framing; it
sequences P1/P2/P3 as separate changes.

### D2. Projection now (A), reification later (B, Linux-only)

Two ways to relate the sandbox to the namespace:

| Axis                | A — Projection                          | B — Reification                                   |
|---------------------|-----------------------------------------|---------------------------------------------------|
| Mechanism           | Derive Seatbelt/Landlock rules from     | Materialize the namespace as a real per-process   |
|                     | host-backed mounts; `bash` runs on      | FS view (Linux mount ns + bind mounts / FUSE);    |
|                     | real host paths                         | `bash` literally sees `/mnt/project`              |
| Path space          | Split (aP sees `/mnt/project`, bash     | Unified                                           |
|                     | sees `/Users/…/proj`)                   |                                                   |
| Read isolation      | Not by default (see D3)                 | Free — an empty mount ns sees only what's mounted |
| Reuse of built code | Full — `sandbox_backend` unchanged,     | Discards the Landlock path; builds a container    |
|                     | only its *input* changes                | runtime (user ns + rootfs bind set) — bubblewrap  |
|                     |                                         | class, its own project                            |
| Platforms           | macOS + Linux                           | Linux only (mac has no unprivileged bind mount;   |
|                     |                                         | FUSE on mac is impractical)                       |

**Decision: land A on both platforms; record B as the future Linux path to full
read isolation, sequenced as its own change.** A's real change is a single seam:
the sandbox's *input* moves from `workspace_root` to a namespace-derived
`SandboxSpec`. The "one table, two enforcements" thesis already holds under A.

### D3. The reads axis

The projection decides three things: writable paths, network, **readable paths**.
Writes and network are already confined. Reads are `(allow default)` today, so a
native subprocess can read `~/.ssh`, `~/.aws`, the `~/.alan` secret store,
keychains. That contradicts a naive "the agent is isolated" claim.

Options weighed: (a) leave reads open; (b) deny-by-default reads (only mounted
paths readable) — the faithful choice, but Landlock is an **allow-list** model so
"broad-minus-a-few" is unexpressible, and locking reads breaks native tooling
(dyld, `/usr/bin`, locale); (c) reads broad but a **sensitive-read denylist** —
Seatbelt can `deny file-read* (subpath …)`, Landlock cannot.

**Decision: land (c) where possible.** macOS (Seatbelt) gets a sensitive-read
denylist. Linux (Landlock) is limited to write+network isolation short-term. Full
read isolation waits for B (a mount ns exposes only what's mounted, giving (b) for
free — the right mechanism, not a fight with Landlock's allow-list model).

Rationale that keeps read isolation off the launch-blocker list: **with network
denied, the marginal value of read isolation is low.** Exfiltration needs
read *and* an outbound channel; write+network confinement already removes the
channel. Reading a secret with nowhere to send it is a much smaller loss.

### D4. Single source = the mount *declaration list*; strict layering

The projection must **not** introspect `Namespace` to recover host paths — the
namespace records none, and teaching `alan-kernel` about host paths would leak
host concerns into an abstraction that depends only on aP.

Instead, the `alan` composition root (which assembles the namespace) produces two
things from one declaration list:

- the namespace: `mount_host("/mnt/project", HostDirFs::new(host_path), RW)`;
- the manifest: append `(host_path, RW)`.

The projection `manifest → SandboxSpec` lives in `alan`. Result: `alan-kernel`
never learns host paths; `alan-agent-engine` never depends on `alan-kernel`; its
`Sandbox::new` simply takes `SandboxSpec` instead of `workspace_root`. Written
once at the declaration site, projected twice; never reconstructed after the fact.

### D5. Threat model — who can mount

The isolation claim rests entirely on the agent being unable to expand its own
namespace. If the agent had a `mount` tool it could `mount_host("/", RW)` and the
isolation is theater. A mount is an authorization act (granting access to a host
path) and must be authorized outside the agent's control.

**Decision:**
- **Landing:** mounts are human/config-declared at namespace assembly. The agent
  has **no** mount tool. The manifest is fixed for the session (changeable only by
  human action). The existing `workspace_root` is generalized to the **seed entry**
  of the manifest — not a special case, just the first, default host mount.
- **Future (P3):** an agent-requestable `mount` routed through the existing
  `PolicyEngine` as a `Yield` escalation, so each new host-path grant is
  human/reviewer-approved — reusing the escalation machinery, semantically correct
  (mount = authorization = escalate).

### D6. Sequencing

```
P1  Sandbox input refactor          workspace_root ─▶ SandboxSpec (manifest w/ 1 seed entry)
    (pure, zero behavior change)     welds the "two projections" seam; no FS semantics touched
        │                            no HostDirFs needed — workspace IS the seed mount
        ▼
P2  HostDirFs + mount_host           host-backed aP FileServer; declaration records (host_path, access)
    (real "mount a local dir")       manifest grows to N entries; flows into both projections
        │
        ▼
P3+ Hardening & fidelity            macOS Seatbelt sensitive-read denylist (D3/c)
                                     agent-requestable mount via PolicyEngine escalation (D5)
                                     Linux reification for full read isolation (D2/B)
```

P1 first is the risk reducer: the seam is welded and tested with **zero behavior
change** before any filesystem semantics move. P2 then only adds manifest entries;
both projection paths are already in place.

## The honest isolation narrative

State it exactly, so the docs never over-claim:

- **Write + network isolation** — both platforms, at landing (P1/P2).
- **Sensitive-read isolation** — macOS first (Seatbelt denylist, P3).
- **Full read isolation** — arrives with Linux reification (B), a later change.

"Namespace instead of sandbox" is false for native subprocesses. "One declaration
list projected into namespace enforcement *and* sandbox enforcement" is the true,
buildable statement.

## Open questions (for the downstream proposals, not blockers here)

- `SandboxSpec.read_denylist` default contents (`~/.ssh`, `~/.aws`, `~/.alan`
  secrets, keychains, browser profiles) — enumerated in P3.
- Per-invocation re-derivation: each `bash` spawn builds its `SandboxSpec` from the
  current manifest at spawn time (natural, since each run is a fresh confined
  subprocess) — confirm in P1.
- Interaction with the existing `.git`/`.alan`/`.agents` protected-subpath residual
  gap: the workspace seed mount stays RW and the path-guard parser story is
  unchanged — verify no regression in P1.
- Reification (B) scope when it comes: unprivileged user namespaces are disabled on
  some distros; a rootfs bind-mount set (`/bin`, `/usr`, `/lib`, `/dev`, `/proc`,
  `resolv.conf`) is required for binaries to run. Treat as a container-runtime-class
  effort, not a flag.
