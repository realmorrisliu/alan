# Spec Delta

## MODIFIED Requirements

### Requirement: Single human-in-the-end auto-approve mode
The default governed posture SHALL remain serialized as autonomous and use explicit project delegation. Approval posture SHALL be separate from resource scope and execution isolation. Routine valid delegated operations SHALL proceed without repeated permission prompts; unresolved judgment SHALL use qualified reviewer evaluation or a correlated human request. This delivery SHALL NOT enable the separately planned fully trusted posture. Retired aliases and malformed profile values SHALL be rejected.

#### Scenario: Routine delegated work
- **WHEN** a read, edit, build, test or local commit is within current delegated scope and backend guarantees
- **THEN** it proceeds without repeated approval and without acquiring additional rights

#### Scenario: Reviewer cannot expand scope
- **WHEN** a reviewer approves an operation outside delegated authority
- **THEN** the action does not execute without a separate trusted-user grant

#### Scenario: Invalid posture
- **WHEN** configuration uses an unrecognized or retired posture alias
- **THEN** configuration fails explicitly rather than selecting a permissive fallback
### Requirement: Conservative escalation boundary before OS sandboxing
In governed autonomous execution, deterministic checks SHALL admit only effects supported by current delegated scope and actual backend enforcement. An exact user-origin publication delegation MAY satisfy the prior per-call publication approval, but ordinary push SHALL NOT authorize force-push or remote deletion. Privilege expansion, control-policy changes, new private resources and uncontainable effects SHALL require a trusted-user decision or denial; reviewer output MUST NOT grant them.

#### Scenario: Delegated branch push
- **WHEN** an ordinary push matches a current user-approved repository, remote and ref scope
- **THEN** it needs no repeated approval, while credentials and backend permissions remain independently checked

#### Scenario: Force push is not ordinary push
- **WHEN** a force-push or remote deletion lacks its own exact delegated capability
- **THEN** the runtime asks the user or denies it before effects

#### Scenario: Scope cannot be enforced
- **WHEN** the active backend cannot enforce a promised delegated resource or network boundary
- **THEN** the runtime reports that limitation without automatically choosing Host-account execution
### Requirement: Permissions are never silently widened
A one-operation approval SHALL apply only to that correlated operation and MUST NOT silently widen future authority. A separate explicit trusted-user action MAY save scoped project delegation for reuse after identity and revocation checks. Project content, model output and ordinary task memory SHALL NOT create or modify standing delegation.

#### Scenario: One action is approved
- **WHEN** the user approves one escalated action
- **THEN** no standing delegation is created

#### Scenario: Delegation is explicitly saved
- **WHEN** the user separately saves a bounded project delegation
- **THEN** later matching actions reuse it only while identity, scope and revocation state remain valid
