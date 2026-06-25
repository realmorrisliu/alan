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

- [ ] 5.0 Create the standalone `alan-ap` crate (ADR-0025 D2) — the aP protocol
  (9P analog) — with no alan-specific dependencies; `alan-kernel` depends on it.
- [ ] 5.1 Define the `FileServer` trait over fids: `walk`, `open`, `read`,
  `write`, `stat`, `create`, `remove`, `clunk`. Inputs/outputs are paths/fids,
  byte buffers, offsets, and error codes only — no borrows, no rich return types
  (D5, wire-shaped).
- [ ] 5.2 Define the fid lifecycle: a fid is a handle to one interaction;
  `walk`/`open` allocate it, `clunk` releases it, and `open` MAY have allocation
  side effects (see clone-via-open, 5.5). Each `open` yields an independent fid so
  concurrent callers do not interfere.
- [ ] 5.3 Define a byte/offset stream file kind with retained history: `read`
  blocks until new bytes are available, resumes from a caller-held offset, and
  retains records up to a server policy so a reconnecting reader neither misses
  nor mis-replays (D8). No separate notification primitive.
- [ ] 5.4 Define clone-via-open: opening a `clone` file allocates a new resource
  (for example a connection directory) and returns its name/handle — an
  open-with-allocation convention, not a new operation. (Used by `alan-llmfs`
  Generations.)
- [ ] 5.5 Define the three-phase error model: dial-time failures (no access, rate
  limited, not found) return an `open` error; commit-time failures (malformed/
  truncated request at `clunk`) return a `write`/`clunk` error and start nothing;
  mid-interaction failures surface as a terminal error record in the stream.
- [ ] 5.6 Implement the in-process fast-path transport that dispatches aP calls
  without serialization, so high-rate streams pay no protocol cost.
- [ ] 5.7 Add a serialization round-trip test over the aP wire shape to prove a
  dumb byte transport could carry every operation unchanged (guards the D5
  discipline before any wire transport exists).

## 6. Namespace engine and process table (crate)

- [ ] 6.1 Implement the per-process namespace: a mount table with `mount`,
  `bind`, `unmount`, union directories, and `walk` resolution.
- [ ] 6.2 Implement namespace inheritance: a child receives a namespace
  constructed by its spawner and may only restrict its own view (D6).
- [ ] 6.3 Enforce no global ambient addressing: resolution is only through the
  namespace; opaque ids resolve within a namespace and are never a global
  capability (D6).
- [ ] 6.4 Implement the process table with one `Process` category — identity,
  parentage, credentials, namespace, lifecycle, status, exit state — and no
  `Agent Process` type (D3).
- [ ] 6.5 Keep the process table, namespaces, and fids as ephemeral runtime state
  that starts empty on restart (D7).

## 7. Synthetic devices (crate)

- [ ] 7.1 Implement `/proc` as a file server rendering the process table, with
  per-process `io/`, `status`, `ctl`, and standard files (D9); `/proc/<pid>` is
  the single source of truth.
- [ ] 7.1a Implement process creation (spawn) via clone-via-open on
  `/proc/clone`: write an exec spec (executable + args + child namespace), return
  the new pid, render `/proc/<pid>`. spawn is aP open+write, no side API. Spawn is
  capability-preserving: reject any exec-spec namespace entry/descriptor the
  spawner could not itself open or delegate (no amplification; D6).
- [ ] 7.2 Implement `/srv` as the bootstrap rendezvous device, access-filtered:
  posted handles carry access rights; a process sees/mounts only permitted
  handles; a withheld service is not remountable via `/srv` (D6).
- [ ] 7.3 Bring the kernel up with only `/proc`, `/srv`, and the namespace engine,
  leaving init / Service Manager to assemble the rest of the root namespace.

## 8. Crate cleanup (relocate the retired ontology)

- [ ] 8.1 Remove the retired modules from `alan-kernel`: `agent_capability`,
  `descriptors` (Object/Buffer/View/Command/Query/Subscription/Task/Artifact/
  Evidence), `views`, `ledger`, `registry`, `invocation`, and the V1 `ids`.
- [ ] 8.2 Relocate any V1 surfaces still needed during migration into a
  compat/app crate (for example `alan-compat`), never back into `alan-kernel`.
- [ ] 8.3 Extend `tests/dependency_boundary.rs` to fail if `alan-kernel` gains a
  dependency on `alan-runtime`, `alan-protocol`, provider clients, memory stores,
  sandbox backends, renderers, or async task handles — and to fail if the retired
  module names reappear (automated D9/D3 discipline).

## 9. Verification

- [ ] 9.1 Run focused `cargo test -p alan-kernel`.
- [ ] 9.2 Run `just verify`.
- [ ] 9.3 Re-run `openspec validate --all --strict`.

## 10. Downstream (other changes)

- [ ] 10.1 `define-agent-file-layout-contract` lands above this substrate.
- [ ] 10.2 `introduce-alan-kernel-runtime` builds the projection file server that
  depends on this crate.
- [ ] 10.3 Later slice: aP wire transport with network transparency
  (import/export remote trees) for distributed agents (ADR-0026 D1).
