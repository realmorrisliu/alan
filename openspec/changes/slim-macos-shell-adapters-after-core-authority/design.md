## Context

`make-shell-core-authoritative` changes the ownership boundary: Rust shell core
is now the authority for portable shell-domain behavior, while Swift remains the
macOS platform adapter. That transition explains the large positive diff. It
adds `alan-shell-core`, `alan-shell-core-ffi`, parity fixtures, and Swift
adapter tests before removing every coarse Swift host file that became too
large during the migration.

The current local baseline after the authority work is:

- PR-scale diff bucket: `crates/shell-core` about +23,954 lines, Apple Swift
  production about +3,749/-1,232 lines, Apple scripts/tests about +6,421 lines.
- Architecture report: 17 non-blocking large-file / bridge-boundary warnings.
- Largest shell-core-adjacent Swift files:
  - `ShellHostController.swift`: 4,637 lines
  - `ShellCoreFFIAdapter.swift`: 2,422 lines
  - `ShellStateMutations.swift`: 2,368 lines
  - `ShellHostControlCommandHandling.swift`: 1,803 lines
  - `TerminalHostRuntime.swift`: 1,399 lines
  - `ShellSettingsSurfaceModel.swift`: 1,263 lines
  - `ShellWorkspaceManifest.swift`: 1,236 lines
  - `ShellActionRegistry.swift`: 1,175 lines

This change treats the remaining Swift implementations of Rust-owned behavior
as authority debt to remove after the core boundary is accepted. File size and
warning counts are useful evidence, but they are not the design goal. It must
not re-open the authority question or preserve duplicate Swift domain logic just
to make moves easier.

## Goals / Non-Goals

**Goals:**

- Remove or move Swift legacy implementations for Rust-owned manifest,
  reducer/control, action registry, Terminal Profile, and settings behavior from
  production Apple sources.
- Keep architecture warning counts as regression evidence, not as the primary
  definition of completion.
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

### Use authority cleanup as the completion signal

This change should not count a file move as progress unless production Swift no
longer compiles or exposes a Rust-owned domain implementation, or unless a
remaining legacy helper is moved behind an explicit script/test-support
boundary. The architecture report should decrease as a consequence of real
owner cleanup and must not gain new warnings, but line count is only a
supporting signal.

Alternative considered: split files opportunistically and leave the report
unchanged. That creates churn without answering the user's concern about whether
the Swift legacy implementation has actually been cleaned up after Rust core
became authoritative.

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
production-facing model files into script support or hidden behind build flags
that the app target does not compile.

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
- Warning targets can distract from authority cleanup -> Treat warning count as
  a regression guard and require `clients/apple/ARCHITECTURE.md` to record the
  semantic owner boundary, not just a line-count change.
- Test-support moves can break ad-hoc Swift scripts -> Move helpers together
  with scripts that consume them, or add explicit test-support files to those
  script compile invocations.
- Reducing Swift file size can tempt Rust overreach -> The platform-effect
  boundary remains unchanged; Rust owns portable semantics, Swift owns macOS
  effects.

## Migration Plan

1. Capture the current authority debt baseline in `ARCHITECTURE.md`: which
   Rust-owned Swift legacy implementations remain in production files, which are
   test fixtures, and which app paths already fail closed through shell-core.
2. Move manifest parity helpers and the Swift action registry fixture out of
   production-facing model files into explicit script support, keeping Rust core
   and FFI tests as the portable behavior authority.
3. Split `ShellCoreFFIAdapter.swift` internals while keeping its public methods
   stable so Swift contains adapter code rather than duplicated domain logic.
   Run the FFI adapter script after each operation-family move.
4. Extract shell startup/manifest/persistence collaborators from
   `ShellHostController.swift`. Run manifest, runtime metadata, shell contract,
   and architecture checks.
5. Extract action dispatch, reducer command routing, and control response
   adoption from `ShellHostController.swift` and
   `ShellHostControlCommandHandling.swift`. Run action, control-command seam,
   shell-core adapter, and shell contract checks.
6. Update the architecture debt ledger after every legacy cleanup or owner
   split.
7. Finish with `git diff --check`, focused Swift/Rust validation, strict
   OpenSpec validation for the change, and repo-wide strict OpenSpec
   validation.

Rollback is slice-scoped. If a move causes semantic drift, revert that slice
while keeping already-validated owner splits.

## Open Questions

- Should this change also split `ShellStateMutations.swift`, or should that be a
  follow-up after the controller and FFI bridge are narrow?
- Which remaining Swift helpers are pure compatibility decode/repair logic that
  must stay in the macOS adapter, rather than legacy Rust-owned domain logic?
