## Context

`Session::load_from_rollout*` already uses matching rollout checkpoint records
to distinguish runtime confirmation control payloads from ordinary user turns
during recovery. The runtime resume path in
`crates/agent-engine/src/runtime/submission_handlers.rs` currently appends the
synthetic control message but does not persist any checkpoint record. Separately,
the namespace-native runtime already exposes the current `machine/tape` root via
`NamespaceRuntimeEnvironment::current_tape_checkpoint`, while rollout checkpoint
records can already carry an optional `knowledge_root`.

## Goals / Non-Goals

**Goals:**
- Persist a matching rollout checkpoint record whenever runtime confirmation
  control is resolved through `Op::Resume`.
- Record the current namespace `machine/tape` root hash on that checkpoint when
  available.
- Preserve current resume behavior if the namespace checkpoint cannot be read.

**Non-Goals:**
- Do not change mount escalation persistence or other non-runtime-confirmation
  checkpoint types.
- Do not mirror synthetic control messages into namespace `machine/tape`.
- Do not redesign rollout checkpoint taxonomy or session recovery rules beyond
  this missing persistence link.

## Decisions

### 1. Persist runtime confirmation checkpoints at confirmation resolution

The runtime will persist the checkpoint record in
`handle_confirmation_resolution` immediately after it appends the synthetic user
control payload for runtime confirmation types. This is the narrowest place
that already knows the checkpoint id, type, chosen resolution, and whether the
message is a runtime confirmation control payload.

Alternative considered: persisting at yield creation time. Rejected because the
restore issue is tied to the resolved synthetic control message, and pending
confirmation state already has separate request files and turn-state handling.

### 2. Read the current namespace tape root on a best-effort basis

The runtime will read the current namespace `machine/tape` root hash through
`state.namespace_environment().current_tape_checkpoint().await` and pass that
hash into rollout checkpoint persistence when available.

Alternative considered: failing resume if the checkpoint root cannot be read.
Rejected because checkpoint-root linkage is useful audit metadata, but resume
flow should not become unavailable because a best-effort durability hint cannot
be loaded.

### 3. Keep rollout writing behind `Session`

`Session` will gain a helper that persists a checkpoint with an optional
`knowledge_root`, while the existing `record_checkpoint` API remains a no-root
wrapper. This keeps rollout-writing semantics centralized in `Session` and lets
runtime code supply the optional root without reaching into `RolloutRecorder`
directly.

Alternative considered: calling `RolloutRecorder` directly from submission
handlers. Rejected because it would split rollout persistence behavior across
layers that already route through `Session`.

## Risks / Trade-offs

- `knowledge_root` may describe the namespace tape state before the synthetic
  control payload, because runtime confirmation control messages are not mirrored
  into `machine/tape` today. → Mitigation: treat `knowledge_root` as the agentfs
  tape checkpoint anchor, not as a byte-for-byte mirror of the session tape.
- Best-effort root reads can fail in narrow test or degraded runtime setups. →
  Mitigation: persist the checkpoint record without `knowledge_root` and keep the
  resume path running.
