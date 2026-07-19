## Why

The Agent Execution Engine now has the correct file-native external boundary, but
transition state is still spread across `RuntimeLoopState`, `AgentMachine`, and a
large environment facade. This obscures the actual Machine owner and makes an
otherwise local transition change require broad runtime knowledge.

## What Changes

- Make Agent Machine the sole owner of Tape and transition-local state, including
  the current submission, turn state, pending Yield, Tool replay, and deferred
  transition action.
- Separate the outer Process loop (input polling, shutdown, cancellation, and
  heartbeat) from a cohesive transition module that advances one accepted
  submission and returns a compact outcome.
- Keep one concrete namespace handle at the transition boundary and pass narrow
  values to child modules instead of the entire runtime state or environment.
- Remove public field-bag access to Agent Machine internals and keep Agent Machine
  an engine-internal implementation detail.
- Preserve all AgentFS, `/proc`, aP, file-layout, persistence, recovery,
  confirmation, compaction, memory, Tool, and child-process behavior.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `agent-namespace-runtime`: Define the internal Agent Machine ownership boundary
  and the separation between Process-loop control and transition execution.

## Impact

This behavior-preserving refactor affects `crates/agent-engine/src/engine.rs` and
the runtime modules that currently reach through `RuntimeLoopState`,
`AgentMachine`, and `NamespaceRuntimeEnvironment`. It introduces no new public
API, dependency, file surface, compatibility path, or product behavior. It is
the prerequisite for `complete-agent-runtime-file-native-seam`.
