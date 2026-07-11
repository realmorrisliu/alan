## REMOVED Requirements

### Requirement: HITE governance semantics are stable
**Reason**: The existing requirement scopes policy replacement and governance state to a Session.
**Migration**: Apply policy to Process and capability execution boundaries.

### Requirement: Policy files and execution backends remain separate
**Reason**: Policy resolution is specified for runtime Sessions.
**Migration**: Resolve policy for Agent Process and Tool Process execution from their AgentRoot, namespace, credentials, and explicit policy files.

### Requirement: Capability routing is provider-location agnostic and governed
**Reason**: CapabilityCall identity includes task/run/Session ids as interchangeable center objects.
**Migration**: Use call, Process, Tool, turn, request, and action identity owned by the execution path.

### Requirement: Capability routing emits stable events and errors
**Reason**: Stable event fields require `session_id`.
**Migration**: Use Process/AgentFS paths and capability-call identity.

### Requirement: Extensions cannot bypass runtime governance or state authority
**Reason**: State authority is expressed as Session state-machine integrity.
**Migration**: Protect Agent Machine, Process, namespace, and owning file-server state.

## ADDED Requirements

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
