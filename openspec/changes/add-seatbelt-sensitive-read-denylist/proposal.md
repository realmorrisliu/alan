## Why

P1 and P2 now route writable roots and network posture through `SandboxSpec`, but
native subprocess reads remain broad. That leaves common local secret stores
readable on macOS even though Seatbelt can deny those paths directly.

This is the next P3 hardening slice from
`define-namespace-driven-sandbox`: land sensitive-read isolation on macOS while
keeping the Linux story honest until namespace reification can provide full read
isolation.

## What Changes

- Add a default sensitive-read denylist to `SandboxSpec` seed creation.
- Include Alan home secret/config paths, common cloud/dev credential stores,
  macOS keychains, and browser profile directories in that denylist.
- Ensure the `alan` composition-root host-mount projection keeps that same
  denylist when it adds writable host mounts.
- Keep Linux/Landlock behavior explicit: the denylist is carried in
  `SandboxSpec`, but only macOS Seatbelt enforces broad-read-minus-denylist
  semantics today.
- Add tests and spec coverage for the macOS read-deny behavior and the host
  mount projection path.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `os-sandbox-enforcement`: macOS Seatbelt sandboxing gains sensitive-read
  denylist enforcement for default sandbox specs, while Linux remains
  write+network confined without read-deny enforcement.

## Impact

- `crates/agent-engine/src/tools/sandbox.rs`
- `crates/agent-engine/src/tools/sandbox_backend.rs`
- `crates/alan/src/host_mounts.rs`
- OpenSpec sandbox enforcement requirements and tests
