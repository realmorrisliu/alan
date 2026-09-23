## Why

Alan for macOS has no remaining user and its desktop implementation is retired.
Keeping the App, workspace core and FFI makes unrelated CLI work carry dead code.

## What Changes

- **BREAKING**: remove tracked Apple App/helper/tests and shell-core/FFI source,
  workspace membership and unused build/test hooks.
- Retire their desktop-only requirements with explicit removal deltas.
- Preserve Rust CLI/Host credentials, Host Mounts, sandbox, stores and q Skill distribution.
- Cancel desktop GUI tasks and remove generic executable/binfs/WASM packaging
  from the current tracer-bullet delivery plan. Future packaging needs a real consumer.
- Preserve immutable archives, installed applications, user data, ignored build
  artifacts and the external Ghostty checkout; do not uninstall anything.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `macos-shell-terminal-lifecycle`: remove requirements whose sole implementation is the retired desktop.
- `shell-workspace-core-contract`: remove requirements whose sole implementation is the retired desktop.
- `macos-terminal-account-provisioning`: remove requirements whose sole implementation is the retired desktop.
- `macos-app-architecture-maintainability`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-keybinding-system`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-workspace-persistence`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-ui-ux-conformance`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-content-containers`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-automation-surfaces`: remove requirements whose sole implementation is the retired desktop.
- `macos-alan-os-attachment`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-control-plane-reliability`: remove requirements whose sole implementation is the retired desktop.
- `macos-app-instance-lifecycle`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-action-registry`: remove requirements whose sole implementation is the retired desktop.
- `macos-terminal-runtime-foundation`: remove requirements whose sole implementation is the retired desktop.
- `macos-privileged-helper`: remove requirements whose sole implementation is the retired desktop.
- `macos-terminal-profiles`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-build-test-contract`: remove requirements whose sole implementation is the retired desktop.
- `macos-terminal-surface-parity`: remove requirements whose sole implementation is the retired desktop.
- `macos-terminal-activity-semantics`: remove requirements whose sole implementation is the retired desktop.
- `shell-core-authority-contract`: remove requirements whose sole implementation is the retired desktop.
- `macos-shell-workspace-interactions`: remove requirements whose sole implementation is the retired desktop.
- `documentation-governance`: identify removed desktop contracts as historical.
- `repository-quality-gate`: enforce desktop-source absence without Swift/Xcode.

## Impact

Deletes clients/apple and crates/shell-core{,-ffi} tracked files. CLI/Host/TUI
manifests have no dependency on these crates. Cargo graph/lockfile, quality
scripts and current guides change; Alan OS runtime code and q remain unchanged.
This follows the archived distribution retirement; it does not edit that archive.
