## MODIFIED Requirements

### Requirement: Product lifecycle and implementation status are explicit
Current specifications and active plans SHALL distinguish supported product
surfaces, accepted but unimplemented direction, parked work, and cancelled
work. Alan for macOS and its desktop implementation are retired; desktop-only
contracts are historical rather than active maintenance obligations. This does
not retire macOS platform support, alan9 Host, credentials, Host Mounts,
sandboxing, or user stores. The standalone CLI/Host distribution is the
supported delivery surface.

#### Scenario: Removed desktop contract is consulted
- **WHEN** an agent encounters a former macOS desktop, shell-workspace,
  shell-core or App distribution contract in historical material
- **THEN** it treats the contract as retired history, not current behavior or
  deferred desktop delivery
- **AND** still-used platform security and data-preservation obligations remain

#### Scenario: Standalone distribution is consulted
- **WHEN** an agent needs to install, package, or validate the supported Alan
  command-line product
- **THEN** it uses the standalone CLI/Host capability and current Rust/Host
  guides
- **AND** it does not select an app-bundle, Sparkle, appcast, or embedded-CLI
  path

#### Scenario: An old implementation plan is resumed
- **WHEN** an agent considers a parked or cancelled change
- **THEN** it reads the disposition before executing tasks
- **AND** parked work requires a new explicit planning decision
- **AND** cancelled tasks remain incomplete history, with no unsatisfied delta
  promoted to delivered canonical behavior

#### Scenario: Mixed cognition direction is documented
- **WHEN** current documentation describes deterministic, evaluation and
  generation transitions or Herdr integration
- **THEN** it distinguishes accepted design from implemented and verified
  support
- **AND** it distinguishes the shipped TypeSafe finite-choice adapter and opt-in
  no-effect Machine shadow advice from unqualified automatic input routing
- **AND** it does not present a Herdr Alan agent kind as shipped
