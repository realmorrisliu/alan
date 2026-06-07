## ADDED Requirements

### Requirement: macOS delegates Terminal Profile domain semantics to shell core
Alan for macOS SHALL delegate Terminal Profile document validation, editor
semantics, deterministic resolution order, missing/unavailable profile state,
and terminal launch-intent construction to the Rust shell core after the
Terminal Profile module has parity fixtures and adapter tests.

The macOS platform layer SHALL continue to own profile store file IO, channel
Application Support path selection, terminal runtime spawn translation, and
privileged Managed Terminal Account apply operations.

#### Scenario: Terminal Profile is resolved for pane startup
- **WHEN** macOS creates terminal content with a Terminal Profile reference
  after shell core profile integration
- **THEN** the Rust shell core resolves the profile and returns a launch intent
- **AND** the macOS terminal runtime adapter translates that intent into the
  concrete Ghostty or shell startup operation

#### Scenario: Profile store is saved
- **WHEN** a Terminal Profile document is edited after shell core profile
  integration
- **THEN** the Rust shell core validates document semantics
- **AND** the macOS platform layer writes the document to its channel-scoped
  store location

### Requirement: Privileged account effects stay platform-owned
Privileged account effects SHALL remain platform-owned.

Managed Terminal Account privileged apply, sudoers writes, AppleScript
authorization, account lookup commands, and platform verification executors SHALL
remain outside the shell core and MUST stay platform-owned.

The shell core MAY own portable request, plan, validation, handoff, or profile
intent semantics only when those semantics do not execute OS effects directly.

#### Scenario: Managed account apply is approved
- **WHEN** the user approves a Managed Terminal Account provisioning plan on
  macOS
- **THEN** macOS executes privileged account and sudoers operations through the
  platform-owned executor
- **AND** shell core does not receive reusable privileged credentials or invoke
  AppleScript, sudoers writes, or account-management commands directly
