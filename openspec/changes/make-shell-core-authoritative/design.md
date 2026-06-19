## Context

`introduce-cross-platform-shell-core` adds `crates/shell-core`,
`crates/shell-core-ffi`, Swift `ShellCoreFFIAdapter`, Xcode build integration,
and many macOS call sites. That change establishes the Rust module and proves
the app can call it, but several macOS paths still retain Swift implementations
for the same reusable shell-domain behavior.

The important remaining problem is authority, not reachability. If a macOS path
tries shell core and then silently falls back to Swift logic for manifest
materialization, reducer mutations, action availability, or control-command
projection, future Linux code can still drift and macOS can hide FFI/schema
failures behind stale behavior.

This change completes the migration by making shell core the source of truth for
the portable domain and by narrowing Swift to an adapter role.

## Goals / Non-Goals

**Goals:**

- Define the post-integration authority contract for Rust shell core and Swift
  platform adapters.
- Remove runtime Swift fallbacks for reusable shell-domain algorithms once the
  equivalent Rust operation is present and tested.
- Preserve macOS platform recovery paths where Swift is still the correct owner:
  corrupt manifest quarantine, manifest file IO, terminal runtime recovery,
  pasteboard/keyboard delivery, windowing, UI presentation, and diagnostics.
- Make failures observable and fail closed instead of silently recomputing
  shell-domain behavior in Swift.
- Add validation guards so replaced areas cannot regrow Swift domain fallback
  logic.
- Keep implementation slices small enough to review and verify independently.

**Non-Goals:**

- Do not redesign the Rust shell core API or replace the FFI envelope shape.
- Do not implement the Linux GTK client in this change.
- Do not move AppKit, SwiftUI, Ghostty, PTY/process handles, socket/file
  transport, or privileged helper execution into Rust shell core.
- Do not remove user-safe platform fallbacks that are unrelated to duplicated
  shell-domain logic.
- Do not require every Swift model type to disappear immediately; DTOs,
  Codable shapes, view projections, and adapter-only preservation of platform
  fields may remain.

## Decisions

### Treat shell core failures as authority failures

For operations whose domain semantics have moved to shell core, Swift callers
should use throwing calls and explicit error handling instead of `try? ... ??
SwiftImplementation`. A core failure should produce a diagnostic, stable
failure response, or safe startup recovery path.

Alternative considered: keep Swift fallback as a safety net. That masks exactly
the class of bugs this change needs to expose: missing dylibs, schema drift,
incorrect reducer payloads, and stale Swift algorithms.

### Classify Swift behavior before deleting it

Each candidate fallback should be classified as one of:

- **Domain duplicate**: reusable manifest, reducer, action, control, profile,
  or settings behavior that belongs in shell core and should be deleted.
- **Adapter projection**: Swift mapping between core DTOs and current UI/runtime
  models; it may remain but must not make independent domain decisions.
- **Platform recovery/effect**: macOS file IO, quarantine, runtime delivery,
  windowing, UI, diagnostics, or privileged helper work; it remains in Swift.
- **Parity fixture only**: temporary Swift code used to generate or compare
  fixtures; it must have a removal task and must not be used at runtime.

Alternative considered: search-and-delete every `fallback` string. That would
remove legitimate UI/runtime recovery behavior and would make the change noisy
without improving shell-core authority.

### Start with manifest authority

Workspace manifest startup is the cleanest first implementation slice because
Rust already exposes default, migrate, prune, and materialize operations, while
Swift currently retains a complete duplicate materializer/default/prune path.

The first slice should:

1. Make default/migrate/prune/materialize calls required.
2. Keep Swift manifest file loading, saving, and quarantine.
3. Delete or quarantine the Swift manifest algorithms after adapter and Rust
   tests cover the same behavior.
4. Tighten shell-contract checks so runtime fallback cannot return.

Alternative considered: start with control commands. Control commands are more
mixed because some commands require terminal runtime side effects, so they are a
better second slice after the fail-closed pattern is established.

### Keep reducer application core-owned, but allow narrow platform post-passes

Swift may perform post-passes that preserve platform-only state, such as
terminal runtime handles, renderer metadata, pending focus requests, or view
selection notifications. Swift must not recompute workspace focus, split
structure, tab organization, pinning, lifecycle pruning, action availability,
or stable command errors after core has returned a result.

Alternative considered: require Rust to own every field of current
`ShellStateSnapshot`. That would force platform handles and transient UI state
into the portable model, making Linux reuse harder rather than easier.

### Make control commands return core domain outcomes plus Swift side effects

The local command executor should route all workspace-domain command validation,
mutation, stable errors, and response projection through shell core. Swift may
execute returned side effects such as sending terminal text, focusing a surface,
or exporting diagnostics, then merge the platform outcome into the response.

Alternative considered: leave read-only and side-effect-heavy commands in one
large Swift switch. That keeps the old duplication point and makes it unclear
which command names are portable shell-domain commands versus macOS host
commands.

### Convert tests to interface tests

Rust shell-core tests should own behavior assertions for portable domain logic.
Swift tests should verify adapter envelope behavior, decoding, error mapping,
platform side effects, and app integration. Any Swift test that only preserves a
removed domain algorithm should be deleted or rewritten against the shell-core
interface.

Alternative considered: keep both Rust and Swift behavior tests permanently.
That creates two authoritative test surfaces and encourages maintaining
duplicate code to satisfy them.

### Use validation guards for drift control

Add explicit reject patterns or focused static checks for runtime Swift fallback
calls in replaced areas. The guards should reject domain fallback patterns while
allowing legitimate platform recovery and UI fallback text.

Alternative considered: rely on review discipline. The codebase already has
large shell files and many existing fallback terms; automated checks are needed
to make the desired final shape durable.

## Risks / Trade-offs

- FFI or dylib failure can become user-visible during startup -> Surface a
  clear diagnostic and keep only a minimal platform-safe recovery path where
  required for app launch.
- Removing Swift fallback can expose fixture gaps -> Land each deletion only
  after Rust tests, FFI tests, and focused Swift adapter tests cover the removed
  branch.
- Static reject patterns can be too broad -> Scope checks to known replaced
  files and explicit domain fallback symbols; allow platform recovery patterns
  by name.
- Keeping Swift DTOs can look like duplicate ownership -> Document adapter-only
  types and ensure they do not contain independent domain algorithms.
- Control commands mix domain validation and runtime effects -> Introduce typed
  side-effect outcomes so Swift can execute runtime work without owning command
  semantics.

## Migration Plan

1. Audit current macOS shell-core call sites and classify every fallback or
   duplicate Swift algorithm as domain duplicate, adapter projection, platform
   recovery/effect, or parity fixture only.
2. Make workspace manifest default, legacy migration, TTL pruning, and
   materialization shell-core required at runtime; keep Swift file IO and
   quarantine.
3. Delete or quarantine Swift manifest default/prune/materialize algorithms and
   update Swift tests to exercise the shell-core adapter instead.
4. Remove reducer-domain Swift fallback/post-computation, leaving only platform
   state preservation and side-effect dispatch.
5. Route workspace-domain control commands through shell core outcomes; split
   macOS-only host commands from portable shell-domain commands.
6. Make action registry and Terminal Profile paths fail closed on core errors
   rather than silently using Swift registry/profile logic.
7. Add validation guards to architecture and shell-contract scripts.
8. Run Rust shell-core tests, shell-core FFI tests, affected Swift scripts,
   architecture checks, OpenSpec validation, and `git diff --check`.

Rollback is slice-scoped. If a shell-core-authoritative slice fails in review or
testing, revert that slice while keeping prior authority slices intact. Do not
reintroduce a permanent Swift fallback without updating this spec.

## Open Questions

- Should the app launch with a minimal empty shell state if shell-core manifest
  materialization fails, or should it block shell startup with a visible
  diagnostic until the core dependency is repaired?
- Which Swift scripts should remain as app integration tests after their
  old domain assertions move into Rust fixture tests?
