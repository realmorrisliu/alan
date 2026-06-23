## MODIFIED Requirements

### Requirement: Conservative escalation boundary before OS sandboxing
When an OS sandbox backend is active, the policy SHALL allow sandboxed bash and policy-permitted network to proceed without prompting, while still escalating effects that escape the sandbox (writes outside the workspace and disallowed network) and operations of unknown capability. When no OS sandbox backend is active, the policy SHALL escalate network access, writes outside the workspace, explicitly destructive or irreversible operations, and operations of unknown capability.

#### Scenario: Sandboxed bash proceeds when a backend is active
- **WHEN** an OS sandbox backend is active and a shell command runs confined to the workspace
- **THEN** the command proceeds without prompting

#### Scenario: Sandbox-escaping effect still escalates
- **WHEN** an operation would write outside the workspace or perform disallowed network access
- **THEN** the policy escalates it for human approval even when a backend is active

#### Scenario: Network access escalates
- **WHEN** an operation is classified as network capability and no OS sandbox backend is active
- **THEN** the policy escalates it for human approval

#### Scenario: Out-of-workspace write escalates
- **WHEN** an operation would write outside the workspace
- **THEN** the policy escalates it for human approval

#### Scenario: Destructive operation escalates
- **WHEN** an operation is explicitly destructive or irreversible and no OS sandbox backend is active
- **THEN** the policy escalates it for human approval

#### Scenario: Unknown capability escalates by default
- **WHEN** an operation's capability cannot be classified
- **THEN** the policy escalates it rather than auto-allowing it
