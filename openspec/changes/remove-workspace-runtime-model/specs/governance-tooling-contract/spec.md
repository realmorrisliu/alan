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

## REMOVED Requirements

### Requirement: Workspace-local tools require explicit runtime binding
**Reason**: Workspace identity and routing are removed from Alan OS.
**Migration**: Pass mounts, descriptors, cwd, credentials, and policy through Process Launch Context.
