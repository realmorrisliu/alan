## Why

Alan for macOS currently owns substantial shell workspace logic in Swift files
that are not UI-only: workspace state mutations, manifest materialization,
action availability, control-plane command reduction, Terminal Profile
resolution, and settings domain summaries. This makes a future Linux GTK client
likely to reimplement the same behavior unless the reusable shell workspace
domain is moved into a platform-neutral Rust core with strong contract tests.

## What Changes

- Introduce a platform-neutral Rust shell workspace core that owns reusable
  Spaces, Tabs, PaneSlot, ContentInstance, split tree, mutation, manifest,
  action, control-command, and Terminal Profile domain logic.
- Keep macOS SwiftUI/AppKit, Ghostty runtime hosting, windowing, file pickers,
  clipboard, IPC transport, diagnostics presentation, and privileged macOS
  account apply paths in the Apple client platform layer.
- Define a coarse-grained, versioned cross-language facade for Swift integration
  after Rust contract coverage is established. The first facade uses stable request/response
  envelopes and must not expose async callbacks, foreign traits, or long-lived
  Rust workspace objects.
- Add focused Rust contract tests and FFI-backed Swift adapter tests before
  replacing each Swift logic module.
- Replace Swift shell workspace logic module by module only after the equivalent
  Rust module passes contract and adapter tests.
- Require architecture warning debt to decrease as Swift logic is replaced,
  preventing new pure workspace domain logic from accumulating in large Swift
  shell model/controller files.

## Capabilities

### New Capabilities

- `shell-workspace-core-contract`: Defines the platform-neutral Rust shell
  workspace core ownership, module boundaries, reducer/manifest/action/control
  contracts, platform adapter responsibilities, binding facade rules, and
  Rust contract and adapter-test requirements.

### Modified Capabilities

- `macos-app-architecture-maintainability`: Clarify that reusable shell
  workspace domain logic migrates out of Swift into the Rust shell core, and
  Apple architecture warning debt must shrink as replacements land.
- `macos-shell-workspace-persistence`: Delegate portable manifest semantics to
  the shell workspace core while keeping macOS manifest file IO in the platform
  layer.
- `macos-shell-control-plane-reliability`: Separate control-plane transport and
  runtime side effects from command validation, stable error codes, and
  authoritative reducer results owned by the shell core.
- `macos-shell-action-registry`: Move shared action IDs, target resolution,
  availability, and effect mapping into the platform-neutral shell core while
  keeping macOS menu, context menu, and keyboard presentation in Swift.
- `macos-terminal-profiles`: Move Terminal Profile document validation,
  resolution order, and launch-intent construction into shell core while keeping
  platform process spawning and privileged account application outside it.
- `macos-shell-build-test-contract`: Add Rust core, binding
  facade, and Swift replacement validation expectations.

## Impact

- Affected Rust workspace:
  - New `crates/shell-core` pure Rust crate.
  - Optional dedicated binding/facade crate or module for Swift integration.
  - Workspace `Cargo.toml`, focused Rust unit tests, contract tests, and
    validation commands.
- Affected Apple client code:
  - `clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift`
  - `clients/apple/alan-macos/Services/Shell/ShellLocalCommandExecutor.swift`
  - `clients/apple/alan-macos/ShellHostController.swift`
  - Apple script tests under `clients/apple/scripts/`
- Future Linux GTK code should depend on the same Rust shell workspace core
  through a platform adapter instead of reimplementing shell domain behavior.
- No daemon API breaking change is intended. Existing shell control-plane JSON
  shapes and stable error codes should be preserved unless a later spec
  explicitly changes them.
