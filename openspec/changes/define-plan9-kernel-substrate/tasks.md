## 1. Anchor the model

- [x] 1.1 Record the consolidated model as ADR-0024.
- [x] 1.2 Define `plan9-kernel-substrate` as the durable kernel capability.

## 2. Substrate spec

- [x] 2.1 Specify kernel scope (namespace + process table + `/proc` + `/srv`).
- [x] 2.2 Specify one `Process` category and remove `Agent Process`.
- [x] 2.3 Specify the wire-shaped file-server contract and in-process v1 path.
- [x] 2.4 Specify byte/offset streams and blocking-read observation.
- [x] 2.5 Specify namespace-as-capability and the no-global-addressing rule.
- [x] 2.5a Specify access rights as the dimension separating awareness (read-only
  mounts) from authority (read-write mounts) (PR #572 review gap).
- [x] 2.6 Specify mount/bind/union assembly and per-process namespace.
- [x] 2.7 Specify kernel ephemerality and file-server persistence.
- [x] 2.8 Specify `/proc` and `/srv` synthetic devices and bootstrap.
- [x] 2.9 Specify kernel crate dependency isolation.

## 3. Supersession

- [x] 3.1 Remove `add-agent-process-kernel-types` (superseded by this change).
- [x] 3.2 Cut the `alan-kernel-contract` spec in `introduce-alan-kernel-runtime`
  down to a superseded pointer to this change.

## 4. Verification

- [x] 4.1 Run `openspec validate define-plan9-kernel-substrate --strict`.
- [x] 4.2 Run `openspec validate --all --strict`.

## 5. aP protocol crate (`alan-ap`)

- [x] 5.0 Create the standalone `alan-ap` crate (ADR-0025 D2) — the aP protocol
  (9P analog) — with no alan-specific dependencies; `alan-kernel` depends on it.
- [x] 5.1 Define the `FileServer` trait over fids: `walk`, `open`, `read`,
  `write`, `stat`, `create`, `remove`, `clunk`. Inputs/outputs are paths/fids,
  byte buffers, offsets, and error codes only — no borrows, no rich return types
  (D5, wire-shaped).
- [x] 5.2 Define the fid lifecycle: a fid is a handle to one interaction;
  `walk`/`open` allocate it, `clunk` releases it, and `open` MAY have allocation
  side effects (see clone-via-open, 5.5). Each `open` yields an independent fid so
  concurrent callers do not interfere.
- [x] 5.3 Define a byte/offset stream file kind with retained history: `read`
  blocks until new bytes are available, resumes from a caller-held offset, and
  retains records up to a server policy so a reconnecting reader neither misses
  nor mis-replays (D8). No separate notification primitive.
- [x] 5.4 Define clone-via-open: opening a `clone` file allocates a new resource
  (for example a connection directory) and returns its name/handle — an
  open-with-allocation convention, not a new operation. (Used by `alan-llmfs`
  Generations.)
- [x] 5.5 Define the three-phase error model: dial-time failures (no access, rate
  limited, not found) return an `open` error; commit-time failures (malformed/
  truncated request at `clunk`) return a `write`/`clunk` error and start nothing;
  mid-interaction failures surface as a terminal error record in the stream.
- [x] 5.6 Implement the in-process fast-path transport that dispatches aP calls
  without serialization, so high-rate streams pay no protocol cost.
- [x] 5.7 Add a serialization round-trip test over the aP wire shape to prove a
  dumb byte transport could carry every operation unchanged (guards the D5
  discipline before any wire transport exists).

## 6. Namespace engine and process table (crate)

- [x] 6.1 Implement the per-process namespace: a mount table with `mount`,
  `bind`, `unmount`, union directories, and `walk` resolution.
- [x] 6.2 Implement namespace inheritance: a child receives a namespace
  constructed by its spawner and may only restrict its own view (D6).
- [x] 6.3 Enforce no global ambient addressing: resolution is only through the
  namespace; opaque ids resolve within a namespace and are never a global
  capability (D6).
- [x] 6.4 Implement the process table with one `Process` category — identity,
  parentage, credentials, namespace, lifecycle, status, exit state — and no
  `Agent Process` type (D3).
- [x] 6.5 Keep the process table, namespaces, and fids as ephemeral runtime state
  that starts empty on restart (D7).

## 7. Synthetic devices (crate)

- [x] 7.1 Implement `/proc` as a file server rendering the process table, with
  per-process `io/`, `status`, `ctl`, and standard files (D9); `/proc/<pid>` is
  the single source of truth.
- [x] 7.1a Implement process creation (spawn) via clone-via-open on
  `/proc/clone`: `open` returns the new pid as a fid-private pending slot (not yet
  in public `/proc`); the caller writes the exec spec (executable + args + child
  namespace) and `clunk`s to commit/start (commit-on-clunk — never start from a
  partial spec); on success `/proc/<pid>` becomes publicly visible, on failure the
  pending slot is discarded (never listed in public `/proc`, so no leak/observer);
  `clunk` returns success or a commit-time error, no payload. spawn is aP
  open+write+clunk, no side API. Spawn is capability-preserving: reject any
  exec-spec namespace entry/descriptor the spawner could not itself open or
  delegate (no amplification; D6).
  Done 2026-07-02: `ProcFs` implements `/proc/clone` as clone-via-open with a
  fid-private pending pid, offset-aware exec-spec writes, commit-on-clunk, public
  `/proc/<pid>` publication only after commit, and discard-on-error. Tests cover
  successful spawn, malformed commit rejection, pending-slot non-listing,
  write-intent enforcement, spawner parent/credentials/namespace inheritance,
  namespace manifest matching, mismatch rejection without pid leaks, and child-pid
  placeholder expansion.
- [x] 7.2 Implement `/srv` as the bootstrap rendezvous device, access-filtered:
  posted handles carry access rights; a process sees/mounts only permitted
  handles; a withheld service is not remountable via `/srv` (D6).
- [x] 7.3 Bring the kernel up with only `/proc`, `/srv`, and the namespace engine,
  leaving init / Service Manager to assemble the rest of the root namespace.
  Done 2026-07-02: `KernelRoot::new()` constructs the substrate-only boot root
  by mounting only `ProcFs` at `/proc` and `SrvFs` at `/srv` behind `MountFs`.
  Bootstrap tests assert the root listing is exactly `proc` and `srv`, that
  `/proc` starts with only `clone`, that `/srv` starts empty, and that posting a
  later service handle in `/srv` does not implicitly mount higher-level trees in
  the kernel boot root.

## 8. Keep the retired ontology out of the new crate

- [x] 8.1 Build `alan-kernel` as a new crate containing only the substrate (no
  retired V1 modules such as `agent_capability`, `descriptors`,
  Object/Buffer/View/Command/Query/Subscription/Task/Artifact/Evidence, `views`,
  `ledger`, `registry`, `invocation`, or V1 `ids`). There is no current
  `alan-kernel` crate to clean — the V1 one was removed.
- [x] 8.2 Relocate any V1 surfaces still needed during migration from the actual
  current owners (`alan-runtime`, `alan-protocol`, `crates/alan`, `crates/tui`)
  into a compat/app crate (for example `alan-compat`), never into `alan-kernel`.
  Done 2026-07-02: no V1 surface needed relocation into `alan-kernel`; the crate
  stays substrate-only. `cargo tree -p alan-kernel --depth 1` shows only
  `alan-ap` among Alan crates, and the targeted retired/legacy-token search only
  finds the boundary test's own forbidden-token list. Existing app/runtime
  compatibility surfaces remain in their current owners while the kernel boundary
  test prevents them from flowing into `alan-kernel`.
- [x] 8.3 Extend `tests/dependency_boundary.rs` to fail if `alan-kernel` gains a
  dependency on `alan-runtime`, `alan-protocol`, provider clients, memory stores,
  sandbox backends, renderers, or async task handles — and to fail if the retired
  module names reappear (automated D9/D3 discipline).

## 9. Verification

- [x] 9.1 Run focused `cargo test -p alan-kernel`.
- [x] 9.2 Run `just verify`.
  Done 2026-07-02: `just verify` passed, including `cargo fmt --all`,
  workspace clippy, workspace tests, doctests, and the final mock smoke suite.
- [x] 9.3 Re-run `openspec validate --all --strict`.
  Done 2026-07-02: all 73 OpenSpec items passed strict validation; targeted
  `openspec validate define-plan9-kernel-substrate --strict` also passed.

## 10. Downstream (other changes)

- [x] 10.1 `define-agent-file-layout-contract` lands above this substrate.
  Done 2026-07-02: `define-agent-file-layout-contract` is complete at 27/27 and
  validates strictly; it now owns the agent file-layout surface above this
  substrate.
- [x] 10.2 `introduce-alan-kernel-runtime` builds the projection file server that
  depends on this crate.
  Done 2026-07-02: the original `introduce-alan-kernel-runtime` implementation
  slice was superseded; the completed `refactor-engine-namespace-native` change
  now builds the namespace-native projection path above `alan-kernel`, with all
  37/37 tasks complete.
- [x] 10.3 Record the later slice: aP wire transport with network transparency
  (import/export remote trees) for distributed agents (ADR-0026 D1).
  Done 2026-07-02: explicitly left as a later Ring 3 implementation slice per
  ADR-0027 D2. This substrate keeps the aP operation shape wire-ready, but does
  not start the import/export transport work in the Ring 2 finish-line change.
