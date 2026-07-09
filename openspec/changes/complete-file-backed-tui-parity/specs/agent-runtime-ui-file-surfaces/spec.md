## ADDED Requirements

### Requirement: Agent runtime projects renderer-visible UI state under `machine/ui`
Alan SHALL expose renderer-visible runtime UI state through a runtime-owned
`machine/ui/` subtree in the agent overlay. The subtree SHALL provide readable
snapshot files for the current activity, plan, thinking, and latest notice
state, plus a watchable `machine/ui/events` stream for ordered live updates.

#### Scenario: A local renderer host hydrates current UI state
- **WHEN** a local renderer host attaches to `/agent/<pid>`
- **THEN** it can read `machine/ui/` snapshot files to learn the current turn
  activity, current plan snapshot, latest renderer-visible thinking state, and
  latest notice state
- **AND** it does not need daemon session hydration APIs to recover that state

### Requirement: Renderer-visible runtime updates are watchable by blocking read
Alan SHALL append renderer-visible runtime updates to `machine/ui/events` as a
watchable blocking-read stream with monotonic offsets. Records SHALL cover turn
lifecycle changes, plan updates, thinking visibility, warnings, and compaction
or memory-flush notices.

#### Scenario: A renderer watches live runtime UI updates
- **WHEN** a turn starts, thinking text changes, a plan is updated, or a warning
  or compaction notice is emitted
- **THEN** `machine/ui/events` appends an ordered record describing that update
- **AND** a renderer can resume from its last offset without polling

### Requirement: UI file surfaces expose renderer-safe text rather than provider wire payloads
Alan SHALL project only renderer-safe, runtime-approved text into `machine/ui/`
surfaces. When reasoning content is available, the file surface SHALL expose the
same redacted or approved thinking text a renderer is allowed to show, never
provider-native wire payloads.

#### Scenario: Thinking is projected to files
- **WHEN** the runtime has renderer-visible thinking content for a turn
- **THEN** `machine/ui/` exposes only the renderer-safe thinking text and
  related lifecycle state
- **AND** it does not expose provider-specific reasoning wire formats or
  unapproved hidden payloads
