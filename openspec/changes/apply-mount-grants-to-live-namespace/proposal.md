## Why

`apply-mount-grants-to-tool-sandbox` makes approved read-write mount grants usable
by later host-path tools, but Alan OS `/mnt/<name>` still remains a recorded
grant rather than a live namespace mount. That leaves a split: `bash` can use the
approved host path through the projected sandbox, while aP file tools still
cannot reach the approved directory through the requested namespace path.

This change closes that projection gap by applying approved mount grants to the
running Agent Process namespace through a host-owned composition hook, without
coupling `alan-agent-engine` to `alan_hostfs` or teaching Alan Kernel about host
paths.

## What Changes

- Add a runtime-facing namespace mount applicator boundary that can apply an
  approved host mount grant to the current Agent Process namespace, including
  the process namespace state used by `/proc/<pid>/namespace` and child spawns.
- Keep host filesystem construction in the `alan` composition root:
  approved grants become `HostMountDeclaration` values there, not inside
  `alan-agent-engine`.
- Update `request_mount` approval resume so successful namespace application
  reports `namespace_applied = true` and records the same state in the
  `host_mount_grant` event.
- Preserve current explicit reporting for partial application:
  `tool_sandbox_applied` and `namespace_applied` remain independent fields.
- Keep read-only namespace grants live-applicable. Read-only grants do not expand
  writable `SandboxSpec` roots, but they can be mounted into Alan OS as read-only
  aP trees.
- Keep Linux mount namespace reification out of this slice.

## Capabilities

### New Capabilities

- `live-mount-grant-namespace-projection`: Approved host mount grants can be
  projected into the running Agent Process Alan OS namespace so aP file tools can
  reach them at their requested `/mnt/<name>` path.

### Modified Capabilities

- `agent-mount-escalation`: Approved `request_mount` results no longer
  unconditionally state that grants are not applied live. They now report the
  actual namespace application outcome with `namespace_applied` and a concise
  error/reason when live namespace application is unavailable or fails.

## Impact

- `crates/agent-engine/src/runtime/agent_loop/namespace_environment.rs`
- `crates/agent-engine/src/runtime/submission_handlers.rs`
- `crates/alan/src/host_mounts.rs`
- `crates/kernel/src/mountfs.rs` and `crates/kernel/src/procfs.rs`, or a narrow
  shared runtime wrapper if needed to expose safe live mutation of the process
  namespace mount table
- Tests for approved read-write apply, approved read-only namespace-only apply,
  duplicate/idempotent apply, rejected no-op, and failure reporting
- Parent OpenSpec task state in `define-namespace-driven-sandbox`
