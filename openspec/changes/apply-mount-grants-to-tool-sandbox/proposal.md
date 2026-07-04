## Why

`add-agent-mount-escalation` records approved mount grants, but the running
tool sandbox still derives from the original single workspace seed. That means
an approved read-write host mount does not yet affect subsequent bash or
workspace-local tool execution.

This change applies approved read-write mount grants to the runtime's projected
tool sandbox so the authorization decision has an immediate, auditable execution
effect without pretending the Alan OS `/mnt` namespace has been live-remounted
yet.

## What Changes

- Extend tool execution binding/context state so a runtime can carry a mutable
  `SandboxSpec` instead of always deriving one from the workspace root.
- Teach the sandbox workspace/root checks to treat every configured writable
  root as an allowed execution root.
- When `request_mount` is approved with `access = read_write`, add the approved
  host path to the runtime tool sandbox's writable roots for subsequent tool
  calls.
- Keep read-only grants as audit-only for now because `SandboxSpec` currently
  has writable roots and a sensitive read denylist, not an explicit read-allow
  root set.
- Keep Alan OS namespace live remounts out of this slice; `/mnt/<name>` remains
  a recorded namespace grant until a host composition hook can mount `HostDirFs`
  into the running `MountFs`.

## Capabilities

### New Capabilities

- `mount-grant-tool-sandbox-projection`: Approved read-write mount grants are
  projected into the running Agent Process tool sandbox for subsequent
  workspace-local tool execution.

### Modified Capabilities

None.

## Impact

- `crates/agent-engine/src/tools/context.rs`
- `crates/agent-engine/src/tools/registry.rs`
- `crates/agent-engine/src/tools/sandbox.rs`
- `crates/agent-engine/src/runtime/submission_handlers.rs`
- OpenSpec namespace/sandbox framing task state
