# governance-tooling-contract Specification

## Purpose
Defines durable governance and tooling boundaries for policy decisions, tool
identity, runtime binding, capability routing, extension points, and workspace
scoping.

## Requirements
### Requirement: Governance and tooling contracts live in OpenSpec
alan SHALL specify HITE governance, policy decisions, tool catalog identity,
runtime tool binding, capability routing, extension points, and workspace
routing in OpenSpec.

#### Scenario: Governance behavior changes
- **WHEN** a change modifies policy `allow`, `deny`, or `escalate` semantics,
  execution-backend boundaries, owner-boundary classes, audit requirements, or
  approval/resume behavior
- **THEN** the change updates this capability,
  `evidence-retention-and-projection`, `delegation-capability-alignment`,
  `agent-file-layout-contract`, `agent-runtime-ui-file-surfaces`, or another
  active OpenSpec owner

#### Scenario: Tool binding behavior changes
- **WHEN** a change modifies tool catalog entries, runtime binding, locality,
  workspace scoping, child-runtime tool materialization, or extension routing
- **THEN** the behavior is specified in OpenSpec before it is documented as
  current guidance

### Requirement: Tool identity is separate from execution binding
alan SHALL keep stable tool catalog definitions separate from per-runtime
execution binding such as workspace root, current directory, profile exposure,
and policy decisions.

#### Scenario: Runtime exposes a tool
- **WHEN** a runtime registers or exposes a tool to an agent
- **THEN** the tool's identity, schema, and locality come from the catalog
- **AND** workspace-specific execution facts come from runtime context and
  policy

#### Scenario: Delegated capability is selected
- **WHEN** alan routes work to a delegated skill or child target
- **THEN** capability matching and mismatch recovery are observable through the
  OpenSpec-defined routing surface

### Requirement: HITE governance semantics are stable
alan SHALL implement Human-in-the-End governance as authorization over
boundaries and outcomes rather than as step-by-step micromanagement.

Core flow:

```text
Human Defines -> Agent Executes -> Human Owns
```

Governance order:

1. classify tool capability as `read`, `write`, `network`, or `unknown`
2. evaluate `PolicyEngine` rule match
3. apply policy action `allow`, `deny`, or `escalate`
4. if execution proceeds, allow the host execution backend to apply additional
   defense-in-depth restrictions

Stable action meanings:

- `allow` means the action is authorized.
- `deny` means the action is not authorized and execution does not happen.
- `escalate` means the action crosses an owner boundary and requires explicit
  human judgment through `Yield` and `Resume`.

Rules:

- There is no approval-policy downgrade path for governance V2.
- Policy files replace builtin profile rules for a session; they are not
  implicitly merged.
- `tool_escalation` is reserved for `PolicyEngine::Escalate`.
- Runtime side-effect replay confirmations use
  `effect_replay_confirmation`.
- Governance remains meaningful even when the host has only the current
  best-effort `workspace_path_guard` backend.

#### Scenario: Policy allows an action
- **WHEN** policy returns `allow`
- **THEN** the action is authorized for execution through the available backend
- **AND** the backend may still reject host-level unsafe execution shapes

#### Scenario: Policy denies an action
- **WHEN** policy returns `deny`
- **THEN** execution does not happen and the denial is returned as a recoverable
  boundary result
- **AND** the agent may replan but must not route around the boundary in bad
  faith

#### Scenario: Policy escalates an action
- **WHEN** policy returns `escalate`
- **THEN** runtime emits a recoverable `Yield` with decision context and waits
  for explicit `Resume`

### Requirement: Governance boundary classes and risk dimensions are explicit
alan SHALL classify governance boundaries by routine, sensitive, and owner
boundary levels with explicit risk dimensions.

Boundary classes:

- **Level A Routine**: low-risk, reversible, local actions. Default policy:
  `allow`.
- **Level B Sensitive**: side effects may affect quality, cost, or external
  state. Default policy: constrained `allow`, `deny`, or `escalate` depending
  on trust boundary and blast radius.
- **Level C Owner Boundary**: high-risk, irreversible, externally visible, or
  ownership-sensitive actions. Default policy: `escalate`, or `deny` when
  outside declared intent.

Typical owner boundaries:

1. production release or deploy
2. destructive data deletion
3. real payments
4. force-push or history rewrite outside the agent working branch
5. push to `main`
6. sharing data to a new external destination
7. security-posture changes
8. actions whose target was inferred rather than explicitly grounded

Risk dimensions:

1. capability type
2. target scope
3. trust boundary
4. blast radius
5. reversibility
6. cost or budget impact
7. authorization clarity

Recommended policy-as-code fields:

1. `risk_level`
2. `trust_boundary`
3. `owner_boundary`
4. `requires_owner`
5. `max_impact`
6. `budget_guard`

Ambiguous authorization must not silently become permission.

#### Scenario: Action target is inferred and high impact
- **WHEN** alan cannot determine whether the user authorized the real blast
  radius or target of a high-impact action
- **THEN** policy denies or escalates rather than inferring permission

### Requirement: Governance yields include decision context and auditability
alan SHALL emit enough context for a real owner decision when policy enters an
escalation path.

Escalation yield payloads include:

1. `request_id`
2. `action_summary`
3. `risk_reason`
4. `boundary_type`
5. `suggested_options`
6. optional constraints or safer alternatives

Resume decisions include explicit allow/deny and may include constraints. No
silent downgrade is allowed once boundary flow starts.

Every governance decision is traceable through rollout/events with:

1. `policy_source`
2. `rule_id`
3. `risk_level` when available
4. `action`
5. `reason`
6. capability classification
7. trust or owner-boundary context when available
8. effective execution backend
9. resolver (`policy` or `human`)
10. side-effect references or outcome summary when relevant

#### Scenario: Human resolves escalation
- **WHEN** a human resumes an escalated action
- **THEN** the rollout records the request, resolver, resolution, constraints,
  and effective backend context

### Requirement: Policy files and execution backends remain separate
alan SHALL distinguish authorization policy from physical execution
containment.

Policy resolution for runtime sessions follows agent-root governance rules and
may use explicit `governance.policy_path`. Current policy file shape supports:

```yaml
rules:
  - id: deny-prod-delete
    tool: bash
    match_command: "kubectl delete"
    action: deny
    reason: protect shared production state

  - id: escalate-publish
    tool: bash
    match_command: "git push origin main"
    action: escalate
    reason: owner boundary for publish

default_action: allow
```

Current matcher surface:

1. `tool`
2. `capability`
3. `match_command`
4. `match_path_prefix`

Rules:

- `PolicyFile` currently deserializes `rules` and `default_action`; extra
  fields are compatibility-forward and may be ignored until implemented.
- The current backend is `workspace_path_guard`, a best-effort guard rather
  than strict OS containment.
- Optional stronger containment backends are additive host capabilities and do
  not redefine HITE governance.
- Presence of containment does not replace policy.
- Lack of strict containment does not mean lack of governance.

#### Scenario: Strong containment is unavailable
- **WHEN** the host only has `workspace_path_guard`
- **THEN** policy semantics still decide authorization
- **AND** docs and events do not claim strict sandbox guarantees

### Requirement: Tool catalog identity is workspace-agnostic
alan SHALL define tool identity in a stable catalog separate from execution
binding, exposure profile, and workspace-local context.

Stable terms:

- **Tool catalog**: stable set of tool definitions available to a runtime or
  host. A catalog entry defines name, description, parameter schema, capability
  classification, timeout hint, and locality.
- **Materialized tool instance**: executable implementation for one catalog
  entry.
- **Tool locality**: whether semantics are global or tied to the runtime's
  bound local workspace.
- **Tool execution binding**: runtime-owned binding supplied at execution time,
  including current `cwd`, scratch area, and optional `workspace_root`.
- **Tool context**: per-call execution object passed to tool implementations.
- **Exposure profile**: allowlisted subset of catalog entries visible to a
  runtime.

Rules:

- Catalog entries are workspace-agnostic.
- Built-in tool constructors do not require a workspace path to define the
  tool.
- Workspace roots, working directories, and scratch directories belong to
  execution binding, not catalog identity.
- Tool visibility answers which tools may be called, not which workspace those
  tools are bound to.

#### Scenario: Runtime exposes a built-in tool in two workspaces
- **WHEN** two runtimes bound to different workspaces expose the same built-in
  tool
- **THEN** the catalog identity is the same
- **AND** workspace-specific facts come from execution binding and policy

### Requirement: Workspace-local tools require explicit runtime binding
alan SHALL execute workspace-local tools only with explicit workspace binding
and SHALL keep workspace routing failures distinct from policy escalation.

Locality classes:

1. `global`: not implicitly tied to the runtime's bound workspace
2. `workspace_local`: acts on the runtime's currently bound local workspace

Workspace-local execution rules:

- Runtime provides explicit `workspace_root`.
- If both `workspace_root` and `cwd` are present, `cwd` must be inside
  `workspace_root`.
- Path resolution stays relative to bound `cwd`.
- Execution backends enforce the bound `workspace_root` rather than a hidden
  process-global default.
- Running a workspace-local tool without explicit binding is a runtime binding
  error.
- A tool is not workspace-local merely because arguments are named `path`,
  `cwd`, or `workspace_root`.

Workspace-routing rules:

- One running `AgentInstance` is bound to one workspace at a time.
- Natural-language acknowledgements do not mutate runtime binding.
- When a task targets a different local workspace, alan should launch a fresh
  child runtime with explicit `workspace_root`, optional nested `cwd`, task
  text, and handles such as `workspace` and `approval_scope`.
- Cross-workspace local shell in the current runtime is a routing failure before
  it is a policy question.
- `tool_escalation` remains reserved for policy boundaries.
- Target-local search must run inside the delegated child rather than by
  searching parent `.alan` state.

#### Scenario: Cross-workspace shell command is attempted
- **WHEN** a local shell command in the current runtime explicitly targets a
  path outside the current workspace
- **THEN** alan reports a recoverable workspace-routing failure and points to
  delegated child launch
- **AND** it does not treat user approval alone as making the current runtime
  the correct execution site

### Requirement: Child runtimes derive tools from catalog, profile, and binding
alan SHALL materialize child-runtime tool surfaces from the shared catalog,
child exposure profile, and child execution binding rather than inheriting
parent-bound tool instances.

Rules:

- Child runtimes use the same tool catalog identity as parents.
- Exposure profile controls which tools are visible.
- Child execution binding controls workspace root, current directory, and
  scratch state.
- Parent and child runtime differences are expressed through exposure profiles
  and execution bindings.
- Persisted launch metadata must match the resolved execution binding the child
  will use.
- Unresolved relative workspace fields must not be persisted and later executed
  through process-local defaults.

#### Scenario: Child runtime launches with repo-local tools
- **WHEN** alan launches a repo-scoped child runtime
- **THEN** the child materializes tools from catalog plus exposure profile plus
  explicit workspace binding
- **AND** it does not inherit parent tool instances carrying workspace-specific
  state

### Requirement: Capability routing is provider-location agnostic and governed
alan SHALL route capability calls by capability name and version across builtin,
local extension, and bridge providers without bypassing governance.

Core objects:

- `CapabilityCall`: includes call id, task/run/session/turn ids, capability
  name, input, side-effect mode, idempotency key, deadline, route mode, and
  trace context.
- `ProviderRef`: includes provider id, source (`builtin`, `extension_local`, or
  `extension_bridge`), priority, health, supported capabilities, and optional
  cost class.
- `RouteDecision`: includes selected provider, fallback chain, policy action,
  and reason.

Router responsibilities:

1. select provider and route calls per capability
2. inject idempotency key, deadline, and trace context
3. emit unified call events and audit fields
4. evaluate request through `PolicyEngine`
5. perform bounded fallback only where safe

Router prohibitions:

1. bypassing `PolicyEngine` for side-effect capabilities
2. silent retry and provider switch after side effects may have happened
3. modifying turn state-machine semantics

Fallback rules:

- `side_effect_mode=none`: fallback allowed under `best_effort`.
- `reversible`: no automatic fallback unless rollback support is declared.
- `irreversible`: automatic fallback forbidden; require governance path.
- `shadow`: evaluation-only mode with no real side effects.

#### Scenario: Irreversible capability provider fails
- **WHEN** an irreversible capability call may have partially executed
- **THEN** the router does not silently retry or switch providers
- **AND** it uses idempotency, effect records, and governance recovery

### Requirement: Capability routing emits stable events and errors
alan SHALL make capability route decisions auditable and testable through stable
events, fields, and error semantics.

Recommended events:

1. `capability_route_selected`
2. `capability_call_started`
3. `capability_call_completed`
4. `capability_call_failed`
5. `capability_call_deduped`
6. `capability_route_fallback`

Recommended fields:

1. `call_id`
2. `provider_id`
3. `capability`
4. `run_id`
5. `session_id`
6. `turn_id`
7. `policy_action`
8. `latency_ms`
9. `status`

Error semantics:

- `provider_unavailable`: retryable or fallbackable according to side-effect
  mode.
- `capability_not_found`: request-level failure.
- `policy_denied` and `policy_escalated`: governance outcomes with no provider
  execution.
- `deadline_exceeded`: execution-level error that may map to retry/backoff.

#### Scenario: Capability call is deduped after recovery
- **WHEN** a restored run replays a capability call with the same idempotency
  key
- **THEN** the router reports a dedupe hit and records auditable call metadata

### Requirement: Extensions cannot bypass runtime governance or state authority
alan SHALL treat extensions as loadable capability providers below the runtime
state machine and governance layer.

Extension types:

1. `tool_provider`
2. `memory_provider`
3. `channel_adapter`
4. `domain_module`

Minimum manifest fields:

1. `id`
2. `version`
3. `contract_version`
4. `kind`
5. `entrypoint`
6. `capabilities[]`
7. `permissions`
8. `config_schema`
9. `state_namespace`
10. `healthcheck`

Capability declarations include name, version, effects, risk level, input and
output schemas, timeout, and idempotency support.

Lifecycle:

1. `load`
2. `init`
3. `start`
4. `stop(reason)`
5. `recover(checkpoint_ref)`
6. `health`

Rules:

- Extension start failure does not crash runtime.
- Lifecycle transitions are auditable.
- Router applies policy before invocation.
- Extensions do not bypass governance.
- Declared permissions are upper bounds.
- Extension private state lives under
  `{workspace}/.alan/extensions/{extension_id}/state/`.
- Extension temp files live under
  `{workspace}/.alan/extensions/{extension_id}/tmp/`.
- Extensions must not mutate rollout or checkpoint source-of-truth files.
- Host rejects unsupported major `contract_version`.

#### Scenario: Extension capability is high risk
- **WHEN** an extension declares a risk-level C capability
- **THEN** policy defaults to escalation unless an explicit policy allows it
- **AND** extension permissions do not override runtime governance

#### Scenario: Extension fails to start
- **WHEN** an extension fails manifest validation, initialization, or startup
- **THEN** runtime isolates the extension, degrades safely, and preserves
  session state-machine integrity
