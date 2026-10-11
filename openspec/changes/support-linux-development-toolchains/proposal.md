## Why

The terminal UI implementation is merged, but ordinary Linux development still
cannot qualify: the real fixture lacks git and the reified runner replaces user
PATH with a fixed system list, hiding installed Rust. Versions alone do not prove
that a confined command can build, test or inspect a project.

## What Changes

- Preserve the selected shell/git/Rust executables and supported PATH search order
  inside the existing Linux namespace backend, including required runtime files.
- Project only validated executable/runtime roots read-only; retain isolation,
  current Host Mount authority, network restrictions and safe degradation.
- Provide private writable Cargo/build scratch through the existing native-runner
  lifetime, without inheriting credentials or granting the user's whole home.
- Resolve external local dependencies through explicitly delegated project mounts;
  read additional live same-Process read-only grants while retaining only the cwd
  selected grant's writable authority. Refuse missing, escaping, revoked or
  retargeted backing without substitute tools or another writable cwd. This is a
  proposed ADR-0058 extension pending user review/merge.
- Freeze a reproducible Linux fixture with git and the repository's Rust 1.97,
  and verify real dependency builds/tests, git diff and negative authority cases.
- Keep wider real-model repetitions in a later independent change. Node/Python
  support requires an actual recorded task and is not added speculatively.

## Capabilities

### New Capabilities

None. Native tool execution already has a durable owner.

### Modified Capabilities

- `os-sandbox-enforcement`: selected Linux development tools, read-only runtime
  projection, private writable scratch, and truthful execution qualification.
- `host-mount-tool-process-sandbox-projection`: native diagnostic paths preserve
  namespace boundaries and conceal normalized private backing ancestors; same-Process
  explicit read-only dependencies with selected writable authority and current root
  revalidation.

## Impact

Existing Host Tool adapters and `SandboxSpec`, `reified_namespace` planning,
runner/helper setup, backend readiness, nearby tests and Linux fixture workflows.
Reuse current Process cancellation and Host Mount grant reconciliation. No new
Kernel dependency, execution manager, provider integration or automatic routing.
The post-merge UI/directory-context closure is separate; its implementation is
already in main. Personal stores and old installation snapshots are outside scope.
