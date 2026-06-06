## ADDED Requirements

### Requirement: Workspace Creates Emacs TUI Terminal Tabs
The macOS shell workspace SHALL expose an explicit command for creating a new
Emacs TUI tab. The command SHALL create ordinary terminal content using the
managed Emacs TUI Terminal Profile rather than introducing an Emacs-specific tab
kind or content renderer.

#### Scenario: New Emacs tab command
- **WHEN** the user invokes the New Emacs Tab command from the shell workspace
- **THEN** alan creates a new terminal tab in the selected Space
- **AND** the tab's terminal content stores `terminal_profile_id` `emacs_tui`
- **AND** the tab participates in normal terminal tab selection, focus, split,
  movement, and close behavior

#### Scenario: Emacs tab uses current working directory
- **WHEN** the focused terminal pane has runtime cwd metadata `/repo/app`
- **AND** the user invokes the New Emacs Tab command without an explicit cwd
- **THEN** alan creates the Emacs terminal tab with working directory
  `/repo/app`

#### Scenario: Emacs tab remains terminal content
- **WHEN** an Emacs TUI tab is shown in the sidebar, command surface, or control
  plane
- **THEN** alan identifies it as terminal content with Terminal Profile metadata
- **AND** alan does not require a new `ShellTabKind`, non-terminal content
  payload, or GUI Emacs renderer to represent the tab

#### Scenario: Space default can use Emacs profile
- **WHEN** the user binds a Space to Terminal Profile `emacs_tui`
- **AND** the user creates a new terminal tab in that Space without an explicit
  profile override
- **THEN** alan creates the new terminal content with `terminal_profile_id`
  `emacs_tui`
