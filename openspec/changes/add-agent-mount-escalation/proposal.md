## Why

`define-namespace-driven-sandbox` D5 says a mount is an authorization act: an
agent must not be able to expand its own host filesystem access without an
external approval path. P2 added human/config-declared host mounts, but agents
still have no explicit way to request a new host-path grant.

This change adds the first P3 mount-escalation slice: agents can request a host
directory mount through the runtime's existing Policy/Yield machinery, and an
approved request is recorded as a mount grant for a follow-up composition-root
reconfiguration slice.

## What Changes

- Add a built-in virtual `request_mount` tool exposed to Agent Processes.
- Validate mount requests before escalation: namespace path, host path, access
  mode, and reason must be well-formed.
- Route every valid mount request through confirmation/Yield; policy may deny a
  request, but it may not silently auto-allow one.
- On approval or rejection, return a structured `request_mount` tool result to
  the agent.
- On approval, record a normalized `host_mount_grant` audit event containing the
  namespace path, host path, access mode, and approval metadata.
- Keep live namespace/sandbox reconfiguration out of this slice. The runtime
  currently owns a static `MountFs` root, so applying approved grants to the
  running namespace and `SandboxSpec` remains a separate follow-up slice.

## Capabilities

### New Capabilities

- `agent-mount-escalation`: Agent Processes can request host directory mounts
  through an approval-gated runtime surface, producing auditable mount grants.

### Modified Capabilities

None.

## Impact

- `crates/agent-engine/src/runtime/virtual_tools.rs`
- `crates/agent-engine/src/runtime/submission_handlers.rs`
- `crates/agent-engine/src/approval.rs`
- `crates/agent-engine/src/policy.rs`
- OpenSpec namespace/sandbox framing task state
