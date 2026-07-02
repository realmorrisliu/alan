## 1. Request Contract

- [x] 1.1 Add `request_mount` to the built-in virtual tool definitions with a
  schema for `namespace_path`, `host_path`, `access`, and `reason`.
- [x] 1.2 Implement mount request parsing and validation for `/mnt/<name>`
  namespace paths, absolute host paths, supported access modes, and non-empty
  reasons.
- [x] 1.3 Cover valid and invalid mount request parsing with unit tests.

## 2. Approval Flow

- [x] 2.1 Add mount-escalation checkpoint constants and a policy rule/handler
  path that denies when policy denies but otherwise forces confirmation Yield.
- [x] 2.2 Implement `request_mount` virtual tool handling with ToolCallStarted,
  ToolCallCompleted, pending confirmation, and Yield emission.
- [x] 2.3 Cover valid request escalation, policy-denied requests, and invalid
  request behavior with focused runtime tests.

## 3. Resume And Audit

- [x] 3.1 Handle approved and rejected mount confirmations in `Op::Resume`.
- [x] 3.2 Record approved `host_mount_grant` events and return structured
  `request_mount` tool results without claiming live remount.
- [x] 3.3 Cover approve/reject resume behavior with tests.

## 4. Verification And PR

- [x] 4.1 Run focused Rust tests for mount request parsing, virtual-tool Yield,
  policy behavior, and resume handling.
- [x] 4.2 Run clippy for touched crates, OpenSpec strict validate, and diff
  checks.
- [x] 4.3 Update the parent namespace-driven sandbox task list to record this P3
  mount-escalation slice while leaving live reconfiguration and Linux
  reification pending.
- [x] 4.4 Commit the slice and open a ready stacked PR above
  `feat/northstar-sensitive-read-denylist`.
