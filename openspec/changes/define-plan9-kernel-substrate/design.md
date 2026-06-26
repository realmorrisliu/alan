## Context

This change is the positive-construction kernel contract anchored by
[ADR-0024](../../../docs/adr/0024-plan9-kernel-model.md). It replaces the
subtractive `alan-kernel-contract` spec (which defined the kernel by listing a
V1 ontology and declaring each piece "above kernel") with a spec that builds the
kernel up from a small set of Plan 9 primitives and never names the retired
concepts at all.

The scope here is deliberately the *substrate only*. The agent file-layout
convention (io / status / ctl / requests / actions / machine, the LLM-as-stream
consumer model, tools as `/bin` projection) is owned by the sibling change
`define-agent-file-layout-contract` and sits above this substrate.

## Goals / Non-Goals

**Goals:**

- Define the kernel as namespace engine + process table + `/proc` + `/srv`
  (depending on `alan-ap` for the fid/file-server contract), and nothing else.
- Define one process category and the namespace-as-capability boundary.
- Define streams as byte/offset file kinds and observation as blocking read.
- Keep the file-server contract wire-shaped so a later transport slice can move
  servers out of process without changing the contract.

**Non-Goals:**

- Implement a 9P wire transport (later slice; v1 is in-process fast path).
- Define agent files, LLM streams, tools, skills, memory, or policy (those are
  user-space file servers / the agent file-layout contract).
- Implement Service Manager or boot sequencing beyond naming the bootstrap roots
  it must assemble.
- Provide hardware/process isolation (see Risk R1).

## Decisions

All decisions are recorded in [ADR-0024](../../../docs/adr/0024-plan9-kernel-model.md);
this change implements D3, D5, D6, D7, and D9 directly, and provides the kernel
primitives that D1/D2/D4/D8 build on above the substrate.

- D3 → one `Process` category; no `Agent Process` Kernel type.
- D5 → wire-shaped file-server contract; in-process fast path v1.
- D6 → per-process namespace is the sole capability boundary; no global
  ambient addressing.
- D7 → kernel is ephemeral; persistence belongs to storage-backed file servers.
- D9 → kernel synthesizes only namespace, `/proc`, and `/srv`.

## Risks / Trade-offs

- **R1 (from ADR-0024): the capability boundary is convention-enforced, not
  isolation-enforced, in v1.** Because v1 runs all file servers in one address
  space, D6's guarantee depends on the later cross-process transport slice. Any
  spec or doc claiming the security model must restate this.
- **R2: observation is Plan 9's known weak spot.** Blocking-read-per-watcher is
  cheap in-process but a held connection per watcher over the wire. We ship the
  pure model first and keep "watch = read on an events stream" as the semantic
  so any future fix is a transport optimization, not a new event system.

## Migration Plan

1. Land this substrate spec as the durable kernel contract.
2. Remove `add-agent-process-kernel-types` (superseded).
3. Cut `introduce-alan-kernel-runtime`'s `alan-kernel-contract` spec down to a
   superseded pointer so there is a single kernel owner.
4. Land `define-agent-file-layout-contract` above this substrate.
5. Create the `alan-kernel` crate for this contract in a later implementation
   change (there is no current `alan-kernel` crate — the V1 one was removed); any
   compatibility code that must move comes from the actual current owners
   (`alan-runtime`, `alan-protocol`, `crates/alan`, `crates/tui`) into the
   projection or `alan-compat`, not the kernel.
