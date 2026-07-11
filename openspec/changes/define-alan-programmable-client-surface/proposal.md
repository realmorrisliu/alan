## Why

Alan already has a namespace-native shell, a headless editable-buffer file
server, and file-backed renderer contracts, but they do not yet form one
programmable interaction loop. Without a shared contract, selected-text
execution, result capture, completion, and future renderer work can drift into
renderer-local commands, hidden editfs authority, or another generic UI/action
framework.

## What Changes

- Define the Programmable Client Surface as part of Alan Shell: a text-first
  interaction contract over the caller's mounted Namespace, not a new service,
  app, UI framework, registry, or top-level namespace root.
- Reuse Alan Shell's existing explicit `ls`, `cat`, `tail`, `write`, `echo`, and
  `spawn` grammar through a shared headless parser/executor; do not introduce a
  second surface-only parser or a full `rc`-like language in this slice.
- Add the short, discoverable `run` Tool through the canonical package/binfs
  mount. Treat that mount as an entry criterion for command exposure. Each
  selected-text execution spawns an ordinary Alan Shell Evaluator Process whose
  `/proc/<pid>` tree is the sole execution identity and lifecycle surface.
- Make the evaluator validate the selected `body`/`addr` revisions through
  `editfs`, execute under its inherited caller Namespace, stream output through
  its Process files, and materialize bounded UTF-8 results back into the buffer
  without overwriting concurrent edits.
- Keep live `tail` output descriptor-backed and transient; only an explicit
  bounded capture becomes editable buffer text.
- Remove `editfs` execution policy as an authority boundary. `editfs` owns
  selection consistency and interaction events; Process spawn, Namespace,
  access rights, Tool governance, and sandbox projection own execution
  authority.
- Derive text-first discovery from the mounted Namespace, `/bin`, Tool
  Manifests, `/man`, `/lib/skill`, file kinds, and access rights; do not define a
  generic UI/form schema.
- Record Rust to WASM Component as the explicit promotion direction for mature
  reusable behavior while deferring WASM hosting, WIT package design, build,
  signing, installation, and projection to a later change.
- Deliver a headless end-to-end harness over the existing single buffer at
  `/mnt/edit`; defer multi-buffer allocation and TUI/macOS surface work.

## Capabilities

### New Capabilities

None. Programmable Client Surface is part of the existing `alan-shell`
capability, not an independent durable subsystem.

### Modified Capabilities

- `alan-shell`: Define the programmable interaction contract, shared command
  execution layer, namespace-derived discovery, and Process-backed `run` Tool.
- `editable-buffer-interaction`: Replace service-side execution with
  caller-spawned evaluator Processes, define result materialization and live
  stream behavior, and keep interaction events linked to `/proc` truth.
- `editable-buffer-file-server`: Replace the headless accept/deny execution
  policy with revision validation and evaluator-originated execution events.
- `alan-renderer-host-contract`: Require renderers to project programmable
  buffers and evaluator Process files without becoming execution or domain
  authority.

## Impact

- Refactors `crates/shell` so its current private line parser/executor can be
  reused by stdio and the `run` Tool.
- Evolves `crates/editfs` control and event semantics and removes
  `ExecutionPolicy::{AcceptAll,DenyAll}` as the execution boundary.
- Adds a first-party `run` executable, Tool Manifest, manual surface, Process
  lifecycle tests, and a headless Shell + editfs + Kernel integration harness
  only through the canonical package/binfs mount; no bootstrap-only command
  binding is introduced.
- Updates the Alan product glossary with Programmable Client Surface, Alan Shell
  Evaluator Process, and WASM Component terminology.
- Does not change Alan Kernel primitives, add a new namespace root, implement a
  WASM host, or modify Ratatui and Alan for macOS in the first slice.
