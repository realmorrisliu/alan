## Why

ADR-0024 D7 makes an agent's durable identity a home tree on a storage-backed
file server, and D2 makes `machine/tape` the truth that the model request is a
view over. ADR-0026 D3 sharpens how that storage should work by adopting Venti's
idea: content-addressed, immutable knowledge. This turns checkpoints, forks,
dedup, and audit from features we would build into properties we get for free.

## What Changes

- Define a content-addressed knowledge store: agent knowledge blocks (tape,
  memory, context) are stored under the hash of their content, write-once, with
  identical content stored once (dedup).
- Define checkpoints and forks as root hashes over a Merkle DAG of those blocks:
  a checkpoint is a root hash; forking an agent from any checkpoint is cheap and
  shares unchanged blocks.
- Define a tamper-evident audit property: because identity is the content hash,
  an agent's history is verifiable and cannot be silently rewritten.
- Define reachability-based garbage collection and retention — NOT Venti's
  never-delete — so agent-scale volume stays bounded.
- Reshape the durable home/persistence model (D7): the home is backed by this
  store; `machine/tape` resumes from a root hash; `memfs` is backed by it.
- Keep the file surfaces unchanged: tape/memory/context are still read as files
  (materialized views over the DAG); content addressing is the backing model, not
  a new agent-facing API.

## Capabilities

### New Capabilities

- `content-addressed-knowledge`: the content-addressed, immutable, GC'd knowledge
  store backing agent home/tape/memory — root-hash checkpoints, cheap forks,
  dedup, tamper-evident audit, and retention/GC.

### Modified Capabilities

- None. (Refines ADR-0024 D7 persistence; the file-layout contract is unchanged.)

## Impact

- Backs `memfs` (`/mnt/mem`) and the agent `machine/` surfaces; depends on the aP
  protocol and the agent file-layout contract.
- Enables cheap forking, which makes speculative / branching agent execution
  (tree search over agent states) practical.
- ADRs: implements ADR-0026 D3; refines ADR-0024 D2/D7.
