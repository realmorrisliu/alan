## ADDED Requirements

### Requirement: macOS control transport delegates reusable command semantics
The macOS shell control plane SHALL delegate reusable command validation,
workspace reducer dispatch, stable error codes, and authoritative response
projection to the Rust shell core after the control reducer module has parity
fixtures and adapter tests.

The macOS control plane SHALL continue to own socket transport, file polling,
request size limits, response deadlines, runtime service calls, event store IO,
and diagnostics recording.

#### Scenario: Domain command is received over socket
- **WHEN** a socket client sends a workspace-domain control command after shell
  core control integration
- **THEN** the macOS transport decodes and bounds the request
- **AND** the Rust shell core validates and reduces the command
- **AND** the macOS control plane publishes the returned state, events, runtime
  intents, and response through existing transport semantics

#### Scenario: Runtime command is reduced
- **WHEN** a control command targets terminal runtime behavior such as text
  delivery
- **THEN** the shell core returns domain target validation and a runtime intent
- **AND** the macOS terminal runtime service supplies the platform-specific
  delivery outcome for the final control response

### Requirement: Stable control response compatibility is maintained
Rust-backed control command reduction SHALL preserve existing macOS shell
control response shapes and stable error codes unless a later spec explicitly
changes them.

#### Scenario: Missing target error is returned
- **WHEN** a Rust-backed control command references a missing Space, Tab,
  PaneSlot, or ContentInstance
- **THEN** the returned response uses the same stable error semantics expected
  by current macOS shell control clients
- **AND** clients do not need to infer the failure from raw state snapshots
