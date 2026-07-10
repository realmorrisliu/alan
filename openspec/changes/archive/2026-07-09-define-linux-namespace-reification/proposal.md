## Why

The projection path now keeps Alan OS namespace grants and native-subprocess
sandbox writable roots in sync, but Linux still lacks full read isolation:
Landlock can confine writes and network, yet broad host reads remain visible to
native subprocesses. `define-namespace-driven-sandbox` records Linux reification
as the future path where a subprocess sees a real, reduced filesystem view
instead of the ambient host filesystem.

This change defines the Linux-only reification contract and implementation
sequence before building a container-runtime-class backend.

## What Changes

- Define a Linux reified namespace backend for native subprocess execution:
  materialize a per-process filesystem view from the mount declaration list.
- Reify host-backed Alan OS mounts as bind mounts in the subprocess view, so a
  mounted `/mnt/project` path is the path the subprocess sees.
- Preserve virtual Alan OS mounts as non-native: `/agent`, `/proc`, `/srv`,
  `/mnt/llm`, and other pure aP file servers are not exposed as host filesystem
  paths unless a later bridge explicitly implements one.
- Add explicit capability detection and safe degradation:
  if user namespaces, mount namespaces, bind mounts, or required helper support
  are unavailable, Linux continues using the existing Landlock path and reports
  that full read isolation is unavailable.
- Keep macOS on Seatbelt projection. macOS reification is a non-goal.
- Sequence implementation as incremental slices: capability probe and planning
  model first, then a minimal runner, then enforcement hardening and verification.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `os-sandbox-enforcement`: Linux may select a reified namespace backend that
  gives native subprocesses a reduced filesystem view with deny-by-default host
  reads, while preserving safe fallback to Landlock/path-guard behavior when
  reification is unavailable.

## Impact

- `crates/agent-engine/src/tools/sandbox_backend.rs`
- `crates/agent-engine/src/tools/sandbox.rs`
- Linux-specific subprocess launch path for `bash`
- A new Linux reification planner/runner module or crate if the implementation
  needs privileged helper isolation
- Tests that model reified mount plans without requiring Linux namespaces, plus
  Linux-only smoke tests guarded by capability detection
- Parent OpenSpec task state in `define-namespace-driven-sandbox`
