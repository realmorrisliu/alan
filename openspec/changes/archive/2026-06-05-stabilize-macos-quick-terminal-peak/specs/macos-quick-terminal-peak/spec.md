## ADDED Requirements

### Requirement: Quick Terminal Peak uses a dedicated presentation boundary
Alan SHALL present the Quick Terminal Peak through a dedicated presentation
controller and window presenter while preserving the existing global
quick-terminal runtime identity.

#### Scenario: Quick terminal show delegates presentation
- **WHEN** the user invokes the quick-terminal show or toggle command
- **THEN** Alan applies the quick-terminal shell mutation through the shared
  shell command path
- **AND** a dedicated Quick Terminal presentation controller observes the visible
  slot and drives Peak window presentation
- **AND** `ShellHostController` does not directly own detailed `NSPanel`
  ordering, delegate, collection-behavior, or terminal-focus timing

#### Scenario: Presentation does not create an independent runtime owner
- **WHEN** the dedicated presentation controller shows the Peak
- **THEN** it uses the existing quick-terminal content identity and terminal
  runtime service
- **AND** it does not create a second quick-terminal runtime owner

### Requirement: Peak panel presentation precedes terminal surface attachment
Alan SHALL separate user-visible Peak panel presentation from terminal surface
attachment and focus so slow terminal renderer setup does not run in the same
synchronous stack as shell-state mutation and panel creation.

#### Scenario: Peak panel becomes visible before terminal attach
- **WHEN** the quick-terminal slot becomes visible
- **THEN** Alan creates or reuses the detached Peak panel and orders it visible
- **AND** terminal surface attachment is deferred until the panel is attached or
  visible on a later main-actor step

#### Scenario: Terminal focus is best-effort after host registration
- **WHEN** the Peak panel is visible and the terminal host view has been
  registered for the quick-terminal content
- **THEN** Alan requests terminal focus as a bounded best-effort action
- **AND** repeated focus retries do not continuously force the panel key status

#### Scenario: Terminal attach fails
- **WHEN** Ghostty bootstrap or surface attachment fails for the Peak content
- **THEN** Alan keeps the quick-terminal shell slot and runtime identity
  recoverable
- **AND** the main workspace window remains responsive

### Requirement: Quick Terminal Peak uses narrow terminal-first content
Alan SHALL render the Peak with a dedicated Quick Terminal content view that
only depends on the quick-terminal pane/content model and minimal Peak commands.

#### Scenario: Peak content excludes normal workspace chrome
- **WHEN** Alan renders the Quick Terminal Peak
- **THEN** the Peak content does not include the regular workspace sidebar,
  normal tab header, general split controls, or workspace selection UI
- **AND** the Peak remains terminal-first with minimal close and promotion
  affordances

#### Scenario: Ordinary workspace updates do not rebuild Peak composition
- **WHEN** unrelated regular workspace state changes while the Peak is visible
- **THEN** Alan avoids rebuilding Peak content from the full workspace
  `TerminalPaneView` tree solely because normal tabs, spaces, or sidebar state
  changed

### Requirement: Quick Terminal promotion remains a runtime move
Alan SHALL preserve `Open in Space` as a move of the existing quick-terminal
runtime into a normal Alan tab.

#### Scenario: Quick terminal is promoted after boundary refactor
- **WHEN** the user promotes the quick terminal into a Space
- **THEN** Alan moves the existing quick-terminal runtime identity into the
  target Space as a normal tab
- **AND** Alan clears the global quick-terminal slot and releases the Peak
  presentation
- **AND** Alan does not copy, link, or duplicate the terminal process
