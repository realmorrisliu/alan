## ADDED Requirements

### Requirement: Background terminal render scheduling has focused verification
The Apple client SHALL include focused automated verification for terminal
runtime priority derivation, render wakeup coalescing, hidden-to-visible
catch-up, foreground-first scheduling, and hidden runtime publication throttling.

#### Scenario: Priority derivation is tested
- **WHEN** tests model selected panes, visible split siblings, hidden tabs,
  hidden spaces, split zoom, and window occlusion
- **THEN** the expected terminal runtime priority is derived for each affected
  terminal ContentInstance

#### Scenario: Hidden wakeups are coalesced
- **WHEN** a fake hidden background terminal handle emits repeated render
  wakeups
- **THEN** tests verify that Alan does not schedule one immediate surface
  refresh per wakeup
- **AND** the fake runtime still records pending state for later catch-up

#### Scenario: Foreground work wins over background output
- **WHEN** fake foreground and background terminal handles have pending work in
  the same coordinator drain
- **THEN** tests verify that foreground interactive work is drained before
  visible background and hidden background work

#### Scenario: Hidden terminal catches up on visibility
- **WHEN** a hidden terminal handle with pending output becomes visible
- **THEN** tests verify that catch-up tick and refresh work is requested for the
  existing terminal ContentInstance handle
- **AND** the fake runtime is not restarted or replaced

#### Scenario: Hidden publication is throttled
- **WHEN** a fake hidden background terminal emits high-frequency scrollback or
  renderer updates
- **THEN** tests verify that SwiftUI-facing publication is coalesced or deferred
- **AND** title, cwd, exit, bell, attention, and failure summaries remain
  observable on a bounded path

### Requirement: High-output background terminal stress smoke is documented
Terminal scheduling changes SHALL include documented stress-smoke evidence for
many live terminals or high-output background panes before implementation is
marked complete.

#### Scenario: Background command emits continuous output
- **WHEN** a developer runs a high-output command in a background tab, space, or
  hidden split sibling while interacting with the foreground terminal
- **THEN** the foreground terminal remains responsive to typing and focus
  changes
- **AND** the background command continues running in real time
- **AND** switching back to the background terminal shows current output without
  process restart or scrollback loss

#### Scenario: Multiple background panes are active
- **WHEN** several background terminal panes emit frequent output at the same
  time
- **THEN** debug evidence or test instrumentation records coalesced wakeups,
  bounded coordinator drain latency, and fewer hidden surface refreshes than
  hidden output wakeups

#### Scenario: Manual app verification is performed after install
- **WHEN** the implementation is ready for visual verification
- **THEN** the Apple client is built and installed with the project-supported
  command
- **AND** the running Alan app is relaunched before manual responsiveness and
  catch-up verification is treated as complete
