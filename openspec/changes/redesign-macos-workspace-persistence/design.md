## Context

`ShellHostController.syncWorkspaceManifestFromShellState` rebuilds the entire
`ShellContentWorkspaceManifest` (structure + per-Tab transcript `liveSnapshot`)
and writes it via `ShellWorkspaceManifestStore.save` →
`Data.write(to:options:.atomic)` **synchronously on the `@MainActor`
controller**. The dominant hot-path trigger is
`activeTaskChanged && !didPublishPaneUpdate` inside `updateTerminalMetadata` /
`projectTerminalRuntime`.

Evidence: `terminalMetadataCallback` avg 10.8ms, **max 24.9ms** (real usage). An
instrumented run attributed the residual to the manifest write (`shellSpacesRebuild`
was negligible). An equality guard that skipped writes differing only in volatile
`now` stamps did not help, because the trigger (`activeTask`) and the heavy field
(transcript) genuinely change — the cost is structural, not redundant.

This change is the **platform-IO half** of `macos-shell-workspace-persistence`.
The cross-platform-shell-core change owns the **portable semantics half**
(manifest schema, materialization, retention, `activeTask` durability) and
explicitly leaves "macOS manifest file IO in the platform layer". The two changes
modify the same spec via non-overlapping requirements.

## Goals / Non-Goals

**Goals:**
- Remove synchronous main-thread manifest writes from the terminal callback path.
- Decouple transcript persistence cadence from `activeTask` churn.
- Keep structural mutations promptly durable.
- Be forward-compatible: the writer is what a future Rust-core facade feeds.

**Non-Goals:**
- Changing the manifest model/schema, removing the `activeTask` field, or
  splitting the manifest into multiple files (all owned by shell-core).
- Changing TTL/retention or restore semantics.
- A general async-persistence framework — scope is this one store.

## Decisions

### Decision 1: Serial background `DispatchQueue`, not a Swift `actor`
Move encode + `Data.write(atomic:)` onto one serial `DispatchQueue`. Structural
writes call it synchronously (low-frequency user actions — a brief main-thread
block is acceptable and keeps the existing "persist immediately" tests passing
without `await`). Content writes are dispatched async.
- *Why not an actor*: the focused test harness runs synchronously via
  `MainActor.run`; `await` across an actor boundary is awkward to drive
  deterministically. GCD keeps both the synchronous-structure and
  deterministic-test paths simple.
- *Concurrency*: the immutable `ShellContentWorkspaceManifest` value is built on
  the main actor (reads `shellState`), then handed to the writer; only the value
  crosses threads, serialized by the queue. Confirm the manifest type is
  `Sendable`.

### Decision 2: Debounced content flush via an injectable scheduler
Content changes set a dirty flag and schedule a single flush (default
`DispatchQueue.main.asyncAfter(window)`, window tunable ~250ms–1s). On fire, the
flush rebuilds the manifest on the main actor and dispatches the encode+write to
the background queue. The scheduler is injected so tests fire it deterministically.
- *Why not next-tick `async` coalescing*: output bursts span many runloop
  iterations, so next-tick neither coalesces nor leaves the main thread.

### Decision 3: Replace the `activeTask` write trigger, keep the field
Remove the `activeTaskChanged → syncWorkspaceManifest` calls on the hot path; the
transcript that those incidentally persisted is now persisted by the debounced
content path. The `activeTask` field stays in the model (its durability is a
shell-core decision). This is purely a change of *when* we write.

### Decision 4: One file in v1 (defer the structure/content split)
Keep the single manifest file. Each write emits a complete current manifest, so a
serialized writer cannot tear or lose data regardless of which cadence triggered
it. Splitting into structure/content files (incremental writes) is a later
optimization and overlaps shell-core's model work — out of scope here.

### Decision 5: Restate the durability contract in tests
Tests that synchronously read the manifest right after `updateTerminalMetadata`
move to: perform the callback, fire the injected scheduler (or the
background/quit flush), then assert.

## Risks / Trade-offs

- **[Hard-crash transcript loss within the debounce window]** → acceptable for
  scrollback; bound the window and flush on background/quit.
- **[Background write races main-actor state mutation]** → build the immutable
  manifest value on the main actor; serialize writes on one queue.
- **[Quit before flush]** → use the app-termination hook (e.g.
  `applicationShouldTerminate` → `.terminateLater` if needed) to complete a
  synchronous flush before exit.
- **[Existing durability tests break]** → intended; migrate to the new contract
  with the injectable scheduler so they stay deterministic.

## Migration Plan

1. Add the serial background writer + injectable scheduler behind
   `ShellWorkspaceManifestStore` / its caller.
2. Make structural writes synchronous-on-write; route content changes through the
   debounced flush; remove the `activeTask` write trigger.
3. Add background/resign-active/quit flush hooks.
4. Migrate the durability tests to the new contract.
5. Re-run `capture-performance-diagnostics-workload.sh`; confirm the main-thread
   write is gone and `terminalMetadataCallback` tail latency (max) drops.

Rollback is local to the macOS persistence layer; the manifest file format is
unchanged.

## Open Questions

- Debounce window (250ms vs ~1s) — tune against the diagnostics workload.
- Whether building the manifest value on the main actor at flush time (transcript
  snapshot assembly) is itself cheap enough at ~1/s, or whether the snapshot
  assembly should also move off-main later.
