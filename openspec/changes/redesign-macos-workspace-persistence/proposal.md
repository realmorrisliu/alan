## Why

The macOS workspace manifest is rebuilt and written to disk with an **atomic,
synchronous write on the main thread**, triggered by `activeTask` changes on the
terminal metadata/runtime callback path. Because the manifest carries heavy
per-Tab transcript `liveSnapshot` content, this write is a main-thread
tail-latency source: in real usage `terminalMetadataCallback` ran avg 10.8ms but
**max 24.9ms** — past the 16.7ms frame budget, i.e. the dropped frame users feel
as stutter. Prior optimization passes (background-rendering, lazy rendering,
boot-profile caching) did not touch this path.

## Scope boundary (coordination with `introduce-cross-platform-shell-core`)

This change is deliberately scoped to the **macOS platform persistence IO layer**
— *when* and *how* (threading/cadence) the manifest is written to disk — which
the cross-platform-shell-core change explicitly leaves in the platform layer
("keeping macOS manifest file IO in the platform layer").

**Out of scope here** (owned by `shell-workspace-core-contract` and handled in
that change): the manifest *schema/model*, materialization, TTL/retention
semantics, and whether `activeTask` is a durable field. This change does **not**
change the manifest model, does **not** remove the `activeTask` field, and does
**not** split the manifest into multiple files — it only changes write
threading/cadence and the write trigger. It is forward-compatible: the off-main
debounced writer it introduces is exactly what a future Rust-core facade hands a
serialized manifest to.

## What Changes

- Move manifest JSON encode + `Data.write(atomic:)` **off the main thread** onto
  a serial background writer; the `@MainActor` controller must not block on disk.
- Replace the single `activeTaskChanged`-triggered synchronous write with a
  cadence split:
  - **Structural mutations** (Tab/Space create/close/reorder/pin/move,
    selection) persist synchronously (preserves existing "persist immediately").
  - **Restore content** (terminal transcript snapshots) is marked dirty on change
    and flushed on a **debounced** cadence plus a forced flush on
    background/resign-active/quit.
- **Stop using `activeTask` changes as a manifest write trigger.** (Timing change
  only — the `activeTask` field and its semantics are untouched and remain owned
  by the shell-core contract.)
- **BREAKING (durability contract)**: terminal transcript snapshots are now
  durable "within a bounded window and on background/quit" instead of
  "synchronously after each terminal callback". Update the tests that assert
  synchronous post-callback persistence.

## Capabilities

### New Capabilities
<!-- None: this restructures persistence timing within an existing capability. -->

### Modified Capabilities
- `macos-shell-workspace-persistence`: Add platform-IO requirements — manifest
  persistence must not block the main thread, and persistence cadence is
  separated by durability class (synchronous structure, debounced off-main
  restore content, transient runtime-state changes do not trigger a write). This
  is additive and IO-only; it does not touch manifest semantics, which the
  cross-platform-shell-core change covers in the same spec via separate
  (non-overlapping) requirements.

## Impact

- **Code**: `ShellHostController.syncWorkspaceManifestFromShellState` (write
  trigger + threading + cadence), `ShellWorkspaceManifestStore` (off-main serial
  write; an injectable scheduler/clock for testability), app lifecycle hooks
  (background/quit flush). No manifest model or `activeTask` field change.
- **Tests**: `clients/apple/scripts/test-shell-runtime-metadata.swift` durability
  assertions that read the manifest synchronously after `updateTerminalMetadata`
  move to the bounded-window/flush contract via the injectable scheduler; relates
  to `macos-shell-build-test-contract`.
- **Durability tradeoff**: a hard crash may lose up to the debounce window of the
  most recent transcript scrollback (acceptable); structure and pin snapshots
  remain synchronously durable.
- **Sequencing**: lands independently of (and before) the shell-core
  rust-ification; that change later moves manifest *semantics* into Rust and
  reuses this IO layer unchanged.
