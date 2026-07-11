## MODIFIED Requirements

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
directory, credentials, namespace reachability, and policy decisions. The live
engine SHALL derive both through the Process namespace and SHALL NOT merge them
inside an in-process Tool registry.

#### Scenario: Runtime exposes a Tool
- **WHEN** a Tool package executable and manifest are mounted for an Agent
  Process
- **THEN** the Tool's identity, schema, capability, and locality come from the
  package manifest
- **AND** workspace-specific execution facts come from the Tool Process exec
  context, descriptors, namespace, and policy

#### Scenario: Delegated capability is selected
- **WHEN** Alan routes work to a delegated child target
- **THEN** the parent/spawner resolves allowed Tool packages into the child's
  namespace before launch
- **AND** capability matching and mismatch recovery are observable through the
  OpenSpec-defined routing and Process/file surfaces

## ADDED Requirements

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

## REMOVED Requirements

### Requirement: Tool catalog identity is workspace-agnostic
**Reason**: Stable Tool identity now belongs to mounted executable-package
manifests; an ambient catalog is not a live engine authority.

**Migration**: Move every required identity, schema, capability, locality, and
hint field into `/lib/exec/<tool>/manifest`, mount the executable in `/bin`, and
derive workspace facts from the spawned Process context.

### Requirement: Child runtimes derive tools from catalog, profile, and binding
**Reason**: Child Agent Processes derive capability from their assembled
namespace, not a registry materialized from parent catalog objects.

**Migration**: Resolve policy/exposure before spawn, mount only permitted
complete Tool packages, and let the child discover its own namespace.
