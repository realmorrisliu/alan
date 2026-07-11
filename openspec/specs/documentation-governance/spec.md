# documentation-governance Specification

## Purpose
Defines how alan separates durable OpenSpec requirements from implementation
guides, operator runbooks, executable fixtures, bridge pages, and retired
historical plans.
## Requirements
### Requirement: OpenSpec owns durable specifications
alan SHALL use OpenSpec as the only durable source of truth for product,
runtime, protocol, provider, governance, testing-contract, and UX
specifications.

#### Scenario: Durable behavior is specified
- **WHEN** a change defines normative behavior, target behavior, acceptance
  criteria, product contracts, runtime contracts, provider contracts, or testing
  contracts
- **THEN** the requirement is authored under `openspec/specs/` or an active
  `openspec/changes/<change>/specs/` delta
- **AND** the requirement is not authored as a new long-form contract under
  `docs/spec/`, `plans/`, or `docs/superpowers/`

#### Scenario: In-flight design is specified
- **WHEN** design decisions, scope changes, task sequencing, verification
  expectations, or requirement deltas are still in flight
- **THEN** they are captured in the relevant active OpenSpec change artifacts
  instead of a standalone plan directory

### Requirement: Non-OpenSpec docs are implementation and operation surfaces
Repository documentation outside OpenSpec SHALL be limited to implementation
guides, operator guides, maintainer runbooks, validation instructions, generated
or executable fixtures, and short bridge pointers.

#### Scenario: Current implementation guide explains behavior
- **WHEN** a `docs/` guide explains current commands, runtime surfaces,
  troubleshooting, architecture context, or validation usage
- **THEN** it may remain outside OpenSpec
- **AND** any normative requirement it references points to an OpenSpec
  capability or active OpenSpec change

#### Scenario: Harness data is documented
- **WHEN** docs describe harness runners, KPI output, self-eval modes, or JSON
  scenario fixtures
- **THEN** those docs may remain under `docs/harness/`
- **AND** the reusable behavior contract behind those fixtures is captured in
  OpenSpec when it is normative

### Requirement: Legacy spec bridges are temporary and narrow
Legacy contract paths outside OpenSpec SHALL either be removed or rewritten as
short bridge pages that identify the authoritative OpenSpec replacement.

#### Scenario: Existing links still target a retired contract doc
- **WHEN** a previously public or heavily linked `docs/spec/*.md` path is still
  needed during migration
- **THEN** the file contains only a short non-authoritative bridge
- **AND** the bridge names the OpenSpec capability or active change that owns
  the contract
- **AND** the bridge does not restate the full legacy contract

#### Scenario: Active references have been updated
- **WHEN** no active non-archived docs, guides, scripts, or agent instructions
  require a legacy bridge path
- **THEN** the bridge page is removed instead of retained as historical archive

### Requirement: Historical execution plans are removed after capture
Historical implementation plans SHALL NOT remain as current repository docs once
their active decisions are captured in OpenSpec or current implementation
guides.

#### Scenario: Plan is implemented or superseded
- **WHEN** a `plans/` or `docs/superpowers/` file describes work that is already
  implemented, archived, or superseded by an active OpenSpec change
- **THEN** the file is deleted after any still-current decisions are captured in
  OpenSpec or a current guide

#### Scenario: Plan still guides active work
- **WHEN** a historical plan still contains live scope, sequencing, or
  verification decisions
- **THEN** those decisions are moved into the relevant OpenSpec proposal,
  design, tasks, specs, or verification artifact before the plan is deleted

### Requirement: Documentation drift is validated
alan SHALL validate that active documentation does not recreate a parallel spec
system outside OpenSpec.

#### Scenario: Documentation cleanup is reviewed
- **WHEN** a change migrates or removes spec-like documentation
- **THEN** OpenSpec strict validation is run
- **AND** the review checks active non-archived references for stale
  `docs/spec/`, `plans/`, or `docs/superpowers/` contract-source links

#### Scenario: New spec-like docs are added outside OpenSpec
- **WHEN** an active non-OpenSpec document introduces normative target behavior
  or acceptance criteria without pointing to OpenSpec
- **THEN** the documentation governance review rejects the document or requires
  the normative content to move into OpenSpec

### Requirement: Current repository surfaces exclude the retired host-service architecture
Alan SHALL reject live retired host-service contracts, code, commands, configuration, tests,
fixtures, and consumers from current repository surfaces. The guard SHALL cover canonical specs,
active changes, source code, build and release wiring, public help, environment variables, and
current docs; it SHALL exclude immutable OpenSpec archive history and SHALL distinguish unrelated
Apple `LaunchDaemon`, terminal-session, authentication-session, and third-party protocol
terminology.

#### Scenario: Retired Alan host-service surface is reintroduced
- **WHEN** a current source, canonical spec, active change, command, configuration field, test, or
  current document reintroduces a retired host-service module, service-control command, Session
  transport route, remote forwarding service, snapshot-based reattachment, or
  host-service-backed consumer
- **THEN** repository verification fails and identifies the live owner that must be removed or
  expressed through its canonical Process, file, namespace, or service boundary

#### Scenario: Historical archive records the former architecture
- **WHEN** an immutable file under `openspec/changes/archive/` contains retired terminology
- **THEN** the current-surface guard ignores that historical record
- **AND** no current spec, code, help surface, or active change may cite it as current authority

#### Scenario: Unrelated platform terminology uses the same word
- **WHEN** current code uses Apple `LaunchDaemon` APIs or a terminal, authentication, or third-party
  protocol session that is not the retired execution-manager abstraction
- **THEN** the semantic guard permits that owned use
- **AND** a broad word-only allowlist SHALL NOT hide a retired host-service or Session-transport
  compatibility surface

### Requirement: Canonical and active OpenSpec surfaces are complete and current
Alan SHALL keep canonical capability metadata, repository OpenSpec
configuration, and active change references complete and valid for the current
repository and installed OpenSpec schema. Immutable archived changes SHALL be
excluded from current-surface rewrite requirements.

#### Scenario: Canonical capability is inspected
- **WHEN** a specification under `openspec/specs/` is validated
- **THEN** its Purpose describes the capability's current ownership and scope
- **AND** it does not contain a generated placeholder or archive reminder

#### Scenario: OpenSpec artifact instructions are loaded
- **WHEN** the repository asks OpenSpec for proposal, design, specs, or tasks
  instructions
- **THEN** every configured artifact-rule key is supported by the active schema
- **AND** instruction lookup emits no unknown-artifact warning

#### Scenario: Active change cites implementation scope
- **WHEN** a non-archived change names a source path, baseline, or implementation
  owner
- **THEN** the referenced surface exists or is explicitly introduced by that
  change
- **AND** deleted Console and retired remote-control surfaces are not counted as
  current scope

### Requirement: Active planning does not authorize temporary architecture bridges
Alan SHALL reject active OpenSpec work that authorizes a temporary callback,
DTO, ContentInstance, host-action, host-compatibility, or namespace-bootstrap
bridge in place of an accepted aP, file-tree, package, or binfs boundary.

#### Scenario: Dependent feature lacks its native boundary
- **WHEN** an active change depends on host attachment, a service file tree, or
  a mounted package command that does not yet exist
- **THEN** the active change records that foundation as an entry criterion or
  dependency
- **AND** it does not schedule a temporary bridge followed by a deletion task

#### Scenario: Current-surface validation finds bridge authorization
- **WHEN** a canonical spec or active change permits a named compatibility
  bridge or an equivalent temporary authority path
- **THEN** repository verification fails with the owning file and matched rule
- **AND** immutable archived change history remains outside the failure scope
