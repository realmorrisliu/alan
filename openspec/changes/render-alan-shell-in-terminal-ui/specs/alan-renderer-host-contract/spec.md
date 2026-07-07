## ADDED Requirements

### Requirement: Renderer hosts read files and write `ctl`
Alan renderer hosts SHALL render from Alan OS file surfaces rather than pulling
core-owned semantic snapshots or session view models. A renderer host reads
files under `/proc`, `/agent`, and mounted service trees, and translates user
input into file writes and `ctl` commands.

#### Scenario: A renderer host boundary is reviewed
- **WHEN** a renderer host such as `alan-terminal-ui` is reviewed against the
  Alan OS client model
- **THEN** its durable truth source is file reads from mounted Alan OS surfaces
- **AND** user actions are expressed as file writes or `ctl` writes
- **AND** it does not require a core-owned semantic snapshot or built-in view
  model contract

### Requirement: Renderer hosts SHALL treat compatibility transports as transitional during migration
Alan renderer hosts SHALL treat any compatibility transport kept during
migration, such as the daemon/session path, as transitional rather than the
renderer-host contract. A renderer host MAY keep that compatibility path during
the migration period, but the target contract remains direct file reading and
`ctl` writing.

#### Scenario: A compatibility renderer path still exists
- **WHEN** `alan-terminal-ui` runs on the daemon-backed session path during the
  migration period
- **THEN** that path is treated as a compatibility adapter
- **AND** the renderer host's terminal architecture target remains direct reads
  from `/proc` and `/agent` plus `ctl` writes

### Requirement: Local file-backed renderer hosts can launch from a mounted namespace
A local renderer host SHALL be able to receive a mounted aP root plus a concrete
agent path and render a live conversation from that surface without first
creating or attaching to a daemon session.

#### Scenario: A local renderer host starts a file-backed conversation
- **WHEN** a local renderer host is launched with a mounted namespace root and a
  concrete root-agent path such as `/agent/1`
- **THEN** it can tail `<agent>/io/output`, write `<agent>/io/input`, and write
  `interrupt` to `/proc/<pid>/ctl`
- **AND** no daemon session creation, history hydration, or event-stream attach
  is required before the conversation begins
