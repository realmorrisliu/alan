# governance-tooling-contract Specification

## Purpose
Defines durable governance and tooling boundaries for policy decisions, tool
identity, Process execution binding, capability routing, extension points, and
namespace authority.
## Requirements
### Requirement: Governance and tooling contracts live in OpenSpec
Alan SHALL specify HITE governance, mounted Tool package identity, Process
execution binding, capability routing, extension points, and Host Mount
authority in OpenSpec.

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
  Process Launch Context, Host Mount authority, child Process mounts, or
  extension routing
- **THEN** the behavior is specified in OpenSpec before it is documented as
  current guidance

### Requirement: Tool identity is separate from execution binding
Alan SHALL keep stable Tool identity and schema in mounted package manifests
separate from per-Process execution binding such as namespace cwd, explicit
Host Mounts, credentials, namespace reachability, and policy decisions. The
transition loop SHALL derive both through the Process namespace and SHALL NOT
use an in-process Tool registry as their authority. During the current
convention-enforced stage, a host Process-runner adapter MAY co-locate an
implementation entry with its default execution binding, but Tool identity and
schema SHALL still come from the mounted manifest and reachability SHALL still
come from the Process namespace.

#### Scenario: Runtime exposes a Tool
- **WHEN** a Tool package executable and manifest are mounted for an Agent
  Process
- **THEN** the Tool's identity, schema, capability, and execution hints come from the
  package manifest
- **AND** Process-specific execution facts come from the Tool Process exec
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
classification, timeout hint, and execution hints. Tool visibility SHALL be determined
by namespace reachability, not an ambient host catalog or exposure list inside
the engine.

#### Scenario: Same Tool package is mounted in two Process namespaces
- **WHEN** two Agent Processes with different launch contexts mount the same
  Tool package version
- **THEN** the Tool identity and schema are the same
- **AND** namespace cwd, Host Mounts, credentials, and scratch state come from each
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

#### Scenario: Child launches with selected Tools
- **WHEN** Alan launches a child Agent Process with a narrower Tool set
- **THEN** the child namespace contains only the permitted complete Tool
  packages plus explicitly inherited mounts and descriptors
- **AND** the child discovers those Tools by walking its own namespace
- **AND** it does not inherit parent Tool objects or execution bindings

#### Scenario: Persisted launch metadata is inspected
- **WHEN** child launch metadata records Process Launch Context and Tool selection
- **THEN** it matches the namespace and Process execution binding committed at
  `/proc/clone`
- **AND** unresolved Host paths or locality fields are not later inferred from
  process-global defaults

### Requirement: Tool execution binding is Process Launch Context
Tool execution SHALL derive executable reachability from `/bin`, path access
from the Process namespace, cwd from a namespace path, and native sandbox roots
from explicit Host Mount grants. Tool identity MUST NOT be global or
encode Host locality, and policy escalation MUST remain distinct from missing
capability.

#### Scenario: Tool accesses mounted source
- **WHEN** a Tool Process inherits a writable `/mnt/source` Host Mount
- **THEN** namespace access and native sandbox access derive from the same grant
- **AND** no Host-locality routing classification is consulted

#### Scenario: Tool Process selects an explicit mount when parent cwd is virtual
- **GIVEN** an Agent Process cwd such as `/` has no Host backing
- **WHEN** the Process receives an approved writable Host Mount at `/mnt/project`
- **THEN** its native Tool Process binding uses `/mnt/project` as the Tool Process cwd
- **AND** the Agent Process cwd remains unchanged
- **AND** runtime scratch, Host cwd, and Host home gain no sandbox authority

#### Scenario: Child Tool Process uses an inherited mount with a virtual cwd
- **GIVEN** a child Agent Process inherits an explicit Host Mount while its cwd is `/`
- **WHEN** the child Tool Process binding is assembled
- **THEN** an authorized inherited mount becomes the native Tool Process cwd
- **AND** the child receives no Host authority beyond its inherited grants

#### Scenario: Read-only Host Mounts remain readable to read-class Tools
- **GIVEN** a Process has an explicit read-only Host Mount at `/mnt/docs`
- **WHEN** a read-class Tool Process reads an ordinary path operand under `/mnt/docs`
- **THEN** the read is permitted by both namespace and native sandbox projection
- **AND** mutation and redirection targets under `/mnt/docs` remain denied
