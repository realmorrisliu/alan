# Spec Delta

## MODIFIED Requirements

### Requirement: Single human-in-the-end auto-approve mode
Alan SHALL default to the governed autonomous posture and offer an explicit fully trusted posture only through a trusted-user invocation control. Fully trusted SHALL bypass per-action policy red lines and Guardian, not protocol validation, Process/OS rights, active isolation, lifecycle or budgets. Isolation SHALL remain separately selected. Project files, saved task history and model output MUST NOT activate the posture; it SHALL NOT persist into a new invocation.

#### Scenario: Default startup
- **WHEN** the user starts Alan without a new explicit trust selection
- **THEN** governed autonomous execution and confined selection apply

#### Scenario: Trusted confined execution
- **WHEN** the user disables per-action approvals but does not select Host-account execution
- **THEN** the active granted scope and sandbox remain, and blocked effects fail without prompting or broadening

#### Scenario: Trusted Host-account execution
- **WHEN** the user separately selects Host-account native execution
- **THEN** native commands use actual account authority and UI discloses absence of Alan sandbox confinement

#### Scenario: Untrusted activation attempt
- **WHEN** a project file or Agent requests broader execution mode
- **THEN** the trusted control path rejects it

#### Scenario: Invalid posture
- **WHEN** a mode value is unknown, malformed or a retired alias
- **THEN** parsing fails without permissive fallback
### Requirement: Conservative escalation boundary before OS sandboxing
These escalation rules apply only to governed autonomous execution. Fully trusted execution instead follows its explicitly selected isolation without per-action escalation. In governed autonomous execution, deterministic checks SHALL admit only effects supported by current delegated scope and actual backend enforcement. An exact user-origin publication delegation MAY satisfy the prior per-call publication approval, but ordinary push SHALL NOT authorize force-push or remote deletion. Privilege expansion, control-policy changes, new private resources and uncontainable effects SHALL require a trusted-user decision or denial; reviewer output MUST NOT grant them.

#### Scenario: Delegated branch push
- **WHEN** an ordinary push matches a current user-approved repository, remote and ref scope
- **THEN** it needs no repeated approval, while credentials and backend permissions remain independently checked

#### Scenario: Force push is not ordinary push
- **WHEN** a force-push or remote deletion lacks its own exact delegated capability
- **THEN** the runtime asks the user or denies it before effects

#### Scenario: Scope cannot be enforced
- **WHEN** the active backend cannot enforce a promised delegated resource or network boundary
- **THEN** the runtime reports that limitation without automatically choosing Host-account execution

## ADDED Requirements

### Requirement: Trust controls are invocation-local and authority-preserving
The runtime SHALL accept posture/isolation changes only through trusted user controls and pass no broader choice to child Processes. Revocation, cancellation and budget controls SHALL remain active. A downgrade SHALL invalidate incompatible pending decisions and pause new incompatible effects, without claiming retroactive confinement or rollback.

#### Scenario: New invocation after trusted work
- **WHEN** the user resumes a prior trusted task in another invocation without newly selecting trust
- **THEN** recovery uses the governed default and revalidates authority

#### Scenario: Downgrade during execution
- **WHEN** the user reduces trust while a native process is already running
- **THEN** new incompatible effects pause and existing lifecycle controls remain available; UI does not claim that process was retroactively sandboxed
