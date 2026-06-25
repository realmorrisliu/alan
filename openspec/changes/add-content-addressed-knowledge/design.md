## Context

ADR-0026 D3 adopts Venti's content-addressing for agent knowledge, with the
explicit departure of using git-style GC rather than Venti's never-delete. This
change specifies that backing model for the durable home, `machine/tape`, and
memory. It does not change agent-facing file surfaces — those stay as in
`define-agent-file-layout-contract`.

## Goals / Non-Goals

**Goals:**

- Content-addressed, immutable blocks with dedup.
- Root-hash checkpoints and cheap forks (shared unchanged blocks).
- Tamper-evident, verifiable history.
- Reachability GC + retention so volume stays bounded.

**Non-Goals:**

- Change agent-facing file surfaces (tape/memory/context stay files; this is the
  backing store).
- Adopt Venti's never-delete immortality.
- Implement a wire transport or remote store (rides ADR-0024 D5 later).
- Implement the speculative-execution scheduler (this only makes forks cheap;
  branching strategy is a separate concern).

## Decisions

Implements [ADR-0026](../../../docs/adr/0026-plan9-application-ideas-for-agents.md)
D3; refines [ADR-0024](../../../docs/adr/0024-plan9-kernel-model.md) D2/D7.

- **Blocks are content-addressed.** A block's name is the hash of its bytes;
  writing identical bytes is idempotent (dedup).
- **State is a Merkle DAG; a checkpoint is a root hash.** `machine/tape` and a
  memory version are DAGs of blocks; a checkpoint/snapshot is the root hash. A
  fork shares all unchanged blocks and only writes the delta.
- **Identity is verifiable.** Any state is retrievable and verifiable by its root
  hash; rewriting history changes the hash, so audit is tamper-evident.
- **GC by reachability, with retention.** Unreachable blocks (no live root, past
  retention) are collected. Retention/GC policy is the storing server's, not the
  kernel's (ADR-0024 D7: persistence belongs to file servers).
- **File surfaces are views.** Reading `machine/tape` materializes a view from
  the DAG; the content-addressed store is the backing model, not a new API.

## Risks / Trade-offs

- **Volume.** Agent token volume is large; GC + retention are mandatory, and
  retention tuning is a real operational concern (mitigates the Venti
  never-delete trap).
- **Hash cost.** Hashing every block has CPU cost; acceptable for the dedup,
  cheap-fork, and audit benefits, and v1 is in-process.
- **GC vs audit tension.** GC can drop history that audit might want; retention
  policy must reconcile the two (e.g. pin audited roots).

## Migration Plan

1. Land the store behind `memfs` and the agent `machine/` surface.
2. Map current tape/rollout/checkpoint persistence onto root-hash checkpoints in
   the `introduce-alan-kernel-runtime` projection.
3. Expose cheap fork as the basis for a later speculative-execution change.
