## Why

Managed Users currently depend on `osascript`-driven privilege prompts and
Alan-owned sudoers drop-ins. That path is slow, failure-prone, hard to reason
about, and leaves the final terminal-entry contract coupled to passwordless
`sudo` rather than a narrow Alan-owned system service.

This change defines the final macOS permission architecture: a signed privileged
helper owns system account repair and managed-user PTY spawning, while Alan and
Ghostty keep the terminal UI and renderer boundaries separate.

## What Changes

- Add a signed macOS privileged helper service with channel-scoped stable/dev
  identities, install/update/uninstall status, client code-signing validation,
  and a typed XPC API.
- Move Managed User account create, diagnose, repair, legacy cleanup, and
  helper-owned terminal spawning behind declarative helper operations.
- Replace Alan-managed sudoers as the Managed User runtime mechanism; do not add
  a sudoers fallback for helper-backed Managed Users.
- Make helper-backed Managed User terminals launch through an Alan-owned PTY
  provider that depends on `add-macos-alan-owned-pty-runtime` and its Ghostty
  external-PTY attachment work.
- Introduce a `managed_user` Terminal Profile launch identity for helper-backed
  managed accounts while preserving `sudo_user` for non-managed, operator-owned
  manual profiles.
- Add product states for helper installation, update, invalid signature,
  legacy sudoers cleanup, account repair, readiness, and helper-backed PTY
  spawn failures.

## Capabilities

### New Capabilities

- `macos-privileged-helper`: Defines the channel-scoped privileged helper
  contract, typed API surface, authorization model, forbidden capabilities,
  client validation, logging, and uninstall behavior.

### Modified Capabilities

- `macos-terminal-account-provisioning`: Replace sudoers-backed Managed User
  readiness with helper-backed account diagnosis, repair, legacy cleanup, and
  managed-user PTY smoke verification.
- `macos-terminal-profiles`: Add helper-backed `managed_user` launch identity
  for Alan-managed accounts while keeping `sudo_user` as a manual profile mode.
- `macos-terminal-runtime-foundation`: Add helper-owned PTY provider semantics
  as a dependency on the Alan-owned PTY runtime foundation.
- `macos-shell-terminal-lifecycle`: Add lifecycle/error semantics for
  helper-owned managed-user terminal sessions.
- `macos-shell-build-test-contract`: Add helper signing, channel isolation,
  fake-helper testing, integration smoke, and contract checks forbidding Managed
  User sudoers fallback.

## Impact

- Apple client Settings > Terminal helper status, Managed Users, Terminal
  Profile launch resolution, and terminal runtime launch paths.
- New privileged helper target, launchd service registration, XPC protocol,
  signing requirements, code-signing validation, and stable/dev channel labels.
- Managed User migration from Alan-owned sudoers drop-ins to helper-backed
  records and helper-owned PTY sessions.
- Focused tests for helper API validation, channel isolation, account planning,
  PTY lifecycle, legacy sudoers cleanup, and no-fallback enforcement.
