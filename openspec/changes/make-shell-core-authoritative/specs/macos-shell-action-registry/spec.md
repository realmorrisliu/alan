## ADDED Requirements

### Requirement: Shared shell action registry is shell-core authoritative
The macOS shell SHALL obtain shared action descriptors, default shortcuts,
keyboard mapping, target availability, and action effects from Rust shell core
once the action is included in the shell-core registry.

#### Scenario: Action descriptor is requested
- **WHEN** a menu, context menu, toolbar, or keyboard surface asks for a shared
  shell action descriptor
- **THEN** Swift obtains the descriptor from shell core
- **AND** Swift does not use a separate registry table for the same action

#### Scenario: Action availability is checked
- **WHEN** Swift checks whether a shared shell action can run for a target
- **THEN** the availability result comes from shell core target resolution
- **AND** Swift does not perform a separate domain availability check after a
  core error

#### Scenario: Core action registry is unavailable
- **WHEN** shell core cannot answer a shared action registry request
- **THEN** the action surface reports the action as unavailable with an explicit
  core failure reason
- **AND** it does not silently dispatch a Swift implementation of the same
  action

