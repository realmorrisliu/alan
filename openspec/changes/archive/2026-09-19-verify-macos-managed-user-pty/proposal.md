## Why

The signed privileged helper is healthy, but end-to-end Managed User ownership and PTY
verification requires an operator-selected Alan-managed account and was explicitly deferred.
Track that manual verification independently so the legacy-persistence hard cut can remain
truthful and its final runtime evidence is not lost during archival.

## What Changes

- Record an operator-run verification of current helper-owned Managed User readiness and a real
  PTY launch using the existing signed Alan Dev build.
- Keep account selection, provisioning authorization, and any local account mutation under the
  operator's control; do not auto-adopt or modify an existing unmarked account.
- Capture sanitized pass/fail evidence and any narrowly scoped follow-up without restoring
  retired host-service, sudoers, migration, or compatibility behavior.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `macos-shell-build-test-contract`: Require deferred live helper/account verification to remain
  explicitly tracked without claiming success or auto-adopting an unmarked local account.

## Impact

This is an operator-run verification change. It may exercise Alan Dev, the signed privileged
helper, current ownership markers, Managed User diagnosis, and the existing live PTY smoke
script. It does not authorize product code changes or automatic local-account adoption.
