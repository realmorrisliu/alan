## MODIFIED Requirements

### Requirement: Package Service owns its durable state

Package Service SHALL own the package subtree of the explicitly bound product
System Store, including catalog records, staged transactions, immutable
revisions, materialization manifests, and reference-retirement state. No raw
backing path SHALL be part of package identity or an Agent-visible record.

#### Scenario: Stable and dev install the same package id

- **WHEN** historical stable and dev stores contain the same package id during adoption
- **THEN** only the explicitly selected source's package records are adopted
- **AND** records from the other source are neither merged nor deleted

#### Scenario: Independent invocations install the same package id

- **WHEN** independent invocations Package Services install the same package id
- **THEN** each service updates only its explicitly bound product System Store subtree
- **AND** publication preserves transaction isolation and immutable live revisions

#### Scenario: Agent inspects package metadata

- **WHEN** an Agent reads Package Service catalog data or projected content
- **THEN** it sees package ids, revisions, exports, and alan9 paths
- **AND** it does not see the System Store backing path

### Requirement: First-party Skills are preinstalled packages

alan9 SHALL seed first-party Skill trees as deterministic, ordinary
preinstalled Package Service packages. The Root Agent Process Boot Unit SHALL
reference them explicitly. Agent Execution Engine MUST NOT append a separate
compiled-in built-in package set during capability resolution. Trusted product
boot code MAY explicitly retire obsolete preinstalled packages through Package
Service. Retirement SHALL preserve operator packages and live revision leases
while preventing new resolution, and SHALL be idempotent.

#### Scenario: Empty channel boots after installation

- **WHEN** an old installation selects an empty historical channel store
- **THEN** the current product does not seed or open that channel as an active identity
- **AND** it requires the explicit migration or canonical product startup path

#### Scenario: Empty product store boots after installation

- **WHEN** Package Service opens an empty product store
- **THEN** it seeds the current first-party package revisions idempotently
- **AND** Root Agent references resolve through the ordinary package path

#### Scenario: Capability view is assembled

- **WHEN** Agent Execution Engine builds the Root Agent capability view
- **THEN** every first-party Skill came from an explicit Package Service
  reference
- **AND** no `builtin_capability_packages()` bypass adds another copy

#### Scenario: Product upgrade retires a preinstalled package

- **WHEN** boot explicitly retires a previously seeded product-owned package
- **THEN** new resolution and acquisition fail after the retirement commits
- **AND** live references in any invocation retain their immutable content until release
- **AND** operator-installed records with the same ID are unchanged
- **AND** repeated retirement is a no-op

## ADDED Requirements

### Requirement: Embedded seeding does not accumulate per-process source copies
First-party package seeding SHALL use the ordinary validated snapshot and immutable revision contract without persistent per-process Host source copies. Enumeration of package identities MUST NOT write files. Exported bytes, identities, filtering and digest semantics SHALL remain equivalent to the existing package contract.

#### Scenario: Repeated product invocations seed unchanged Skills
- **WHEN** many invocations seed the same embedded first-party content
- **THEN** they reuse the owning Package Service's deterministic revisions
- **AND** no new PID-keyed Skill extraction directory remains

#### Scenario: Embedded content violates snapshot constraints
- **WHEN** embedded entries exceed package limits or contain invalid paths or duplicate identities
- **THEN** ordinary snapshot validation rejects them before publication
- **AND** trusted origin does not bypass validation

#### Scenario: Consumers request package identities
- **WHEN** boot constructs references from preinstalled package IDs
- **THEN** enumeration does not materialize another Host directory tree
