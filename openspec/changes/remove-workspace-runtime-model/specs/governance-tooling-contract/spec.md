## ADDED Requirements

### Requirement: Tool execution binding is Process Launch Context
Tool execution SHALL derive executable reachability from `/bin`, path access
from the Process namespace, cwd from a namespace path, and native sandbox roots
from explicit Host Mount grants. Tool identity MUST NOT be global or
workspace-local and policy escalation MUST remain distinct from missing
capability.

#### Scenario: Tool accesses mounted source
- **WHEN** a Tool Process inherits a writable `/mnt/source` Host Mount
- **THEN** namespace access and native sandbox access derive from the same grant
- **AND** no workspace routing classification is consulted

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

## REMOVED Requirements

### Requirement: Workspace-local tools require explicit runtime binding
**Reason**: Workspace identity and routing are removed from Alan OS.
**Migration**: Pass mounts, descriptors, cwd, credentials, and policy through Process Launch Context.
