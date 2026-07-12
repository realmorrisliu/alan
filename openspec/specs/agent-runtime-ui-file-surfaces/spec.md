# agent-runtime-ui-file-surfaces Specification

## Purpose
Define the runtime-owned `machine/ui/` file surfaces that project
renderer-visible agent activity, plan state, thinking, and notices for local
renderer hosts.
## Requirements
### Requirement: Agent runtime projects renderer-visible UI state under `machine/ui`
Alan SHALL expose renderer-visible runtime UI state through the runtime-owned
`machine/ui/` subtree in the Agent Process overlay. The component that owns
activity, plan, thinking, or notice state SHALL write the corresponding readable
snapshot file directly and append its ordered update record. The live path
SHALL NOT derive these files from a generic runtime event broadcast or an
event-to-file projector.

#### Scenario: A renderer host hydrates current UI state
- **WHEN** a renderer host attaches to `/agent/<pid>`
- **THEN** it reads `machine/ui/` snapshot files for current activity, plan,
  renderer-visible thinking, and notice state
- **AND** hydration requires no second runtime state object, callback history,
  or transport-owned event history

#### Scenario: Plan owner updates renderer state
- **WHEN** the runtime accepts a plan update
- **THEN** the plan owner writes `machine/ui/plan` and appends the corresponding
  `machine/ui/events` record directly
- **AND** no `RuntimeEventEnvelope` projection step is required

### Requirement: Renderer-visible runtime updates are watchable by blocking read
Alan SHALL append renderer-visible runtime updates directly to
`machine/ui/events` as a watchable blocking-read stream with monotonic offsets.
Records SHALL cover turn lifecycle changes, plan updates, thinking visibility,
warnings, and compaction or memory-flush notices. The stream SHALL be owned by
AgentFS and SHALL NOT be a mirror of an engine broadcast receiver.

#### Scenario: A renderer watches live runtime UI updates
- **WHEN** a turn starts, thinking text changes, a plan is updated, or a warning
  or compaction notice is produced
- **THEN** the owning runtime component appends an ordered record to
  `machine/ui/events`
- **AND** a renderer can resume from its last file offset without polling or
  subscribing to an engine handle

#### Scenario: Renderer attaches after prior updates
- **WHEN** a renderer begins watching after the Agent Process has already
  produced UI updates
- **THEN** it hydrates current snapshots and reads retained stream records from
  an authorized offset
- **AND** no missed in-memory event makes the current state unavailable

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
