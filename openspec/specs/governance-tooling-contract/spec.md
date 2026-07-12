# governance-tooling-contract Specification

## Purpose
Defines durable governance and tooling boundaries for policy decisions, tool
identity, runtime binding, capability routing, extension points, and workspace
scoping.
## Requirements
### Requirement: Governance and tooling contracts live in OpenSpec
Alan SHALL specify HITE governance, mounted Tool package identity, Process
execution binding, capability routing, extension points, and workspace routing
in OpenSpec.

#### Scenario: Governance behavior changes
- **WHEN** a change modifies policy `allow`, `deny`, or `escalate` semantics,
  execution-backend boundaries, owner-boundary classes, audit requirements, or
  approval/resume behavior
- **THEN** the change updates this capability,
  `evidence-retention-and-projection`, `delegation-capability-alignment`,
  `agent-file-layout-contract`, `agent-runtime-ui-file-surfaces`, or another
  active OpenSpec owner

#### Scenario: Tool binding behavior changes
- **WHEN** a change modifies Tool package manifests, namespace composition,
  locality, workspace scoping, child Process mounts, or extension routing
- **THEN** the behavior is specified in OpenSpec before it is documented as
  current guidance

### Requirement: Tool identity is separate from execution binding
Alan SHALL keep stable Tool identity and schema in mounted package manifests
separate from per-Process execution binding such as workspace root, current
directory, credentials, namespace reachability, and policy decisions. The
transition loop SHALL derive both through the Process namespace and SHALL NOT
use an in-process Tool registry as their authority. During the current
convention-enforced stage, a host Process-runner adapter MAY co-locate an
implementation entry with its default execution binding, but Tool identity and
schema SHALL still come from the mounted manifest and reachability SHALL still
come from the Process namespace.

#### Scenario: Runtime exposes a Tool
- **WHEN** a Tool package executable and manifest are mounted for an Agent
  Process
- **THEN** the Tool's identity, schema, capability, and locality come from the
  package manifest
- **AND** workspace-specific execution facts come from the Tool Process exec
  context, descriptors, namespace, and policy
- **AND** any host-adapter implementation registry cannot expose or execute a
  Tool that is not mounted for that Process

#### Scenario: Delegated capability is selected
- **WHEN** Alan routes work to a delegated child target
- **THEN** the parent/spawner resolves allowed Tool packages into the child's
  namespace before launch
- **AND** capability matching and mismatch recovery are observable through the
  OpenSpec-defined routing and Process/file surfaces

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

### Requirement: Tool package identity is namespace-discovered
Alan SHALL define a Tool's stable model and governance identity in its mounted
package files. A complete Tool package SHALL include a visible executable and a
validated manifest containing name, description, parameter schema, capability
classification, timeout hint, and locality. Tool visibility SHALL be determined
by namespace reachability, not an ambient host catalog or exposure list inside
the engine.

#### Scenario: Same Tool package is mounted in two workspaces
- **WHEN** two Agent Processes with different workspace bindings mount the same
  Tool package version
- **THEN** the Tool identity and schema are the same
- **AND** workspace root, cwd, credentials, and scratch state come from each
  Process execution context

#### Scenario: Policy withholds a Tool
- **WHEN** namespace composition excludes a Tool package from a child Process
- **THEN** the child cannot discover, describe, or execute that Tool
- **AND** no process-global catalog can restore visibility

#### Scenario: Package metadata is incomplete
- **WHEN** a visible executable lacks required manifest identity or governance
  fields
- **THEN** it is not exposed as a model-callable Tool
- **AND** capability classification does not fall back to an in-process table

### Requirement: Child Process Tools are selected by namespace composition
Alan SHALL select child Tool access before spawn by mounting complete permitted
Tool packages into the child's namespace. Parent and child differences SHALL be
expressed by their mounted packages and Process execution contexts, not by
cloning parent Tool instances or constructing child registries.

#### Scenario: Child launches with repo-local Tools
- **WHEN** Alan launches a repo-scoped child Agent Process
- **THEN** the child namespace contains only the permitted complete Tool
  packages plus explicit workspace binding
- **AND** the child discovers those Tools by walking its own namespace
- **AND** it does not inherit parent Tool objects carrying workspace-specific
  state

#### Scenario: Persisted launch metadata is inspected
- **WHEN** child launch metadata records workspace and Tool selection
- **THEN** it matches the namespace and Process execution binding committed at
  `/proc/clone`
- **AND** unresolved relative workspace fields are not later resolved through
  process-global defaults
