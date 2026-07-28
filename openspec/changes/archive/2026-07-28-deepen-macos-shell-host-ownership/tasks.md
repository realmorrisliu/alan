## 1. Establish The Behavior Baseline

- [x] 1.1 Begin after `complete-agent-runtime-file-native-seam` is merged,
  synced, and archived so only one refactor stack is active.
- [x] 1.2 Inventory duplicated controller selection, terminal runtime,
  active-task, projection queue, persistence scheduling, close workflow, and
  local control-command state against their accepted owners.
- [x] 1.3 Add or identify focused checks for selection publication, local/socket
  control parity, terminal lifecycle/publication, close behavior, persistence,
  and adapter normalization before moving ownership.

## 2. Unify Control Command Execution

- [x] 2.1 Route portable and state-mutating in-process control commands through
  the existing `AlanShellLocalCommandExecutor` and adopt its returned state,
  response, and side-effect intents.
- [x] 2.2 Keep terminal delivery, diagnostics, render metrics, and explicit
  platform effects in one small concrete host handler without a new protocol or
  factory.
- [x] 2.3 Delete `ShellHostControlCommandHandling.swift`, its duplicate command
  switch, and parity-only support that no longer protects an independent owner.
- [x] 2.4 Verify socket and in-process callers share command semantics while IPC
  transport and platform effects remain separate.

## 3. Remove Duplicate Shell Host State

- [x] 3.1 Make controller workspace and selection publications derive directly
  from the adopted `ShellStateSnapshot`; remove independently mutable selected
  Space/Tab fields and synchronization logic.
- [x] 3.2 Make terminal runtime, active-task, render, and lifecycle publications
  derive from `TerminalRuntimeRegistry`; remove duplicate controller maps and
  queued projection state.
- [x] 3.3 Keep manifest loading and write scheduling in
  `ShellWorkspacePersistenceCoordinator`; remove controller-owned persistence
  workflow state and forwarding.
- [x] 3.4 Add architecture checks that reject reintroduced duplicate state or a
  replacement global shell store.

## 4. Consolidate Workflows And Audit Adapters

- [x] 4.1 Move close confirmation and graceful shutdown into one concrete close
  workflow owner while keeping pane mutation in shell-core authority.
- [x] 4.2 Move terminal release/finalization, buffering, active-task state, and
  publication deduplication to `TerminalRuntimeRegistry` or the existing
  stateful reporter.
- [x] 4.3 Delete `TerminalContentLifecycleAdapter` after its active-mount
  projection and lifecycle calls move to shell state and the runtime registry.
- [x] 4.4 Verify and retain the projection, publication-policy, metadata,
  activity-normalization, stateful reporter, and C ABI adapters only at their
  real complexity boundaries.
- [x] 4.5 Remove displaced controller workflow extensions and shallow forwarding
  without creating one class per extension or a new adapter framework.

## 5. Verify And Deliver The Stack

- [x] 5.1 Run the affected focused Swift scripts,
  `just apple-shell-focused-tests`, shell-core and shell-core-ffi tests, shell
  contract validation, and Apple architecture-maintainability validation.
- [x] 5.2 Build and install a fresh Alan Dev app, relaunch it, and run rendered
  smoke for touched selection, control, terminal, close, and persistence
  surfaces.
- [x] 5.3 Lower `clients/apple/ARCHITECTURE.md` and the executable warning ledger
  in every slice that removes debt; reject new warnings, fallback paths, and
  replacement stubs.
- [x] 5.4 Deliver focused stacked PRs in dependency order; each PR must move one
  complete owner and delete the old path.
- [x] 5.5 For every PR, resolve all actionable Codex Review comments, rerun CI on
  the current HEAD, wait through a follow-up review window, and merge only when
  no unresolved or new issue remains.

## 6. Sync And Archive

- [x] 6.1 After all implementation PRs merge, sync the delta requirements into
  canonical `macos-app-architecture-maintainability` and run strict OpenSpec
  validation.
- [x] 6.2 Confirm UI, command protocol, persistence format, window interaction,
  and terminal behavior remain unchanged and no duplicate owner has returned.
- [x] 6.3 Archive the change only after implementation, rendered verification,
  review, and canonical spec sync are complete.
