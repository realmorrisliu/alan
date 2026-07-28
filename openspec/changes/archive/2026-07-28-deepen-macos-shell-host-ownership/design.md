## Context

Rust shell core already owns portable workspace mutation, and the Apple client
already contains concrete owners for terminal runtime, persistence, actions,
and pane projection. `ShellHostController` nevertheless carries duplicated
selection and runtime state across eleven source extensions, while
`ShellHostControlCommandHandling` independently implements much of the command
behavior already owned by `AlanShellLocalCommandExecutor`.

This change completes the residual ownership move without changing the macOS
product. It is independent of the runtime changes but is scheduled third so the
repository carries one active refactor stack.

## Goals / Non-Goals

**Goals:**

- Leave one mutable authority for each shell, terminal, and persistence state.
- Route portable control commands through one existing executor.
- Keep `ShellHostController` as a small observable composition/presentation
  owner.
- Consolidate close, runtime publication, active-task, and persistence workflows
  into their concrete owners.
- Delete shallow adapters while retaining modules that hide real policy,
  normalization, state, FFI, or platform complexity.
- Reduce and ratchet Apple architecture debt with focused verification.

**Non-Goals:**

- Change UI, command protocol, persistence format, window interaction, terminal
  semantics, or Alan for macOS attachment design.
- Create a second global shell model, a generic service layer, or protocol and
  factory pairs for concrete collaborators.
- Eliminate every Swift extension or small adapter based on size alone.
- Move platform-only UI state or AppKit effects into Rust shell core.

## Decisions

### Decision: Each mutable fact has one durable owner

| State or workflow | Owner |
| --- | --- |
| Portable workspace, selection, and mutation truth | `ShellStateSnapshot` from `alan-shell-core` |
| Terminal runtime, active task, render, and lifecycle truth | `TerminalRuntimeRegistry` |
| Manifest writes and persistence scheduling | `ShellWorkspacePersistenceCoordinator` |
| Portable/state-mutating control execution | `AlanShellLocalCommandExecutor` |
| Published projections, transient view state, and explicit platform effects | `ShellHostController` |

The controller may publish derived values required by SwiftUI, but those values
are projections rather than independently mutable authority. Duplicated
`selectedSpaceID`, `selectedTabID`, terminal runtime maps, active-task maps, and
projection scheduling queues are removed; there is no synchronization method
that reconciles two truths.

Alternative considered: introduce a new `ShellModel` store above all existing
owners. Rejected because it would create another authority and a migration
layer rather than remove duplication.

### Decision: Local and socket control share the existing executor

Portable and state-mutating control commands flow through
`AlanShellLocalCommandExecutor`, which already owns core invocation, state
mutation, response construction, and explicit side-effect intents for the
socket path. The in-process controller adopts the same result and applies only
the returned platform effects.

Host-only terminal delivery, diagnostics, and render metrics remain in one
small concrete handler. `ShellHostControlCommandHandling.swift` and its
duplicated switch are deleted; no replacement protocol, router hierarchy, or
factory is introduced.

Alternative considered: keep both switches synchronized with parity tests.
Rejected because tests cannot turn duplicated mutation logic into one owner.

### Decision: Stateful workflows leave controller extensions

Close confirmation and graceful shutdown become one concrete close workflow
owner. Terminal runtime buffering, active-task state, release/finalization, and
publication deduplication live in `TerminalRuntimeRegistry` or the existing
stateful reporter. Persistence loading/writes remain in
`ShellWorkspacePersistenceCoordinator`. Pane mutation remains shell-core state,
not part of the close workflow.

Small stateless view-facing computations may remain as controller extensions
when they make the presentation code clearer; extension count is not a target.

Alternative considered: create one class per existing extension. Rejected
because file movement without state ownership would increase indirection.

### Decision: Adapter existence is justified by hidden complexity

An adapter remains only when it isolates an FFI/platform boundary, mapping or
normalization policy, stateful deduplication, failure translation, or a
substantial projection algorithm. Small size alone is neither a reason to keep
nor delete it.

`TerminalContentLifecycleAdapter` is removed because it only forwards active
mount calculation and registry release/finalization. The projection comes from
shell state and the lifecycle operation belongs directly to
`TerminalRuntimeRegistry`. `TerminalContentProjectionAdapter`,
`TerminalRuntimePublicationPolicy`, `TerminalMetadataAdapter`,
`TerminalHostRuntimeReporter`, `TerminalAgentActivityAdapter`, and the C ABI
adapters remain because they hide real projection, policy, normalization,
state, or platform seams.

Alternative considered: ban every type named `Adapter`. Rejected because it
would inline genuine platform complexity into the controller.

### Decision: Every slice preserves rendered behavior

Each stacked PR moves one complete owner, deletes the old path, updates the
Apple architecture ledger, and runs the focused source/contract tests. Touched
runtime or UI surfaces are verified in a freshly built and relaunched Alan Dev
app. Product changes require a separate OpenSpec change.

Alternative considered: combine cleanup with UI improvements. Rejected because
rendered differences would obscure ownership regressions.

## Risks / Trade-offs

- [SwiftUI publication changes despite equivalent state] → Characterize
  selection and runtime publication ordering and verify the fresh Alan Dev
  rendering after each affected slice.
- [Command paths diverge around platform effects] → Make side-effect intents
  explicit in the shared executor result and keep platform execution in one
  host handler.
- [A moved workflow becomes a new shallow service] → Move its state and
  lifecycle together, or leave the stateless helper in place.
- [Removing lifecycle forwarding leaks terminal runtimes] → Add focused registry
  release/finalization checks before deleting the adapter.
- [Architecture debt moves instead of shrinking] → Lower the executable ledger
  in the same PR and reject new warning classes or replacement stubs.

## Migration Plan

1. Characterize selection, control response, terminal publication, close, and
   persistence behavior with the existing focused tests.
2. Route the controller control path through
   `AlanShellLocalCommandExecutor`, keep one host-only effect handler, and
   delete `ShellHostControlCommandHandling.swift`.
3. Remove duplicated controller selection/runtime state and make published
   values direct projections of `ShellStateSnapshot` and
   `TerminalRuntimeRegistry`.
4. Move close, terminal lifecycle/publication, active-task, and persistence
   workflows to their concrete owners; delete displaced controller extensions
   and `TerminalContentLifecycleAdapter`.
5. Run focused Swift scripts, shell-core and shell-core-ffi tests, Apple
   architecture validation, a fresh Alan Dev build/relaunch, and rendered smoke.
6. Merge each PR only after CI and Codex Review remain clean on the current HEAD
   through a follow-up review window; archive only after all tasks are complete.

Rollback is per behavior-preserving PR. There is no persisted format migration,
so reverting a slice restores its previous coordinator without data conversion.

## Open Questions

None.
