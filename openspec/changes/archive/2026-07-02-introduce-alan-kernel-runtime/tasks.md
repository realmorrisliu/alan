> **Re-scoped by ADR-0024.** This change is the migration slice: a user-space
> projection file server that maps the current Agent Execution Engine and session
> protocol onto agent-conforming process files (`define-agent-file-layout-contract`)
> above the substrate (`define-plan9-kernel-substrate`). It owns only
> `alan-agent-adapter-contract`. The prior task list (building an `Agent Process`
> kernel type, opaque ids, a Kernel Journal, ViewModels, and semantic-view
> snapshots) is fully replaced; that work targeted the retired ontology.
>
> **Superseded for engine-native rewrite.** Remaining live wiring, `io/input`
> resume, namespace overlay, LLM generation, tool execution, and direct agent-file
> state-writing tasks are now owned by
> `refactor-engine-namespace-native`. Keep this change as historical ADR-0024
> migration context; do not implement new engine-runtime work here.

## Constraints (from ADR-0024)

- v1 file servers run in-process over the substrate's fast path (D5); no 9P wire
  transport in this change.
- The namespace capability boundary is convention-enforced, not isolation-
  enforced, in v1 (R1); do not claim hard isolation.
- `alan-kernel` depends only on `alan-ap` (ADR-0025 D1) and MUST NOT gain
  legacy/runtime/provider/client dependencies; the projection (which may use
  `alan-runtime` / `alan-protocol`) is a separate user-space file-server crate.

## 1. Prerequisites (owned elsewhere)

- [ ] 1.1 Substrate crate provides the file-server contract, namespace/mount,
  byte/offset stream files, `/proc`, and `/srv` (owned by
  `define-plan9-kernel-substrate` §5–§9).
- [ ] 1.2 `define-agent-file-layout-contract` is accepted as the file layout this
  projection conforms to.

## 2. Projection file server skeleton

- [ ] 2.1 Add a user-space projection crate that depends on `alan-runtime` and
  `alan-protocol` internally and on the substrate's file-server contract, while
  `alan-kernel` stays free of those deps.
- [ ] 2.2 Start the projection as a long-running process that posts a mountable
  handle under `/srv/agent-runtime` and serves a tree mounted at `/agent`.
- [ ] 2.3 Add a feature flag / fallback so the legacy session path keeps working
  when the projection is disabled or incomplete.

## 3. Process and IO surfaces

- [ ] 3.1 Create each session's backing process via the aP spawn path
  (`/proc/clone` → exec spec → clunk; e.g. an exec wrapper around the engine), so
  the kernel renders it in `/proc` (no kernel agent type, no registration API) and
  serve its agent surface under `/agent`; do not fabricate `/proc/<pid>` entries
  from the projection. Keep the session id as an internal runtime reference, never
  as kernel identity.
- [ ] 3.2 Map session metadata to the agent-owned `machine/status` (leave the
  generic top-level `status` as the kernel's process status).
- [ ] 3.3 Map conversation input/output and lifecycle to `io/input`, `io/output`,
  and `io/events` as byte/offset stream files; assistant text and thinking deltas
  append to `io/output` and `io/events`.

## 4. Machine surface

- [ ] 4.1 Map tape to `machine/tape` as the append-only source of truth.
- [ ] 4.2 Expose the model context window as a *view* over `machine/tape`
  (compaction is a view, not a hidden step).
- [ ] 4.3 Map machine state, transition events, and recovery checkpoints to
  `machine/` files, gated by access rights.

## 5. Request surface

- [ ] 5.1 Project confirmation, structured-input, dynamic-tool, approval, and
  credential yields into `requests/<id>/` trees (kind, prompt, options, status,
  response).
- [ ] 5.2 Expose a `requests/` events stream so watchers learn of new requests by
  blocking read, not polling.
- [ ] 5.3 Deliver a written `requests/<id>/response` back into the current
  engine's resume path.

## 6. Action surface

- [ ] 6.1 Project tool calls and external effects into `actions/<id>/` trees
  (status, output, result, risk, approval).
- [ ] 6.2 Link an action to its tool process at `/proc/<tool-pid>` when a concrete
  process exists, rather than duplicating it.

## 7. Control surface

- [ ] 7.1 Route generic control (interrupt, cancel) through the kernel
  `/proc/<pid>/ctl`, and agent-runtime control (compact, rollback) through the
  agentfs-owned `machine/ctl` in the `/agent` overlay — never the kernel ctl, so
  `alan-kernel` interprets no runtime semantics. Map both onto current engine
  operations; no per-action side files.

## 8. `/agent` overlay

- [ ] 8.1 Present `/agent/<pid>` as an overlay: union the full kernel `/proc/<pid>`
  generic layout (identity/parentage/credentials/namespace/exit state + io/status/
  ctl) with the projection's agent surfaces
  (requests/actions/machine/context/children/events). Do not put agent files into
  `/proc`; do not expose any `/agent` entry without a backing `/proc` Process
  (not a second process table).
- [ ] 8.2 Resolve `/agent/root` to whichever pid currently embodies the root
  agent's home; durable identity stays the home path, not the pid.

## 9. Existing TUI migration

- [ ] 9.1 Read agent files (`io/output`, `requests/`, `actions/`, `status`) and
  write `ctl` from `crates/tui`, behind a compatibility-first path.
- [ ] 9.2 Preserve session create/attach, hydration, reconnect replay, submit,
  resume, interrupt, compact, rollback, and pending-yield behavior.
- [ ] 9.3 Run the file path in parallel with the legacy reducer and add parity
  tests before removing legacy behavior for a surface.
- [ ] 9.4 Leave unsupported surfaces on the legacy path until file parity holds.

## 10. Dependency boundary

- [x] 10.1 Add tests proving `alan-kernel` has no dependency on `alan-runtime`,
  `alan-protocol`, provider clients, memory stores, or sandbox backends, and that
  those live only in the projection crate. (Kernel side: `alan-kernel`
  `dependency_boundary`; projection side: `alan-agentfs` `dependency_boundary`
  asserts the file server never depends on the kernel/another server/a client.)

## 11. Verification and review

- [ ] 11.1 Run focused `cargo test` for the projection crate and affected
  `crates/tui` tests.
- [ ] 11.2 Run `just verify`.
- [ ] 11.3 Run `openspec validate introduce-alan-kernel-runtime --strict`.
- [ ] 11.4 PR review against ADR-0024: single `Process` category, file-layout
  conformance, compaction-as-view, no global addressing, no second event system.
- [ ] 11.5 After merge, sync accepted delta specs into `openspec/specs/` and
  prepare archive-readiness notes.
