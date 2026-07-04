## Context

Alan's current native subprocess confinement is projection-based:

- macOS uses Seatbelt profiles generated from `SandboxSpec`.
- Linux uses Landlock plus network controls where available.
- `SandboxSpec.writable_roots` now follows approved host mounts, but the Linux
  subprocess still sees the ambient host filesystem. Landlock cannot express
  "read everything except these sensitive paths" or "read only mounted host
  paths" while still allowing binaries, dynamic libraries, locale data, and shell
  support files.

The namespace-driven sandbox framing records the faithful Linux solution as
reification: build a real per-process filesystem view where only declared mounts
and required execution substrate are visible. In that model, native subprocesses
do not merely receive rules about host paths; they run inside a mount namespace
where `/mnt/project` is a bind mount and `/home/user/.ssh` is absent unless
explicitly declared.

This is Linux-only. macOS remains on Seatbelt because unprivileged bind-mount
namespace construction is not available there.

## Goals / Non-Goals

**Goals:**

- Define the runtime contract for a Linux reified namespace backend.
- Preserve the existing projection backend as fallback.
- Make full read isolation concrete: deny-by-default host reads, not just
  sensitive-read denylist.
- Keep namespace/path semantics honest: native subprocesses see reified `/mnt`
  paths only for host-backed mounts.
- Keep Alan Kernel host-path agnostic and keep `alan-agent-engine` decoupled from
  Alan Kernel.
- Sequence implementation so capability detection and plan generation land
  before privileged/namespace execution mechanics.

**Non-Goals:**

- macOS reification.
- Exposing virtual aP file servers (`/agent`, `/mnt/llm`, `/srv`, `/proc`) as
  native filesystem paths.
- A setuid helper, root daemon, or privileged install path in the first slice.
- Replacing Seatbelt or the existing Linux Landlock path.
- Making every Linux distribution support reification. Unsupported hosts must
  degrade safely.

## Decisions

### D1. Add a third sandbox execution mode: Linux reified namespace

Current backend selection is roughly:

```text
macOS Seatbelt -> Linux Landlock -> workspace path guard
```

Reification adds a Linux-only mode that ranks above Landlock when all required
capabilities are present:

```text
macOS Seatbelt
Linux ReifiedNamespace (if available)
Linux Landlock (projection fallback)
WorkspacePathGuard
```

The backend name and decision audit must distinguish `linux_reified_namespace`
from `landlock`, because their read-isolation semantics differ.

### D2. Introduce a reified plan model before a runner

The first implementation slice should create a pure planning model, not jump
straight into `unshare`:

```rust
struct ReifiedNamespacePlan {
    root: ReifiedRoot,
    mounts: Vec<ReifiedMount>,
    cwd: PathBuf,
    argv: Vec<String>,
    network: NetworkPosture,
}
```

The plan is derived from host-backed mount declarations / `SandboxSpec` plus the
requested command. It is unit-testable on macOS and Linux because it does not
perform namespace operations.

Plan validation must keep three categories separate:

- **declared host mounts**: user/workspace data that becomes visible under
  reified namespace paths such as `/mnt/project`;
- **execution substrate**: read-only system paths needed for `/bin/sh`, dynamic
  linking, locale, certificates, and temporary runtime files;
- **virtual Alan OS mounts**: aP-only resources that are not exposed natively.

### D3. Reified paths are not the same as projected host paths

Projection lets `bash` access the real host path:

```text
aP:   /mnt/project/file.txt
bash: /Users/me/project/file.txt
```

Reification makes the native subprocess see the namespace path:

```text
aP:   /mnt/project/file.txt
bash: /mnt/project/file.txt
```

That means command execution needs path translation at the tool boundary. A bash
request whose cwd is the approved host path may continue to work under projection,
but under reification the host must map it to the declared namespace path when
possible. If a path cannot be mapped into the reified view, the operation must be
rejected or fall back through explicit policy, not silently run against ambient
host paths.

### D4. Build the runner as an unprivileged helper path

The runner should be implemented behind a narrow trait, for example:

```rust
trait ReifiedNamespaceRunner {
    fn run(&self, plan: ReifiedNamespacePlan) -> Result<ExecResult>;
}
```

The first concrete runner should use unprivileged Linux user and mount
namespaces when available. It may be an internal helper executable if UID/GID map
setup or fork/exec sequencing is cleaner outside `pre_exec`; it must not require
a setuid binary in the initial path.

If the helper cannot create the namespace, cannot bind required paths, or cannot
set up the network policy, selection must fall back to Landlock/path-guard
according to existing safety rules.

### D5. Read isolation comes from absence, not parser heuristics

Under the reified backend, read isolation is enforced because unmounted host
paths do not exist in the subprocess filesystem view. The command-shape parser
may still reject suspicious or unsupported invocations as preflight, but it is
not the read isolation mechanism.

This is the key semantic upgrade over Landlock:

- declared workspace/mount paths are visible;
- required OS substrate is visible read-only;
- user home, secret stores, and arbitrary host paths are absent by default.

### D6. Network confinement remains required

Reification is about filesystem visibility. Network confinement still has to be
applied through seccomp, Landlock network rules, namespace configuration, or an
equivalent host mechanism before the backend is considered fully enforcing.

If the filesystem view can be reified but network cannot be confined for a
network-denied command, the backend must report degraded state and the policy
layer must route network-capable operations to a human, deny them, or fall back
to a backend with a network-confinement backstop. The autonomous reviewer is not
eligible to approve network-capable execution while network is unconfined.

### D7. Capability detection is explicit and auditable

Backend detection should report which requirements are available:

- Linux host;
- unprivileged user namespaces;
- mount namespace creation;
- bind mounts;
- read-only remount support;
- private `/tmp` or scratch mount support;
- network confinement support.

Audit output must say whether reification is active, unavailable, or degraded,
and why.

## Risks / Trade-offs

- **Container-runtime scope creep** -> Land capability detection and plan
  generation first; keep the concrete runner behind a trait.
- **Breaking normal shell tools by hiding too much OS substrate** -> Maintain a
  minimal execution substrate list and test common shell commands before making
  reification the preferred backend.
- **Linux distro variance** -> Treat missing user namespace or mount namespace
  support as expected fallback, not test failure.
- **Path confusion during migration** -> Make backend audit and tool results
  report whether paths are projected host paths or reified namespace paths.
- **Security mistakes in helper sequencing** -> Prefer a small helper surface,
  no setuid initial path, and tests that inspect the generated plan separately
  from privileged operations.

## Migration Plan

1. Add `linux_reified_namespace` as a detectable-but-not-selected backend state
   plus an auditable capability probe.
2. Add `ReifiedNamespacePlan` and pure plan tests for workspace seed mounts,
   additional host mounts, read-only mounts, virtual mount exclusion, cwd/path
   translation, and execution substrate.
3. Implement a Linux-only runner behind a trait and keep it opt-in or capability
   gated.
4. Add Linux-only smoke tests that verify:
   - `/mnt/project` is visible when declared;
   - arbitrary home paths are absent;
   - write access follows mount access;
   - network-denied commands cannot connect.
5. Make backend selection prefer reification over Landlock only after the smoke
   gates pass and degradation reporting is precise.

Rollback is straightforward while Landlock remains the fallback: disable
`linux_reified_namespace` selection and continue using projection.
