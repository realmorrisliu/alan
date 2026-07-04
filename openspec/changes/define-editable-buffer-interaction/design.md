## Context

ADR-0026 D4 adopts the Acme idea, not Acme's literal UI: text is the
programmable interaction surface, and the surface itself is a file server. The
current Alan OS path already has the substrate needed below it: aP file servers,
blocking-read streams, `io/` + `ctl` agent files, routefs for composition, and
content-addressed backing for durable knowledge. This change defines the M4+
interaction layer so future work can build it deliberately instead of adding
ad-hoc command palettes or UI-only text affordances.

The first implementation should prove the file contract before native UI. A
headless file-server harness is enough to show that humans, agents, tests, and a
future macOS view can all operate the same buffer through files.

## Goals / Non-Goals

**Goals:**

- Define an editable interaction buffer as an Alan OS file-server surface.
- Make text ranges addressable and executable through explicit file operations.
- Keep events observable through blocking reads, consistent with ADR-0024 D8.
- Make human and agent control symmetric: both use files, not hidden UI-only
  state.
- Keep the M0-M2 agent path independent; `io/` + `ctl` remains enough for the
  current North Star stack.

**Non-Goals:**

- Recreate Acme's mouse chords, visual style, or editing taste.
- Replace terminal panes, Alan Shell, or macOS shell navigation.
- Add a native macOS UI in the first contract slice.
- Add new authority outside namespace mounts, descriptors, and normal policy.
- Treat arbitrary text execution as implicit approval for side effects.

## Decisions

1. **Name the first server shape `editfs`, not Acme.**

   The name describes Alan's own surface: editable interaction buffers. The
   historical source remains in ADR-0026 only. A future crate can be
   `alan-editfs`, but this change defines the contract before requiring that
   crate to exist.

   Alternative considered: call it `alan-acme`. That would make the inspiration
   obvious but would invite copying UI details and user expectations that Alan
   explicitly does not adopt.

2. **Use one directory per buffer surface.**

   A buffer directory exposes `body`, `tag`, `ctl`, `addr`, and `event`.
   `body` is editable text, `tag` is a small command/status text surface, `addr`
   names the active range together with the source `body` revision and an
   address-selection revision, `ctl` commits operations, and `event` records
   edits, range changes, and executions. Successful `body`/`tag` writes and
   range replacements update the text returned by later reads; accepting an
   edit event while keeping stale text is not conforming behavior.
   `ctl` is a complete-document write entry point: implementations accumulate
   writes and act only when the client clunks the file, never while an `exec`
   document may still be partial.

   Alternative considered: one monolithic JSON document. That is easy to parse
   but not file-native; it would make shell usage and agent inspection worse.

3. **Represent selection as address state, not hidden UI state.**

   `addr` contains a stable range expression over `body` content plus the body
   revision observed when that range was selected and an address revision. Reads
   reveal the current range snapshot; writes propose a new range and advance the
   address revision; `ctl` operations that consume the range carry the expected
   range, body revision, and address revision so execution binds atomically to
   the selection and bytes the caller observed. A UI selection is only one
   client projection of `addr`.

   Alternative considered: let the native UI own selection and publish events
   after the fact. That breaks the symmetry goal because agents cannot drive the
   same surface without a UI adapter.

4. **Execution is explicit and capability-bounded.**

   Writing and clunking an `exec` document in `ctl` executes the expected range
   or supplied text through normal Alan Shell / process / routefs mechanisms.
   For range execution, the operation includes the range, body revision, and
   address revision observed by the caller; if any no longer matches, the server
   rejects the operation instead of executing another client's selection or
   mutated bytes. It must produce an event and any side effect still depends on
   the process namespace, descriptors, policy, and the ADR-0027 D3
   qualification: until ADR-0024 R1 lands, the mount set is an architectural
   discipline rather than a security property, and native subprocesses still
   need OS sandbox projection as a permanent second enforcement mechanism
   because they cannot see Alan namespaces directly. The buffer server does not
   become a privileged command runner.

   Alternative considered: every click or newline on command-looking text
   executes implicitly. That is faster but unsafe and hard to audit.

## Risks / Trade-offs

- [Risk] Text execution can hide side effects in prose. -> Mitigation:
  execution is an explicit `ctl` operation, records an event, uses normal
  capability checks, and keeps the R1 / OS sandbox boundary explicit.
- [Risk] A buffer file server could fork from terminal/shell conventions. ->
  Mitigation: first implementation is headless and uses Alan Shell/process
  adapters below it rather than owning a parallel command system.
- [Risk] Range addresses can become invalid after concurrent edits. ->
  Mitigation: `addr` and `exec` carry revision metadata; stale body or address
  commits fail with a typed error instead of editing or executing a different
  range.
- [Risk] UI work could dominate the contract. -> Mitigation: this change's first
  tasks stop at file contract and tests; native host work is a later change.
