## MODIFIED Requirements

### Requirement: Local entry creates a Shell Process
Local Entry Service SHALL create `/bin/alan-shell` as an ordinary Process with
Alan OS credentials, Login Namespace Template, descriptors, cwd, PID, and
parentage, then hand its namespace to an authorized local renderer. Service
Manager SHALL bind the Agent Runtime Service top-level launch capability at
`/mnt/agent-runtime` in that Login Namespace. It MUST NOT publish the capability
through `/srv` or include it in Agent Process namespace templates.

#### Scenario: macOS requests local entry
- **WHEN** Host transport has authorized the peer
- **THEN** Local Entry Service creates a Shell Process
- **AND** commands launched by the Shell become child Processes
- **AND** the handed-off Login Namespace can open
  `/mnt/agent-runtime/clone`

#### Scenario: Agent Process namespace is assembled
- **WHEN** Agent Runtime Service creates a child Agent Process
- **THEN** `/mnt/agent-runtime` is absent unless a future explicit delegation
  contract authorizes it
- **AND** read-write access to `/agent` does not imply top-level launch
  authority
