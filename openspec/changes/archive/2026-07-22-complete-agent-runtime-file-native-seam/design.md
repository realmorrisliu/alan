## Context

ADR-0008, ADR-0009, ADR-0021, ADR-0024, ADR-0025, ADR-0034, ADR-0035, and
ADR-0050 already establish the target: Agent Runtime Service starts Agent
Processes, Process creation is `/proc/clone`, authority is descriptor-passed,
and Host Mount Service owns Host file grants. The current runtime still carries
three transitional inversions:

- `alan-agent-engine` depends normally on `alan-kernel` and owns
  Kernel-shaped launch and Tool Process context.
- child Agent creation enters Agent Runtime Service through engine-owned
  assembly and lifecycle callbacks instead of `/bin/alan-agent`.
- `request_mount` accepts a raw Host path and propagates it through Agent
  Machine, AgentFS, Tool results, rollout evidence, namespace applicators, and
  Tool sandbox roots.

This change starts after `deepen-agent-machine-transition-module` so engine
transition ownership is stable before system composition is removed from it.

## Goals / Non-Goals

**Goals:**

- Make Agent Runtime Service the concrete Agent Executable implementation.
- Make Host Mount Service the sole request, grant, projection, revocation,
  status, and audit authority.
- Keep raw Host OS paths exclusively inside Host adapters.
- Pass Host Mount authority through real file-server handles, mounted trees, or
  descriptors rather than records or IDs.
- Remove Kernel-shaped engine collaborators and the normal engine-to-Kernel
  dependency.
- Delete every displaced compatibility path in the same focused slice that
  moves its owner.

**Non-Goals:**

- Redesign Kernel Process semantics, `/proc/clone`, aP, the Standard Namespace,
  or AgentFS generally.
- Introduce a second Agent Process type or a separate child-agent spawn API.
- Preserve legacy `host_path`, flat Host Mount request files, aggregate Host
  Mount inheritance, or callback-based child assembly.
- Refactor unrelated `SpawnHandle` variants, provider adapters, Tool behavior,
  or Alan for macOS attachment design.
- Claim hard multi-process isolation beyond the enforcement state recorded by
  ADR-0024.

## Confirmed Live Seam Inventory

The implementation baseline after `deepen-agent-machine-transition-module` contains these complete
replacement seams:

- raw Host backing and launch identity: `HostMountGrant` and `ProcessLaunchContext` in
  `crates/agent-engine/src/process_launch.rs`;
- runtime projection callbacks: `ApprovedMountGrant`, `MountGrantApplicator`, and
  `MountGrantApplicatorFactory` in the engine namespace environment, with the Service Manager
  implementation in `host_mount.rs`;
- aggregate inheritance: `SpawnHandle::HostMounts` plus child launch-context and delegated-Skill
  inference in the engine runtime;
- child assembly and cleanup callbacks: `ChildAgentProcessAssembler` and `AgentProcessLifecycle`
  in the engine, implemented by Agent Runtime Service;
- Kernel-shaped Tool and child launch state across engine runtime, Tool execution, and sandbox
  modules, backed by the normal `alan-kernel` dependency in `crates/agent-engine/Cargo.toml`.

The pre-replacement characterization rail covers logical request Yield/resume and recovery, live
read-only/read-write namespace projection, Tool sandbox reconciliation, explicit child projection,
grant-ID non-authority, revocation, and Process lifecycle cleanup. Each implementation slice keeps
the unaffected characterization tests while replacing and deleting its listed seam.

## Decisions

### Decision: Host Mount requests contain intent, never Host location

`request_mount` accepts a normalized `/mnt/<name>` path, access, non-empty
reason, and optional human label. It creates a Host Mount Service request and
Yields while that request is pending. The Host adapter presents the native
chooser or authorization surface, selects the Host directory, and returns a
hostfs export to Host Mount Service. Agent Execution Engine, AgentFS, Tool
results, rollout/checkpoint evidence, and Alan OS-visible audit records never
receive the raw Host OS path.

Agent policy may reject a request before native authorization, but it cannot
approve one. AgentFS may expose the opaque request reference and waiting state
needed by Agent Machine; writing an AgentFS decision cannot bypass Host Mount
Service or the Host adapter.

Alternative considered: keep `host_path` but redact it from selected evidence.
Rejected because the engine would still own Host location and every missed
projection would remain a disclosure and authority leak.

### Decision: Host Mount Service exposes one clone-based request protocol

The service is mounted at `/mnt/host-mount` with this minimal tree:

```text
/mnt/host-mount/
├── requests/
│   ├── clone
│   ├── events
│   └── <id>/
│       ├── request
│       ├── status
│       ├── grant
│       └── error
├── grants/
│   └── <id>/...
└── events
```

Opening `requests/clone` allocates a fid-private request; writing the logical
request document and clunking commits it. Status is one of `pending`,
`approved`, `rejected`, `cancelled`, or `failed`. `grant` is populated only for
approval, `error` only for terminal failure/rejection detail, and the event
streams support offset-based waiting. The request reference is stable enough to
resume after a Yield or runtime restart.

The old flat `request`, `projection`, and approval files are removed. There is
no dual protocol or adapter translating old requests to new ones.

Alternative considered: add the request tree beside the flat protocol.
Rejected because two writable approval surfaces would create two authorities.

### Decision: A grant ID is metadata; a handle is authority

Host Mount Service retains the native backing and issues an opaque mountable
file-server handle. A Process gains access only when that handle or its mounted
tree is explicitly delegated into its namespace. Alan OS-visible grant records
may carry the grant ID, label, access, provenance, and namespace path, but the
ID cannot resolve ambient authority.

The engine-owned raw `HostMountGrant` backing and aggregate
`SpawnHandle::HostMounts` are deleted. Child launch explicitly lists selected
grant handles and target namespace paths; the default is none. The spawner must
already possess each delegated handle and cannot amplify access. A cwd below a
Host Mount does not imply inheritance.

Alternative considered: pass grant IDs and look them up globally in the child.
Rejected because a globally resolvable ID bypasses the Process namespace
capability boundary.

### Decision: Tool sandbox authority derives from the same grant

When Alan OS starts a native Tool Process, the Host adapter maps the explicitly
delegated Host Mount handles to native sandbox rights. Read-write grants may add
native write authority; read-only grants do not. Agent Execution Engine sees
only the Alan OS namespace and spawn intent and never stores or updates raw
native writable roots.

Alternative considered: keep an engine-owned list of native paths synchronized
with namespace mounts. Rejected because it is a second authority ledger with a
different lifetime.

### Decision: Agent Runtime Service implements `/bin/alan-agent`

A child Agent Process is an ordinary `/proc/clone` execution of
`/bin/alan-agent`. The parent supplies the `SpawnSpec`, namespace capabilities,
descriptors, and optional explicit Host Mount handles. Agent Runtime Service
implements the Agent Executable by binding AgentFS, selecting the mounted
connection, starting Agent Machine, and cleaning up runtime state when the
Process exits.

`/proc/<pid>` remains lifecycle truth. `/agent/<pid>` and the parent's AgentFS
children projection provide the agent view; no `/agent/.../children/clone`
spawn protocol is introduced.

Alternative considered: retain `ChildAgentProcessAssembler` behind a narrower
trait. Rejected because the callback itself keeps Process assembly owned by the
engine.

### Decision: The engine stops depending on Kernel-shaped runtime types

`ProcessLaunchContext`, Tool Process native sandbox construction, child
assembly callbacks, lifecycle callbacks, and live Host Mount applicators leave
`alan-agent-engine`. The engine uses aP and agent-protocol file/spawn records for
runtime effects. Its normal `alan-kernel` dependency is removed and the
repository dependency ledger is tightened in the same PR; a dev-dependency is
allowed only for public contract tests.

Alternative considered: move existing types to a neutral crate without
changing their contents. Rejected because that would preserve raw Host backing
and lifecycle inversion under a new package name.

## Risks / Trade-offs

- [Native approval resumes the wrong request] → Bind every decision to the
  service request fid/reference and make terminal states immutable.
- [Opaque handles accidentally become global IDs] → Require delegation from a
  handle the spawner can already open; test that ID-only and unpassed grants
  confer no access.
- [Namespace and native sandbox authority diverge] → Derive both projections
  from the same service-owned grant and revoke them through the same owner.
- [Restart loses a pending Yield] → Persist the opaque request reference in
  Agent Machine evidence and resume by reading service status/events.
- [Breaking protocol leaves hidden callers] → Remove legacy fields and symbols,
  update all fixtures atomically, and add absence checks for retired surfaces.
- [Large cross-crate rewrite is hard to review] → Use focused stacked PRs that
  each move one complete owner and delete its old path before merge.

## Migration Plan

1. Merge and archive `deepen-agent-machine-transition-module`.
2. Replace `request_mount` end to end with the logical Host Mount Service
   request tree; update AgentFS/evidence and delete `host_path` plus the flat
   approval protocol in the same slice.
3. Replace raw Host Mount launch backing, live applicators, aggregate inheritance,
   and engine sandbox roots with explicit service-issued handles; tighten
   capability and revocation tests.
4. Route child Agent launch through `/proc/clone` and `/bin/alan-agent`; move
   AgentFS/Machine lifecycle ownership into Agent Runtime Service and delete the
   assembler/lifecycle callback path.
5. Remove remaining Kernel-shaped engine DTOs and the normal `alan-kernel`
   dependency; tighten the architecture dependency ledger and retired-symbol
   checks.
6. Run focused runtime/service tests, `just check`, `just test`, strict OpenSpec
   validation, and the dependency gate. Merge each PR only after CI and Codex
   Review remain clean on the current HEAD through a follow-up review window.

The repository is in early development, so migration is an atomic contract
replacement rather than a compatibility period. Rollback is by reverting the
complete focused PR; no old protocol remains active alongside the new one.

## Open Questions

None.
