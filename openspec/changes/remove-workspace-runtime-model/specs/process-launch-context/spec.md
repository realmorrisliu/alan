## ADDED Requirements

### Requirement: Process launch has no workspace identity
Alan OS SHALL create every Process from a Process Launch Context containing its
parent namespace snapshot, explicit mounts and descriptors, credentials, and
initial namespace cwd. It MUST NOT assign a workspace id or Host root identity.

#### Scenario: Agent starts with a Host Mount
- **WHEN** a Shell Process spawns an Agent Executable with `/mnt/source` mounted
  and cwd set to `/mnt/source`
- **THEN** the child receives that namespace context without a workspace field
- **AND** no raw Host OS path is required by the Agent Process

### Requirement: Child context follows namespace inheritance
A child Process SHALL inherit a snapshot of its parent's namespace and SHALL
gain additional authority only through explicitly passed mounts or descriptors.

#### Scenario: Child lacks an unpassed mount
- **WHEN** a parent launches a child without passing a separate Host Mount
- **THEN** the child cannot reach that Host Mount by name, path inference, or
  workspace discovery

### Requirement: Agent Definitions are descriptor-passed
An Agent Process SHALL receive its Agent Definition explicitly at Process
creation. Alan OS MUST NOT search Host directories or boot arguments for named
or default Agent overlays.

#### Scenario: Ordinary Agent Process starts
- **WHEN** Alan Shell executes an Agent Executable with an Agent Definition
  descriptor
- **THEN** the Agent Process uses that definition
- **AND** Host cwd, `--agent`, and `.alan/agents` do not affect resolution
