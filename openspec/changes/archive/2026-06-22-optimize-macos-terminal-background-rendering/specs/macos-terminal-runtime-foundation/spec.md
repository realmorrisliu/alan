## ADDED Requirements

### Requirement: Terminal render coordination is window scoped
Each macOS shell window SHALL own a terminal render coordinator that coalesces
embedded Ghostty wakeups for terminal ContentInstance handles in that window and
drains render work by terminal runtime priority.

#### Scenario: Multiple terminal surfaces wake at once
- **WHEN** several terminal ContentInstances in one shell window request Ghostty
  tick or refresh work during the same scheduling interval
- **THEN** the window render coordinator coalesces those requests into bounded
  main-actor drain work
- **AND** foreground interactive surfaces are processed before visible
  background and hidden background surfaces

#### Scenario: Hidden surface requests repeated refreshes
- **WHEN** a hidden background terminal ContentInstance repeatedly requests
  refresh work because of high output
- **THEN** the coordinator avoids scheduling one immediate surface paint per
  wakeup
- **AND** the coordinator retains enough pending state to catch up when that
  terminal becomes visible

#### Scenario: Window closes with pending render work
- **WHEN** a shell window closes while the render coordinator has pending
  terminal wakeups
- **THEN** the coordinator cancels or drains only work that is safe for closing
  terminal ContentInstance handles
- **AND** no pending wakeup resurrects a closed surface handle

### Requirement: Ghostty app tick and surface refresh are separate scheduling concerns
Alan SHALL distinguish embedded Ghostty app tick processing from terminal
surface refresh painting so that required state and lifecycle events can be
processed without forcing hidden surfaces to paint on every wakeup.

#### Scenario: Hidden terminal has lifecycle events
- **WHEN** a hidden terminal ContentInstance has pending child-exit, close,
  error, title, cwd, or attention events
- **THEN** Alan drains the required Ghostty or runtime state needed to publish
  truthful lifecycle metadata
- **AND** Alan does not treat that drain as permission to repaint the hidden
  surface on every output wakeup

#### Scenario: Visible terminal needs repaint
- **WHEN** a visible terminal ContentInstance has pending rendered output
- **THEN** Alan schedules surface refresh according to foreground or visible
  background priority
- **AND** the refresh path uses the existing terminal ContentInstance surface
  handle

#### Scenario: Hidden terminal is promoted to foreground
- **WHEN** a hidden background terminal ContentInstance becomes foreground
  interactive
- **THEN** Alan performs catch-up tick processing and schedules a surface
  refresh for that same ContentInstance before treating the terminal as current
  for user interaction

### Requirement: Runtime update publication is priority aware
The terminal runtime service SHALL retain the latest runtime state for every
terminal ContentInstance while publishing SwiftUI-facing updates at a cadence
appropriate to foreground interactive, visible background, and hidden background
priority.

#### Scenario: Foreground terminal metadata changes
- **WHEN** a foreground interactive terminal ContentInstance reports scrollback,
  renderer phase, title, cwd, process, input readiness, or attention changes
- **THEN** Alan publishes the update immediately enough for active terminal
  interaction and visible controls to remain current

#### Scenario: Visible background terminal metadata changes
- **WHEN** a visible background terminal ContentInstance reports runtime state
  changes
- **THEN** Alan coalesces SwiftUI-facing publication to the display cadence
- **AND** the terminal runtime service retains the latest state even if several
  updates are merged into one UI publication

#### Scenario: Hidden background terminal produces high-frequency updates
- **WHEN** a hidden background terminal ContentInstance reports high-frequency
  scrollback metrics, renderer phase changes, or output-driven refresh state
- **THEN** Alan retains the latest runtime state without continuously
  invalidating the shell root view
- **AND** sidebar-relevant summaries such as title, cwd, child exit, bell,
  attention, and failure remain publishable on a bounded slower path

### Requirement: Hidden terminal surfaces are unfocused and occluded for Ghostty
Alan SHALL propagate terminal focus and visibility priority to embedded Ghostty
surfaces so hidden background terminals are treated as unfocused and occluded
for rendering while remaining live for terminal state and IO.

#### Scenario: Selected pane changes
- **WHEN** terminal focus moves from one visible terminal pane to another
- **THEN** Alan marks the newly focused terminal foreground interactive
- **AND** Alan marks the previously focused terminal visible background or
  hidden background according to its actual visibility

#### Scenario: Tab is no longer visible
- **WHEN** a terminal ContentInstance belongs to a tab that is no longer visible
- **THEN** Alan marks the embedded Ghostty surface unfocused and occluded for
  rendering coordination
- **AND** the terminal runtime handle remains live in the window runtime service

#### Scenario: Hidden terminal becomes visible
- **WHEN** a hidden terminal ContentInstance becomes visible again
- **THEN** Alan updates Ghostty focus and occlusion state before the terminal is
  treated as ready for foreground interaction
