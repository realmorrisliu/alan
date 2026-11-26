# os-boundary-security Specification

## Purpose
Defines the OS confinement and explicit mount-grant security boundary for
confined native execution, and establishes mounted project instruction files as
non-authoritative knowledge.

## Requirements

### Requirement: The OS boundary is the security boundary for confined execution
When native execution runs inside an active OS confinement backend, Alan SHALL rely
on that confinement and the explicit mount grants as the security boundary. Alan
SHALL NOT reject or escalate a command solely because its shell text matches a
static pattern, and SHALL NOT invoke Guardian for an effect the active confinement
already contains.

#### Scenario: A contained command runs without review
- **WHEN** a command whose effects stay inside the granted confinement is proposed
- **THEN** it runs without a static text ban and without a Guardian call

#### Scenario: An uncontained effect still escalates
- **WHEN** a proposed effect escapes the active confinement, such as network egress beyond grants, an out-of-grant write, publication or a control-policy change
- **THEN** it follows the existing red-line and review path

#### Scenario: No confinement backend is active
- **WHEN** no confinement backend is active and Host-account execution was not explicitly selected
- **THEN** the conservative pre-sandbox escalation boundary remains in force

### Requirement: Project instruction files are non-authoritative knowledge
An Agent SHALL be able to read `AGENTS.md` and `CLAUDE.md` from an explicitly mounted
project as ordinary knowledge. Their content SHALL NOT widen authority, add grants,
change approval posture or isolation, or alter control policy.

#### Scenario: A mounted project carries an instruction file
- **WHEN** the project contains `AGENTS.md` or `CLAUDE.md` and the Agent reads it
- **THEN** its text informs the work like any other file

#### Scenario: An instruction file demands more authority
- **WHEN** an instruction file asks to disable review or enable full trust
- **THEN** posture, grants and control policy remain unchanged

### Requirement: Confined tool packages receive Host Store secrets without ambient Keychain access
Enabled `tool` subprocesses SHALL run inside OS confinement with outbound network restricted to their manifest `network_domains` and named secrets injected exclusively by Host adapters from Host Store (`SecretStore`) as short-lived environment variables or ephemeral `0600` ADC files, without ambient OS Keychain access.

#### Scenario: Confined gemini-api and colab-compute run without OS Keychain access
- **WHEN** an operator enables `gemini-api` or `colab-compute` with `--allow-network` and invokes a command
- **THEN** Host adapters project only the declared `GEMINI_API_KEY` env var or scoped `GOOGLE_APPLICATION_CREDENTIALS` file from Host Store into the confined subprocess
- **AND** outbound network egress is limited to the manifest's declared `network_domains` while ambient OS Keychain reads remain blocked

#### Scenario: Missing credentials or high-cost remote actions fail safely or confirm first
- **WHEN** a required `credential_id` is absent in Host Store or a user requests paid GPU provisioning via `colab-compute`
- **THEN** missing credentials fail deterministically with an explicit setup hint without leaking secret values
- **AND** Companion Skills require `colab usage` inspection and explicit user confirmation before provisioning paid GPU runtimes

### Requirement: Smart approval operates strictly inside the active OS boundary
`TrustPosture::ConfinedSmart` SHALL keep active OS sandbox confinement and
explicit Host Mount grants as the security boundary. Effects already contained
by active OS confinement SHALL run without invoking `decide()`. Decision
verdicts and scores SHALL remain evidence only and MUST NOT disable OS
confinement or widen grants. Without an active OS confinement backend,
`ConfinedSmart` SHALL keep the conservative pre-sandbox boundary.

#### Scenario: Contained action skips System 1 evaluation
- **WHEN** an Agent runs under `ConfinedSmart` inside an active OS confinement
  backend and proposes a command whose effects remain inside the granted
  confinement
- **THEN** the command executes without calling `connections/<conn>/decide`

#### Scenario: System 1 verdict cannot widen a Host Mount grant
- **WHEN** a proposed action writes outside explicit writable Host Mounts or
  requests ungranted private resources
- **THEN** a favorable System 1 verdict cannot authorize the effect without an
  explicit trusted-user grant

#### Scenario: No OS confinement backend is active under ConfinedSmart
- **WHEN** no OS sandbox backend is active on the current platform
- **THEN** Alan discloses the absence of confinement and keeps the conservative
  pre-sandbox boundary in force
