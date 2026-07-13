# host-command-plane Specification

## Purpose
Defines the boundary between Host lifecycle and native integration commands and
namespace-native Alan Shell operations, including permanent workspace-era CLI
removal.
## Requirements
### Requirement: Host and Alan OS commands remain separate
The system SHALL use the Host Command Plane for Host lifecycle, attachment,
Host Mount authorization, credentials, and native integration. Namespace file
operations, service control, and executable invocation SHALL use Alan Shell and
MUST NOT be duplicated as typed Host manager commands.

#### Scenario: User starts Alan
- **WHEN** the user runs `alan`
- **THEN** the Host Command Plane boots or attaches Alan OS
- **AND** control passes to Alan Shell without selecting an Agent profile

### Requirement: Removed workspace commands have no aliases
Alan MUST remove `alan init`, `alan workspace`, workspace registry operations,
and `--agent` boot selection without hidden aliases or compatibility mode.

#### Scenario: Retired command is invoked
- **WHEN** a caller invokes a removed workspace command
- **THEN** the CLI rejects it and points to Host Mount or Alan Shell operations
- **AND** it does not recreate workspace state
