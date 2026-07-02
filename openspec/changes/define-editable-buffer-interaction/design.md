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
   names the active range, `ctl` commits operations, and `event` records edits,
   range changes, and executions.

   Alternative considered: one monolithic JSON document. That is easy to parse
   but not file-native; it would make shell usage and agent inspection worse.

3. **Represent selection as address state, not hidden UI state.**

   `addr` contains a stable range expression over `body` content. Reads reveal
   the current range; writes propose a new range; `ctl` operations consume that
   range. A UI selection is only one client projection of `addr`.

   Alternative considered: let the native UI own selection and publish events
   after the fact. That breaks the symmetry goal because agents cannot drive the
   same surface without a UI adapter.

4. **Execution is explicit and capability-bounded.**

   Writing `exec` to `ctl` executes the current range or supplied text through
   normal Alan Shell / process / routefs mechanisms. It must produce an event and
   any side effect still depends on the process namespace, descriptors, and
   policy. The buffer server does not become a privileged command runner.

   Alternative considered: every click or newline on command-looking text
   executes implicitly. That is faster but unsafe and hard to audit.

## Risks / Trade-offs

- [Risk] Text execution can hide side effects in prose. -> Mitigation: execution
  is an explicit `ctl` operation, records an event, and uses normal capability
  checks.
- [Risk] A buffer file server could fork from terminal/shell conventions. ->
  Mitigation: first implementation is headless and uses Alan Shell/process
  adapters below it rather than owning a parallel command system.
- [Risk] Range addresses can become invalid after concurrent edits. ->
  Mitigation: events include revision metadata; stale `addr` commits fail with a
  typed error instead of executing a different range.
- [Risk] UI work could dominate the contract. -> Mitigation: this change's first
  tasks stop at file contract and tests; native host work is a later change.
