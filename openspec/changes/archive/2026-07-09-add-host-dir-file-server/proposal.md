## Why

The namespace-driven sandbox design has a projection seam, but Alan still cannot
mount a real host directory as an aP tree. P2 makes "mount a local directory"
real: a human/config declaration installs a host-backed file server into the
namespace and contributes the same host path to the OS sandbox manifest.

## What Changes

- Add a host-directory-backed aP `FileServer` (`HostDirFs`) that exposes a real
  host directory through normal walk/open/read/write/stat/create/remove/clunk
  operations.
- Add a composition-root mount declaration shape for host directories:
  `(namespace_path, host_path, access)`.
- Project each host declaration twice:
  - into the `alan-kernel` namespace as an aP mount with `Access`; and
  - into `SandboxSpec` so native subprocesses are writable only where the mount
    declaration allows host write authority.
- Treat the current workspace as the seed host mount. Sessions with only the
  workspace continue to behave like the single-root sandbox path.
- Keep mounts human/config-declared at landing. No agent-visible `mount` tool is
  added in this change.

## Capabilities

### New Capabilities

- `host-directory-mounts`: host-backed directories can be mounted into the Alan
  OS namespace by human/config declaration and exposed as ordinary aP file
  trees.

### Modified Capabilities

- `os-sandbox-enforcement`: non-seed host directory declarations with write
  access contribute writable roots to the active `SandboxSpec`; read-only host
  declarations are visible to aP file tools but do not grant native-subprocess
  write authority.

## Impact

- Affected crates: `alan-ap`/`alan-kernel` consumers, likely a new host-dir file
  server crate or module, `crates/alan` session composition, and
  `alan-agent-engine` call sites that build a workspace sandbox.
- No host-path provenance is added to `alan-kernel`; host provenance stays in the
  declaration list held by the composition root.
- No new dependency from `alan-agent-engine` to `alan-kernel`.
- This change carries forward ADR-0027 D3 and
  `define-namespace-driven-sandbox` D4/D5/D6: the namespace is not a substitute
  for the OS sandbox, mounts are authorized outside the agent, and the workspace
  is the seed host mount.
