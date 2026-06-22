## Why

`introduce-cross-platform-shell-core` proves that the macOS shell can call a
Rust shell core, but the current integration still leaves Swift fallback paths
and duplicate shell-domain implementations in place. To make the core useful for
future Linux work and keep macOS behavior from drifting, Rust must become the
authoritative owner for the portable shell domain while Swift narrows to a
platform adapter.

## What Changes

- Make Rust shell core the fail-closed authority for portable manifest,
  reducer, action registry, control-command, Terminal Profile, and reusable
  settings-domain behavior.
- Remove Swift runtime fallbacks that silently reimplement shell-core behavior
  after an FFI, schema, or reducer failure.
- Keep macOS-owned fallback behavior where it is truly platform-level:
  corrupt-file quarantine, default manifest file creation after quarantine,
  Ghostty/runtime recovery, window/UI presentation, pasteboard/keyboard
  delivery, and diagnostics presentation.
- Convert remaining Swift shell-domain implementations into one of three
  states: deleted, adapter-only projection code, or short-lived parity fixtures
  with an explicit removal task.
- Tighten validation scripts so new Swift domain fallbacks cannot be added to
  replaced areas.
- Sequence this after `introduce-cross-platform-shell-core`; it completes the
  authority cleanup rather than creating a second core.

## Capabilities

### New Capabilities

- `shell-core-authority-contract`: Defines the post-integration authority
  boundary: which shell-domain behavior must be owned by Rust core, which
  platform behavior remains in Swift, and how failures, tests, and validation
  prevent fallback drift.

### Modified Capabilities

- `macos-shell-workspace-persistence`: Manifest defaulting, migration, pruning,
  and materialization must be shell-core authoritative at runtime; Swift file IO
  and corrupt-file quarantine remain platform responsibilities.
- `macos-shell-control-plane-reliability`: Workspace-domain control commands
  must derive validation, stable errors, reducer results, and response
  projection from shell core, while Swift executes only platform side effects.
- `macos-shell-action-registry`: Shared action descriptors, shortcuts,
  availability, target resolution, and effect mapping must not fall back to a
  separate Swift registry implementation.
- `macos-terminal-profiles`: Terminal Profile validation and launch-intent
  resolution must be shell-core authoritative while macOS continues to own
  storage, process spawning, and privileged helper execution.
- `macos-app-architecture-maintainability`: Swift shell model/controller files
  must not retain duplicate reusable shell-domain implementations once a core
  authority path exists.
- `macos-shell-build-test-contract`: Validation must reject new Swift domain
  fallbacks for replaced core-owned behavior and keep adapter tests focused on
  envelope, decoding, error mapping, and platform side effects.

## Impact

- Affected Rust crates:
  - `crates/shell-core`
  - `crates/shell-core-ffi`
- Affected Apple client areas:
  - `clients/apple/alan-macos/Services/Shell/ShellCoreFFIAdapter.swift`
  - `clients/apple/alan-macos/ShellHostController.swift`
  - `clients/apple/alan-macos/Services/Shell/ShellLocalCommandExecutor.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift`
  - focused shell/action/profile/control-plane Swift scripts
- Affected validation:
  - Rust shell-core unit and fixture tests
  - shell-core FFI contract tests
  - Apple shell scripts for workspace manifest, split/reducer, action registry,
    settings surface, runtime metadata, and control-command seams
  - architecture and shell-contract guard scripts
