## REMOVED Requirements

### Requirement: Rust shell core owns reusable workspace domain logic

**Reason**: The dependency boundary names daemon hosting and an Axum daemon as live architectural comparison points.

**Migration**: Keep the shell core platform-neutral and define its allowed dependencies positively.

## ADDED Requirements

### Requirement: Rust shell core owns platform-neutral workspace domain logic

Alan SHALL provide a platform-neutral Rust shell workspace core that owns reusable Space, Tab, split, focus, lifecycle, and action semantics shared by host clients. The core SHALL depend only on portable domain types and explicit adapter contracts.

#### Scenario: Platform client mutates workspace state

- **WHEN** a platform client requests a reusable workspace mutation
- **THEN** the Rust shell core returns the next state, domain events, and adapter intents
- **AND** platform UI code does not reimplement the mutation semantics

#### Scenario: Shell core is built independently

- **WHEN** the shell core crate is compiled in isolation
- **THEN** it requires no Apple or GTK framework, terminal renderer, socket transport, privileged executor, clipboard, or file picker
- **AND** platform and OS effects remain behind adapters
