# host-command-plane Specification

## Purpose
Defines the boundary between Host lifecycle and native integration commands and
namespace-native Alan Shell operations, including permanent workspace-era CLI
removal.
## Requirements
### Requirement: Host and alan9 commands remain separate
The Host Command Plane SHALL own explicit Host lifecycle and local attachment,
Host Mount authorization, credentials, and native integration. Namespace file
operations, service control, and executable invocation inside alan9 SHALL use
Alan Shell and MUST NOT be duplicated as typed Host manager commands. Bare
`alan` startup SHALL create a foreground instance per invocation.

#### Scenario: User starts Alan
- **WHEN** the user runs bare `alan`
- **THEN** the CLI starts one independent foreground alan9 instance
- **AND** the renderer attaches directly to that instance's `/agent/root`
- **AND** it does not select a channel-wide Host or another invocation's Root

### Requirement: Removed workspace commands have no aliases
Alan MUST remove `alan init`, `alan workspace`, workspace registry operations,
and `--agent` boot selection without hidden aliases or compatibility mode.

#### Scenario: Retired command is invoked
- **WHEN** a caller invokes a removed workspace command
- **THEN** the CLI rejects it and points to Host Mount or Alan Shell operations
- **AND** it does not recreate workspace state
