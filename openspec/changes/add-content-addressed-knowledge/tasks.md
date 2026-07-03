## 1. Content-addressed store

- [x] 1.1 Implement a content-addressed block store (hash-named blocks,
  write-once, idempotent writes, dedup).
  Done 2026-07-02: added `alan-knowledge`, with `KnowledgeStore::put_block`
  storing raw blocks under `sha256:<hex>` content hashes. Re-writing identical
  bytes is idempotent and keeps one stored copy; tests cover cross-write dedup.
- [x] 1.2 Implement Merkle DAG assembly with root-hash checkpoints and
  block-sharing forks.
  Done 2026-07-02: `KnowledgeStore` builds Merkle DAG checkpoints from block
  hashes and forks by referencing the base checkpoint node plus delta blocks, so
  unchanged blocks are shared. Tests cover checkpoint materialization,
  verification, and fork block/node counts.
- [x] 1.3 Retrieve state through an authorized, namespace-bound root (gated by
  reachability + access rights, ADR-0024 D6), then use the root hash only for
  integrity verification — possessing a hash is never the authority to read.
  Done 2026-07-02: reads require a private `BoundRoot` created by
  `KnowledgeStore::bind_root`; `authorize_reachable_hash` rejects a bare hash
  until that root is live in the namespace model. Stale bindings fail after
  unbind, while `verify_root_hash` uses hashes only for integrity.

## 2. Retention and GC

- [x] 2.1 Implement reachability GC from live roots with a retention policy.
  Done 2026-07-02: `KnowledgeStore::collect_garbage` walks live root bindings
  plus pinned roots and, under `RetentionPolicy::CollectUnreachable`, removes
  unreachable DAG nodes and raw blocks. `RetentionPolicy::KeepUnreachable`
  preserves deferred retention behavior.
- [x] 2.2 Support pinning audited roots so GC does not drop them.
  Done 2026-07-02: `pin_root` and `unpin_root` keep audited checkpoint roots
  alive independently from namespace root bindings; tests verify pinned roots
  survive GC and become collectible after unpinning.

## 3. Backing the agent surfaces

- [x] 3.1 Back `memfs` (`/mnt/mem`) and the agent `machine/` surface with the
  store; expose tape/memory/context as file views over the DAG.
  Done 2026-07-02: added `alan-memfs`, a Memory Store file server whose ordinary
  files materialize from namespace-bound `alan-knowledge` checkpoint roots, and
  backed `agentfs` `machine/tape` writes with content-addressed blocks plus a
  current checkpoint root exposed at `machine/checkpoints/current`. Existing tape
  reads remain file-shaped and are verified against the checkpoint in tests.
- [x] 3.2 Back the durable home (ADR-0024 D7): resume from a root hash; ephemeral
  homes persist no roots.
  Done 2026-07-02: `alan-memfs` can rebind an existing checkpoint root into the
  same storage-backed home after the file's namespace authority is removed,
  while a fresh ephemeral store rejects the same root because it did not persist
  the backing blocks/nodes. Tests cover both durable resume and ephemeral
  non-resume behavior.
- [ ] 3.3 Map current tape/rollout/checkpoint persistence onto root-hash
  checkpoints in the `introduce-alan-kernel-runtime` projection.
  Partial 2026-07-02: `agentfs` exposes the current `machine/tape` root hash at
  `machine/checkpoints/current`, and rollout `CheckpointRecord`s carry an
  optional `knowledge_root` while preserving legacy JSON compatibility when
  absent. Remaining: production checkpoint recording still needs to read the
  namespace-native current checkpoint root and call the knowledge-root recorder
  path instead of `record_checkpoint_nowait`.

## 4. Verification

- [x] 4.1 Tests: dedup, cheap fork (block sharing), checkpoint restore, tamper
  detection, GC reachability + pinning.
  Done 2026-07-02: `crates/knowledge/tests/store.rs` covers identical-content
  dedup, checkpoint materialization/restore, block-sharing forks, hash-is-not-
  authority behavior, tamper detection by rehashing, and GC reachability plus
  audit pinning.
- [x] 4.2 Run `just verify`.
  Done 2026-07-02: `just verify` passed after the content-addressed knowledge,
  memfs, agentfs checkpoint backing, and rollout checkpoint-root mapping changes.
- [x] 4.3 Run `openspec validate add-content-addressed-knowledge --strict`.
  Done 2026-07-02: strict validation passed after the implementation and task
  updates.

## 5. Follow-up (separate change)

- [ ] 5.1 Speculative / branching agent execution over cheap forks (tree search
  across agent states).
