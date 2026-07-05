## Why

Runtime confirmation resumes currently persist only the synthetic control
message in rollout history. They do not emit matching checkpoint records, and
they never attach the current namespace `machine/tape` root hash. That leaves
rollout recovery without the durable checkpoint evidence it already expects for
runtime confirmation control payloads and leaves the content-addressed
checkpoint linkage unfinished.

## What Changes

- Persist a rollout checkpoint record when a runtime confirmation resume resolves
  a `tool_escalation` or `effect_replay_confirmation` checkpoint.
- Attach the current namespace `machine/tape` root hash to that checkpoint
  record when the runtime can read it.
- Fall back to persisting the checkpoint record without `knowledge_root` if the
  namespace checkpoint cannot be read, without blocking resume flow.
- Add regression coverage for both the knowledge-root and fallback paths.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `runtime-core-contract`: runtime confirmation resumes now require matching
  rollout checkpoint records and best-effort linkage to the current namespace
  tape checkpoint root.

## Impact

- Affects `crates/agent-engine/src/runtime/submission_handlers.rs` checkpoint
  persistence during `Op::Resume`.
- Affects `crates/agent-engine/src/session.rs` rollout checkpoint helper APIs.
- Adds regression tests around rollout persistence and recovery prerequisites
  for runtime confirmation control messages.
