## 1. Confirm The Prerequisite And Live Seams

- [x] 1.1 Start from main after `deepen-agent-machine-transition-module` is
  merged, synced, and archived.
- [x] 1.2 Map every live `host_path`, raw `HostMountGrant`, aggregate
  `SpawnHandle::HostMounts`, mount applicator, Process launch context, child
  assembler/lifecycle callback, and normal engine-to-Kernel dependency use.
- [x] 1.3 Add focused characterization tests for mount request Yield/resume,
  live namespace projection, Tool sandbox authority, child Agent launch, and
  lifecycle cleanup before replacing the seams.

## 2. Replace The Host Mount Request Contract

- [x] 2.1 Implement the clone-via-open Host Mount Service request tree,
  commit-on-clunk validation, terminal status files, grant/error files, and
  offset-based event streams.
- [x] 2.2 Change `request_mount` to accept only namespace path, access, reason,
  and optional label; persist the opaque request reference across Yield/restart.
- [x] 2.3 Move native directory choice and authorization exclusively into the
  Host adapter and make Host Mount Service the only approval/status authority.
- [x] 2.4 Remove `host_path` from Agent Machine, AgentFS, Tool results,
  rollout/checkpoint records, audit records, protocol DTOs, fixtures, and tests.
- [x] 2.5 Delete the flat Host Mount request/projection/approval surfaces and add
  absence checks so no compatibility protocol or AgentFS approval bypass
  remains.

## 3. Replace Host Backing With Handles

- [x] 3.1 Make Host Mount Service retain native backing and issue the opaque
  mountable handle used for namespace projection and revocation.
- [x] 3.2 Replace engine-owned raw Host Mount launch records with explicit
  mounts/descriptors and prove a grant ID alone confers no access.
- [x] 3.3 Delete aggregate Host Mount inheritance; require each child launch to
  list selected grant handles and target namespace paths, defaulting to none
  without cwd-based inheritance.
- [x] 3.4 Move live namespace projection to Host Mount Service and delete engine
  namespace applicator traits, implementations, and callbacks.
- [x] 3.5 Derive native Tool Process sandbox rights from the same delegated grant
  inside Host adapters and delete engine-owned native writable-root mutation.
- [x] 3.6 Verify read-only/read-write behavior, non-amplification, idempotent
  projection, revocation, restart resume, and absence of Host paths in evidence.

## 4. Make Agent Runtime Service The Agent Executable

- [x] 4.1 Route root and child Agent Process launch through `/proc/clone` with
  `/bin/alan-agent`, explicit `SpawnSpec`, namespace capabilities, and
  descriptors.
- [x] 4.2 Make Agent Runtime Service bind AgentFS, select the mounted connection,
  start Agent Machine, and clean up runtime backing from `/proc` lifecycle.
- [x] 4.3 Preserve parent/child observation through `/proc/<pid>`,
  `/agent/<pid>`, and the existing AgentFS child projection without adding a
  second child-spawn protocol.
- [x] 4.4 Delete `ChildAgentProcessAssembler`, `AgentProcessLifecycle`,
  engine-owned `ProcessLaunchContext`, Kernel-shaped Tool Process DTOs, and all
  displaced assembly callbacks.
- [x] 4.5 Remove the normal `alan-agent-engine` dependency on `alan-kernel` and
  tighten the dependency ledger and retired-symbol checks in the same PR.

## 5. Verify And Deliver The Stack

- [x] 5.1 Run focused Agent Execution Engine, AgentFS, Kernel, Service Manager,
  Host Mount, Tool, and sandbox contract tests plus `just check`, `just test`,
  and strict OpenSpec validation.
- [x] 5.2 Deliver the logical-request, handle/projection, and Agent Executable
  ownership slices as stacked PRs; every PR must delete its displaced path and
  leave no scaffolding-only or dual-protocol state.
- [x] 5.3 For every PR, resolve all actionable Codex Review comments, rerun CI on
  the current HEAD, wait through a follow-up review window, and merge only when
  no unresolved or new issue remains.
- [x] 5.4 Verify the final dependency graph has no normal engine-to-Kernel edge,
  retired symbols and `host_path` contracts are absent, and ID-only or unpassed
  Host Mounts confer no authority.

## 6. Sync And Archive

- [x] 6.1 After every implementation PR is merged, sync all affected delta specs
  into their canonical capabilities and run strict OpenSpec validation.
- [x] 6.2 Confirm canonical specs and merged code expose only the logical request
  protocol, service-issued grant handles, and `/bin/alan-agent` launch path.
- [x] 6.3 Archive the change only after implementation, review, verification,
  and canonical spec sync are complete.
