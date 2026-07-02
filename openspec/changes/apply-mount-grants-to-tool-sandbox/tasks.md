## 1. Tool Sandbox Projection

- [x] 1.1 Extend `ToolExecutionBinding` and `ToolContext` with an optional
  runtime `SandboxSpec` while preserving existing workspace-root behavior.
- [x] 1.2 Update `ToolContext::workspace_sandbox` to use the runtime
  `SandboxSpec` when present and fall back to `SandboxSpec::seed`.
- [x] 1.3 Teach `Sandbox` containment checks and error wording to honor every
  configured writable root, not only the first workspace root.
- [x] 1.4 Add unit tests for multi-root sandbox read/write/list/cwd containment
  and out-of-root rejection.

## 2. Mount Grant Application

- [x] 2.1 Add `ToolRegistry` helpers to inspect and idempotently extend the
  default binding's sandbox writable roots.
- [x] 2.2 Parse approved mount confirmation details into a reusable mount grant
  payload, including host path and access mode.
- [x] 2.3 On approved `request_mount` resume, add read-write host paths to the
  active tool sandbox before returning the tool result.
- [x] 2.4 Keep approved read-only grants audit-only and report that tool sandbox
  projection was not applied.
- [x] 2.5 Cover read-write apply, duplicate apply, read-only no-op, and reject
  no-op behavior with focused runtime tests.

## 3. Verification And PR

- [x] 3.1 Run focused Rust tests for sandbox multi-root behavior and mount grant
  resume projection.
- [x] 3.2 Run clippy for touched crates, OpenSpec strict validate, and diff
  checks.
- [x] 3.3 Update the parent namespace-driven sandbox task list to record this
  tool-sandbox projection slice while leaving Alan OS `/mnt` live remount and
  Linux reification pending.
- [x] 3.4 Commit the slice and open a ready stacked PR above
  `feat/northstar-agent-mount-escalation`.
