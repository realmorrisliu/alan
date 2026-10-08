## MODIFIED Requirements

### Requirement: First-party Skills are preinstalled packages

alan9 SHALL seed first-party Skill trees as deterministic, ordinary
preinstalled Package Service packages. The Root Agent Process Boot Unit SHALL
reference them explicitly. Agent Execution Engine MUST NOT append a separate
compiled-in built-in package set during capability resolution. Trusted product
boot code MAY explicitly retire obsolete preinstalled packages through Package
Service. Retirement SHALL preserve operator packages and live revision leases
while preventing new resolution, and SHALL be idempotent.

#### Scenario: Empty channel boots after installation

- **WHEN** Package Service opens an empty channel store
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
