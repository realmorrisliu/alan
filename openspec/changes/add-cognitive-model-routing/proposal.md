## Why

Alan should normally use a fast model path and deliberately escalate complex or
high-cost work to a deeper model. The previous change hid that behavior inside a
runtime `CognitiveRouter`, session overrides, daemon DTOs, and an internal virtual
action; the Plan 9-like design requires model choice, attempt isolation,
escalation, and observability to be expressed through mounted LLM Connections,
Processes, files, and streams.

## What Changes

- Bind configured System 1 and System 2 LLM Connections into the coordinating
  Agent Process namespace under stable cognitive-role aliases.
- Represent each routed attempt as an inspectable Process/Generation rather than
  an invisible provider-dispatch phase.
- Give speculative System 1 attempts a structurally restricted namespace with
  read-only context and no side-effecting Tool bindings; System 2 receives only
  the mounts explicitly assembled for the deeper attempt.
- Treat System 1 escalation output as a typed suggestion in its model stream;
  the coordinator records the decision and spawns the System 2 attempt. It is not
  a privileged virtual Tool.
- Project routing configuration, current attempt, bounded reason, result,
  status, and ordered events under `machine/routing/` in the agent overlay.
- Accept explicit next-attempt/default control through the owning routing `ctl`;
  remove new session/fork/turn daemon DTO requirements.
- Compose the selected LLM Connection with canonical reasoning-effort controls
  in the provider-neutral llmfs Generation request. Provider adapters remain
  unaware of cognitive roles.
- Preserve provider-native continuation only within a compatible Connection,
  prompt fingerprint, visible Tool set, and attempt role.

## Capabilities

### New Capabilities

- `cognitive-model-routing`: Defines cognitive-role Connection mounts,
  restricted attempt Processes, routing precedence, stream-based escalation,
  file-backed observability, explicit `ctl` intent, and continuation boundaries.

### Modified Capabilities

- `provider-request-controls`: Reasoning controls compose with the selected
  cognitive-role LLM Connection and remain provider-neutral Generation input.

## Impact

- Agent Runtime Service/AgentFS gains the `machine/routing/` projection and
  coordinates ordinary child Process/Generation lifecycles.
- `alan-llmfs` remains the callable model boundary at `/mnt/llm`; configured
  Connections are mounted under cognitive-role aliases and attempts receive one
  active Connection.
- Side-effect isolation is enforced by the attempt namespace and visible `/bin`
  union, not a runtime-only Tool classification gate.
- Daemon compatibility surfaces may mirror routing files temporarily, but this
  change adds no daemon/session API requirements and remote clients use the
  returned namespace.
