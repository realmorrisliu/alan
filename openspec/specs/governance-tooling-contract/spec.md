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

### Requirement: Governance is scoped to Process and capability execution
Alan SHALL resolve governance for Agent Process, Tool Process, and capability execution from the
owning policy files, namespace, credentials, executable identity, requested effect, and explicit
human decisions. Policy state SHALL be recorded against the concrete execution owner and evidence
files to which the decision applies.

#### Scenario: A Tool Process requires approval
- **WHEN** a Tool Process requests an effect that policy classifies as approval-required
- **THEN** the request and decision identify the Tool Process, parent Agent Process when applicable,
  capability call, and action/request files
- **AND** authorization is derived from those concrete owners and the resolved policy

### Requirement: Governance events identify concrete execution owners
Capability and governance events SHALL identify their call, Process, Tool, turn, request, action,
policy, and evidence owners as applicable. Each identifier SHALL correspond to one concrete owner
or durable record.

#### Scenario: A capability decision is audited
- **WHEN** governance records a capability decision
- **THEN** the audit resolves to concrete Process and capability-call evidence
- **AND** a renderer or reviewer can inspect the decision through the owning files
