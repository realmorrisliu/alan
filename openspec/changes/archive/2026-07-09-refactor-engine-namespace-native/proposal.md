## Why

Alan is a deliberate, ground-up refactor toward the Plan 9 model (ADR-0024): an
LLM is a typed stream a process reads, and the agent is the process that consumes
that stream and turns it into effects governed by its own namespace. The Agent
Execution Engine (`alan-agent-engine`) still embodies the pre-Plan-9 worldview:
its transition function is a `Box<dyn LlmProvider>` call, its side effects go
through an in-process `ToolRegistry`, and its state is published by emitting the
`alan-agent-protocol` `Event`/`EventEnvelope` alphabet. The first projection
slices papered over this with adapters — `alan-llmfs-client::FileLlmProvider`
(files → `LlmProvider` trait → engine) and `alan-agentfs` (engine events →
files). In a thorough refactor those adapters are exactly the compatibility
bridges the Architecture Progression Principle warns against: they preserve the
old abstractions instead of replacing them.

This change makes the engine **namespace-native**: its only environment is its
namespace, and the three Turing-machine concerns become file operations on it.

## What Changes

- **BREAKING**: The engine's environment becomes a single namespace handle (an
  aP client over its mounted root), replacing injected `provider`/`tools`/event
  emission. An agent's capabilities are exactly what its spawner mounts.
- **Generation** becomes a namespace file operation: open
  `/mnt/llm/connections/<conn>/clone`, write the request to `data`, read the
  token stream from `events`. The `LlmProvider` trait is removed from the
  engine's call path.
- **Tools** become `/bin` executables invoked via `/proc/clone` spawn + reading
  the tool process's output files, replacing the in-process `ToolRegistry`.
- **State/output** is written directly to the agent's `/agent/<pid>` files
  (`io/output`, `machine/tape`, `requests/`, `actions/`); the engine no longer
  emits the `Event`/`EventEnvelope` alphabet as its publication mechanism.
- An agent runs as a `Process` spawned via `/proc/clone` with a spawner-assembled
  namespace (mounted llm connection + tools = its capability set; D6).
- **BREAKING**: `alan-llmfs-client::FileLlmProvider` (the `LlmProvider` adapter)
  is retired — its clone/write/read-events client logic moves into the engine's
  generation step.
- `alan-agentfs` is re-aimed from an `EventEnvelope` projector to the file
  backing of agent state the engine writes; the `EventEnvelope` projection path
  is removed from the live model (kept only as legacy compatibility transport per
  ADR-0025 D4, not the engine's publication path).
- M2 (the shell talking to a real agent) is delivered as the natural result of
  the above — `io/input` → engine reads → generate via `/mnt/llm` → write
  `io/output` → shell tails — with no RPC and no provider injection.

## Capabilities

### New Capabilities

- `agent-namespace-runtime`: the durable contract that the Agent Execution
  Engine's transition function (generation), side effects (tools), and state
  publication (output/tape/requests/actions) are namespace file operations over
  a single mounted root, and that an agent's capabilities are exactly its mounted
  namespace.

### Modified Capabilities

- None as a merged-spec delta. `alan-agent-adapter-contract` is still owned by the
  open, unarchived `introduce-alan-kernel-runtime` change (not in
  `openspec/specs/`), so its re-aiming from an `EventEnvelope` projector to the
  file backing of engine-written state is handled by superseding that change here,
  not by a spec delta.

## Impact

- **Largest blast radius so far**: rewrites `alan-agent-engine` (the `agent_loop`
  / `RuntimeLoopState` core), the deepest existing crate, plus its tests.
- Retires `alan-llmfs-client` ([#579](https://github.com/realmorrisliu/Alan/pull/579))
  as an `LlmProvider` adapter; re-works `alan-agentfs`
  ([#576](https://github.com/realmorrisliu/Alan/pull/576)).
- Supersedes the adapter framing of `introduce-alan-kernel-runtime`: that change
  is folded into this one for the engine-native rewrite (its remaining deferred
  tasks — live wiring, io/input resume, overlay — are owned here).
- Pure-substrate PRs are unaffected: `alan-ap` (#573), `alan-kernel` (#574), the
  rename (#575), `alan-shell` (#577), `alan-llmfs` server (#578) stand.
- The `LlmProvider` trait and `alan-agent-protocol` remain only as legacy
  compatibility transport behind file servers, never on the engine's live path.
