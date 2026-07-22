## MODIFIED Requirements

### Requirement: UI conformance is verified visually
Mac shell UI changes SHALL be reviewed against the documented UI contract before
the UI conformance tasks are marked complete.

#### Scenario: Default screenshot review
- **WHEN** a UI conformance implementation pass is ready for review
- **THEN** maintainers can inspect a running-app screenshot of the default light-mode window showing the top Space slider, active-space tab list, the workspace home content area, and no inspector surface

#### Scenario: Removed-inspector review
- **WHEN** inspector-removal UI tasks are marked complete
- **THEN** maintainers can inspect screenshots or recorded notes confirming the default shell has no right-side inspector and no inspector toggle

### Requirement: UI conformance has repeatable smoke evidence
Mac shell UI conformance work SHALL include repeatable smoke or screenshot
evidence for launch, space/tab switching, split creation, pane-scoped Find
behavior, and the absence of removed Ask alan and alan-tab surfaces.

#### Scenario: Default launch evidence
- **WHEN** a UI conformance implementation is ready
- **THEN** maintainers can run or inspect a smoke artifact showing the light-mode default window with material sidebar and the workspace home surface as the selected content

#### Scenario: Removed Ask alan evidence
- **WHEN** Ask alan or alan-tab removal is marked complete
- **THEN** maintainers can inspect evidence that the default shell has no
  `Ask alan...`, `Go to or Command...`, Command-P command input, or `New alan
  tab` surface

#### Scenario: Find evidence
- **WHEN** pane-scoped Find behavior changes
- **THEN** maintainers can run or inspect evidence confirming the active pane shows a native-feeling Find surface without restoring inspector chrome or debug-first panels

### Requirement: Automation surfaces do not add default chrome
Adding App Intents and automation support SHALL not add visible default UI chrome
or explanatory panels to the shell workflow.

#### Scenario: App Intents installed
- **WHEN** automation support is present in the app
- **THEN** the default shell window keeps its workspace home presentation and does not show automation setup cards, implementation jargon, or dashboard sections

#### Scenario: Intent result activates app
- **WHEN** an App Intent activates a shell target
- **THEN** the window opens to the relevant space, tab, or pane using normal shell UI rather than a special automation debug surface

### Requirement: Terminal content is the center of gravity
Within a terminal tab, the main content region SHALL make the active terminal
canvas visually dominant and SHALL avoid nested decorative panels around the
terminal in the default single-pane state. When the selected tab carries the
workspace home content kind, the main content region SHALL present the
workspace home surface as the dominant content instead; terminal dominance
SHALL NOT be required for non-terminal content kinds.

#### Scenario: Single-pane tab
- **WHEN** a tab contains one pane
- **THEN** the terminal appears nearly full-bleed within the content region and
  does not show a pane selector strip

#### Scenario: Split-pane tab
- **WHEN** a tab contains multiple panes
- **THEN** pane chrome stays lightweight and focus is conveyed by subtle
  selection treatment rather than explicit engineering labels

#### Scenario: alan as optional capability
- **WHEN** a terminal tab is active and no alan-specific surface has been opened
- **THEN** alan appears as an optional command or attachment capability layered
  onto the terminal workflow, not as the structural center of the window

#### Scenario: Workspace home tab
- **WHEN** the selected tab carries the workspace home content kind
- **THEN** the main content region presents the workspace home surface as the
  dominant content
- **AND** no terminal canvas is required to fill the region
