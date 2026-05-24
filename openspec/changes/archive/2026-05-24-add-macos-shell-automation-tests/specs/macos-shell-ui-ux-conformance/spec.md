## ADDED Requirements

### Requirement: UI conformance has repeatable smoke evidence
Mac shell UI conformance work SHALL include repeatable smoke or screenshot
evidence for launch, space/tab switching, split creation, command UI, and
pane-scoped Find behavior.

#### Scenario: Default launch evidence
- **WHEN** a UI conformance implementation is ready
- **THEN** maintainers can run or inspect a smoke artifact showing the light-mode default window with material sidebar and terminal-first content

#### Scenario: Command UI evidence
- **WHEN** command UI behavior changes
- **THEN** maintainers can run or inspect evidence showing `Go to or Command...` results with user-facing labels

#### Scenario: Find evidence
- **WHEN** pane-scoped Find behavior changes
- **THEN** maintainers can run or inspect evidence confirming the active pane shows a native-feeling Find surface without restoring inspector chrome or debug-first panels

### Requirement: Automation surfaces do not add default chrome
Adding App Intents and automation support SHALL not add visible default UI chrome
or explanatory panels to the terminal workflow.

#### Scenario: App Intents installed
- **WHEN** automation support is present in the app
- **THEN** the default shell window remains terminal-first and does not show automation setup cards, implementation jargon, or dashboard sections

#### Scenario: Intent result activates app
- **WHEN** an App Intent activates a shell target
- **THEN** the window opens to the relevant space, tab, or pane using normal shell UI rather than a special automation debug surface
