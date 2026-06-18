## Context

`make-shell-core-authoritative` changes the ownership boundary: Rust shell core
is now the authority for portable shell-domain behavior, while Swift remains the
macOS platform adapter. That transition explains the large positive diff. It
adds `alan-shell-core`, `alan-shell-core-ffi`, parity fixtures, and Swift
adapter tests before removing every coarse Swift host file that became too
large during the migration.

The current local baseline after the authority work is:

- PR-scale diff bucket: `crates/shell-core` about +23,886 lines, Apple Swift
  production about +3,743/-1,232 lines, Apple scripts/tests about +6,349 lines.
- Architecture report: 17 non-blocking large-file / bridge-boundary warnings.
- Largest shell-core-adjacent Swift files:
  - `ShellHostController.swift`: 4,637 lines
  - `ShellCoreFFIAdapter.swift`: 2,416 lines
  - `ShellStateMutations.swift`: 2,368 lines
  - `ShellHostControlCommandHandling.swift`: 1,803 lines
  - `TerminalHostRuntime.swift`: 1,399 lines
  - `ShellSettingsSurfaceModel.swift`: 1,263 lines
  - `ShellWorkspaceManifest.swift`: 1,236 lines
  - `ShellActionRegistry.swift`: 1,175 lines

This change treats those numbers as architecture debt to burn down after the
core authority boundary is accepted. It must not re-open the authority question
or preserve duplicate Swift domain logic just to make moves easier.

## Goals / Non-Goals

**Goals:**

- Reduce the architecture warning count through behavior-preserving slices, not
  by relaxing the checker.
- Split `ShellCoreFFIAdapter.swift` into smaller operation-family owners while
  keeping the public facade and versioned JSON FFI contract stable.
- Narrow `ShellHostController.swift` to observable orchestration by extracting
  startup/manifest, persistence, shell-core reducer routing, action dispatch,
  and platform metadata preservation into named collaborators.
- Move fixture-only Swift shell-domain helpers out of normal production owners
  or behind explicit test-support gates.
- Update `clients/apple/ARCHITECTURE.md` and architecture checks with each
  resolved warning so reduced debt cannot silently return.
- Keep shell-core authority guards intact: no `try? ShellCoreFFIAdapter... ??
  SwiftDomainImplementation`, no runtime use of Swift parity registries, and no
  Swift fallback for core-owned domain behavior.

**Non-Goals:**

- Do not redesign Rust shell-core semantics or the current FFI envelope.
- Do not migrate AppKit, SwiftUI, Ghostty, PTY/process ownership, socket IO, or
  privileged macOS effects into Rust.
- Do not combine terminal UI visual polish or non-shell-core large-file cleanup
  into this change unless it directly enables a shell adapter split.
- Do not claim the full Apple client is small at the end of this change; the
  target is the shell-core adapter/controller debt introduced or exposed by the
  authority migration.

## Decisions

### Use warning burn-down as the completion signal

This change should not count a file move as progress unless the architecture
report or documented debt ledger becomes smaller or more precise. The initial
target is to reduce the report from 17 warnings to 12 or fewer while adding no
new warnings.

Alternative considered: split files opportunistically and leave the report
unchanged. That creates churn without answering the user's concern about when
large-file debt is actually done.

### Keep the public shell-core facade stable while splitting internals

`ShellCoreFFIAdapter` should remain the small public entry point used by call
sites, but its implementation should delegate to narrow files: dynamic library
loading, envelope send/receive, portable state materialization, manifest
operations, reducer operations, control commands, action registry, settings
summary, and Terminal Profile resolution.

Alternative considered: replace the hand-written bridge with UniFFI in this
slice. That is a binding strategy change and risks obscuring the simpler
ownership cleanup this spec needs to deliver.

### Extract behavior owners before deleting compatibility helpers

For `ShellHostController`, first move responsibilities into named services with
the same behavior and tests, then delete or shrink the old methods. Good owners
are narrow and platform-specific: manifest startup/persistence, shell action
dispatch, shell-core reducer command routing, runtime metadata preservation,
and control-plane response adoption.

Alternative considered: aggressively delete old methods first. That increases
regression risk because the controller still coordinates SwiftUI observation,
terminal runtime effects, and shell-core authority failures.

### Treat fixture-only Swift code as test support

`ShellActionRegistry.standard` and gated manifest parity helpers can remain only
as fixture/test support while Rust coverage exists. They should be moved out of
production-facing model files or hidden behind build flags that the app target
does not compile.

Alternative considered: leave fixture code in place with comments. That keeps
the files large and makes future contributors wonder whether Swift still owns
the domain.

### Preserve validation locality

Each refactor slice must run the narrow scripts for the moved behavior in
addition to architecture checks. A move from manifest startup needs manifest and
runtime metadata checks; a move from action/settings/profile needs the
corresponding adapter script; terminal-runtime extraction needs terminal runtime
or surface checks.

Alternative considered: rely on a full Xcode build. Builds catch compile
errors but not semantic authority regressions or legacy response projection
drift.

## Risks / Trade-offs

- Broad refactors can hide behavior changes -> Keep each slice focused on one
  owner boundary and require the same focused script before marking the task
  done.
- Splitting the FFI adapter can duplicate DTOs -> Keep shared portable DTOs in a
  single internal module and operation-family files limited to payload/response
  facades.
- Tight warning targets can block useful intermediate moves -> Allow a slice to
  document an intermediate boundary only when the next slice and remaining
  warning are recorded in `clients/apple/ARCHITECTURE.md`.
- Test-support moves can break ad-hoc Swift scripts -> Move helpers together
  with scripts that consume them, or add explicit test-support files to those
  script compile invocations.
- Reducing Swift file size can tempt Rust overreach -> The platform-effect
  boundary remains unchanged; Rust owns portable semantics, Swift owns macOS
  effects.

## Migration Plan

1. Capture the current line-count and warning baseline in `ARCHITECTURE.md` and
   tighten the architecture report so target warnings are named.
2. Split `ShellCoreFFIAdapter.swift` internals while keeping its public methods
   stable. Run the FFI adapter script after each operation-family move.
3. Extract shell startup/manifest/persistence collaborators from
   `ShellHostController.swift`. Run manifest, runtime metadata, shell contract,
   and architecture checks.
4. Extract action dispatch, reducer command routing, and control response
   adoption from `ShellHostController.swift` and
   `ShellHostControlCommandHandling.swift`. Run action, control-command seam,
   shell-core adapter, and shell contract checks.
5. Move fixture-only manifest/action helpers into test support or stricter build
   gates. Run the Swift scripts that compile those helpers plus Rust fixture
   tests.
6. Update the architecture debt ledger and warning threshold after every
   warning reduction.
7. Finish with `git diff --check`, focused Swift/Rust validation, strict
   OpenSpec validation for the change, and repo-wide strict OpenSpec
   validation.

Rollback is slice-scoped. If a move causes semantic drift, revert that slice
while keeping already-validated owner splits.

## Open Questions

- Should this change also split `ShellStateMutations.swift`, or should that be a
  follow-up after the controller and FFI bridge are narrow?
- Should the architecture checker enforce per-file line thresholds for named
  target owners immediately, or first record a decreasing warning budget?
