## Context

The current P3 stack has three pieces:

- P2 added `HostDirFs` and `HostMountDeclaration` in the `alan` composition root.
  Human/config-declared host mounts can already be projected into both
  `Namespace` and `SandboxSpec` at session assembly.
- `add-agent-mount-escalation` added `request_mount`, approval, and
  `host_mount_grant` audit events, but approval originally did not apply the
  grant to any live enforcement surface.
- `apply-mount-grants-to-tool-sandbox` applies approved read-write grants to the
  runtime tool sandbox projection for later host-path tool calls. It deliberately
  leaves `namespace_applied = false`.

The remaining split is Alan OS namespace visibility. After a read-only grant,
the approved path should be reachable to aP file tools under `/mnt/<name>` even
though it does not expand native-subprocess writable roots. After a read-write
grant, both the namespace and tool sandbox projections should report their own
application state.

Today `NamespaceRuntimeEnvironment` holds an `InProcessTransport` over a
`MountFs` built from a static `Namespace`. `MountFs` owns the namespace table
privately. Live grant application therefore needs a narrow, kernel-native way to
mutate the mount table for future path walks without exposing host paths or
backing-server construction to Alan Kernel or `alan-agent-engine`.

## Goals / Non-Goals

**Goals:**

- Apply approved `request_mount` grants to the running Agent Process Alan OS
  namespace.
- Keep host-backed file-server construction in the `alan` composition root.
- Keep `alan-agent-engine` hosting-agnostic: it may request application of a
  mount grant, but it must not depend on `alan_hostfs`.
- Keep Alan Kernel host-path agnostic: live mutation takes a mountable
  `InProcessTransport` and `Access`, not a host path.
- Report namespace projection independently from tool sandbox projection.
- Mount read-only grants into the namespace as read-only aP trees while keeping
  them out of `SandboxSpec.writable_roots`.

**Non-Goals:**

- Linux mount namespace reification or a unified host `/mnt` path space for
  native subprocesses.
- Persisting approved grants across session restart.
- Adding a general agent-controlled `mount` command. `request_mount` remains an
  approval-gated escalation surface.
- Teaching `alan-kernel` to remember host provenance.

## Decisions

### D1. Add a live process namespace mount handle in Alan Kernel

Alan Kernel needs a single live mount table handle for the running process
namespace, not only a mutable `MountFs` view. Standard runtime assembly already
passes namespace clones into `MountFs`, `ProcFs::for_spawner`, and process-table
state; mutating only the file-server view would let later `MountFs` walks see a
grant while `/proc/<pid>/namespace` reads and `/proc/clone` child spawns keep a
stale namespace snapshot.

The live handle exposes host-agnostic operations such as:

```rust
mount(at: &str, tree: InProcessTransport, access: Access)
replace_mount(at: &str, tree: InProcessTransport, access: Access)
describe()
snapshot()
generation()
```

Internally this can be an `Arc` around a lock-protected `Namespace`. `MountFs`
walks and synthetic directory listings read from the handle. `ProcFs` spawner
state and process namespace descriptions must also read or snapshot from the
same handle, so a mount grant approved after runtime assembly becomes visible to
future `/proc/<pid>/namespace` reads and child process namespaces. Existing fids
keep their already-resolved backing transport; new walks and newly cloned child
namespaces see the updated mount table. That matches process namespace behavior
well enough for this slice and avoids invalidating active file handles.

`MountFs::new(namespace)` should remain available for tests and static call
sites. It can wrap the namespace in the same live handle internally, while the
standard runtime path should construct one handle and share it with `MountFs`,
`ProcFs`, process records, and the host mount applicator.

Every successful mount or exact-path replacement must increment a monotonic
namespace generation. Metadata exposed from the live namespace must reflect that
generation: `MountFs` synthetic directory qids for mount-table-derived listings
such as `/mnt`, and `ProcFs` namespace information qids/content versions, must
change after the grant is applied. Implementations can either derive the qid
version directly from the live handle generation or bump the process-table
generation when the handle mutates, but cached clients must be able to observe
that namespace listings/descriptions changed.

### D2. Agent engine calls a host-provided mount grant applicator

`alan-agent-engine` already knows the approved request payload
`(namespace_path, host_path, access, reason)` because `request_mount` is an
agent-facing authorization request. It should not know how to turn that host path
into a file server.

Introduce a small runtime interface, owned by the engine but implemented by the
host composition layer:

```rust
trait MountGrantApplicator {
    fn apply_mount_grant(&self, grant: ApprovedMountGrant) -> Result<MountGrantApplication>;
}
```

The default is absent, in which case approval still records the grant and may
apply the tool sandbox projection, but `namespace_applied` remains `false` with
an explicit reason.

### D3. The `alan` composition root owns HostDirFs construction

The concrete applicator in `crates/alan` converts an approved grant into a
`HostMountDeclaration`, builds `HostDirFs::new(host_path, access)`, and calls the
live namespace handle with the resulting `InProcessTransport` and `Access`.

This preserves the P2 layering:

- `alan-kernel` receives only a mounted file server and access mode.
- `alan-agent-engine` receives only the application outcome.
- host path canonicalization and `HostDirFs` safety stay with the host-facing
  file-server crate and composition helper.

### D4. Replace exact namespace paths for idempotence

A repeated approval for the same `namespace_path` should not accumulate duplicate
mount entries. The live applicator should replace the exact mount path for future
walks before mounting the approved file server. This gives a simple rule:

- the latest approved grant at a namespace path wins for future walks;
- already-open fids keep their previous resolved backing server; and
- rejected requests never change the namespace.

This is intentionally path-scoped. It does not unmount descendants or sibling
mounts.

### D5. Read-only and read-write grants share namespace projection

Read-only host grants are meaningful for Alan OS: aP file tools should be able
to walk and read the requested `/mnt/<name>` tree, and `MountFs` should reject
mutating operations through `Access::ReadOnly`.

Only read-write grants expand `SandboxSpec.writable_roots`. The result can
therefore be:

```json
{
  "namespace_applied": true,
  "tool_sandbox_applied": false
}
```

for a successful read-only grant.

### D6. Report partial application honestly

Namespace application and tool sandbox projection remain independent result
fields. If namespace application fails, the tool result and `host_mount_grant`
event should include `namespace_applied = false` and a concise
`namespace_error`, while preserving the approval record. If the tool sandbox was
already updated, it should continue to report its actual state.

The result must never imply Linux reification or native subprocess visibility at
`/mnt/<name>`.

## Risks / Trade-offs

- **Live mount mutation can race with active reads** -> Existing fids keep their
  resolved backing server; only future walks see replacements.
- **Host path construction failure after approval** -> Keep the approval audit
  event, report `namespace_applied = false`, and do not hide partial state.
- **Layering pressure toward engine->hostfs dependency** -> Use the applicator
  trait boundary; do not import `alan_hostfs` from `alan-agent-engine`.
- **Path replacement can shadow an existing mount** -> This is an approved
  authorization act. Exact-path replacement is simpler and safer than stacking
  duplicates.
- **Namespace and native subprocess paths still differ** -> Keep the result
  fields explicit; reification remains a later Linux-only change.

## Migration Plan

1. Add the host-agnostic live process namespace handle in Alan Kernel and keep
   `MountFs::new(namespace)` compatibility.
2. Add the engine-level approved mount grant applicator interface and thread it
   through namespace runtime state.
3. Implement the concrete `alan` applicator using `HostMountDeclaration` /
   `HostDirFs` and the live process namespace handle shared by `MountFs` and
   `ProcFs`.
4. Update `request_mount` resume to call the applicator and report
   `namespace_applied`, `namespace_error`, and existing tool sandbox fields.
5. Add tests for read-write apply, read-only apply, duplicate replacement,
   rejected no-op, missing applicator, and failed application reporting.

Rollback is local: leave the applicator unset and the runtime reverts to the
current behavior of recording grants plus sandbox projection while reporting
`namespace_applied = false`.
