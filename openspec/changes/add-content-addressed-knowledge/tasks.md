## 1. Content-addressed store

- [ ] 1.1 Implement a content-addressed block store (hash-named blocks,
  write-once, idempotent writes, dedup).
- [ ] 1.2 Implement Merkle DAG assembly with root-hash checkpoints and
  block-sharing forks.
- [ ] 1.3 Add integrity verification (retrieve + verify state by root hash).

## 2. Retention and GC

- [ ] 2.1 Implement reachability GC from live roots with a retention policy.
- [ ] 2.2 Support pinning audited roots so GC does not drop them.

## 3. Backing the agent surfaces

- [ ] 3.1 Back `memfs` (`/mnt/mem`) and the agent `machine/` surface with the
  store; expose tape/memory/context as file views over the DAG.
- [ ] 3.2 Back the durable home (ADR-0024 D7): resume from a root hash; ephemeral
  homes persist no roots.
- [ ] 3.3 Map current tape/rollout/checkpoint persistence onto root-hash
  checkpoints in the `introduce-alan-kernel-runtime` projection.

## 4. Verification

- [ ] 4.1 Tests: dedup, cheap fork (block sharing), checkpoint restore, tamper
  detection, GC reachability + pinning.
- [ ] 4.2 Run `just verify`.
- [ ] 4.3 Run `openspec validate add-content-addressed-knowledge --strict`.

## 5. Follow-up (separate change)

- [ ] 5.1 Speculative / branching agent execution over cheap forks (tree search
  across agent states).
