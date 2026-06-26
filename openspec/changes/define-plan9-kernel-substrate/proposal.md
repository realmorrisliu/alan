## Why

The kernel design carried two incompatible worldviews under one name: an older
"semantic runtime" ontology (Object / Buffer / View / Command / Query /
Subscription / Task / Artifact / Evidence / Journal / ViewModel, plus a
first-class Agent Process Kernel category) and a newer semantic-UNIX direction.
ADR-0024 reconciles them onto a single Plan 9-inspired model. This change makes
that model the durable `alan-kernel` contract, defined by positive construction
from UNIX/Plan 9 primitives rather than by subtraction from the old ontology.

The load-bearing reframing (ADR-0024): an LLM is a typed stream a process reads;
the agent is the process that consumes that stream and turns its content into
effects governed by its own namespace. Once that is taken seriously, the kernel
needs to own almost nothing agent-specific.

## What Changes

- Define `alan-kernel` as exactly: the namespace engine, the process table, and
  the `/proc` and `/srv` synthetic devices, depending on `alan-ap` (the fid /
  file-server contract; ADR-0025 D2), not owning it.
- Model one process category (`Process`); remove `Agent Process` as a Kernel
  type. Agent-ness becomes a file-layout convention owned by
  `define-agent-file-layout-contract`, not a Kernel category.
- Define the file-server contract as wire-shaped (fid-based, byte/offset,
  error-coded) so it can later cross a process boundary, while v1 runs built-in
  servers over an in-process fast path.
- Make the per-process namespace the sole capability boundary, with no global
  ambient addressing; opaque ids must resolve within a namespace.
- Make the kernel ephemeral: persistence is a property of storage-backed file
  servers, not Kernel state.
- Model streams as byte/offset file kinds and observation as a blocking read on
  an events stream, with no second event system.

This change supersedes `add-agent-process-kernel-types` and the
`alan-kernel-contract` spec inside `introduce-alan-kernel-runtime`, which encode
the retired ontology.

## Capabilities

### New Capabilities

- `plan9-kernel-substrate`: the durable `alan-kernel` contract — namespace,
  mounts, bind/union, paths, files, byte/offset streams, fids, the file-server
  protocol shape, the single Process category, the process table, `/proc`,
  `/srv`, namespace-as-capability, and kernel ephemerality.

### Modified Capabilities

- None. (Supersessions are handled by removing the retired delta specs.)

## Impact

- Affected architecture: `alan-kernel` (the crate) keeps only namespace engine +
  process table + `/proc` + `/srv` (depending on `alan-ap` for the contract), with
  no dependency
  on agent, llm, provider, tape, memory, or runtime code. It becomes the crate
  that changes least.
- Affected planning: the agent runtime, LLM providers, memory, and tools are all
  user-space file servers above this substrate, specified by separate changes.
- Affected existing changes: `add-agent-process-kernel-types` is superseded and
  removed; the `alan-kernel-contract` spec in `introduce-alan-kernel-runtime` is
  cut down to a superseded pointer.
- Affected ADRs: implements ADR-0024; builds on 0005/0014/0017/0018/0019/0020/
  0023; amends 0016; supersedes the Agent-Process-as-Kernel-type framing of 0004
  and `add-agent-process-kernel-types`.
