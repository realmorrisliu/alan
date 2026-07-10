## Context

`alan-llmfs` already separates Provider from callable Connection and models each
Generation as `clone`, `data`, `events`, `status`, and `ctl`. Agent requests are
assembled from namespace files; changing model means binding a different
Connection. AgentFS already exposes machine state and events as files.

The old cognitive-routing design predated those surfaces. It selected providers
inside one runtime loop, gave System 1 an internal virtual escalation Tool,
withheld side effects through runtime classification, and spread result metadata
across session/fork/reconnect DTOs. That made the daemon and in-process engine the
architectural center.

## Goals / Non-Goals

**Goals:**

- Support configurable fast and deep LLM Connections with automatic and explicit
  routing.
- Make every attempt, Connection role, escalation, and accepted result
  inspectable through `/proc`, `/agent`, `/mnt/llm`, and stream files.
- Structurally prevent speculative System 1 attempts from performing
  side-effecting Tool actions.
- Preserve provider-neutral reasoning controls and explicit continuation
  compatibility.
- Present one coherent parent Agent Process result while retaining attempt
  provenance.

**Non-Goals:**

- Add cognitive roles to Alan Kernel or provider adapters.
- Run System 1 and System 2 in parallel by default.
- Expose chain-of-thought or provider-private reasoning.
- Create daemon routing endpoints, session override DTOs, or a global model
  router service.
- Let a System 1 child acquire mounts withheld by its spawner.

## Decisions

### 1. Cognitive roles are namespace aliases to LLM Connections

Agent configuration names two connection profiles and optional reasoning effort
intent. During process construction, the spawner resolves them and binds callable
Connection trees at stable role aliases in the coordinator namespace, for
example:

```text
/mnt/llm/cognition/system-1  -> bound llmfs Connection
/mnt/llm/cognition/system-2  -> bound llmfs Connection
```

These aliases are local namespace bindings, not new globally callable llmfs
objects. Credentials remain inside the Connection. Missing or unauthorized role
mounts make that route unavailable.

Alternative considered: let runtime hold provider/model/credential structs and
dispatch directly. Rejected: model capability would no longer equal mounted
Connection reachability.

### 2. Each attempt is an ordinary inspectable process

The coordinating Agent Process owns the logical user task and routing state. It
spawns a System 1 Agent Process with a bounded task descriptor, one active LLM
Connection, read-only context mounts, and a `/bin` union containing only
read-only Tools. If a deeper route is required, it spawns a System 2 Agent
Process with the explicitly authorized namespace for that attempt.

Attempt processes appear in `/proc`, conform under `/agent`, stream output
through `io/output`, and return bounded result/provenance through the parent
action record. They are sequential by default. The parent does not merge hidden
transcripts; it records which attempt produced the accepted result.

Alternative considered: rebind Tools inside one long-lived Process between
phases. Rejected for V1: retained descriptors and in-process state make the
speculative side-effect boundary harder to audit than a freshly constructed
child namespace.

### 3. Routing precedence is explicit and file-visible

The coordinator applies:

1. an explicit `system-2` next-attempt intent;
2. deterministic safety/complexity rules requiring System 2;
3. an eligible explicit `system-1` next-attempt intent;
4. the configured default role;
5. System 1 fallback with self-escalation available.

The agent-runtime-owned `machine/ctl` accepts routing commands such as `route
next system-1`, `route next system-2`, and `route auto`; `machine/routing/`
carries state and events but no `ctl` file, keeping the agent overlay's control
surfaces to the two defined by `agent-file-layout-contract` (`/proc/<pid>/ctl`
and `machine/ctl`). `route next` is consumed by the next logical input. A
deterministic System 2 gate may refuse `route next system-1`; the refusal is
recorded in routing status/events. Compatibility transports may translate an old
override into the same `machine/ctl` write but gain no independent semantics.

### 4. Escalation is typed stream content, not a Tool

System 1 receives a provider-neutral instruction that may emit a typed
`route/escalate` record containing a bounded reason and needed-context labels.
The record has no authority. The coordinator reads it from the attempt's output
stream, records it under `machine/routing`, and decides whether to spawn System
2. No `escalate_to_system2` executable or virtual Tool appears in `/bin`.

If System 1 completes without escalation, its result may be accepted only if the
task required no withheld effects. Proposed mutations remain parent-visible
action proposals executed later under normal governance.

### 5. Routing state lives under machine/routing

AgentFS projects:

```text
machine/routing/
├── config       # role aliases, mode, default; secrets absent
├── status       # idle/running/escalating/completed/failed
├── current      # attempt pid, role, Connection alias, bounded reason
├── result       # accepted attempt reference and outcome metadata
└── events       # offset-resumable ordered records
```

Clients hydrate snapshots and block-read events. Rollout/tape records may carry
the same bounded references, but no daemon DTO is a second source of truth.
Routing control remains on `machine/ctl` through the `route auto` and
`route next <role>` commands defined above; `machine/routing` is read-only.

### 6. Request controls compose after Connection selection

The attempt chooses its bound Connection first. Canonical reasoning-effort intent
is then validated against that Connection's model metadata and written into the
provider-neutral llmfs Generation request. `alan-llmfs` maps the request to
`alan-llm`; provider adapters only project normalized controls.

### 7. Continuation is partitioned by observable compatibility

Tape-level accepted context remains provider-neutral. Provider-native
continuation may be reused only when Connection identity, model, credential
scope, prompt fingerprint, visible Tool manifest fingerprint, cognitive role,
and relevant request controls match. A role switch normally starts a fresh
Generation and reprojects accepted context.

## Risks / Trade-offs

- [Risk] Process-per-attempt adds latency → Mitigation: process/file-server paths
  are local and sequential; measure before introducing hidden in-process phases.
- [Risk] Parent result presentation exposes internal complexity → Mitigation:
  default UI shows one accepted result while routing files preserve inspection.
- [Risk] Deterministic gates become policy sprawl → Mitigation: keep a bounded,
  testable V1 set and record every forced route reason.
- [Risk] System 1 uses a writable mount accidentally → Mitigation: assert the
  spawned namespace and `/bin` union before first model Generation.
- [Risk] Provider continuation leaks System 1 prompt/tools into System 2 →
  Mitigation: default fresh Generation on role change and strict fingerprints.

## Migration Plan

1. Add cognitive role alias resolution and `machine/routing` files without
   changing the single-Connection default.
2. Add restricted System 1 attempt spawn and accepted-result handoff.
3. Add deterministic gates and explicit `route` intent on `machine/ctl`.
4. Add typed stream escalation and sequential System 2 attempt spawn.
5. Remove cognitive session/fork/turn DTO deltas and internal virtual escalation
   Tool assumptions.
6. Delete the compatibility mirror after shipped clients read routing files.

## Open Questions

- Which deterministic gates belong in the first policy set; the mechanism does
  not depend on the final list.
- Whether a later optimized same-Process attempt is worth adding after namespace
  equivalence can be proven; it is not the V1 contract.
