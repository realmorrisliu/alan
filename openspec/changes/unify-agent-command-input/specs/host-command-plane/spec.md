## MODIFIED Requirements

### Requirement: Host and alan9 commands remain separate
The system SHALL use the Host Command Plane for Host lifecycle, attachment,
Host Mount authorization, credentials and native integration. Internal namespace
operations and service control SHALL remain owned by existing aP services.
Task-oriented alan9 command executables MAY encapsulate those operations as thin,
caller-authorized aP clients primarily for Agents, reusing ordinary execution and
evidence. They MUST NOT duplicate service state or create privileged typed Host
manager APIs. Normal callers SHALL NOT need protocol documents or internal mount
paths to use supported task operations.

#### Scenario: User starts Alan
- **WHEN** the user runs `alan`
- **THEN** the Host Command Plane boots or attaches alan9
- **AND** control passes to Alan Shell without selecting an Agent profile

#### Scenario: Agent invokes an alan9 control command
- **WHEN** a command requests an internal service operation
- **THEN** the command calls the existing aP owner with caller-scoped authority
- **AND** the Host Command Plane does not become a second service-control authority
- **AND** Host authorization or credential changes still require their existing owner
