## Context

Alan's native macOS shell has become the product's richest terminal workspace
surface. The current Swift implementation already contains durable domain logic
that is not inherently macOS-specific:

- workspace state models for Spaces, Tabs, PaneSlots, ContentInstances, and
  split trees
- reducer-style mutations for split, focus, movement, close, pinning, reordering,
  zoom, resize, and attention
- workspace manifest defaulting, legacy upgrade, materialization, pruning, and
  restore snapshot behavior
- shell action registry availability and effect mapping
- control-plane command validation and response projection
- Terminal Profile validation and launch resolution
- settings summaries for local workspace/profile/account state

Future Linux GTK work should not copy these semantics into another UI client.
The cross-platform boundary should move the reusable shell workspace domain into
Rust while leaving UI, terminal runtime hosting, file IO, IPC transport, and
privileged OS operations in platform adapters.

The current Apple architecture checks already identify large Swift files and
controller/AppKit leakage as maintainability debt. This change treats that debt
as migration evidence: domain truth should move out of Swift module by module,
with parity fixtures proving equivalence before each replacement.

## Goals / Non-Goals

**Goals:**

- Define a platform-neutral Rust shell workspace core as the long-term owner of
  reusable shell domain logic.
- Keep the Rust core pure: no Swift, GTK, AppKit, Ghostty, Axum daemon, socket,
  file-system persistence, clipboard, file picker, or privileged executor
  dependencies.
- Use module boundaries that match the domain: model, manifest, reducer,
  actions, control, terminal profiles, settings summaries, and fixtures.
- Build a parity-first migration path where Rust behavior is proven against
  Swift-exported fixture cases before macOS calls are replaced.
- Use a coarse-grained, versioned binding facade for Swift integration after
  Rust parity is established.
- Preserve existing shell control-plane response shapes and stable error codes
  unless a later spec explicitly changes them.
- Make architecture warning reduction a completion criterion for Swift
  replacement work.

**Non-Goals:**

- Do not implement Linux GTK UI in this change.
- Do not move Ghostty/AppKit terminal hosting, PTY/process handles, renderer
  state, or delivery queues into shell core.
- Do not move macOS IPC transport, file polling, socket serving, clipboard,
  folder opening, diagnostics presentation, Sparkle, App Intents, or windowing
  into shell core.
- Do not move privileged account application or AppleScript privilege execution
  into shell core. Core may plan or validate portable domain state; platform
  adapters own OS effects.
- Do not expose async bindings, callbacks, foreign traits, or long-lived Rust
  workspace objects in the first Swift integration path.
- Do not replace Swift callers before the corresponding Rust module has parity
  fixtures and adapter tests.

## Decisions

### Add `crates/shell-core` as a pure Rust domain crate

The new crate should be named after the durable product/runtime contract, not
after a platform. It should expose ordinary Rust APIs for internal use and tests.
The crate should not depend on the Apple client, GTK, daemon server, terminal
renderer, or platform IO.

The module layout should be:

- `model`: `WorkspaceState`, `Space`, `Tab`, `PaneSlot`, `ContentInstance`,
  `SplitTree`, attention, lifecycle, content kind, terminal metadata, and
  stable IDs.
- `manifest`: versioned workspace manifest schema, default manifest, legacy
  upgrade, materialization, TTL pruning, pin/live restore snapshots, and quick
  terminal restore records.
- `reducer`: pure mutations for space, tab, pane, content, split, focus, move,
  close, pin, unpin, reorder, resize, zoom, attention, and agent activity.
- `actions`: stable action IDs, target resolution, availability, shortcut
  metadata, and action-to-effect mapping.
- `control`: shell control command DTOs, validation, stable error codes,
  authoritative response projection, and runtime intent separation.
- `terminal_profile`: Terminal Profile document, editor/validation, resolution
  order, and `TerminalLaunchIntent` construction.
- `settings_summary`: reusable settings/domain summaries that are independent
  from SwiftUI or GTK layout.
- `fixtures`: test-only fixture loading and comparison helpers.

Alternative considered: put this logic in `crates/tui` or `crates/alan`.
`crates/tui` is a daemon-backed conversation UI, not the native shell workspace
domain. `crates/alan` owns daemon/CLI hosting and would couple the core to a
server implementation.

### Keep data flow authority explicit

The desired data flow is:

```text
Manifest -> WorkspaceState -> Platform Runtime -> Published Projection
```

The manifest is the restore authority and stores only restorable intent:
Spaces, Tabs, layout, content restore payloads, profile references, TTL metadata,
pin/live snapshots, and quick terminal restore data.

`WorkspaceState` is the current domain state. Reducers return a new state plus
events, runtime intents, manifest sync hints, and response fields. Platform
runtime adapters execute intents such as starting terminal content, sending
terminal input, closing terminal content, or capturing transcript snapshots.

Published projection remains a compatibility surface for UI, automation, and
control-plane clients. During migration, the Rust core should preserve the
current JSON-visible shape where existing clients depend on it.

Alternative considered: make platform runtime handles part of the state model.
That would make the model non-portable and would violate the current manifest
contract that excludes PTYs, process handles, renderer objects, and delivery
queues.

### Use parity-first migration before Swift replacement

The first line of work should build Rust modules and tests without connecting
them to the macOS app. Existing Swift script tests become behavior sources for
fixture generation:

- `test-shell-split-model`
- `test-shell-workspace-manifest`
- `test-shell-automation-command-seams`
- `test-shell-sidebar-tab-rows`
- `test-shell-settings-surface`

Each migrated module should add fixture cases that include input state or
manifest, operation input, and expected output. Rust tests should compare
semantics against those fixtures. When exact JSON ordering is irrelevant, tests
should compare normalized semantic forms rather than raw bytes.

Alternative considered: replace Swift directly and rely on app smoke tests.
That would make regressions harder to localize and would turn FFI/debugging
issues into product behavior uncertainty.

### Use a separate binding facade and keep the core binding-agnostic

The pure Rust crate should not be shaped around Swift binding macros. A separate
facade crate or module should expose cross-language entrypoints after parity
exists. The first facade should be coarse-grained and versioned, passing
request/response envelopes rather than many small functions.

UniFFI may be used for the generated Swift layer, but it must not determine the
core module boundaries. The first integration should avoid async UniFFI,
foreign traits, platform callbacks, and long-lived Rust objects. If UniFFI is
used initially, it should expose stable facade functions over bytes or a small
number of stable DTOs instead of exporting the entire evolving workspace model.

Alternative considered: hand-written C ABI only. That is simple and explicit,
but UniFFI can reduce Swift wrapper boilerplate once the facade is constrained.
The important decision is the facade shape, not the binding generator.

### Keep platform adapters narrow

macOS Swift and future GTK code should call shell core through platform
adapters. Platform adapters own:

- SwiftUI/GTK presentation and controller state
- window, sidebar, drag/drop, keyboard, menu, and context-menu rendering
- terminal widget/runtime attachment, PTY/process handles, renderer objects,
  delivery queues, and transcript extraction
- control-plane transport such as socket serving and file polling
- file-system persistence locations and corrupt-file quarantine mechanics
- clipboard, folder opening, file picker, update UI, and diagnostics
  presentation
- privileged account apply executors and platform-specific verification

The Rust core owns whether a command is valid, how domain state changes, what
stable result/error should be reported, and what runtime or persistence intent
the platform adapter should execute.

### Migrate modules from inner domain outward

Implementation should follow dependency order:

1. scaffold shell-core, schema/version envelopes, fixture format, and validation
2. workspace model and split tree
3. state reducer
4. manifest default/upgrade/materialize/prune
5. action registry
6. control command reducer/result projection
7. Terminal Profile domain
8. settings summaries
9. Swift binding adapter replacement module by module
10. Swift logic deletion and architecture warning burn-down

This order keeps each step testable and avoids building platform bindings before
the underlying semantics are stable.

## Risks / Trade-offs

- JSON or byte-envelope bindings can be less ergonomic than typed Swift APIs ->
  keep them behind `ShellCoreFFIAdapter` and upgrade only stable DTOs later.
- UniFFI can generate large Swift/modulemap diffs and has Swift 6 concurrency
  edges -> start with synchronous coarse-grained facade functions and pin the
  generator version in build checks.
- Rust and Swift semantic drift can hide in fixture gaps -> require fixtures for
  every removed Swift branch and keep existing focused Swift scripts in the
  validation matrix.
- Large workspace states may make binding calls expensive -> keep high-frequency
  terminal input/rendering outside shell core bindings and reserve core calls for
  workspace mutations, manifest operations, action resolution, and command
  reduction.
- A compatibility bug in manifest migration could lose user workspace state ->
  preserve current manifest JSON compatibility, keep corrupt evidence, and add
  fixture cases for old and malformed manifests before replacing Swift manifest
  logic.
- Architecture warnings might remain unchanged if Swift wrappers simply call
  Rust but old logic stays in place -> each replacement task must remove or
  narrow the replaced Swift implementation and update the architecture ledger.

## Migration Plan

1. Add the Rust crate and fixture harness without changing the app.
2. Build Rust modules and parity fixtures in dependency order.
3. Add a constrained Swift binding facade once at least one module has parity.
4. Replace Swift call sites module by module, retaining short-lived fallback or
   oracle paths only while adapter tests are being introduced.
5. Delete replaced Swift logic after the Rust-backed path passes focused Swift
   scripts, Rust tests, and binding tests.
6. Update `clients/apple/ARCHITECTURE.md` and architecture validation
   expectations as warning debt decreases.

Rollback for any app-connected slice should be module-scoped: restore the Swift
call path for that module while keeping already-proven Rust modules and fixtures
in place. Manifest-affecting slices must keep read compatibility with previously
written files.

## Open Questions

- Whether the first binding facade should use UniFFI over byte envelopes or a
  hand-written C ABI over byte envelopes can be decided during the binding slice.
  The architectural requirement is a coarse-grained, versioned facade.
- Whether `settings_summary` should live in `crates/shell-core` or a later
  companion crate depends on how much of the summary logic remains shell-domain
  rather than host-configuration presentation.
