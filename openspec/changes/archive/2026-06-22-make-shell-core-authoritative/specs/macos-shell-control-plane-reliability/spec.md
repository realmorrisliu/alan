## ADDED Requirements

### Requirement: Portable control commands use shell-core outcomes
The macOS shell control plane SHALL route portable workspace-domain command
validation, stable errors, reducer dispatch, and response projection through
Rust shell core once the command is covered by the shell-core control reducer.

#### Scenario: Workspace command succeeds
- **WHEN** a local control client sends a portable command such as `space.create`,
  `tab.open`, `tab.pin`, `pane.split`, `pane.focus`, or `attention.set`
- **THEN** the applied response is derived from the shell-core control result
- **AND** Swift applies the returned state or side effects without recomputing
  command validity or response fields through a duplicate switch branch

#### Scenario: Workspace command is rejected
- **WHEN** shell core rejects a portable control command with a stable error
- **THEN** the macOS control response reports that shell-core error
- **AND** Swift does not translate the same command through an alternate
  platform mutation path to produce a different result

#### Scenario: Command requires terminal runtime delivery
- **WHEN** a control command requires platform terminal runtime work, such as
  sending text or focusing a terminal surface
- **THEN** shell core owns target validation and the portable intent
- **AND** Swift owns the runtime delivery attempt and merges the platform
  outcome into the response

### Requirement: Host-only commands are explicit platform commands
Mac-only control commands MUST remain in Swift under explicit host ownership
when they cannot be represented as portable shell-domain commands, rather than
being mixed into shell-core fallback branches.

#### Scenario: Performance diagnostic command is handled
- **WHEN** a control command operates only on macOS diagnostics, exported files,
  or app runtime settings
- **THEN** Swift handles it as a host command
- **AND** the command does not masquerade as a failed shell-core domain command
