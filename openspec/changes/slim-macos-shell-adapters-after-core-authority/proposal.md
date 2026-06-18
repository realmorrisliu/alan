## Why

`make-shell-core-authoritative` moves portable shell-domain authority into Rust,
but it necessarily leaves a large Swift adapter surface behind. The current PR
adds the core, FFI, fixture, and validation matrix first; the next step is a
separate behavior-preserving architecture slice that turns the now-authoritative
boundary into smaller, reviewable Swift owners.

## What Changes

- Split the coarse `ShellCoreFFIAdapter.swift` bridge into focused request,
  envelope, materialization, control, action, settings, manifest, and Terminal
  Profile adapter owners without changing the JSON FFI contract.
- Narrow `ShellHostController.swift` by moving shell-core-backed manifest
  startup, action dispatch, reducer command routing, persistence scheduling, and
  platform metadata preservation into named collaborators.
- Move fixture-only Swift shell-domain helpers out of production-facing files or
  behind explicit test-support boundaries once Rust core and FFI tests cover the
  same behavior.
- Convert the architecture-maintainability report from broad visible debt into
  staged burn-down gates with a documented baseline, target warning count, and
  per-slice verification requirements.
- Preserve macOS-owned behavior in Swift: SwiftUI/AppKit presentation, Ghostty
  runtime attachment, file IO, terminal delivery, diagnostics, and other OS
  effects stay out of Rust shell core.
- Sequence this after `make-shell-core-authoritative`; it must not reintroduce
  Swift shell-domain fallback logic while slimming files.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `macos-app-architecture-maintainability`: Adds measurable post-shell-core
  adapter/file-size burn-down requirements, slice boundaries, validation gates,
  and documentation expectations for reducing known large-file warnings.

## Impact

- Affected Apple client areas:
  - `clients/apple/ARCHITECTURE.md`
  - `clients/apple/scripts/check-architecture-maintainability.sh`
  - `clients/apple/alan-macos/Services/Shell/ShellCoreFFIAdapter.swift`
  - `clients/apple/alan-macos/ShellHostController.swift`
  - `clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift`
  - `clients/apple/alan-macos/Services/Shell/ShellLocalCommandExecutor.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift`
  - `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift`
  - `clients/apple/alan-macos/TerminalHostRuntime.swift`
- Affected validation:
  - Architecture maintainability report and any tightened warning thresholds
  - Existing shell contract checks guarding shell-core authority
  - Focused Swift scripts for shell-core adapter, workspace manifest, runtime
    metadata, action registry, settings surface, control-command seams, and
    terminal runtime/surface behavior
  - `openspec validate` for this change and repo-wide strict validation
