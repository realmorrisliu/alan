## Why

Alan for macOS has established shell-core, terminal-runtime, persistence, and
projection owners, but `ShellHostController` still duplicates their state and
command paths across large extensions. This residual coordination debt makes
selection, terminal lifecycle, persistence, and control behavior harder to
reason about than the underlying architecture requires.

## What Changes

- Keep `ShellHostController` as the observable renderer-facing composition
  owner, but remove duplicated workspace selection, terminal runtime, active
  task, projection queue, and persistence workflow state.
- Make `ShellStateSnapshot` the single portable workspace and selection truth,
  `TerminalRuntimeRegistry` the terminal runtime/lifecycle truth, and
  `ShellWorkspacePersistenceCoordinator` the persistence scheduling owner.
- Route portable and state-mutating control commands through the existing
  `AlanShellLocalCommandExecutor`; retain only a small host-only handler for
  terminal, diagnostics, render metrics, and explicit platform effects.
- Consolidate close confirmation, graceful shutdown, runtime publication, and
  activity deduplication into their existing concrete workflow owners.
- Delete shallow pass-through adapters, beginning with
  `TerminalContentLifecycleAdapter`, while retaining small adapters that isolate
  real projection policy, normalization, stateful reporting, FFI, or platform
  complexity.
- Preserve UI, command protocol, persistence format, window interaction, and
  terminal behavior.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `macos-app-architecture-maintainability`: Make the residual shell-host state,
  command, terminal lifecycle, persistence, and adapter ownership boundaries
  explicit and enforce their behavior-preserving deletion path.

## Impact

This behavior-preserving refactor affects `ShellHostController` and its
extensions, `AlanShellLocalCommandExecutor`, `TerminalRuntimeRegistry`, shell
workspace persistence/projection collaborators, focused Apple validation, and
`clients/apple/ARCHITECTURE.md`. It is technically independent of the runtime
changes but is scheduled after them to keep one active refactor stack.
