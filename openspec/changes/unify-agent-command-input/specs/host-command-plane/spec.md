## MODIFIED Requirements

### Requirement: Host and alan9 commands remain separate
The foreground composition SHALL own native application startup and shutdown.
The Host Command Plane SHALL remain the native boundary for explicit local
attachment, Host Mount authorization, credentials and platform integration.
Operations on a live instance SHALL require an explicitly selected instance
endpoint or an existing authorized handle; a channel name MUST NOT select another
terminal's live runtime implicitly. Durable profile/credential operations SHALL
retain their existing channel store and native authorization boundaries without
starting a background Host. Internal namespace
operations and service control SHALL remain owned by existing aP services.
Task-oriented alan9 command executables MAY encapsulate those operations as thin,
caller-authorized aP clients primarily for Agents, reusing ordinary execution and
evidence. They MUST NOT duplicate service state or create privileged typed Host
manager APIs. Normal callers SHALL NOT need protocol documents or internal mount
paths to use supported task operations.

#### Scenario: User starts Alan
- **WHEN** the user runs `alan`
- **THEN** product composition boots an independent foreground alan9 instance
- **AND** its renderer reads that instance's namespace through Alan Shell
- **AND** it neither discovers an ambient Root Agent nor bypasses Service Manager

#### Scenario: Agent invokes an alan9 control command
- **WHEN** a command requests an internal service operation
- **THEN** the command calls the existing aP owner with caller-scoped authority
- **AND** the Host Command Plane does not become a second service-control authority
- **AND** Host authorization or credential changes still require their existing owner

#### Scenario: Management command has no live instance target
- **WHEN** a command needs live Process or Host Mount request state but has no explicit instance target
- **THEN** it reports the missing target rather than launching or selecting a channel-wide Host
- **AND** it does not infer authority from a PID, path or Herdr identifier

#### Scenario: Credential command updates a shared channel store
- **WHEN** an operator changes a connection profile or credential outside a running Alan instance
- **THEN** the existing owning service/native adapter validates and commits that operation
- **AND** concurrent updates cannot silently overwrite one another or borrow another instance's authority
- **AND** it does not start an unrelated Root Agent as a side effect
