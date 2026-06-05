# macos-quick-terminal-peak Specification

## Purpose
Define the detached global quick-terminal Peak contract for native macOS:
runtime ownership, global identity, display/Space behavior, deterministic cwd
selection, and promotion into a normal Alan space.
## Requirements
### Requirement: Quick Terminal Uses Normal Terminal Runtime Ownership
Alan SHALL implement quick terminal behavior as a detached global macOS Peak
presentation over the existing shell model and terminal runtime service, not as
an independent terminal runtime owner.

#### Scenario: Quick terminal is summoned
- **WHEN** the user invokes the quick terminal command
- **THEN** Alan presents the single global quick-terminal instance using a
  normal pane runtime, restores its scrollback and process state when available,
  and focuses terminal input

#### Scenario: Quick terminal is dismissed
- **WHEN** the user dismisses the quick terminal without closing its terminal
- **THEN** Alan hides the presentation, preserves the terminal runtime state,
  and does not mutate normal tab or split runtime ownership

#### Scenario: Quick terminal is closed
- **WHEN** the user explicitly closes the quick terminal's terminal session
- **THEN** Alan tears down the underlying pane runtime through the same lifecycle
  path used by regular terminal panes

#### Scenario: Quick terminal is promoted
- **WHEN** the user promotes the quick terminal into an Alan space
- **THEN** Alan transfers ownership of the existing pane runtime into a normal
  tab in that Space and clears the quick-terminal slot

### Requirement: Quick Terminal Has One Global Instance
The quick terminal SHALL have a deterministic relationship to Alan's spaces,
tabs, focus, persistence, and runtime identity. Alan SHALL model the MVP as one
global quick-terminal instance, not one instance per Alan space or one instance
per macOS Space.

#### Scenario: Existing quick terminal is reused globally
- **WHEN** Alan creates or restores the quick terminal
- **THEN** Alan reuses the single global quick-terminal runtime when it is live
  rather than creating another quick terminal for the current Alan space or
  macOS Space

#### Scenario: Quick terminal appears in the current macOS context
- **WHEN** the user summons the hidden global quick terminal from a different
  macOS Space or display
- **THEN** Alan presents the same quick-terminal instance on the current active
  display and Space without changing its runtime identity

#### Scenario: Regular workspace remains stable
- **WHEN** the quick terminal appears or disappears
- **THEN** selected space, selected tab, split tree, regular terminal focus, and
  pane runtime identities remain stable unless the user explicitly moves focus
  or closes a session

#### Scenario: Quick terminal cwd is deterministic
- **WHEN** Alan creates a new quick-terminal runtime because no live global
  quick-terminal instance exists
- **THEN** Alan chooses the working directory from the focused Alan pane when
  available, otherwise from the last quick-terminal cwd when available,
  otherwise from the user's home directory

#### Scenario: Quick terminal promotes into a Space
- **WHEN** the user chooses `Open in Space` for a target Alan space
- **THEN** Alan moves the quick-terminal runtime into that Space as a normal tab,
  hides the Peak presentation, and clears the global quick-terminal slot

#### Scenario: Quick terminal promotion is not copy or link
- **WHEN** Alan promotes the quick terminal into a normal tab
- **THEN** Alan does not copy the terminal process and does not keep the same
  runtime visible in both the Peak and the target tab

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
