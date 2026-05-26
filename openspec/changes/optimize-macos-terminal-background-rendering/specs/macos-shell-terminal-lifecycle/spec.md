## ADDED Requirements

### Requirement: Background terminal execution remains real-time while rendering is priority-scoped
The macOS shell host SHALL keep terminal ContentInstance processes, PTYs,
terminal state, pending input delivery, and scrollback running in real time while
controlling surface focus, occlusion, refresh, and SwiftUI publication by
terminal runtime priority.

#### Scenario: Hidden terminal produces output
- **WHEN** a terminal ContentInstance is hidden by tab selection, space
  selection, split zoom, pane movement, or window occlusion
- **THEN** the terminal child process, PTY reads, terminal state, and scrollback
  continue running without requiring that terminal to become visible
- **AND** output remains available through the same terminal ContentInstance
  runtime when the terminal is reattached

#### Scenario: Background terminal receives text
- **WHEN** `terminal.send_text` targets a live background terminal
  ContentInstance
- **THEN** Alan delivers or durably queues the text according to the existing
  delivery contract without selecting the tab, space, or pane
- **AND** the terminal runtime priority does not cause a false success response

#### Scenario: Hidden terminal becomes visible
- **WHEN** a hidden terminal ContentInstance becomes visible
- **THEN** Alan runs a catch-up path that presents current terminal state from
  the existing runtime
- **AND** the terminal process, scrollback, cwd, title, terminal mode, and
  metadata are not reset solely because visibility changed

### Requirement: Terminal visibility transitions preserve runtime continuity
The macOS shell host SHALL treat visibility, focus, split zoom, tab selection,
space selection, and window occlusion changes as render scheduling inputs rather
than terminal lifecycle finalizers.

#### Scenario: Split zoom hides sibling terminals
- **WHEN** a user zooms one terminal pane and sibling terminal panes become
  hidden
- **THEN** sibling terminal ContentInstances remain registered in the terminal
  runtime service
- **AND** their processes and scrollback continue while their rendering priority
  changes to hidden background

#### Scenario: User switches spaces with live terminals
- **WHEN** a user switches from one shell space to another
- **THEN** terminal ContentInstances in the previous space keep their runtime
  identities and background execution
- **AND** terminal ContentInstances in the new space receive visible or
  foreground priority according to selection and focus

#### Scenario: Window becomes occluded
- **WHEN** the macOS window is occluded or hidden while terminal runtimes remain
  active
- **THEN** Alan marks affected terminal surfaces hidden for rendering
  coordination
- **AND** Alan does not close, recreate, or detach their terminal runtime
  identities solely because the window is not visible
