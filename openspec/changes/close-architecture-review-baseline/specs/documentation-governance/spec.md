## ADDED Requirements

### Requirement: Product lifecycle and implementation status are explicit
Current specifications and active plans SHALL distinguish supported product
surfaces, retained legacy maintenance, accepted but unimplemented direction,
parked work, and cancelled work. Alan for macOS is retired as a product direction;
its retained source and associated contracts are maintenance-only until a
separately scoped removal. This does not retire macOS platform support, Alan OS
Host, credentials, Host Mounts, sandboxing, or user stores.

#### Scenario: Retained desktop contract is consulted
- **WHEN** an agent reads a macOS desktop, shell-workspace, shell-core or App
  distribution contract retained for legacy source
- **THEN** its lifecycle notice identifies maintenance-only applicability
- **AND** it does not authorize new desktop features or require Herdr to
  reproduce that contract
- **AND** still-used platform security and data-preservation obligations remain

#### Scenario: An old implementation plan is resumed
- **WHEN** an agent considers a parked or cancelled change
- **THEN** it reads the disposition before executing tasks
- **AND** parked work requires a new explicit planning decision
- **AND** cancelled tasks remain incomplete history, with no unsatisfied delta
  promoted to delivered canonical behavior

#### Scenario: Mixed cognition direction is documented
- **WHEN** current documentation describes deterministic, evaluation and
  generation transitions or Herdr integration
- **THEN** it distinguishes accepted design from implemented and verified support
- **AND** it does not present Jev support or a Herdr Alan agent kind as shipped

#### Scenario: Desktop source removal is planned
- **WHEN** retained Apple source or shell-core consumers are removed
- **THEN** the removal inventories build, release, safety-adapter and storage
  consumers and supplies explicit requirement-removal deltas
- **AND** no account, credential, store, installed app or external release
  endpoint is deleted merely because the product direction was retired
